//! Declarations of the `archicraft::` attributes (IFX-060, IFX-061).
//!
//! Every attribute ArchiCraft writes in its own namespace is declared in the
//! file's `schemas` section, so the file validates on its own. The
//! declarations follow from the kinds the project stores
//! ([`kinds::present`]): a kind's attribute, its option in the kind
//! enumeration, its defaults slot and its counter are declared only when the
//! project has that kind, so a test build, which has the test kind's codec,
//! writes a project without it exactly as a release build does. The kinds
//! that are not IFCX nodes (ANN-010) have no attribute of their own: their
//! members are declared inside `archicraft::annotations` ([`annotations`]),
//! which is declared only when the project stores such a kind. The
//! `formatVersion` in `archicraft::project` versions this schema (IFX-062).
//! [`document`] puts the declarations of every kind a release stores into a
//! standalone schema document that could be published on ifcx.dev (IFX-092).

use indexmap::IndexMap;
use serde_json::{Value, json};

use archicraft_model::material::{FillPattern, MaterialCategory};
use archicraft_model::units::{AngleUnit, AreaUnit, LengthUnit, SlopeFormat, VolumeUnit};
use archicraft_model::{KindTag, Position, PredefinedType};

use super::kinds::{self, Codec, annotations};
use super::relations;
use super::writer::IFCX_VERSION;
use crate::container::{Header, IfcxFile};

/// Version of the ArchiCraft schema written by this release: format 2
/// stores spaces (SPC-001; [`super::migrate::MIGRATIONS`]).
pub const FORMAT_VERSION: u32 = 2;

pub const PROJECT: &str = "archicraft::project";
pub const STOREY: &str = "archicraft::storey";
pub const ELEMENT: &str = "archicraft::element";

/// The key of the group counter among the name counters (ELM-015).
pub const GROUP_COUNTER: &str = "group";

fn real() -> Value {
    json!({"dataType": "Real"})
}

pub(crate) fn length() -> Value {
    json!({"dataType": "Real", "quantityKind": "Length"})
}

fn angle() -> Value {
    json!({"dataType": "Real", "quantityKind": "Plane angle"})
}

pub(crate) fn integer() -> Value {
    json!({"dataType": "Integer"})
}

pub(crate) fn string() -> Value {
    json!({"dataType": "String"})
}

pub(crate) fn boolean() -> Value {
    json!({"dataType": "Boolean"})
}

pub(crate) fn reference() -> Value {
    json!({"dataType": "Reference"})
}

pub(crate) fn enumeration<'a>(options: impl IntoIterator<Item = &'a str>) -> Value {
    let options: Vec<&str> = options.into_iter().collect();
    json!({"dataType": "Enum", "enumRestrictions": {"options": options}})
}

pub(crate) fn array(item: Value) -> Value {
    json!({"dataType": "Array", "arrayRestrictions": {"value": item}})
}

pub(crate) fn object(fields: &[(&str, Value)]) -> Value {
    let values: serde_json::Map<String, Value> = fields.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect();
    json!({"dataType": "Object", "objectRestrictions": {"values": values}})
}

pub(crate) fn optional(mut value: Value) -> Value {
    if let Some(map) = value.as_object_mut() {
        map.insert("optional".into(), Value::Bool(true));
    }
    value
}

/// The name counters (ELM-019): one integer per present kind, keyed by kind
/// code. The counter of a kind without a tool is optional, because the
/// writer writes it only once it is non-zero, like the group counter
/// (ELM-015).
fn counters(present: &[&dyn Codec]) -> Value {
    let mut fields: Vec<(&str, Value)> = present
        .iter()
        .map(|codec| {
            let kind = codec.tag();
            let counter = if kind.descriptor().has_tool {
                integer()
            } else {
                optional(integer())
            };
            (kind.code(), counter)
        })
        .collect();
    fields.push((GROUP_COUNTER, optional(integer())));
    object(&fields)
}

/// The element groups (ELM-015), outermost and nested. Optional: written
/// only when the project has groups, so files without groups stay as they
/// were. A group's `parent` names the group it is nested in, the element's
/// `group` its innermost group; both are group identities, not node paths.
fn groups() -> Value {
    optional(array(object(&[
        ("id", string()),
        ("name", string()),
        ("parent", optional(string())),
    ])))
}

fn rgb() -> Value {
    array(real())
}

pub(crate) fn point2() -> Value {
    array(length())
}

fn positions() -> Value {
    enumeration(Position::ALL.map(Position::code))
}

/// The predefined type of an element or a tool's defaults (IFX-024), as its
/// IFC 4.3 literal. Optional: written only when set.
fn predefined_type() -> Value {
    optional(enumeration(PredefinedType::ALL.map(PredefinedType::code)))
}

/// The flag that excludes an element or a layer from producing snap targets
/// (INP-017). Optional: written only when set.
fn snap_excluded() -> Value {
    optional(boolean())
}

/// The common data of an element (IFX-060), with its kind among `kinds`:
/// `archicraft::element` of an element node, and part of the envelope of an
/// annotation entry ([`annotations`]).
pub(crate) fn element_data<'a>(kinds: impl IntoIterator<Item = &'a str>) -> Vec<(&'static str, Value)> {
    vec![
        ("kind", enumeration(kinds)),
        ("layer", string()),
        ("locked", boolean()),
        ("position", positions()),
        ("predefinedType", predefined_type()),
        ("snapExcluded", snap_excluded()),
        ("group", optional(string())),
    ]
}

/// A tool's defaults slot (ELM-001): the kind's parameters from its codec,
/// then the common members (the material a string here, a reference on the
/// element, and optional for a kind that may have none).
fn defaults_slot(codec: &dyn Codec) -> Value {
    let mut fields = codec.slot_schema();
    let material = if codec.optional_material() {
        optional(string())
    } else {
        string()
    };
    fields.extend([
        ("material", material),
        ("layer", string()),
        ("position", positions()),
        ("ifcClass", string()),
        ("predefinedType", predefined_type()),
    ]);
    object(&fields)
}

/// The tool defaults, one slot per present kind, keyed by kind code. The
/// slot of a kind without template defaults is optional: every project has
/// the template's slots, a slot of another kind only when it was given one.
fn tool_defaults(present: &[&dyn Codec]) -> Value {
    let slots: Vec<(&str, Value)> = present
        .iter()
        .map(|c| {
            let slot = defaults_slot(*c);
            let slot = if c.tag().descriptor().template_defaults.is_some() {
                slot
            } else {
                optional(slot)
            };
            (c.tag().code(), slot)
        })
        .collect();
    object(&slots)
}

/// The drafting grid of the plan views (INP-013). Optional, because files
/// written before it existed do not have it.
fn plan_grid() -> Value {
    object(&[
        ("snapStep", point2()),
        ("snapEnabled", boolean()),
        ("mainSpacing", point2()),
        ("auxiliarySpacing", point2()),
        ("rotation", angle()),
        ("origin", point2()),
        ("visible", boolean()),
    ])
}

fn project(present: &[&dyn Codec]) -> Value {
    object(&[
        ("formatVersion", string()),
        (
            "info",
            object(&[
                ("number", string()),
                ("client", string()),
                ("address", string()),
                ("author", string()),
            ]),
        ),
        (
            "settings",
            object(&[
                ("lengthUnit", enumeration(LengthUnit::ALL.map(LengthUnit::code))),
                ("lengthDecimals", integer()),
                ("angleUnit", enumeration(AngleUnit::ALL.map(AngleUnit::code))),
                ("angleDecimals", integer()),
                ("slopeFormat", enumeration(SlopeFormat::ALL.map(SlopeFormat::code))),
                ("areaUnit", enumeration(AreaUnit::ALL.map(AreaUnit::code))),
                ("volumeUnit", enumeration(VolumeUnit::ALL.map(VolumeUnit::code))),
                ("cutPlaneHeight", length()),
                ("chordDeviation", length()),
                ("maxSegmentAngle", real()),
                ("planGrid", optional(plan_grid())),
            ]),
        ),
        (
            "location",
            object(&[
                ("altitude", length()),
                ("surveyPoint", point2()),
                ("trueNorth", angle()),
            ]),
        ),
        ("groundStorey", reference()),
        (
            "layers",
            array(object(&[
                ("id", string()),
                ("name", string()),
                ("visible", boolean()),
                ("locked", boolean()),
                ("snapExcluded", snap_excluded()),
            ])),
        ),
        (
            "materials",
            array(object(&[
                ("id", string()),
                ("name", string()),
                ("code", string()),
                ("description", string()),
                (
                    "category",
                    enumeration(MaterialCategory::ALL.map(MaterialCategory::code)),
                ),
                ("color", rgb()),
                ("opacity", real()),
                ("priority", integer()),
                (
                    "cutFill",
                    object(&[
                        ("pattern", enumeration(FillPattern::ALL.map(FillPattern::code))),
                        ("foreground", rgb()),
                        ("background", optional(rgb())),
                    ]),
                ),
            ])),
        ),
        ("toolDefaults", tool_defaults(present)),
        (
            "views",
            array(object(&[
                ("id", string()),
                ("name", string()),
                ("projection", enumeration(["perspective", "parallel"])),
            ])),
        ),
        ("counters", counters(present)),
        ("groups", groups()),
    ])
}

/// The `schemas` section of a file that stores the kinds `present`, in
/// registry order (IFX-061): the project, the storey, the common element
/// data, the attribute of each present kind that has a codec other than a
/// placeholder and whose elements are nodes (IFX-060), the annotation
/// entries when a present kind's elements are not nodes (ANN-010) and,
/// with `relations`, the explicit relations between elements (SRS 3.3).
/// The writer passes [`kinds::present`] and [`relations::present`] of the
/// project; [`document`] passes [`kinds::published`] and declares the
/// relations.
pub fn declarations(present: &[KindTag], relations: bool) -> IndexMap<String, Value> {
    let present: Vec<&dyn Codec> = present
        .iter()
        .filter_map(|&kind| kinds::codec(kind))
        .filter(|codec| !codec.placeholder())
        .collect();
    let (entries, nodes): (Vec<&dyn Codec>, Vec<&dyn Codec>) =
        present.iter().copied().partition(|c| annotations::entry_kind(c.tag()));
    let mut schemas = IndexMap::new();
    schemas.insert(PROJECT.to_owned(), json!({"value": project(&present)}));
    schemas.insert(
        STOREY.to_owned(),
        json!({"value": object(&[
            ("elevation", length()),
            ("height", length()),
            ("cutPlaneHeight", optional(length())),
        ])}),
    );
    // An element node's common data, and the element's own name when its
    // node shows another (SPC-001).
    let mut element = element_data(nodes.iter().map(|c| c.tag().code()));
    element.push(("name", optional(string())));
    schemas.insert(ELEMENT.to_owned(), json!({"value": object(&element)}));
    for codec in &nodes {
        schemas.insert(codec.attribute().to_owned(), json!({"value": codec.schema()}));
    }
    if !entries.is_empty() {
        schemas.insert(
            annotations::ATTRIBUTE.to_owned(),
            json!({"value": annotations::schema(&entries)}),
        );
    }
    if relations {
        schemas.insert(relations::ATTRIBUTE.to_owned(), json!({"value": relations::schema()}));
    }
    schemas
}

/// The `archicraft::` schema as a standalone, schema-only IFCX document, in
/// the shape of the bundled standard schemas: no imports, no data and the
/// [`declarations`] of every kind a release stores ([`kinds::published`]) as
/// its `schemas`, the same whether a test build or a release writes it.
/// `header.dataVersion` is the [`FORMAT_VERSION`] it describes. This is what
/// would be published on ifcx.dev under ArchiCraft's own namespace (IFX-092);
/// the writer keeps declaring the schema in-file (IFX-005, IFX-061).
pub fn document(timestamp: &str) -> IfcxFile {
    IfcxFile {
        header: Header {
            id: "archicraft".to_owned(),
            ifcx_version: IFCX_VERSION.to_owned(),
            data_version: FORMAT_VERSION.to_string(),
            author: "ArchiCraft".to_owned(),
            timestamp: timestamp.to_owned(),
        },
        imports: Vec::new(),
        schemas: declarations(&kinds::published(), true),
        data: Vec::new(),
    }
}
