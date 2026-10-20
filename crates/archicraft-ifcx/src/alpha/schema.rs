//! Declarations of the `archicraft::` attributes (IFX-060, IFX-061).
//!
//! Every attribute ArchiCraft writes in its own namespace is declared in the
//! file's `schemas` section, so the file validates on its own. The
//! `formatVersion` in `archicraft::project` versions this schema (IFX-062).
//! [`document`] puts the same declarations into a standalone schema document
//! that could be published on ifcx.dev (IFX-092).

use indexmap::IndexMap;
use serde_json::{Value, json};

use super::writer::IFCX_VERSION;
use crate::container::{Header, IfcxFile};

use archicraft_model::material::{FillPattern, MaterialCategory};
use archicraft_model::units::{AngleUnit, AreaUnit, LengthUnit, SlopeFormat, VolumeUnit};
use archicraft_model::{KindTag, Position, SlabReference};

/// Version of the ArchiCraft schema written by this release.
pub const FORMAT_VERSION: u32 = 1;

pub const PROJECT: &str = "archicraft::project";
pub const STOREY: &str = "archicraft::storey";
pub const ELEMENT: &str = "archicraft::element";
pub const WALL: &str = "archicraft::wall";
pub const SLAB: &str = "archicraft::slab";

pub const REFERENCE_LINES: [&str; 4] = ["outside", "center", "inside", "custom"];
pub const TOP_LINKS: [&str; 3] = ["fixed", "storey", "relative"];
pub const KINDS: [&str; 2] = ["wall", "slab"];

fn real() -> Value {
    json!({"dataType": "Real"})
}

fn length() -> Value {
    json!({"dataType": "Real", "quantityKind": "Length"})
}

fn angle() -> Value {
    json!({"dataType": "Real", "quantityKind": "Plane angle"})
}

fn integer() -> Value {
    json!({"dataType": "Integer"})
}

fn string() -> Value {
    json!({"dataType": "String"})
}

fn boolean() -> Value {
    json!({"dataType": "Boolean"})
}

fn reference() -> Value {
    json!({"dataType": "Reference"})
}

fn enumeration<'a>(options: impl IntoIterator<Item = &'a str>) -> Value {
    let options: Vec<&str> = options.into_iter().collect();
    json!({"dataType": "Enum", "enumRestrictions": {"options": options}})
}

fn array(item: Value) -> Value {
    json!({"dataType": "Array", "arrayRestrictions": {"value": item}})
}

fn object(fields: &[(&str, Value)]) -> Value {
    let values: serde_json::Map<String, Value> = fields.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect();
    json!({"dataType": "Object", "objectRestrictions": {"values": values}})
}

fn optional(mut value: Value) -> Value {
    if let Some(map) = value.as_object_mut() {
        map.insert("optional".into(), Value::Bool(true));
    }
    value
}

/// The name counters (ELM-019): one integer per kind that has a tool, keyed
/// by kind code. Counters of other kinds are written only when non-zero and
/// are not declared.
fn counters() -> Value {
    let fields: Vec<(&str, Value)> = KindTag::ALL
        .iter()
        .filter(|kind| kind.descriptor().has_tool)
        .map(|kind| (kind.code(), integer()))
        .collect();
    object(&fields)
}

fn rgb() -> Value {
    array(real())
}

fn point2() -> Value {
    array(length())
}

fn top_link() -> Value {
    object(&[
        ("link", enumeration(TOP_LINKS)),
        ("value", length()),
        ("storey", optional(reference())),
        ("relative", optional(integer())),
    ])
}

fn positions() -> Value {
    enumeration(Position::ALL.map(Position::code))
}

fn wall_defaults() -> Value {
    object(&[
        ("thickness", length()),
        ("referenceLine", enumeration(REFERENCE_LINES)),
        ("referenceOffset", optional(length())),
        ("baseOffset", length()),
        ("top", top_link()),
        ("material", string()),
        ("layer", string()),
        ("position", positions()),
        ("ifcClass", string()),
    ])
}

fn slab_defaults() -> Value {
    object(&[
        ("thickness", length()),
        (
            "reference",
            enumeration([SlabReference::Top.code(), SlabReference::Bottom.code()]),
        ),
        ("offset", length()),
        ("material", string()),
        ("layer", string()),
        ("position", positions()),
        ("ifcClass", string()),
    ])
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

fn project() -> Value {
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
        (
            "toolDefaults",
            object(&[("wall", wall_defaults()), ("slab", slab_defaults())]),
        ),
        (
            "views",
            array(object(&[
                ("id", string()),
                ("name", string()),
                ("projection", enumeration(["perspective", "parallel"])),
            ])),
        ),
        ("counters", counters()),
    ])
}

/// The `schemas` section written to every file (IFX-061).
pub fn declarations() -> IndexMap<String, Value> {
    let mut schemas = IndexMap::new();
    schemas.insert(PROJECT.to_owned(), json!({"value": project()}));
    schemas.insert(
        STOREY.to_owned(),
        json!({"value": object(&[
            ("elevation", length()),
            ("height", length()),
            ("cutPlaneHeight", optional(length())),
        ])}),
    );
    schemas.insert(
        ELEMENT.to_owned(),
        json!({"value": object(&[
            ("kind", enumeration(KINDS)),
            ("layer", string()),
            ("locked", boolean()),
            ("position", positions()),
        ])}),
    );
    schemas.insert(
        WALL.to_owned(),
        json!({"value": object(&[
            ("start", point2()),
            ("end", point2()),
            ("thickness", length()),
            ("referenceLine", enumeration(REFERENCE_LINES)),
            ("referenceOffset", optional(length())),
            ("baseOffset", length()),
            ("top", top_link()),
            ("material", reference()),
        ])}),
    );
    schemas.insert(
        SLAB.to_owned(),
        json!({"value": object(&[
            ("outline", array(point2())),
            ("holes", array(array(point2()))),
            ("thickness", length()),
            ("reference", enumeration([SlabReference::Top.code(), SlabReference::Bottom.code()])),
            ("offset", length()),
            ("material", reference()),
        ])}),
    );
    schemas
}

/// The `archicraft::` schema as a standalone, schema-only IFCX document, in
/// the shape of the bundled standard schemas: no imports, no data and
/// [`declarations`] as its `schemas`. `header.dataVersion` is the
/// [`FORMAT_VERSION`] it describes. This is what would be published on
/// ifcx.dev under ArchiCraft's own namespace (IFX-092); the writer keeps
/// declaring the schema in-file (IFX-005, IFX-061).
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
        schemas: declarations(),
        data: Vec::new(),
    }
}
