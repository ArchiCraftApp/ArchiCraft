//! Reads an `ifcx_alpha` document written by ArchiCraft back into an editable
//! project (IFX-064, IFX-080).
//!
//! Reading is lenient (IFX-083): every part that can be read is loaded, and
//! every skipped, repaired or placeholder object is reported with the reason
//! (PRJ-008, NFR-REL-004). Nothing is dropped silently.

use archicraft_model::glam::DVec2;
use archicraft_model::material::{CutFill, FillPattern, Material, MaterialCategory, Rgb};
use archicraft_model::project::{
    Building, Location, NameCounters, Projection, Site, SlabDefaults, Tessellation, ToolDefaults, View3d, WallDefaults,
};
use archicraft_model::storey::MIN_STOREY_HEIGHT;
use archicraft_model::units::{AngleUnit, AreaUnit, LengthUnit, SlopeFormat, UnitSettings, VolumeUnit};
use archicraft_model::validate::validate_element;
use archicraft_model::{
    BuildingId, Element, ElementId, ElementKind, IfcClass, KindTag, Layer, LayerId, MaterialId, Position, Project,
    ProjectId, ProjectInfo, ProjectSettings, ReferenceLine, SiteId, Slab, SlabReference, Storey, StoreyId, StoreyList,
    TopLink, ViewId, Wall,
};
use indexmap::IndexMap;
use serde_json::{Map, Value};
use uuid::Uuid;

use super::schema::{self, FORMAT_VERSION};
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
}

type Object = Map<String, Value>;

/// Collects problems for one node.
struct Report<'a> {
    problems: &'a mut Vec<Problem>,
    path: String,
}

impl Report<'_> {
    fn warn(&mut self, message: impl Into<String>) {
        self.problems.push(Problem::warning(Some(&self.path), message));
    }

    fn error(&mut self, message: impl Into<String>) {
        self.problems.push(Problem::error(Some(&self.path), message));
    }

    /// Unwraps a field, or reports it and uses `default`.
    fn or<T>(&mut self, result: Result<T, String>, default: T) -> T {
        match result {
            Ok(v) => v,
            Err(e) => {
                self.warn(format!("{e}; a default value is used"));
                default
            }
        }
    }
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
    let items = array(map, key)?;
    let values: Vec<f64> = items.iter().filter_map(Value::as_f64).collect();
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
        report.warn("the path is not a UUID; the object gets a new identity");
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

fn read_material(record: &Value, problems: &mut Vec<Problem>) -> Option<Material> {
    let Some(map) = record.as_object() else {
        problems.push(Problem::warning(
            None,
            "a material record is not an object and was skipped",
        ));
        return None;
    };
    let mut report = Report {
        problems,
        path: map.get("id").and_then(Value::as_str).unwrap_or("material").to_owned(),
    };
    let id = match uuid(map, "id") {
        Ok(id) => MaterialId::from_uuid(id),
        Err(e) => {
            report.error(format!("material skipped: {e}"));
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
            background: rgb(&fill, "background").ok(),
        },
    };
    if let Err(e) = material.validate() {
        report.warn(format!("material “{}” repaired: {e}", material.name));
        material.opacity = material.opacity.clamp(0.0, 1.0);
        material.priority = material.priority.clamp(Material::MIN_PRIORITY, Material::MAX_PRIORITY);
        if material.name.trim().is_empty() {
            material.name = "Material".into();
        }
    }
    Some(material)
}

fn read_layer(record: &Value, problems: &mut Vec<Problem>) -> Option<Layer> {
    let map = record.as_object()?;
    let mut report = Report {
        problems,
        path: map.get("id").and_then(Value::as_str).unwrap_or("layer").to_owned(),
    };
    let id = match uuid(map, "id") {
        Ok(id) => LayerId::from_uuid(id),
        Err(e) => {
            report.error(format!("layer skipped: {e}"));
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
    counters: NameCounters,
    newer_format: bool,
}

fn read_project_data(attr: &Object, path: &str, problems: &mut Vec<Problem>) -> ProjectData {
    let mut newer_format = false;
    {
        let mut report = Report {
            problems,
            path: path.to_owned(),
        };
        match text(attr, "formatVersion").map(|v| v.parse::<u32>()) {
            Ok(Ok(version)) if version > FORMAT_VERSION => {
                newer_format = true;
                report.warn(format!(
                    "the file was written by a newer ArchiCraft version (format {version}); data this version does not know may be lost on saving"
                ));
            }
            Ok(Ok(_)) => {}
            _ => report.warn("the ArchiCraft format version is missing or invalid"),
        }
    }

    let mut report = Report {
        problems,
        path: path.to_owned(),
    };
    let defaults = ProjectSettings::default();
    let settings_map = report.or(object(attr, "settings").cloned(), Object::new());
    let units = UnitSettings {
        length: report.or(
            code(&settings_map, "lengthUnit", LengthUnit::from_code),
            LengthUnit::Meter,
        ),
        length_decimals: report.or(
            integer(&settings_map, "lengthDecimals").map(|v| v.clamp(0, 8) as u8),
            defaults.units.length_decimals,
        ),
        angle: report.or(
            code(&settings_map, "angleUnit", AngleUnit::from_code),
            AngleUnit::DecimalDegrees,
        ),
        angle_decimals: report.or(
            integer(&settings_map, "angleDecimals").map(|v| v.clamp(0, 8) as u8),
            defaults.units.angle_decimals,
        ),
        slope: report.or(
            code(&settings_map, "slopeFormat", SlopeFormat::from_code),
            SlopeFormat::Degrees,
        ),
        area: report.or(
            code(&settings_map, "areaUnit", AreaUnit::from_code),
            AreaUnit::SquareMeter,
        ),
        volume: report.or(
            code(&settings_map, "volumeUnit", VolumeUnit::from_code),
            VolumeUnit::CubicMeter,
        ),
    };
    let settings = ProjectSettings {
        units,
        cut_plane_height: report.or(real(&settings_map, "cutPlaneHeight"), defaults.cut_plane_height),
        tessellation: Tessellation {
            chord_deviation: report.or(
                real(&settings_map, "chordDeviation"),
                defaults.tessellation.chord_deviation,
            ),
            max_angle_deg: report.or(
                real(&settings_map, "maxSegmentAngle"),
                defaults.tessellation.max_angle_deg,
            ),
        },
    };
    let location_map = report.or(object(attr, "location").cloned(), Object::new());
    let survey = location_map.get("surveyPoint").map(point);
    let location = Location {
        altitude: report.or(real(&location_map, "altitude"), 0.0),
        survey_point: match survey {
            Some(Ok(p)) => [p.x, p.y],
            _ => [0.0, 0.0],
        },
        true_north: report.or(real(&location_map, "trueNorth"), 0.0),
    };
    let info_map = report.or(object(attr, "info").cloned(), Object::new());
    let info = ProjectInfo {
        name: String::new(),
        description: String::new(),
        number: text(&info_map, "number").unwrap_or_default(),
        client: text(&info_map, "client").unwrap_or_default(),
        address: text(&info_map, "address").unwrap_or_default(),
        author: text(&info_map, "author").unwrap_or_default(),
    };
    let ground = uuid(attr, "groundStorey").ok().map(StoreyId::from_uuid);
    let tool_defaults = object(attr, "toolDefaults").ok().cloned();
    let views: Vec<View3d> = array(attr, "views")
        .map(|items| {
            items
                .iter()
                .filter_map(|v| {
                    let m = v.as_object()?;
                    Some(View3d {
                        id: ViewId::from_uuid(uuid(m, "id").ok()?),
                        name: text(m, "name").ok()?,
                        projection: code(m, "projection", Projection::from_code).ok()?,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let counters_map = object(attr, "counters").cloned().unwrap_or_default();
    let counters = NameCounters {
        wall: integer(&counters_map, "wall").map_or(0, |v| v.clamp(0, i64::from(u32::MAX)) as u32),
        slab: integer(&counters_map, "slab").map_or(0, |v| v.clamp(0, i64::from(u32::MAX)) as u32),
    };
    let material_records = report.or(array(attr, "materials").cloned(), Vec::new());
    let layer_records = report.or(array(attr, "layers").cloned(), Vec::new());
    drop(report);
    let materials = material_records
        .iter()
        .filter_map(|m| read_material(m, problems))
        .collect();
    let layers = layer_records.iter().filter_map(|l| read_layer(l, problems)).collect();
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

fn read_tool_defaults(
    map: Option<&Object>,
    materials: &[Material],
    layers: &[Layer],
    problems: &mut Vec<Problem>,
) -> ToolDefaults {
    let first_material = materials[0].id;
    let first_layer = layers[0].id;
    let mut report = Report {
        problems,
        path: "toolDefaults".to_owned(),
    };
    let empty = Object::new();
    let map = map.unwrap_or(&empty);
    let wall = report.or(object(map, "wall").cloned(), Object::new());
    let slab = report.or(object(map, "slab").cloned(), Object::new());
    let material_ref = |m: &Object, report: &mut Report<'_>| {
        let id = report.or(uuid(m, "material").map(MaterialId::from_uuid), first_material);
        if materials.iter().any(|m| m.id == id) {
            id
        } else {
            report.warn("a tool's default material does not exist; the first material is used");
            first_material
        }
    };
    let layer_ref = |m: &Object, report: &mut Report<'_>| {
        let id = report.or(uuid(m, "layer").map(LayerId::from_uuid), first_layer);
        if layers.iter().any(|l| l.id == id) {
            id
        } else {
            report.warn("a tool's default layer does not exist; the first layer is used");
            first_layer
        }
    };
    let wall_defaults = WallDefaults {
        thickness: report.or(real(&wall, "thickness"), 0.25),
        reference_line: report.or(reference_line(&wall), ReferenceLine::Outside),
        base_offset: report.or(real(&wall, "baseOffset"), 0.0),
        top: report.or(
            object(&wall, "top").and_then(top_link),
            TopLink::Relative {
                storeys: 1,
                offset: 0.0,
            },
        ),
        material: material_ref(&wall, &mut report),
        layer: layer_ref(&wall, &mut report),
        position: report.or(code(&wall, "position", Position::from_code), Position::Exterior),
        ifc_class: report.or(code(&wall, "ifcClass", IfcClass::from_code), IfcClass::Wall),
    };
    let slab_defaults = SlabDefaults {
        thickness: report.or(real(&slab, "thickness"), 0.2),
        reference: report.or(code(&slab, "reference", SlabReference::from_code), SlabReference::Top),
        offset: report.or(real(&slab, "offset"), 0.0),
        material: material_ref(&slab, &mut report),
        layer: layer_ref(&slab, &mut report),
        position: report.or(code(&slab, "position", Position::from_code), Position::Undefined),
        ifc_class: report.or(code(&slab, "ifcClass", IfcClass::from_code), IfcClass::Slab),
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
        let mut report = Report {
            problems,
            path: path.clone(),
        };
        let id = StoreyId::from_uuid(node_uuid(path, &mut report));
        let attr = node.attributes.get(schema::STOREY).and_then(Value::as_object);
        let Some(attr) = attr else {
            report.warn("storey without ArchiCraft data; elevation 0 and height 3 m are assumed");
            let storey = Storey {
                id,
                name: name(node).unwrap_or_else(|| "Storey".into()),
                elevation: 0.0,
                height: 3.0,
                cut_plane_height: None,
            };
            storeys.push((storey, node.children.values().flatten().cloned().collect()));
            continue;
        };
        let storey = Storey {
            id,
            name: name(node).unwrap_or_else(|| {
                report.warn("storey without a name");
                "Storey".into()
            }),
            elevation: report.or(real(attr, "elevation"), 0.0),
            height: report.or(real(attr, "height"), 3.0).max(MIN_STOREY_HEIGHT),
            cut_plane_height: real(attr, "cutPlaneHeight").ok(),
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
            let g = storeys
                .iter()
                .find(|s| s.elevation >= -1e-9)
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

fn read_element(
    project: &Project,
    path: &str,
    node: &FlatNode,
    storey: StoreyId,
    problems: &mut Vec<Problem>,
) -> Option<Element> {
    let mut report = Report {
        problems,
        path: path.to_owned(),
    };
    let common = node.attributes.get(schema::ELEMENT).and_then(Value::as_object)?;
    let kind = match code(common, "kind", |c| match c {
        "wall" => Some(KindTag::Wall),
        "slab" => Some(KindTag::Slab),
        _ => None,
    }) {
        Ok(kind) => kind,
        Err(e) => {
            report.error(format!("element skipped: {e}"));
            return None;
        }
    };
    let id = ElementId::from_uuid(node_uuid(path, &mut report));
    let name = name(node).unwrap_or_else(|| {
        report.warn("element without a name");
        format!("{}-?", kind.name_prefix())
    });
    let mut layer = report.or(uuid(common, "layer").map(LayerId::from_uuid), project.layers[0].id);
    if project.layer(layer).is_none() {
        report.warn(format!("{name}: the layer does not exist; the first layer is used"));
        layer = project.layers[0].id;
    }
    let default_class = match kind {
        KindTag::Wall => IfcClass::Wall,
        KindTag::Slab => IfcClass::Slab,
    };
    let ifc_class = match class_code(node).and_then(IfcClass::from_code) {
        Some(c) if kind.ifc_classes().contains(&c) => c,
        _ => {
            report.warn(format!(
                "{name}: unsupported IFC class; {} is used",
                default_class.code()
            ));
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
            report.warn(format!(
                "{name}: the material does not exist; “{}” is used",
                project.materials[0].name
            ));
            project.materials[0].id
        }
    };
    let element_kind = match kind {
        KindTag::Wall => {
            let Some(attr) = node.attributes.get(schema::WALL).and_then(Value::as_object) else {
                report.error(format!("{name} skipped: wall parameters are missing"));
                return None;
            };
            let start = field(attr, "start").and_then(point);
            let end = field(attr, "end").and_then(point);
            let (start, end) = match (start, end) {
                (Ok(s), Ok(e)) => (s, e),
                (Err(e), _) | (_, Err(e)) => {
                    report.error(format!("{name} skipped: {e}"));
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
                report.error(format!("{name} skipped: slab parameters are missing"));
                return None;
            };
            let outline = match field(attr, "outline").and_then(points) {
                Ok(o) => o,
                Err(e) => {
                    report.error(format!("{name} skipped: {e}"));
                    return None;
                }
            };
            let holes = match array(attr, "holes").and_then(|h| h.iter().map(points).collect()) {
                Ok(h) => h,
                Err(e) => {
                    report.warn(format!("{name}: holes dropped: {e}"));
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
        position: code(common, "position", Position::from_code).unwrap_or_default(),
        locked: boolean(common, "locked").unwrap_or(false),
        kind: element_kind,
    };
    match validate_element(project, &element) {
        Ok(valid) => Some(valid),
        Err(e) => {
            report.error(format!("{} skipped: {e}", element.name));
            None
        }
    }
}

/// Reads a document written by ArchiCraft. Fails only if the file holds no
/// ArchiCraft project at all.
pub fn read(file: &IfcxFile) -> Result<ReadOutcome, Problem> {
    let mut problems: Vec<Problem> = validate::validate(file);
    let nodes = validate::flatten(file);

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
    let mut data = read_project_data(project_attr, root_path, &mut problems);
    let mut root_report = Report {
        problems: &mut problems,
        path: root_path.clone(),
    };
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
    let site_id = site.map_or_else(SiteId::new, |(p, _)| {
        Uuid::parse_str(p).map_or_else(|_| SiteId::new(), SiteId::from_uuid)
    });
    let building_id = building.map_or_else(BuildingId::new, |(p, _)| {
        Uuid::parse_str(p).map_or_else(|_| BuildingId::new(), BuildingId::from_uuid)
    });

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
    drop(root_report);
    let tool_defaults = read_tool_defaults(
        data.tool_defaults.as_ref(),
        &data.materials,
        &data.layers,
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
        counters: data.counters,
        data_version: 0,
        revision: 0,
    };
    if project.views.is_empty() {
        project.views.push(View3d {
            id: ViewId::new(),
            name: "3D perspective".into(),
            projection: Projection::Perspective,
        });
    }
    project.data_version = file.header.data_version.parse().unwrap_or_else(|_| {
        problems.push(Problem::warning(None, "the data version in the header is not a number"));
        0
    });

    for (storey, children) in &storey_entries {
        if project.storeys.get(storey.id).is_none() {
            continue;
        }
        for child in children {
            let Some(node) = nodes.get(child.as_str()) else {
                continue;
            };
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
    Ok(ReadOutcome {
        project,
        problems,
        newer_format: data.newer_format,
    })
}
