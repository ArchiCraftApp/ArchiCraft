//! Reads an `ifcx_alpha` document written by ArchiCraft back into an editable
//! project (IFX-064, IFX-080).
//!
//! Reading is lenient (IFX-083): every part that can be read is loaded, and
//! every skipped, repaired or placeholder object is reported with the reason
//! (PRJ-008, NFR-REL-004). Nothing is dropped silently.
//!
//! Every report names its record and says what happened to it, as
//! “<record> skipped: <reason>” (`ifcx-skipped`, an error: the record was not
//! loaded) or “<record> repaired: <reason>” (`ifcx-repaired`, a warning: the
//! record was loaded with a default or corrected value). Records, reasons and
//! the model's own errors are [`Message`]s nested in these, so the
//! application renders the whole report in the user's language
//! (NFR-L10N-001). The report's path is the node holding the
//! record: the project node for project-level records (settings, materials,
//! layers, views, tool defaults, counters), else the storey or element node.
//!
//! An absent field is damage, like a present field of the wrong type or out
//! of range, because the writer writes every field. The exceptions are the
//! optional fields the writer leaves out when they are not set: a material's
//! cut-fill `background`, a storey's `cutPlaneHeight`, the settings'
//! `planGrid` (files from before INP-013), the `snapExcluded` flag of
//! elements and layers, written only when set (INP-017), and the project's
//! `groups`, a group's `parent`, an element's `group` and the `group`
//! counter, written only for projects with groups (ELM-015), an element
//! node's `archicraft::relations`, written only for an element with
//! explicit relations (SRS 3.3, [`super::relations`]), an element's `name`
//! in `archicraft::element`, written only when its node shows another name
//! (a space's room name, SPC-001), and a storey node's
//! `archicraft::annotations`, written only for a storey with elements of the
//! kinds that are not IFCX nodes (ANN-010, [`super::kinds::annotations`]),
//! whose entries leave out the same optional members as an element node.
//! Their absence is no problem.
//!
//! The element nodes are read first, storey by storey, then the nodes'
//! explicit relations, then the annotation entries, which may need both
//! (a dimension's binding), and last the relations that name an entry.
//!
//! An opened file holds its attribute values as raw JSON text
//! ([`crate::validate::parse_raw`]); the reader parses the attributes it
//! reads, element by element, and drops each tree once the element is read
//! (NFR-SEC-003); a storey's annotation entries are one attribute, read as
//! one tree. It never reads the `usd::` geometry, which is evaluated
//! again from the parameters (IFX-010).

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use archicraft_model::glam::DVec2;
use archicraft_model::material::{CutFill, FillPattern, Material, MaterialCategory, Rgb};
use archicraft_model::project::{
    Building, DefaultsSlot, Location, NameCounters, Projection, Site, Tessellation, ToolDefaults, View3d,
};
use archicraft_model::relations::RelationStore;
use archicraft_model::storey::MIN_STOREY_HEIGHT;
use archicraft_model::template::{TemplateContext, template_text_styles, template_tool_defaults};
use archicraft_model::tolerance::MODEL_TOLERANCE;
use archicraft_model::units::{AngleUnit, AreaUnit, LengthUnit, SlopeFormat, UnitSettings, VolumeUnit};
use archicraft_model::validate::{MAX_DECIMALS, valid_grid_origin, valid_grid_spacing, validate_element};
use archicraft_model::{
    BuildingId, Element, ElementId, Group, GroupId, IfcClass, KindTag, Layer, LayerId, MaterialId, PlanGrid, Position,
    PredefinedType, Project, ProjectId, ProjectInfo, ProjectSettings, SiteId, Storey, StoreyId, StoreyList, ViewId,
};
use indexmap::IndexMap;
use serde_json::value::RawValue;
use serde_json::{Map, Value};
use uuid::Uuid;

use super::kinds::{self, Codec, ReadContext, annotations};
use super::migrate::{self, Migration};
use super::relations;
use super::schema::{self, FORMAT_VERSION};
use super::writer::IFCX_VERSION;
use crate::container::{AttributeValue, IfcxFile};
use crate::message::Message;
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
    /// The ArchiCraft format the file was written in, when the reader
    /// migrated it from that earlier format (IFX-062); each step is also
    /// reported as an info problem. `None` when no migration step ran: for a
    /// file at the current format or a newer one ([`Self::newer_format`]),
    /// and for one whose format version is unreadable or has no migration
    /// (both reported as warnings).
    pub migrated_from: Option<u32>,
}

pub(crate) type Object = Map<String, Value>;

/// Collects problems for one record, reported at the node `path`.
pub(crate) struct Report<'a> {
    problems: &'a mut Vec<Problem>,
    path: String,
    /// Names the record in every message, for example “layer record 1”
    /// (`ifcx-record-layer`) or “element W-001” (`ifcx-record-element`).
    record: Message,
}

impl<'a> Report<'a> {
    pub(crate) fn new(problems: &'a mut Vec<Problem>, path: &str, record: Message) -> Self {
        Self {
            problems,
            path: path.to_owned(),
            record,
        }
    }

    fn warn(&mut self, message: Message) {
        self.problems.push(Problem::warning(Some(&self.path), message));
    }

    /// The record was not loaded (PRJ-008).
    pub(crate) fn skipped(&mut self, reason: Message) {
        let message = Message::new("ifcx-skipped")
            .nested("record", self.record.clone())
            .nested("reason", reason);
        self.problems.push(Problem::error(Some(&self.path), message));
    }

    /// The record was loaded with a default or corrected value (PRJ-008).
    pub(crate) fn repaired(&mut self, reason: Message) {
        let message = Message::new("ifcx-repaired")
            .nested("record", self.record.clone())
            .nested("reason", reason);
        self.warn(message);
    }

    /// Unwraps a field, or reports the record as repaired and uses `default`.
    pub(crate) fn or<T>(&mut self, result: Result<T, Message>, default: T) -> T {
        match result {
            Ok(v) => v,
            Err(e) => {
                self.repaired(Message::new("ifcx-default-used").nested("reason", e));
                default
            }
        }
    }

    /// An optional field the writer may leave out: absent is `None`, present
    /// but invalid is reported and left out.
    fn optional<T>(
        &mut self,
        map: &Object,
        key: &str,
        read: impl Fn(&Object, &str) -> Result<T, Message>,
    ) -> Option<T> {
        if !map.contains_key(key) {
            return None;
        }
        match read(map, key) {
            Ok(v) => Some(v),
            Err(e) => {
                self.repaired(Message::new("ifcx-left-out").nested("reason", e));
                None
            }
        }
    }
}

/// Names the record at `index` of a project-level list, like “layer record
/// 1” for `ifcx-record-layer`.
fn record(key: &'static str, index: usize) -> Message {
    Message::new(key).count("index", index)
}

/// A reason about the field `key` of a record, such as
/// `ifcx-field-missing`.
pub(crate) fn about(reason: &'static str, key: &str) -> Message {
    Message::new(reason).text("key", key)
}

pub(crate) fn field<'v>(map: &'v Object, key: &str) -> Result<&'v Value, Message> {
    map.get(key).ok_or_else(|| about("ifcx-field-missing", key))
}

pub(crate) fn real(map: &Object, key: &str) -> Result<f64, Message> {
    field(map, key)?
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or_else(|| about("ifcx-field-not-number", key))
}

pub(crate) fn integer(map: &Object, key: &str) -> Result<i64, Message> {
    let v = field(map, key)?;
    v.as_i64()
        .or_else(|| {
            v.as_f64()
                .filter(|f| f.fract() == 0.0 && f.abs() < 9e15)
                .map(|f| f as i64)
        })
        .ok_or_else(|| about("ifcx-field-not-integer", key))
}

pub(crate) fn text(map: &Object, key: &str) -> Result<String, Message> {
    field(map, key)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| about("ifcx-field-not-text", key))
}

pub(crate) fn boolean(map: &Object, key: &str) -> Result<bool, Message> {
    field(map, key)?
        .as_bool()
        .ok_or_else(|| about("ifcx-field-not-boolean", key))
}

pub(crate) fn object<'v>(map: &'v Object, key: &str) -> Result<&'v Object, Message> {
    field(map, key)?
        .as_object()
        .ok_or_else(|| about("ifcx-field-not-object", key))
}

pub(crate) fn array<'v>(map: &'v Object, key: &str) -> Result<&'v Vec<Value>, Message> {
    field(map, key)?
        .as_array()
        .ok_or_else(|| about("ifcx-field-not-list", key))
}

pub(crate) fn code<T>(map: &Object, key: &str, parse: impl Fn(&str) -> Option<T>) -> Result<T, Message> {
    let value = text(map, key)?;
    parse(&value).ok_or_else(|| about("ifcx-field-unknown-value", key).text("value", value))
}

pub(crate) fn uuid(map: &Object, key: &str) -> Result<Uuid, Message> {
    let value = text(map, key)?;
    Uuid::parse_str(&value).map_err(|_| about("ifcx-field-not-uuid", key))
}

pub(crate) fn point(value: &Value) -> Result<DVec2, Message> {
    let items = value.as_array().ok_or_else(|| Message::new("ifcx-point-not-list"))?;
    match items.as_slice() {
        [x, y] => match (x.as_f64(), y.as_f64()) {
            (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Ok(DVec2::new(x, y)),
            _ => Err(Message::new("ifcx-point-not-numeric")),
        },
        _ => Err(Message::new("ifcx-point-coordinates")),
    }
}

pub(crate) fn points(value: &Value) -> Result<Vec<DVec2>, Message> {
    value
        .as_array()
        .ok_or_else(|| Message::new("ifcx-points-not-list"))?
        .iter()
        .map(point)
        .collect()
}

/// A point in the field `key`, its problem named with the field.
fn point_field(map: &Object, key: &str) -> Result<DVec2, Message> {
    point(field(map, key)?).map_err(|e| about("ifcx-field-point", key).nested("reason", e))
}

fn rgb(map: &Object, key: &str) -> Result<Rgb, Message> {
    let values = array(map, key)?
        .iter()
        .map(Value::as_f64)
        .collect::<Option<Vec<f64>>>()
        .ok_or_else(|| about("ifcx-field-colour-component", key))?;
    match values.as_slice() {
        [r, g, b] if values.iter().all(|c| (0.0..=1.0).contains(c)) => Ok(Rgb([*r, *g, *b])),
        _ => Err(about("ifcx-field-not-colour", key)),
    }
}

/// The optional `predefinedType` of an element or a tool's defaults
/// (IFX-024). A literal this version does not know, or a type that `kind`
/// does not offer with `class`, is reported and left out.
fn predefined_type(map: &Object, kind: KindTag, class: IfcClass, report: &mut Report<'_>) -> Option<PredefinedType> {
    let predefined = report.optional(map, "predefinedType", |m, key| code(m, key, PredefinedType::from_code))?;
    if kind.predefined_types(class).contains(&predefined) {
        Some(predefined)
    } else {
        report.repaired(
            Message::new("ifcx-predefined-type-not-offered")
                .text("type", predefined.code())
                .text("class", class.code()),
        );
        None
    }
}

/// The attribute `id` of `node` as a tree, parsed if it is raw text. `None`
/// when it is absent, or when it cannot be parsed, which validation reports.
pub(crate) fn attribute<'n, A: AttributeValue>(node: &'n FlatNode<A>, id: &str) -> Option<Cow<'n, Value>> {
    node.attributes.get(id)?.value().ok()
}

fn class_code<A: AttributeValue>(node: &FlatNode<A>) -> Option<String> {
    attribute(node, "bsi::ifc::class")?
        .get("code")?
        .as_str()
        .map(str::to_owned)
}

fn name<A: AttributeValue>(node: &FlatNode<A>) -> Option<String> {
    attribute(node, "bsi::ifc::prop::Name")?.as_str().map(str::to_owned)
}

fn node_uuid(path: &str, report: &mut Report<'_>) -> Uuid {
    Uuid::parse_str(path).unwrap_or_else(|_| {
        report.repaired(Message::new("ifcx-path-not-uuid"));
        Uuid::new_v4()
    })
}

/// Children of `node` whose class is `class`, in order.
fn children_with_class<'n, A: AttributeValue>(
    nodes: &'n IndexMap<String, FlatNode<A>>,
    node: &FlatNode<A>,
    class: &str,
) -> Vec<(&'n String, &'n FlatNode<A>)> {
    node.children
        .values()
        .flatten()
        .filter_map(|path| nodes.get_key_value(path.as_str()))
        .filter(|(_, n)| class_code(n).as_deref() == Some(class))
        .collect()
}

/// Material record `index` of the project at `path`.
fn read_material(index: usize, value: &Value, path: &str, problems: &mut Vec<Problem>) -> Option<Material> {
    let mut report = Report::new(problems, path, record("ifcx-record-material", index));
    let Some(map) = value.as_object() else {
        report.skipped(Message::new("ifcx-not-object"));
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
            integer(map, "priority")
                .and_then(|p| u16::try_from(p).map_err(|_| about("ifcx-field-out-of-range", "priority"))),
            500,
        ),
        cut_fill: CutFill {
            pattern: report.or(code(&fill, "pattern", FillPattern::from_code), FillPattern::Solid),
            foreground: report.or(rgb(&fill, "foreground"), color),
            background: report.optional(&fill, "background", rgb),
        },
    };
    if let Err(e) = material.validate() {
        report.repaired(
            Message::new("ifcx-material-corrected")
                .text("name", material.name.clone())
                .nested("reason", e),
        );
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
    let mut report = Report::new(problems, path, record("ifcx-record-layer", index));
    let Some(map) = value.as_object() else {
        report.skipped(Message::new("ifcx-not-object"));
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
        snap_excluded: report.optional(map, "snapExcluded", boolean).unwrap_or(false),
    })
}

/// View record `index` of the project at `path`. A view has no field with a
/// sensible default, so an invalid one skips the view.
fn read_view(index: usize, value: &Value, path: &str, problems: &mut Vec<Problem>) -> Option<View3d> {
    let mut report = Report::new(problems, path, record("ifcx-record-view", index));
    let Some(map) = value.as_object() else {
        report.skipped(Message::new("ifcx-not-object"));
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

/// Group record `index` of the project at `path` (ELM-015). A record without
/// a valid identity is skipped; a missing or blank name falls back to
/// `Group`, an invalid parent is left out.
fn read_group(index: usize, value: &Value, path: &str, problems: &mut Vec<Problem>) -> Option<Group> {
    let mut report = Report::new(problems, path, record("ifcx-record-group", index));
    let Some(map) = value.as_object() else {
        report.skipped(Message::new("ifcx-not-object"));
        return None;
    };
    let id = match uuid(map, "id") {
        Ok(id) => GroupId::from_uuid(id),
        Err(e) => {
            report.skipped(e);
            return None;
        }
    };
    let mut name = report.or(text(map, "name"), "Group".to_owned());
    if name.trim().is_empty() {
        report.repaired(Message::new("ifcx-no-name").text("name", "Group"));
        name = "Group".to_owned();
    }
    Some(Group {
        id,
        name,
        parent: report.optional(map, "parent", uuid).map(GroupId::from_uuid),
    })
}

/// The group records of the project at `path`, repaired into a tree
/// (ELM-015, PRJ-008): a second record of an identity is skipped, a parent
/// that names no group is dropped, and a parent cycle is broken at the
/// first of its groups in file order. Each repair is reported against its
/// record.
fn read_groups(records: &[Value], path: &str, problems: &mut Vec<Problem>) -> IndexMap<GroupId, Group> {
    let mut groups = IndexMap::new();
    let mut indices = HashMap::new();
    for (index, value) in records.iter().enumerate() {
        let Some(group) = read_group(index, value, path, problems) else {
            continue;
        };
        if groups.contains_key(&group.id) {
            Report::new(problems, path, record("ifcx-record-group", index))
                .skipped(Message::new("ifcx-group-listed-twice"));
            continue;
        }
        indices.insert(group.id, index);
        groups.insert(group.id, group);
    }
    let ids: Vec<GroupId> = groups.keys().copied().collect();
    for id in &ids {
        let parent = groups[id].parent;
        if parent.is_some_and(|p| !groups.contains_key(&p)) {
            Report::new(problems, path, record("ifcx-record-group", indices[id]))
                .repaired(Message::new("ifcx-group-parent-missing"));
            groups[id].parent = None;
        }
    }
    // Every parent exists now. Each group is walked once (NFR-SEC-003): a
    // walk ends at an outermost group, at a group an earlier walk passed, or
    // at a group already on its own path, which closes a cycle.
    let mut walked = HashSet::new();
    for id in &ids {
        let mut walk = Vec::new();
        let mut on_walk = HashSet::new();
        let mut next = Some(*id);
        while let Some(group) = next.filter(|g| !walked.contains(g)) {
            if !on_walk.insert(group) {
                let cycle = walk
                    .iter()
                    .position(|g| *g == group)
                    .map_or(&walk[..], |at| &walk[at..]);
                if let Some(&first) = cycle.iter().min_by_key(|g| indices[*g]) {
                    Report::new(problems, path, record("ifcx-record-group", indices[&first]))
                        .repaired(Message::new("ifcx-group-cycle"));
                    groups[&first].parent = None;
                }
                break;
            }
            walk.push(group);
            next = groups[&group].parent;
        }
        walked.extend(walk);
    }
    groups
}

/// A two-component grid value, checked against the model's range.
fn grid_vector(map: &Object, key: &str, valid: fn(DVec2) -> bool) -> Result<DVec2, Message> {
    let value = point_field(map, key)?;
    if valid(value) {
        Ok(value)
    } else {
        Err(about("ifcx-field-out-of-range", key))
    }
}

/// The drafting grid of the plan views (INP-013). A missing `planGrid` is no
/// problem: files written before it existed get the default grid. Inside a
/// present grid, each invalid field is reported and falls back to its default.
///
/// An auxiliary spacing larger than the main spacing is reported only when
/// both were read from the file, and then both take their defaults. When one
/// of them fell back, its problem is already reported, and the auxiliary
/// spacing is clamped to the main spacing so that the grid stays valid.
fn read_plan_grid(settings: &Object, report: &mut Report) -> PlanGrid {
    let defaults = PlanGrid::default();
    let Ok(map) = object(settings, "planGrid") else {
        return defaults;
    };
    let snap_step = report.or(grid_vector(map, "snapStep", valid_grid_spacing), defaults.snap_step);
    let snap_enabled = report.or(boolean(map, "snapEnabled"), defaults.snap_enabled);
    let main_spacing = grid_vector(map, "mainSpacing", valid_grid_spacing);
    let auxiliary_spacing = grid_vector(map, "auxiliarySpacing", valid_grid_spacing);
    let spacings_read = main_spacing.is_ok() && auxiliary_spacing.is_ok();
    let mut grid = PlanGrid {
        snap_step,
        snap_enabled,
        main_spacing: report.or(main_spacing, defaults.main_spacing),
        auxiliary_spacing: report.or(auxiliary_spacing, defaults.auxiliary_spacing),
        rotation: report.or(real(map, "rotation"), defaults.rotation),
        origin: report.or(grid_vector(map, "origin", valid_grid_origin), defaults.origin),
        visible: report.or(boolean(map, "visible"), defaults.visible),
    };
    if !spacings_read {
        grid.auxiliary_spacing = grid.auxiliary_spacing.min(grid.main_spacing);
    } else if grid.auxiliary_spacing.cmpgt(grid.main_spacing).any() {
        report.warn(Message::new("ifcx-grid-spacings"));
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
    /// The storey `groundStorey` names, if it is a valid identity.
    ground: Option<StoreyId>,
    /// An invalid `groundStorey` was reported with the project data, so the
    /// storey list does not report the fallback ground storey again.
    ground_repaired: bool,
    layers: Vec<Layer>,
    materials: Vec<Material>,
    tool_defaults: Option<Object>,
    views: Vec<View3d>,
    /// The element groups (ELM-015), repaired into a tree.
    groups: IndexMap<GroupId, Group>,
    /// The `counters` object, read once the elements are known.
    counters: Result<Object, Message>,
    /// The file's `formatVersion`, if it is readable.
    format: Option<u32>,
    newer_format: bool,
}

/// Display decimals: an integer from 0 to [`MAX_DECIMALS`]. An integer out of
/// that range is clamped to it and reported (PRJ-008); anything else is
/// reported and `default` is used.
fn decimals(map: &Object, key: &str, default: u8, report: &mut Report<'_>) -> u8 {
    let value = report.or(integer(map, key), i64::from(default));
    let clamped = value.clamp(0, i64::from(MAX_DECIMALS));
    if clamped != value {
        report.repaired(about("ifcx-out-of-range-replaced", key).count("value", usize::try_from(clamped).unwrap_or(0)));
    }
    u8::try_from(clamped).unwrap_or(default)
}

fn read_settings(map: &Object, report: &mut Report<'_>) -> ProjectSettings {
    let defaults = ProjectSettings::default();
    let units = UnitSettings {
        length: report.or(code(map, "lengthUnit", LengthUnit::from_code), LengthUnit::Meter),
        length_decimals: decimals(map, "lengthDecimals", defaults.units.length_decimals, report),
        angle: report.or(code(map, "angleUnit", AngleUnit::from_code), AngleUnit::DecimalDegrees),
        angle_decimals: decimals(map, "angleDecimals", defaults.units.angle_decimals, report),
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
    let mut report = Report::new(problems, path, Message::new("ifcx-record-project"));
    let format = text(attr, "formatVersion").ok().and_then(|v| v.parse::<u32>().ok());
    match format {
        Some(version) if version > current => {
            newer_format = true;
            report.warn(
                Message::new("ifcx-newer-format").count("version", usize::try_from(version).unwrap_or(usize::MAX)),
            );
        }
        Some(_) => {}
        None => report.warn(Message::new("ifcx-format-version-invalid")),
    }
    let settings_map = report.or(object(attr, "settings").cloned(), Object::new());
    let location_map = report.or(object(attr, "location").cloned(), Object::new());
    let info_map = report.or(object(attr, "info").cloned().map(Some), None);
    // A missing ground storey, or one that names no storey, is reported when
    // the storeys are known; an invalid one only here.
    let (ground, ground_repaired) = match attr.get("groundStorey").map(|_| uuid(attr, "groundStorey")) {
        None => (None, false),
        Some(Ok(id)) => (Some(StoreyId::from_uuid(id)), false),
        Some(Err(e)) => {
            report.repaired(Message::new("ifcx-ground-storey-repaired").nested("reason", e));
            (None, true)
        }
    };
    let material_records = report.or(array(attr, "materials").cloned(), Vec::new());
    let layer_records = report.or(array(attr, "layers").cloned(), Vec::new());
    let view_records = report.or(array(attr, "views").cloned(), Vec::new());
    let group_records = report
        .optional(attr, "groups", |m, key| array(m, key).cloned())
        .unwrap_or_default();
    let counters = object(attr, "counters").cloned();

    let settings = read_settings(
        &settings_map,
        &mut Report::new(problems, path, Message::new("ifcx-record-project-settings")),
    );
    let mut report = Report::new(problems, path, Message::new("ifcx-record-project-location"));
    let location = Location {
        altitude: report.or(real(&location_map, "altitude"), 0.0),
        survey_point: report.or(
            point_field(&location_map, "surveyPoint").map(|p| [p.x, p.y]),
            [0.0, 0.0],
        ),
        true_north: report.or(real(&location_map, "trueNorth"), 0.0),
    };
    // A missing info object is reported once above, not field by field.
    let info = info_map.map_or_else(ProjectInfo::default, |map| {
        let mut report = Report::new(problems, path, Message::new("ifcx-record-project-information"));
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
            Report::new(problems, path, Message::new("ifcx-record-tool-defaults"))
                .repaired(Message::new("ifcx-template-defaults-used").nested("reason", e));
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
    let groups = read_groups(&group_records, path, problems);
    ProjectData {
        settings,
        location,
        info,
        ground,
        ground_repaired,
        layers,
        materials,
        tool_defaults,
        views,
        groups,
        counters,
        format,
        newer_format,
    }
}

/// A counter value as a message count.
fn count_of(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

/// The name counters (ELM-019), keyed by kind code. An invalid, negative,
/// out-of-range or missing counter is repaired to the number of elements of
/// its kind, so that automatic names do not repeat. Out of range is above the
/// largest `u32` or equal to it, which leaves no number for the next element.
/// A kind without a tool has no counter until it has named elements. Counters
/// of kinds this build does not know are ignored: their elements are skipped
/// and reported on their own. A kind the file's format `written` does not
/// store, a placeholder or a kind of a later format, has no counter in the
/// file: it counts its elements without a report.
fn read_counters(
    map: Result<&Object, Message>,
    project: &Project,
    written: u32,
    path: &str,
    problems: &mut Vec<Problem>,
) -> NameCounters {
    let count = |tag: KindTag| {
        let n = project.elements.values().filter(|e| e.tag() == tag).count();
        u32::try_from(n).unwrap_or(u32::MAX)
    };
    let mut report = Report::new(problems, path, Message::new("ifcx-record-counters"));
    let mut counters = NameCounters::default();
    let map = match map {
        Ok(map) => map,
        Err(e) => {
            let counts: Vec<String> = KindTag::ALL
                .iter()
                .map(|&kind| format!("{}: {}", kind.code(), count(kind)))
                .collect();
            report.repaired(
                Message::new("ifcx-counters-from-elements")
                    .nested("reason", e)
                    .text("counts", counts.join(", ")),
            );
            for &kind in KindTag::ALL {
                counters.set(kind, count(kind));
            }
            return counters;
        }
    };
    for &kind in KindTag::ALL {
        let (key, existing) = (kind.code(), count(kind));
        if kinds::unstored(kind, written) {
            counters.set(kind, existing);
            continue;
        }
        let value = match integer(map, key) {
            Ok(v) if v < 0 => {
                report.repaired(about("ifcx-counter-negative", key).count("value", count_of(existing)));
                existing
            }
            Ok(v) => match u32::try_from(v) {
                Ok(n) if n < u32::MAX => n,
                _ => {
                    report.repaired(about("ifcx-out-of-range-replaced", key).count("value", count_of(existing)));
                    existing
                }
            },
            Err(_) if !map.contains_key(key) && !kind.descriptor().has_tool && existing == 0 => 0,
            Err(e) => {
                report.repaired(
                    Message::new("ifcx-value-used")
                        .nested("reason", e)
                        .count("value", count_of(existing)),
                );
                existing
            }
        };
        counters.set(kind, value);
    }
    counters
}

/// The group counter (ELM-015), optional in the `counters` object: absent is
/// zero. Without a readable `counters` object, which the name counters
/// report, and when it is invalid, negative or out of range like a name
/// counter, it is the number of groups, so that group names do not repeat.
fn read_group_counter(map: Option<&Object>, project: &Project, path: &str, problems: &mut Vec<Problem>) -> u32 {
    let existing = u32::try_from(project.groups.len()).unwrap_or(u32::MAX);
    let key = schema::GROUP_COUNTER;
    let Some(map) = map.filter(|m| m.contains_key(key)) else {
        return if map.is_some() { 0 } else { existing };
    };
    let mut report = Report::new(problems, path, Message::new("ifcx-record-counters"));
    match integer(map, key) {
        Ok(v) if v < 0 => {
            report.repaired(about("ifcx-counter-negative", key).count("value", count_of(existing)));
            existing
        }
        Ok(v) => match u32::try_from(v) {
            Ok(n) if n < u32::MAX => n,
            _ => {
                report.repaired(about("ifcx-out-of-range-replaced", key).count("value", count_of(existing)));
                existing
            }
        },
        Err(e) => {
            report.repaired(
                Message::new("ifcx-value-used")
                    .nested("reason", e)
                    .count("value", count_of(existing)),
            );
            existing
        }
    }
}

/// The tool defaults of the project at `path`, one slot per kind with
/// template defaults (ELM-001). Missing tool defaults were reported with the
/// project data. The template's slots, with their material and layer found
/// by name among `materials` and `layers`, stand in for missing tool
/// defaults and missing slots, and their values for invalid fields. Members
/// that name no kind with a tool are ignored and reported. A kind the file's
/// format `written` does not store, a placeholder or a kind of a later
/// format, has no slot in the file: it gets the template's slot without a
/// report.
fn read_tool_defaults(
    map: Option<&Object>,
    materials: &[Material],
    layers: &[Layer],
    written: u32,
    path: &str,
    problems: &mut Vec<Problem>,
) -> ToolDefaults {
    let template = template_tool_defaults(&TemplateContext::new(materials, layers));
    let Some(map) = map else {
        return template;
    };
    let mut report = Report::new(problems, path, Message::new("ifcx-record-tool-defaults"));
    for key in map.keys() {
        if !template.iter().any(|(kind, _)| kind.code() == key) {
            report.repaired(about("ifcx-tool-unknown", key));
        }
    }
    template
        .iter()
        .map(|(kind, default)| {
            if kinds::unstored(kind, written) {
                return default.clone();
            }
            match object(map, kind.code()) {
                Ok(slot) => read_defaults_slot(slot, default, materials, layers, &mut report),
                Err(e) => {
                    report.repaired(Message::new("ifcx-tool-template-used").nested("reason", e));
                    default.clone()
                }
            }
        })
        .collect()
}

/// One tool's defaults, with `default`'s values for invalid fields and its
/// placeholder geometry (ELM-001). A class the kind does not allow is
/// replaced by the default's (ELM-011), and a predefined type the class does
/// not offer is left out (IFX-024). The kind's codec reads the parameters.
fn read_defaults_slot(
    map: &Object,
    default: &DefaultsSlot,
    materials: &[Material],
    layers: &[Layer],
    report: &mut Report<'_>,
) -> DefaultsSlot {
    let kind = default.tag();
    // A slot of a kind without a material has none to read or repair.
    let material = default.kind.material().map(|fallback| {
        let id = report.or(uuid(map, "material").map(MaterialId::from_uuid), fallback);
        if materials.iter().any(|m| m.id == id) {
            id
        } else {
            report.repaired(Message::new("ifcx-tool-material-missing"));
            fallback
        }
    });
    let layer = {
        let id = report.or(uuid(map, "layer").map(LayerId::from_uuid), default.layer);
        if layers.iter().any(|l| l.id == id) {
            id
        } else {
            report.repaired(Message::new("ifcx-tool-layer-missing"));
            default.layer
        }
    };
    let mut ifc_class = report.or(code(map, "ifcClass", IfcClass::from_code), default.ifc_class);
    if !kind.ifc_classes().contains(&ifc_class) {
        report.repaired(
            Message::new("ifcx-tool-class-not-allowed")
                .text("class", ifc_class.code())
                .kind("kind", kind)
                .text("default", default.ifc_class.code()),
        );
        ifc_class = default.ifc_class;
    }
    let predefined_type = predefined_type(map, kind, ifc_class, report);
    let mut payload = kinds::codec(kind)
        .and_then(|codec| codec.read_slot(map, &default.kind, report))
        .unwrap_or_else(|| default.kind.clone());
    if let Some(material) = material {
        payload.set_material(material);
    }
    DefaultsSlot {
        layer,
        position: report.or(code(map, "position", Position::from_code), default.position),
        ifc_class,
        predefined_type,
        kind: payload,
    }
}

/// A storey as its node was read: the node's path, the storey and the paths
/// of the node's children.
pub(crate) struct StoreyEntry {
    pub path: String,
    pub storey: Storey,
    pub children: Vec<String>,
}

fn read_storeys<A: AttributeValue>(
    nodes: &IndexMap<String, FlatNode<A>>,
    building: &FlatNode<A>,
    problems: &mut Vec<Problem>,
) -> Vec<StoreyEntry> {
    let mut storeys = Vec::new();
    let mut seen = HashSet::new();
    for (path, node) in children_with_class(nodes, building, "IfcBuildingStorey") {
        let mut report = Report::new(
            problems,
            path,
            Message::new("ifcx-record-storey").text("name", name(node).as_deref().unwrap_or(path)),
        );
        // NFR-SEC-003: a storey listed again is read once, so a file cannot
        // multiply it (found by the first fuzz campaign, docs/testing.md).
        if !seen.insert(path) {
            report.skipped(Message::new("ifcx-storey-listed-twice"));
            continue;
        }
        let id = StoreyId::from_uuid(node_uuid(path, &mut report));
        let name = name(node).unwrap_or_else(|| {
            report.repaired(Message::new("ifcx-no-name").text("name", "Storey"));
            "Storey".into()
        });
        let attr = attribute(node, schema::STOREY);
        let Some(attr) = attr.as_deref().and_then(Value::as_object) else {
            report.repaired(
                Message::new("ifcx-storey-no-data")
                    .length("elevation", 0.0)
                    .length("height", 3.0),
            );
            let storey = Storey {
                id,
                name,
                elevation: 0.0,
                height: 3.0,
                cut_plane_height: None,
            };
            storeys.push(StoreyEntry {
                path: path.clone(),
                storey,
                children: node.children.values().flatten().cloned().collect(),
            });
            continue;
        };
        let mut height = report.or(real(attr, "height"), 3.0);
        if height < MIN_STOREY_HEIGHT {
            report.repaired(Message::new("ifcx-storey-height-min").length("min", MIN_STOREY_HEIGHT));
            height = MIN_STOREY_HEIGHT;
        }
        let storey = Storey {
            id,
            name,
            elevation: report.or(real(attr, "elevation"), 0.0),
            height,
            cut_plane_height: report.optional(attr, "cutPlaneHeight", real),
        };
        storeys.push(StoreyEntry {
            path: path.clone(),
            storey,
            children: node.children.values().flatten().cloned().collect(),
        });
    }
    storeys
}

/// Builds a storey list, repairing coincident elevations if necessary. A
/// ground storey that is missing or names no storey is reported here, unless
/// `ground_repaired` says the project data reported it already.
fn storey_list(
    mut storeys: Vec<Storey>,
    ground: Option<StoreyId>,
    ground_repaired: bool,
    problems: &mut Vec<Problem>,
) -> StoreyList {
    if storeys.is_empty() {
        problems.push(Problem::warning(None, Message::new("ifcx-no-storeys")));
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
            if !ground_repaired {
                problems.push(Problem::warning(None, Message::new("ifcx-ground-storey-undefined")));
            }
            g
        }
    };
    for i in 1..storeys.len() {
        if storeys[i].elevation - storeys[i - 1].elevation < MIN_STOREY_HEIGHT {
            storeys[i].elevation = storeys[i - 1].elevation + storeys[i - 1].height.max(MIN_STOREY_HEIGHT);
            problems.push(Problem::warning(
                Some(&storeys[i].id.to_string()),
                Message::new("ifcx-storey-moved-up").text("name", storeys[i].name.clone()),
            ));
        }
    }
    match StoreyList::new(storeys.clone(), ground) {
        Ok(list) => list,
        Err(e) => {
            problems.push(Problem::error(
                None,
                Message::new("ifcx-storeys-unreadable").nested("reason", e),
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
pub(crate) fn element_record<A: AttributeValue>(path: &str, node: &FlatNode<A>) -> Message {
    Message::new("ifcx-record-element").text("name", name(node).as_deref().unwrap_or(path))
}

/// Reads the element at `path`, its parameters through the kind's codec
/// (IFX-060). Its attributes are parsed here and dropped when it is read
/// (NFR-SEC-003).
fn read_element<A: AttributeValue>(
    project: &Project,
    path: &str,
    node: &FlatNode<A>,
    storey: StoreyId,
    problems: &mut Vec<Problem>,
) -> Option<Element> {
    let mut report = Report::new(problems, path, element_record(path, node));
    let common = attribute(node, schema::ELEMENT);
    let Some(common) = common.as_deref().and_then(Value::as_object) else {
        report.skipped(Message::new("ifcx-element-data-not-object"));
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
    // A node that shows another name, a space's room name (SPC-001), keeps
    // the element's own in `archicraft::element`.
    let own_name = report.optional(common, "name", text);
    let record = ElementRecord {
        kind,
        id,
        name: own_name.or_else(|| name(node)),
        class: class_code(node),
        storey,
        common,
    };
    read_element_record(project, record, |codec| attribute(node, codec.attribute()), &mut report)
}

/// An element's record wherever the file keeps it, an element node or an
/// annotation entry ([`annotations`]), once its kind and identity are
/// known: its name and IFC class code as found, its home storey and the
/// object with its common data (`archicraft::element` of a node, the entry
/// itself).
pub(crate) struct ElementRecord<'a> {
    pub kind: KindTag,
    pub id: ElementId,
    pub name: Option<String>,
    pub class: Option<String>,
    pub storey: StoreyId,
    pub common: &'a Object,
}

/// Reads the element of `record` (IFX-060): a missing name, layer, class,
/// predefined type, group, position or flag is repaired as reported on
/// `report`, and the parameters, which `parameters` finds for the kind's
/// codec, are read through the codec. Without a codec or parameters, or
/// when they or the element are invalid, the element is skipped.
pub(crate) fn read_element_record<'v>(
    project: &Project,
    record: ElementRecord<'_>,
    parameters: impl FnOnce(&dyn Codec) -> Option<Cow<'v, Value>>,
    report: &mut Report<'_>,
) -> Option<Element> {
    let ElementRecord {
        kind,
        id,
        name,
        class,
        storey,
        common,
    } = record;
    let name = name.unwrap_or_else(|| {
        let name = format!("{}-?", kind.name_prefix());
        report.repaired(Message::new("ifcx-no-name").text("name", name.clone()));
        name
    });
    let mut layer = report.or(uuid(common, "layer").map(LayerId::from_uuid), project.layers[0].id);
    if project.layer(layer).is_none() {
        report.repaired(Message::new("ifcx-element-layer-missing"));
        layer = project.layers[0].id;
    }
    // ELM-011: a class the kind does not allow falls back to its default.
    let default_class = kind.descriptor().default_class;
    let ifc_class = match class.as_deref().and_then(IfcClass::from_code) {
        Some(c) if kind.ifc_classes().contains(&c) => c,
        _ => {
            report.repaired(Message::new("ifcx-element-class-unsupported").text("default", default_class.code()));
            default_class
        }
    };
    let predefined_type = predefined_type(common, kind, ifc_class, report);
    // The writer never writes a kind it cannot read back.
    let Some(codec) = kinds::codec(kind) else {
        report.skipped(Message::new("ifcx-kind-unreadable").kind("kind", kind));
        return None;
    };
    let attr = parameters(codec);
    let Some(attr) = attr.as_deref().and_then(Value::as_object) else {
        report.skipped(Message::new("ifcx-parameters-missing").kind("kind", kind));
        return None;
    };
    let element_kind = match codec.read(attr, &ReadContext { project }, report) {
        Ok(parameters) => parameters,
        Err(e) => {
            report.skipped(e);
            return None;
        }
    };
    // ELM-015: a group that the project does not have is dropped.
    let group = report
        .optional(common, "group", uuid)
        .map(GroupId::from_uuid)
        .filter(|g| {
            let known = project.group(*g).is_some();
            if !known {
                report.repaired(Message::new("ifcx-element-group-missing"));
            }
            known
        });
    let element = Element {
        id,
        name,
        storey,
        layer,
        ifc_class,
        predefined_type,
        position: report.or(code(common, "position", Position::from_code), Position::default()),
        locked: report.or(boolean(common, "locked"), false),
        snap_excluded: report.optional(common, "snapExcluded", boolean).unwrap_or(false),
        group,
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

/// Reads a document written by ArchiCraft, opened with raw attribute values
/// ([`validate::parse_raw`]), after migrating it from an earlier ArchiCraft
/// format (IFX-062, IFX-091). Fails only if the file holds no ArchiCraft
/// project at all. Not generic, so that it is the `fn` pointer of the
/// adapter table ([`crate::Adapter::read`]).
pub fn read(file: &IfcxFile<&RawValue>) -> Result<ReadOutcome, Problem> {
    read_with(file, migrate::MIGRATIONS, FORMAT_VERSION)
}

/// [`read`] with another migration chain up to format `current`, so that
/// tests can run synthetic migrations, and for either type of attribute
/// value.
pub(crate) fn read_with<A: AttributeValue>(
    file: &IfcxFile<A>,
    migrations: &[Migration],
    current: u32,
) -> Result<ReadOutcome, Problem> {
    let mut problems = Vec::new();
    match migrate::upgrade(file, migrations, current, &mut problems) {
        Some((migrated, from)) => read_migrated(&migrated, Some(from), current, problems),
        None => read_migrated(file, None, current, problems),
    }
}

/// Reads `file`, at format `current` or not migratable to it; `problems`
/// holds the migration's reports.
fn read_migrated<A: AttributeValue>(
    file: &IfcxFile<A>,
    migrated_from: Option<u32>,
    current: u32,
    mut problems: Vec<Problem>,
) -> Result<ReadOutcome, Problem> {
    // Flattened once, for validation and reading.
    let nodes = validate::flatten(file);
    problems.extend(validate::validate_flattened(file, &nodes, &[]));

    let Some((root_path, root)) = nodes.iter().find(|(_, n)| n.attributes.contains_key(schema::PROJECT)) else {
        return Err(Problem::error(None, Message::new("ifcx-no-project")));
    };
    let project_attr = attribute(root, schema::PROJECT);
    let Some(project_attr) = project_attr.as_deref().and_then(Value::as_object) else {
        return Err(Problem::error(Some(root_path), Message::new("ifcx-project-not-object")));
    };
    let mut data = read_project_data(project_attr, root_path, current, &mut problems);
    let mut root_report = Report::new(&mut problems, root_path, Message::new("ifcx-record-project"));
    let project_id = ProjectId::from_uuid(node_uuid(root_path, &mut root_report));
    data.info.name = name(root).unwrap_or_else(|| "Untitled project".into());
    data.info.description = attribute(root, "bsi::ifc::prop::Description")
        .as_deref()
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();

    let site = children_with_class(&nodes, root, "IfcSite").into_iter().next();
    if site.is_none() {
        root_report.warn(Message::new("ifcx-no-site"));
    }
    let building = site.and_then(|(_, s)| children_with_class(&nodes, s, "IfcBuilding").into_iter().next());
    if building.is_none() {
        root_report.warn(Message::new("ifcx-no-building"));
    }
    if data.views.is_empty() {
        root_report.warn(Message::new("ifcx-no-views"));
        data.views.push(View3d {
            id: ViewId::new(),
            name: "3D perspective".into(),
            projection: Projection::Perspective,
        });
    }

    if data.materials.is_empty() {
        root_report.warn(Message::new("ifcx-no-materials"));
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
        root_report.warn(Message::new("ifcx-no-layers"));
        data.layers.push(Layer::new("Default"));
    }
    let site_id = site.map_or_else(SiteId::new, |(p, _)| {
        SiteId::from_uuid(node_uuid(
            p,
            &mut Report::new(&mut problems, p, Message::new("ifcx-record-site")),
        ))
    });
    let building_id = building.map_or_else(BuildingId::new, |(p, _)| {
        BuildingId::from_uuid(node_uuid(
            p,
            &mut Report::new(&mut problems, p, Message::new("ifcx-record-building")),
        ))
    });
    // The format the file was written in, which says what it can lack
    // (IFX-062): before its migration, else as it states it.
    let written = migrated_from.or(data.format).unwrap_or(current);
    let tool_defaults = read_tool_defaults(
        data.tool_defaults.as_ref(),
        &data.materials,
        &data.layers,
        written,
        root_path,
        &mut problems,
    );

    let storey_entries = building
        .map(|(_, b)| read_storeys(&nodes, b, &mut problems))
        .unwrap_or_default();
    let storeys = storey_list(
        storey_entries.iter().map(|entry| entry.storey.clone()).collect(),
        data.ground,
        data.ground_repaired,
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
        // Not stored by this format version (kinds::Stored::Template).
        text_styles: template_text_styles(),
        elements: IndexMap::new(),
        groups: data.groups,
        relations: RelationStore::default(),
        tool_defaults,
        views: data.views,
        counters: NameCounters::default(),
        group_counter: 0,
        data_version: 0,
        revision: 0,
    };
    project.data_version = file.header.data_version.parse().unwrap_or_else(|_| {
        problems.push(Problem::warning(None, Message::new("ifcx-data-version-invalid")));
        0
    });

    let mut seen = HashSet::new();
    // The elements read, in file order, for the relations pass below.
    let mut read_elements: Vec<(String, ElementId)> = Vec::new();
    for StoreyEntry { storey, children, .. } in &storey_entries {
        let storey_lost = project.storeys.get(storey.id).is_none();
        for child in children {
            // A missing child is reported by `validate`.
            let Some(node) = nodes.get(child.as_str()) else {
                continue;
            };
            if storey_lost {
                if node.attributes.contains_key(schema::ELEMENT) {
                    Report::new(&mut problems, child, element_record(child, node))
                        .skipped(Message::new("ifcx-storey-lost").text("name", storey.name.clone()));
                }
                continue;
            }
            if !node.attributes.contains_key(schema::ELEMENT) {
                problems.push(Problem::warning(
                    Some(child),
                    Message::new("ifcx-foreign-node").text("name", name(node).unwrap_or_else(|| child.clone())),
                ));
                continue;
            }
            // NFR-SEC-003: an element listed again is read once, so a file
            // cannot multiply its validation (docs/testing.md, Fuzzing).
            if !seen.insert(child.as_str()) {
                problems.push(Problem::error(
                    Some(child),
                    Message::new("ifcx-duplicate").text("name", name(node).unwrap_or_else(|| child.clone())),
                ));
                continue;
            }
            if let Some(element) = read_element(&project, child, node, storey.id, &mut problems) {
                if project.elements.contains_key(&element.id) {
                    problems.push(Problem::error(
                        Some(child),
                        Message::new("ifcx-duplicate").text("name", element.name.clone()),
                    ));
                    continue;
                }
                read_elements.push((child.clone(), element.id));
                project.elements.insert(element.id, element);
            }
        }
    }
    // SRS 3.3: the nodes' explicit relations, once every element node is
    // known, so that a relation to an element later in the file resolves.
    let waiting = relations::read(&mut project, &nodes, &read_elements, &mut problems);
    // ANN-010: the annotation entries after every element node and its
    // relations, then the relations that name an entry, once every entry is
    // known.
    let entry_relations = annotations::read(&mut project, &nodes, &storey_entries, &mut problems);
    relations::settle(&mut project, waiting.into_iter().chain(entry_relations), &mut problems);
    project.counters = read_counters(
        data.counters.as_ref().map_err(Clone::clone),
        &project,
        written,
        root_path,
        &mut problems,
    );
    project.group_counter = read_group_counter(data.counters.as_ref().ok(), &project, root_path, &mut problems);
    Ok(ReadOutcome {
        project,
        problems,
        newer_format: data.newer_format,
        source_version: IFCX_VERSION,
        migrated_from,
    })
}

#[cfg(test)]
#[cfg(feature = "test-kinds")]
mod tests {
    use archicraft_model::kinds::Dummy;
    use archicraft_model::template::default_metric_project;
    use serde_json::json;

    use super::*;

    /// The members the writer writes for `slot`: the codec's, then the
    /// common ones, the material only when the slot has one.
    fn written(slot: &DefaultsSlot) -> Object {
        let mut map = kinds::codec(slot.tag()).unwrap().write_slot(&slot.kind).unwrap();
        if let Some(material) = slot.kind.material() {
            map.insert("material".into(), json!(material.to_string()));
        }
        map.insert("layer".into(), json!(slot.layer.to_string()));
        map.insert("position".into(), json!(slot.position.code()));
        map.insert("ifcClass".into(), json!(slot.ifc_class.code()));
        map
    }

    fn read(map: &Object, default: &DefaultsSlot, p: &Project, problems: &mut Vec<Problem>) -> DefaultsSlot {
        let mut report = Report::new(problems, "test", Message::new("ifcx-record-tool-defaults"));
        read_defaults_slot(map, default, &p.materials, &p.layers, &mut report)
    }

    /// SPC-001, ANN-007 (preparation): a slot of a kind without a material
    /// has no material member and reads back as written without a repair;
    /// a slot of a kind with one still repairs a missing member (PRJ-008).
    #[test]
    fn a_slot_without_a_material_reads_back_without_a_repair() {
        let p = default_metric_project();
        let slot = DefaultsSlot {
            layer: p.layers[1].id,
            position: Position::Undefined,
            ifc_class: KindTag::Dummy.descriptor().default_class,
            predefined_type: None,
            kind: Dummy::without_material(DVec2::ZERO, DVec2::ONE).into(),
        };
        let map = written(&slot);
        assert!(!map.contains_key("material"));
        let mut problems = Vec::new();
        assert_eq!(read(&map, &slot, &p, &mut problems), slot);
        assert!(problems.is_empty(), "{problems:#?}");

        let (_, with) = p
            .tool_defaults
            .iter()
            .find(|(_, s)| s.kind.material().is_some())
            .unwrap();
        let mut map = written(with);
        assert!(map.remove("material").is_some());
        assert_eq!(read(&map, with, &p, &mut problems), *with);
        assert_eq!(problems.len(), 1, "{problems:#?}");
        assert!(
            problems[0].message.to_string().contains("ifcx-default-used"),
            "{problems:#?}"
        );
    }
}
