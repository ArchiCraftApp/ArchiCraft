//! Reads an `ifcx_alpha` document written by ArchiCraft back into an editable
//! project (IFX-064, IFX-080).
//!
//! Reading is lenient (IFX-083): every part that can be read is loaded, and
//! every skipped, repaired or placeholder object is reported with the reason
//! (PRJ-008, NFR-REL-004). Nothing is dropped silently.
//!
//! Every report names its record and says what happened to it, as
//! “<record> skipped: <reason>” (an error: the record was not loaded) or
//! “<record> repaired: <reason>” (a warning: the record was loaded with a
//! default or corrected value). The report's path is the node holding the
//! record: the project node for project-level records (settings, materials,
//! layers, views, tool defaults, counters), else the storey or element node.
//!
//! An absent field is damage, like a present field of the wrong type or out
//! of range, because the writer writes every field. The exceptions are the
//! optional fields the writer leaves out when they are not set: a material's
//! cut-fill `background`, a storey's `cutPlaneHeight` and the settings'
//! `planGrid` (files from before INP-013). Their absence is no problem.

use archicraft_model::glam::DVec2;
use archicraft_model::material::{CutFill, FillPattern, Material, MaterialCategory, Rgb};
use archicraft_model::project::{
    Building, Location, NameCounters, Projection, Site, SlabDefaults, Tessellation, ToolDefaults, View3d, WallDefaults,
};
use archicraft_model::storey::MIN_STOREY_HEIGHT;
use archicraft_model::template::default_metric_project;
use archicraft_model::tolerance::MODEL_TOLERANCE;
use archicraft_model::units::{AngleUnit, AreaUnit, LengthUnit, SlopeFormat, UnitSettings, VolumeUnit};
use archicraft_model::validate::{valid_grid_origin, valid_grid_spacing, validate_element};
use archicraft_model::{
    BuildingId, Element, ElementId, ElementKind, IfcClass, KindTag, Layer, LayerId, MaterialId, PlanGrid, Position,
    Project, ProjectId, ProjectInfo, ProjectSettings, ReferenceLine, SiteId, Slab, SlabReference, Storey, StoreyId,
    StoreyList, TopLink, ViewId, Wall,
};
use std::fmt::Display;

use std::borrow::Cow;

use indexmap::IndexMap;
use serde_json::{Map, Value};
use uuid::Uuid;

use super::migrate::{self, Migration};
use super::schema::{self, FORMAT_VERSION};
use super::writer::IFCX_VERSION;
use crate::container::IfcxFile;
use crate::validate::{self, FlatNode, Problem};

/// A project read from a file, with everything worth reporting.
#[derive(Clone, Debug)]
pub struct ReadOutcome {
    pub project: Project,
    pub problems: Vec<Problem>,
    /// The file was written by a newer ArchiCraft version (IFX-062); it must
    /// not be overwritten without confirmation.
    pub newer_format: bool,
    /// The IFCX version the file was read as (IFX-090). When it is not
    /// [`crate::WRITTEN_VERSION`], saving converts the file to that version.
    pub source_version: &'static str,
}

type Object = Map<String, Value>;

/// Collects problems for one record, reported at the node `path`.
struct Report<'a> {
    problems: &'a mut Vec<Problem>,
    path: String,
    /// Names the record in every message, for example “layer record 1” or
    /// “element W-001”.
    record: String,
}

impl<'a> Report<'a> {
    fn new(problems: &'a mut Vec<Problem>, path: &str, record: impl Into<String>) -> Self {
        Self {
            problems,
            path: path.to_owned(),
            record: record.into(),
        }
    }

    fn warn(&mut self, message: impl Into<String>) {
        self.problems.push(Problem::warning(Some(&self.path), message));
    }

    /// The record was not loaded (PRJ-008).
    fn skipped(&mut self, reason: impl Display) {
        let message = format!("{} skipped: {reason}", self.record);
        self.problems.push(Problem::error(Some(&self.path), message));
    }

    /// The record was loaded with a default or corrected value (PRJ-008).
    fn repaired(&mut self, reason: impl Display) {
        let message = format!("{} repaired: {reason}", self.record);
        self.warn(message);
    }

    /// Unwraps a field, or reports the record as repaired and uses `default`.
    fn or<T>(&mut self, result: Result<T, String>, default: T) -> T {
        match result {
            Ok(v) => v,
            Err(e) => {
                self.repaired(format!("{e}; a default value is used"));
                default
            }
        }
    }

    /// An optional field the writer may leave out: absent is `None`, present
    /// but invalid is reported and left out.
    fn optional<T>(&mut self, map: &Object, key: &str, read: impl Fn(&Object, &str) -> Result<T, String>) -> Option<T> {
        if !map.contains_key(key) {
            return None;
        }
        match read(map, key) {
            Ok(v) => Some(v),
            Err(e) => {
                self.repaired(format!("{e}; it is left out"));
                None
            }
        }
    }
}

/// Names the record at `index` of a project-level list, like “layer record 1”.
fn record(what: &str, index: usize) -> String {
    format!("{what} record {index}")
}

fn field<'v>(map: &'v Object, key: &str) -> Result<&'v Value, String> {
    map.get(key).ok_or_else(|| format!("“{key}” is missing"))
}

fn real(map: &Object, key: &str) -> Result<f64, String> {
    field(map, key)?
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("“{key}” is not a number"))
}

fn integer(map: &Object, key: &str) -> Result<i64, String> {
    let v = field(map, key)?;
    v.as_i64()
        .or_else(|| {
            v.as_f64()
                .filter(|f| f.fract() == 0.0 && f.abs() < 9e15)
                .map(|f| f as i64)
        })
        .ok_or_else(|| format!("“{key}” is not an integer"))
}

fn text(map: &Object, key: &str) -> Result<String, String> {
    field(map, key)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("“{key}” is not text"))
}

fn boolean(map: &Object, key: &str) -> Result<bool, String> {
    field(map, key)?
        .as_bool()
        .ok_or_else(|| format!("“{key}” is not true or false"))
}

fn object<'v>(map: &'v Object, key: &str) -> Result<&'v Object, String> {
    field(map, key)?
        .as_object()
        .ok_or_else(|| format!("“{key}” is not an object"))
}

fn array<'v>(map: &'v Object, key: &str) -> Result<&'v Vec<Value>, String> {
    field(map, key)?
        .as_array()
        .ok_or_else(|| format!("“{key}” is not a list"))
}

fn code<T>(map: &Object, key: &str, parse: impl Fn(&str) -> Option<T>) -> Result<T, String> {
    let value = text(map, key)?;
    parse(&value).ok_or_else(|| format!("“{key}” has the unknown value “{value}”"))
}

fn uuid(map: &Object, key: &str) -> Result<Uuid, String> {
    let value = text(map, key)?;
    Uuid::parse_str(&value).map_err(|_| format!("“{key}” is not a UUID"))
}

fn point(value: &Value) -> Result<DVec2, String> {
    let items = value.as_array().ok_or("a point is not a list")?;
    match items.as_slice() {
        [x, y] => match (x.as_f64(), y.as_f64()) {
            (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Ok(DVec2::new(x, y)),
            _ => Err("a point has non-numeric coordinates".into()),
        },
        _ => Err("a point does not have two coordinates".into()),
    }
}

fn points(value: &Value) -> Result<Vec<DVec2>, String> {
    value
        .as_array()
        .ok_or("a point list is not a list")?
        .iter()
        .map(point)
        .collect()
}

fn rgb(map: &Object, key: &str) -> Result<Rgb, String> {
    let values = array(map, key)?
        .iter()
        .map(Value::as_f64)
        .collect::<Option<Vec<f64>>>()
        .ok_or_else(|| format!("“{key}” has a non-numeric component"))?;
    match values.as_slice() {
        [r, g, b] if values.iter().all(|c| (0.0..=1.0).contains(c)) => Ok(Rgb([*r, *g, *b])),
        _ => Err(format!("“{key}” is not a colour")),
    }
}

fn top_link(map: &Object) -> Result<TopLink, String> {
    let value = real(map, "value")?;
    match text(map, "link")?.as_str() {
        "fixed" => Ok(TopLink::Fixed { height: value }),
        "storey" => Ok(TopLink::Storey {
            storey: StoreyId::from_uuid(uuid(map, "storey")?),
            offset: value,
        }),
        "relative" => Ok(TopLink::Relative {
            storeys: i32::try_from(integer(map, "relative")?).map_err(|_| "“relative” is out of range")?,
            offset: value,
        }),
        other => Err(format!("unknown top link “{other}”")),
    }
}

fn reference_line(map: &Object) -> Result<ReferenceLine, String> {
    match text(map, "referenceLine")?.as_str() {
        "outside" => Ok(ReferenceLine::Outside),
        "center" => Ok(ReferenceLine::Center),
        "inside" => Ok(ReferenceLine::Inside),
        "custom" => Ok(ReferenceLine::Custom(real(map, "referenceOffset")?)),
        other => Err(format!("unknown reference-line location “{other}”")),
    }
}

fn class_code(node: &FlatNode) -> Option<&str> {
    node.attributes.get("bsi::ifc::class")?.get("code")?.as_str()
}

fn name(node: &FlatNode) -> Option<String> {
    node.attributes
        .get("bsi::ifc::prop::Name")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn node_uuid(path: &str, report: &mut Report<'_>) -> Uuid {
    Uuid::parse_str(path).unwrap_or_else(|_| {
        report.repaired("the path is not a UUID; the object gets a new identity");
        Uuid::new_v4()
    })
}

/// Children of `node` whose class is `class`, in order.
fn children_with_class<'n>(
    nodes: &'n IndexMap<String, FlatNode>,
    node: &FlatNode,
    class: &str,
) -> Vec<(&'n String, &'n FlatNode)> {
    node.children
        .values()
        .flatten()
        .filter_map(|path| nodes.get_key_value(path.as_str()))
        .filter(|(_, n)| class_code(n) == Some(class))
        .collect()
}

/// Material record `index` of the project at `path`.
fn read_material(index: usize, value: &Value, path: &str, problems: &mut Vec<Problem>) -> Option<Material> {
    let mut report = Report::new(problems, path, record("material", index));
    let Some(map) = value.as_object() else {
        report.skipped("it is not an object");
        return None;
    };
    let id = match uuid(map, "id") {
        Ok(id) => MaterialId::from_uuid(id),
        Err(e) => {
            report.skipped(e);
            return None;
        }
    };
    let color = report.or(rgb(map, "color"), Rgb([0.7, 0.7, 0.7]));
    let fill = report.or(object(map, "cutFill").cloned(), Object::new());
    let mut material = Material {
        id,
        name: report.or(text(map, "name"), "Material".to_owned()),
        code: report.or(text(map, "code"), String::new()),
        description: report.or(text(map, "description"), String::new()),
        category: report.or(
            code(map, "category", MaterialCategory::from_code),
            MaterialCategory::Other,
        ),
        color,
        opacity: report.or(real(map, "opacity"), 1.0),
        priority: report.or(
            integer(map, "priority").and_then(|p| u16::try_from(p).map_err(|_| "“priority” is out of range".into())),
            500,
        ),
        cut_fill: CutFill {
            pattern: report.or(code(&fill, "pattern", FillPattern::from_code), FillPattern::Solid),
            foreground: report.or(rgb(&fill, "foreground"), color),
            background: report.optional(&fill, "background", rgb),
        },
    };
    if let Err(e) = material.validate() {
        report.repaired(format!("“{}”: {e}; the values are corrected", material.name));
        material.opacity = material.opacity.clamp(0.0, 1.0);
        material.priority = material.priority.clamp(Material::MIN_PRIORITY, Material::MAX_PRIORITY);
        if material.name.trim().is_empty() {
            material.name = "Material".into();
        }
    }
    Some(material)
}

/// Layer record `index` of the project at `path`.
fn read_layer(index: usize, value: &Value, path: &str, problems: &mut Vec<Problem>) -> Option<Layer> {
    let mut report = Report::new(problems, path, record("layer", index));
    let Some(map) = value.as_object() else {
        report.skipped("it is not an object");
        return None;
    };
    let id = match uuid(map, "id") {
        Ok(id) => LayerId::from_uuid(id),
        Err(e) => {
            report.skipped(e);
            return None;
        }
    };
    Some(Layer {
        id,
        name: report.or(text(map, "name"), "Layer".to_owned()),
        visible: report.or(boolean(map, "visible"), true),
        locked: report.or(boolean(map, "locked"), false),
    })
}

/// View record `index` of the project at `path`. A view has no field with a
/// sensible default, so an invalid one skips the view.
fn read_view(index: usize, value: &Value, path: &str, problems: &mut Vec<Problem>) -> Option<View3d> {
    let mut report = Report::new(problems, path, record("view", index));
    let Some(map) = value.as_object() else {
        report.skipped("it is not an object");
        return None;
    };
    let view = uuid(map, "id").and_then(|id| {
        Ok(View3d {
            id: ViewId::from_uuid(id),
            name: text(map, "name")?,
            projection: code(map, "projection", Projection::from_code)?,
        })
    });
    view.map_err(|e| report.skipped(e)).ok()
}

/// A two-component grid value, checked against the model's range.
fn grid_vector(map: &Object, key: &str, valid: fn(DVec2) -> bool) -> Result<DVec2, String> {
    let value = point(field(map, key)?).map_err(|e| format!("“{key}”: {e}"))?;
    if valid(value) {
        Ok(value)
    } else {
        Err(format!("“{key}” is out of range"))
    }
}

/// The drafting grid of the plan views (INP-013). A missing `planGrid` is no
/// problem: files written before it existed get the default grid. Inside a
/// present grid, each invalid field is reported and falls back to its default.
fn read_plan_grid(settings: &Object, report: &mut Report) -> PlanGrid {
    let defaults = PlanGrid::default();
    let Ok(map) = object(settings, "planGrid") else {
        return defaults;
    };
    let mut grid = PlanGrid {
        snap_step: report.or(grid_vector(map, "snapStep", valid_grid_spacing), defaults.snap_step),
        snap_enabled: report.or(boolean(map, "snapEnabled"), defaults.snap_enabled),
        main_spacing: report.or(
            grid_vector(map, "mainSpacing", valid_grid_spacing),
            defaults.main_spacing,
        ),
        auxiliary_spacing: report.or(
            grid_vector(map, "auxiliarySpacing", valid_grid_spacing),
            defaults.auxiliary_spacing,
        ),
        rotation: report.or(real(map, "rotation"), defaults.rotation),
        origin: report.or(grid_vector(map, "origin", valid_grid_origin), defaults.origin),
        visible: report.or(boolean(map, "visible"), defaults.visible),
    };
    if grid.auxiliary_spacing.cmpgt(grid.main_spacing).any() {
        report.warn("“auxiliarySpacing” is larger than “mainSpacing”; the default spacings are used");
        grid.main_spacing = defaults.main_spacing;
        grid.auxiliary_spacing = defaults.auxiliary_spacing;
    }
    grid
}

/// Project-level data from `archicraft::project`, before storeys and
/// elements are known.
struct ProjectData {
    settings: ProjectSettings,
    location: Location,
    info: ProjectInfo,
    ground: Option<StoreyId>,
    layers: Vec<Layer>,
    materials: Vec<Material>,
    tool_defaults: Option<Object>,
    views: Vec<View3d>,
    /// The `counters` object, read once the elements are known.
    counters: Result<Object, String>,
    newer_format: bool,
}

/// Display decimals: an integer from 0 to 8.
fn decimals(map: &Object, key: &str) -> Result<u8, String> {
    u8::try_from(integer(map, key)?)
        .ok()
        .filter(|v| *v <= 8)
        .ok_or_else(|| format!("“{key}” is out of range"))
}

fn read_settings(map: &Object, report: &mut Report<'_>) -> ProjectSettings {
    let defaults = ProjectSettings::default();
    let units = UnitSettings {
        length: report.or(code(map, "lengthUnit", LengthUnit::from_code), LengthUnit::Meter),
        length_decimals: report.or(decimals(map, "lengthDecimals"), defaults.units.length_decimals),
        angle: report.or(code(map, "angleUnit", AngleUnit::from_code), AngleUnit::DecimalDegrees),
        angle_decimals: report.or(decimals(map, "angleDecimals"), defaults.units.angle_decimals),
        slope: report.or(code(map, "slopeFormat", SlopeFormat::from_code), SlopeFormat::Degrees),
        area: report.or(code(map, "areaUnit", AreaUnit::from_code), AreaUnit::SquareMeter),
        volume: report.or(code(map, "volumeUnit", VolumeUnit::from_code), VolumeUnit::CubicMeter),
    };
    ProjectSettings {
        units,
        cut_plane_height: report.or(real(map, "cutPlaneHeight"), defaults.cut_plane_height),
        tessellation: Tessellation {
            chord_deviation: report.or(real(map, "chordDeviation"), defaults.tessellation.chord_deviation),
            max_angle_deg: report.or(real(map, "maxSegmentAngle"), defaults.tessellation.max_angle_deg),
        },
        plan_grid: read_plan_grid(map, report),
    }
}

/// Reads the project attribute of a file migrated to format `current`.
fn read_project_data(attr: &Object, path: &str, current: u32, problems: &mut Vec<Problem>) -> ProjectData {
    let mut newer_format = false;
    let mut report = Report::new(problems, path, "project");
    match text(attr, "formatVersion").map(|v| v.parse::<u32>()) {
        Ok(Ok(version)) if version > current => {
            newer_format = true;
            report.warn(format!(
                "the file was written by a newer ArchiCraft version (format {version}); data this version does not know may be lost on saving"
            ));
        }
        Ok(Ok(_)) => {}
        _ => report.warn("the ArchiCraft format version is missing or invalid"),
    }
    let settings_map = report.or(object(attr, "settings").cloned(), Object::new());
    let location_map = report.or(object(attr, "location").cloned(), Object::new());
    let info_map = report.or(object(attr, "info").cloned().map(Some), None);
    // A missing ground storey is reported when the storeys are known.
    let ground = attr.get("groundStorey").and_then(|_| match uuid(attr, "groundStorey") {
        Ok(id) => Some(StoreyId::from_uuid(id)),
        Err(e) => {
            report.repaired(format!(
                "{e}; the lowest storey at or above project zero is the ground storey"
            ));
            None
        }
    });
    let material_records = report.or(array(attr, "materials").cloned(), Vec::new());
    let layer_records = report.or(array(attr, "layers").cloned(), Vec::new());
    let view_records = report.or(array(attr, "views").cloned(), Vec::new());
    let counters = object(attr, "counters").cloned();

    let settings = read_settings(&settings_map, &mut Report::new(problems, path, "project settings"));
    let mut report = Report::new(problems, path, "project location");
    let location = Location {
        altitude: report.or(real(&location_map, "altitude"), 0.0),
        survey_point: report.or(
            field(&location_map, "surveyPoint")
                .and_then(point)
                .map(|p| [p.x, p.y])
                .map_err(|e| format!("“surveyPoint”: {e}")),
            [0.0, 0.0],
        ),
        true_north: report.or(real(&location_map, "trueNorth"), 0.0),
    };
    // A missing info object is reported once above, not field by field.
    let info = info_map.map_or_else(ProjectInfo::default, |map| {
        let mut report = Report::new(problems, path, "project information");
        ProjectInfo {
            number: report.or(text(&map, "number"), String::new()),
            client: report.or(text(&map, "client"), String::new()),
            address: report.or(text(&map, "address"), String::new()),
            author: report.or(text(&map, "author"), String::new()),
            ..ProjectInfo::default()
        }
    });
    let tool_defaults = match object(attr, "toolDefaults") {
        Ok(map) => Some(map.clone()),
        Err(e) => {
            Report::new(problems, path, "tool defaults").repaired(format!("{e}; the template defaults are used"));
            None
        }
    };
    let materials = material_records
        .iter()
        .enumerate()
        .filter_map(|(i, m)| read_material(i, m, path, problems))
        .collect();
    let layers = layer_records
        .iter()
        .enumerate()
        .filter_map(|(i, l)| read_layer(i, l, path, problems))
        .collect();
    let views = view_records
        .iter()
        .enumerate()
        .filter_map(|(i, v)| read_view(i, v, path, problems))
        .collect();
    ProjectData {
        settings,
        location,
        info,
        ground,
        layers,
        materials,
        tool_defaults,
        views,
        counters,
        newer_format,
    }
}

/// The name counters (ELM-019), keyed by kind code. An invalid, negative or
/// missing counter is repaired to the number of elements of its kind, so that
/// automatic names do not repeat; a counter above the largest value is
/// clamped to it. A kind without a tool has no counter until it has named
/// elements. Counters of kinds this build does not know are ignored: their
/// elements are skipped and reported on their own.
fn read_counters(
    map: Result<&Object, String>,
    project: &Project,
    path: &str,
    problems: &mut Vec<Problem>,
) -> NameCounters {
    let count = |tag: KindTag| {
        let n = project.elements.values().filter(|e| e.tag() == tag).count();
        u32::try_from(n).unwrap_or(u32::MAX)
    };
    let mut report = Report::new(problems, path, "counters");
    let mut counters = NameCounters::default();
    let map = match map {
        Ok(map) => map,
        Err(e) => {
            let counts: Vec<String> = KindTag::ALL
                .iter()
                .map(|&kind| format!("{}: {}", kind.label().to_lowercase(), count(kind)))
                .collect();
            report.repaired(format!(
                "{e}; the number of elements of each kind ({}) is used",
                counts.join(", ")
            ));
            for &kind in KindTag::ALL {
                counters.set(kind, count(kind));
            }
            return counters;
        }
    };
    for &kind in KindTag::ALL {
        let (key, existing) = (kind.code(), count(kind));
        let value = match integer(map, key) {
            Ok(v) if v < 0 => {
                report.repaired(format!("“{key}” is negative; {existing} is used"));
                existing
            }
            Ok(v) => u32::try_from(v).unwrap_or_else(|_| {
                report.repaired(format!("“{key}” is out of range; {} is used", u32::MAX));
                u32::MAX
            }),
            Err(_) if !map.contains_key(key) && !kind.descriptor().has_tool && existing == 0 => 0,
            Err(e) => {
                report.repaired(format!("{e}; {existing} is used"));
                existing
            }
        };
        counters.set(kind, value);
    }
    counters
}

/// The template's tool defaults, with its material and layer found by name
/// among `materials` and `layers`, else the first of each.
fn template_tool_defaults(materials: &[Material], layers: &[Layer]) -> ToolDefaults {
    let template = default_metric_project();
    let material = |id: MaterialId| {
        let name = template.material(id).map(|m| m.name.as_str());
        materials
            .iter()
            .find(|m| Some(m.name.as_str()) == name)
            .unwrap_or(&materials[0])
            .id
    };
    let layer = |id: LayerId| {
        let name = template.layer(id).map(|l| l.name.as_str());
        layers
            .iter()
            .find(|l| Some(l.name.as_str()) == name)
            .unwrap_or(&layers[0])
            .id
    };
    let mut defaults = template.tool_defaults.clone();
    defaults.wall.material = material(defaults.wall.material);
    defaults.wall.layer = layer(defaults.wall.layer);
    defaults.slab.material = material(defaults.slab.material);
    defaults.slab.layer = layer(defaults.slab.layer);
    defaults
}

/// The tool defaults of the project at `path`. Missing tool defaults were
/// reported with the project data; the template's are used then, and they
/// are the default of every invalid field.
fn read_tool_defaults(
    map: Option<&Object>,
    materials: &[Material],
    layers: &[Layer],
    path: &str,
    problems: &mut Vec<Problem>,
) -> ToolDefaults {
    let template = template_tool_defaults(materials, layers);
    let Some(map) = map else {
        return template;
    };
    let mut report = Report::new(problems, path, "tool defaults");
    let wall = report.or(object(map, "wall").cloned(), Object::new());
    let slab = report.or(object(map, "slab").cloned(), Object::new());
    let material_ref = |m: &Object, default: MaterialId, report: &mut Report<'_>| {
        let id = report.or(uuid(m, "material").map(MaterialId::from_uuid), default);
        if materials.iter().any(|m| m.id == id) {
            id
        } else {
            report.repaired("a tool's default material does not exist; the template default is used");
            default
        }
    };
    let layer_ref = |m: &Object, default: LayerId, report: &mut Report<'_>| {
        let id = report.or(uuid(m, "layer").map(LayerId::from_uuid), default);
        if layers.iter().any(|l| l.id == id) {
            id
        } else {
            report.repaired("a tool's default layer does not exist; the template default is used");
            default
        }
    };
    let (w, s) = (&template.wall, &template.slab);
    let wall_defaults = WallDefaults {
        thickness: report.or(real(&wall, "thickness"), w.thickness),
        reference_line: report.or(reference_line(&wall), w.reference_line),
        base_offset: report.or(real(&wall, "baseOffset"), w.base_offset),
        top: report.or(object(&wall, "top").and_then(top_link), w.top),
        material: material_ref(&wall, w.material, &mut report),
        layer: layer_ref(&wall, w.layer, &mut report),
        position: report.or(code(&wall, "position", Position::from_code), w.position),
        ifc_class: report.or(code(&wall, "ifcClass", IfcClass::from_code), w.ifc_class),
    };
    let slab_defaults = SlabDefaults {
        thickness: report.or(real(&slab, "thickness"), s.thickness),
        reference: report.or(code(&slab, "reference", SlabReference::from_code), s.reference),
        offset: report.or(real(&slab, "offset"), s.offset),
        material: material_ref(&slab, s.material, &mut report),
        layer: layer_ref(&slab, s.layer, &mut report),
        position: report.or(code(&slab, "position", Position::from_code), s.position),
        ifc_class: report.or(code(&slab, "ifcClass", IfcClass::from_code), s.ifc_class),
    };
    ToolDefaults {
        wall: wall_defaults,
        slab: slab_defaults,
    }
}

fn read_storeys(
    nodes: &IndexMap<String, FlatNode>,
    building: &FlatNode,
    problems: &mut Vec<Problem>,
) -> Vec<(Storey, Vec<String>)> {
    let mut storeys = Vec::new();
    for (path, node) in children_with_class(nodes, building, "IfcBuildingStorey") {
        let mut report = Report::new(
            problems,
            path,
            format!("storey {}", name(node).as_deref().unwrap_or(path)),
        );
        let id = StoreyId::from_uuid(node_uuid(path, &mut report));
        let name = name(node).unwrap_or_else(|| {
            report.repaired("it has no name; “Storey” is used");
            "Storey".into()
        });
        let attr = node.attributes.get(schema::STOREY).and_then(Value::as_object);
        let Some(attr) = attr else {
            report.repaired("it has no ArchiCraft data; elevation 0 and height 3 m are assumed");
            let storey = Storey {
                id,
                name,
                elevation: 0.0,
                height: 3.0,
                cut_plane_height: None,
            };
            storeys.push((storey, node.children.values().flatten().cloned().collect()));
            continue;
        };
        let mut height = report.or(real(attr, "height"), 3.0);
        if height < MIN_STOREY_HEIGHT {
            report.repaired(format!("“height” is below the minimum; {MIN_STOREY_HEIGHT} m is used"));
            height = MIN_STOREY_HEIGHT;
        }
        let storey = Storey {
            id,
            name,
            elevation: report.or(real(attr, "elevation"), 0.0),
            height,
            cut_plane_height: report.optional(attr, "cutPlaneHeight", real),
        };
        storeys.push((storey, node.children.values().flatten().cloned().collect()));
    }
    storeys
}

/// Builds a storey list, repairing coincident elevations if necessary.
fn storey_list(mut storeys: Vec<Storey>, ground: Option<StoreyId>, problems: &mut Vec<Problem>) -> StoreyList {
    if storeys.is_empty() {
        problems.push(Problem::warning(
            None,
            "the building has no storeys; a ground floor was added",
        ));
        storeys.push(Storey::new("00 Ground floor", 0.0, 3.0));
    }
    storeys.sort_by(|a, b| a.elevation.total_cmp(&b.elevation));
    let ground = match ground.filter(|g| storeys.iter().any(|s| s.id == *g)) {
        Some(g) => g,
        None => {
            // At or above project zero, within the model tolerance (NFR-ACC-003).
            let g = storeys
                .iter()
                .find(|s| s.elevation > -MODEL_TOLERANCE)
                .unwrap_or(&storeys[storeys.len() - 1])
                .id;
            problems.push(Problem::warning(
                None,
                "the ground storey is not defined; the lowest storey at or above project zero is used",
            ));
            g
        }
    };
    for i in 1..storeys.len() {
        if storeys[i].elevation - storeys[i - 1].elevation < MIN_STOREY_HEIGHT {
            storeys[i].elevation = storeys[i - 1].elevation + storeys[i - 1].height.max(MIN_STOREY_HEIGHT);
            problems.push(Problem::warning(
                Some(&storeys[i].id.to_string()),
                format!(
                    "storey “{}” coincided with the one below and was moved up",
                    storeys[i].name
                ),
            ));
        }
    }
    match StoreyList::new(storeys.clone(), ground) {
        Ok(list) => list,
        Err(e) => {
            problems.push(Problem::error(
                None,
                format!("the storeys could not be read ({e}); one storey is used"),
            ));
            let storey = Storey::new("00 Ground floor", 0.0, 3.0);
            let id = storey.id;
            match StoreyList::new(vec![storey], id) {
                Ok(list) => list,
                Err(e) => unreachable!("a single default storey is valid: {e}"),
            }
        }
    }
}

/// Names an element node in reports, by its name if it has one.
fn element_record(path: &str, node: &FlatNode) -> String {
    format!("element {}", name(node).as_deref().unwrap_or(path))
}

fn read_element(
    project: &Project,
    path: &str,
    node: &FlatNode,
    storey: StoreyId,
    problems: &mut Vec<Problem>,
) -> Option<Element> {
    let mut report = Report::new(problems, path, element_record(path, node));
    let Some(common) = node.attributes.get(schema::ELEMENT).and_then(Value::as_object) else {
        report.skipped("its ArchiCraft data is not an object");
        return None;
    };
    let kind = match code(common, "kind", KindTag::from_code) {
        Ok(kind) => kind,
        Err(e) => {
            report.skipped(e);
            return None;
        }
    };
    let id = ElementId::from_uuid(node_uuid(path, &mut report));
    let name = name(node).unwrap_or_else(|| {
        let name = format!("{}-?", kind.name_prefix());
        report.repaired(format!("it has no name; “{name}” is used"));
        name
    });
    let mut layer = report.or(uuid(common, "layer").map(LayerId::from_uuid), project.layers[0].id);
    if project.layer(layer).is_none() {
        report.repaired("the layer does not exist; the first layer is used");
        layer = project.layers[0].id;
    }
    // ELM-011: a class the kind does not allow falls back to its default.
    let default_class = kind.descriptor().default_class;
    let ifc_class = match class_code(node).and_then(IfcClass::from_code) {
        Some(c) if kind.ifc_classes().contains(&c) => c,
        _ => {
            report.repaired(format!("unsupported IFC class; {} is used", default_class.code()));
            default_class
        }
    };
    let material_of = |attr: &Object, report: &mut Report<'_>| {
        let id = report.or(
            uuid(attr, "material").map(MaterialId::from_uuid),
            project.materials[0].id,
        );
        if project.material(id).is_some() {
            id
        } else {
            report.repaired(format!(
                "the material does not exist; “{}” is used",
                project.materials[0].name
            ));
            project.materials[0].id
        }
    };
    let element_kind = match kind {
        KindTag::Wall => {
            let Some(attr) = node.attributes.get(schema::WALL).and_then(Value::as_object) else {
                report.skipped("wall parameters are missing");
                return None;
            };
            let start = field(attr, "start").and_then(point);
            let end = field(attr, "end").and_then(point);
            let (start, end) = match (start, end) {
                (Ok(s), Ok(e)) => (s, e),
                (Err(e), _) | (_, Err(e)) => {
                    report.skipped(e);
                    return None;
                }
            };
            ElementKind::Wall(Wall {
                start,
                end,
                thickness: report.or(real(attr, "thickness"), 0.25),
                reference_line: report.or(reference_line(attr), ReferenceLine::Outside),
                base_offset: report.or(real(attr, "baseOffset"), 0.0),
                top: report.or(
                    object(attr, "top").and_then(top_link),
                    TopLink::Relative {
                        storeys: 1,
                        offset: 0.0,
                    },
                ),
                material: material_of(attr, &mut report),
            })
        }
        KindTag::Slab => {
            let Some(attr) = node.attributes.get(schema::SLAB).and_then(Value::as_object) else {
                report.skipped("slab parameters are missing");
                return None;
            };
            let outline = match field(attr, "outline").and_then(points) {
                Ok(o) => o,
                Err(e) => {
                    report.skipped(e);
                    return None;
                }
            };
            let holes = match array(attr, "holes").and_then(|h| h.iter().map(points).collect()) {
                Ok(h) => h,
                Err(e) => {
                    report.repaired(format!("the holes are left out: {e}"));
                    Vec::new()
                }
            };
            ElementKind::Slab(Slab {
                outline,
                holes,
                thickness: report.or(real(attr, "thickness"), 0.2),
                reference: report.or(code(attr, "reference", SlabReference::from_code), SlabReference::Top),
                offset: report.or(real(attr, "offset"), 0.0),
                material: material_of(attr, &mut report),
            })
        }
    };
    let element = Element {
        id,
        name,
        storey,
        layer,
        ifc_class,
        position: report.or(code(common, "position", Position::from_code), Position::default()),
        locked: report.or(boolean(common, "locked"), false),
        kind: element_kind,
    };
    match validate_element(project, &element) {
        Ok(valid) => Some(valid),
        Err(e) => {
            report.skipped(e);
            None
        }
    }
}

/// Reads a document written by ArchiCraft, after migrating it from an
/// earlier ArchiCraft format (IFX-062, IFX-091). Fails only if the file holds
/// no ArchiCraft project at all.
pub fn read(file: &IfcxFile) -> Result<ReadOutcome, Problem> {
    read_with(file, migrate::MIGRATIONS, FORMAT_VERSION)
}

/// [`read`] with another migration chain up to format `current`, so that
/// tests can run synthetic migrations.
pub(crate) fn read_with(file: &IfcxFile, migrations: &[Migration], current: u32) -> Result<ReadOutcome, Problem> {
    let mut problems = Vec::new();
    let mut file = Cow::Borrowed(file);
    migrate::upgrade(&mut file, migrations, current, &mut problems);
    problems.extend(validate::validate(&file));
    let nodes = validate::flatten(&file);

    let Some((root_path, root)) = nodes.iter().find(|(_, n)| n.attributes.contains_key(schema::PROJECT)) else {
        return Err(Problem::error(
            None,
            "this IFCX file holds no ArchiCraft project; opening IFCX files from other tools is not supported yet",
        ));
    };
    let Some(project_attr) = root.attributes.get(schema::PROJECT).and_then(Value::as_object) else {
        return Err(Problem::error(
            Some(root_path),
            "the ArchiCraft project data is not an object",
        ));
    };
    let mut data = read_project_data(project_attr, root_path, current, &mut problems);
    let mut root_report = Report::new(&mut problems, root_path, "project");
    let project_id = ProjectId::from_uuid(node_uuid(root_path, &mut root_report));
    data.info.name = name(root).unwrap_or_else(|| "Untitled project".into());
    data.info.description = root
        .attributes
        .get("bsi::ifc::prop::Description")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();

    let site = children_with_class(&nodes, root, "IfcSite").into_iter().next();
    if site.is_none() {
        root_report.warn("the project has no site; a site was added");
    }
    let building = site.and_then(|(_, s)| children_with_class(&nodes, s, "IfcBuilding").into_iter().next());
    if building.is_none() {
        root_report.warn("the project has no building; a building was added");
    }
    if data.views.is_empty() {
        root_report.warn("the project has no views; a default view was added");
        data.views.push(View3d {
            id: ViewId::new(),
            name: "3D perspective".into(),
            projection: Projection::Perspective,
        });
    }

    if data.materials.is_empty() {
        root_report.warn("the project has no materials; a default material was added");
        data.materials.push(Material {
            id: MaterialId::new(),
            name: "Default".into(),
            code: String::new(),
            description: String::new(),
            category: MaterialCategory::Other,
            color: Rgb([0.7, 0.7, 0.7]),
            opacity: 1.0,
            priority: 500,
            cut_fill: CutFill {
                pattern: FillPattern::Solid,
                foreground: Rgb([0.6, 0.6, 0.6]),
                background: None,
            },
        });
    }
    if data.layers.is_empty() {
        root_report.warn("the project has no layers; a default layer was added");
        data.layers.push(Layer::new("Default"));
    }
    let site_id = site.map_or_else(SiteId::new, |(p, _)| {
        SiteId::from_uuid(node_uuid(p, &mut Report::new(&mut problems, p, "site")))
    });
    let building_id = building.map_or_else(BuildingId::new, |(p, _)| {
        BuildingId::from_uuid(node_uuid(p, &mut Report::new(&mut problems, p, "building")))
    });
    let tool_defaults = read_tool_defaults(
        data.tool_defaults.as_ref(),
        &data.materials,
        &data.layers,
        root_path,
        &mut problems,
    );

    let storey_entries = building
        .map(|(_, b)| read_storeys(&nodes, b, &mut problems))
        .unwrap_or_default();
    let storeys = storey_list(
        storey_entries.iter().map(|(s, _)| s.clone()).collect(),
        data.ground,
        &mut problems,
    );

    let mut project = Project {
        id: project_id,
        info: data.info,
        settings: data.settings,
        site: Site {
            id: site_id,
            name: site.and_then(|(_, s)| name(s)).unwrap_or_else(|| "Site".into()),
            location: data.location,
        },
        building: Building {
            id: building_id,
            name: building.and_then(|(_, b)| name(b)).unwrap_or_else(|| "Building".into()),
        },
        storeys,
        layers: data.layers,
        materials: data.materials,
        elements: IndexMap::new(),
        tool_defaults,
        views: data.views,
        counters: NameCounters::default(),
        data_version: 0,
        revision: 0,
    };
    project.data_version = file.header.data_version.parse().unwrap_or_else(|_| {
        problems.push(Problem::warning(None, "the data version in the header is not a number"));
        0
    });

    for (storey, children) in &storey_entries {
        let storey_lost = project.storeys.get(storey.id).is_none();
        for child in children {
            // A missing child is reported by `validate`.
            let Some(node) = nodes.get(child.as_str()) else {
                continue;
            };
            if storey_lost {
                if node.attributes.contains_key(schema::ELEMENT) {
                    Report::new(&mut problems, child, element_record(child, node))
                        .skipped(format!("its storey “{}” could not be read", storey.name));
                }
                continue;
            }
            if !node.attributes.contains_key(schema::ELEMENT) {
                problems.push(Problem::warning(
                    Some(child),
                    format!(
                        "“{}” has no ArchiCraft data and was not loaded; importing elements from other tools is not supported yet",
                        name(node).unwrap_or_else(|| child.clone())
                    ),
                ));
                continue;
            }
            if let Some(element) = read_element(&project, child, node, storey.id, &mut problems) {
                if project.elements.contains_key(&element.id) {
                    problems.push(Problem::error(
                        Some(child),
                        format!("{} appears twice; the second occurrence was skipped", element.name),
                    ));
                    continue;
                }
                project.elements.insert(element.id, element);
            }
        }
    }
    project.counters = read_counters(
        data.counters.as_ref().map_err(Clone::clone),
        &project,
        root_path,
        &mut problems,
    );
    Ok(ReadOutcome {
        project,
        problems,
        newer_format: data.newer_format,
        source_version: IFCX_VERSION,
    })
}
