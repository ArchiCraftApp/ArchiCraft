//! The project's profiles as `profiles` of `archicraft::project` (PRF-004,
//! PRF-005, PRJ-007, IFX-060).
//!
//! Profiles are authoring data with no counterpart in the pinned IFC
//! schema, so they are records of the project attribute, not nodes. Each
//! record holds the profile's identity, name, insertion `origin`, `usage`
//! flags, the `catalogue` designation of a catalogue profile (PRF-011) and
//! its `shape`: the `kind`, a parametric shape's code or `custom`; for a
//! parametric shape its `dimensions`, one optional member per
//! [`ProfileDimension`], so the declaration needs no tagged union, and its
//! `material`; for a custom shape its `regions`, each with its material,
//! its `outer` ring and its `holes`. Numbers are written at full precision
//! (NFR-ACC-002).
//!
//! The reader checks per shape which dimensions it needs and runs every
//! record through [`validate_profile`], whose cost is bounded for every
//! input (NFR-SEC-003): a record it rejects is skipped with the model's
//! reason, a material the project does not have is repaired to the first
//! one, and a later record of a loaded profile's identity is skipped
//! (PRJ-008).

use std::collections::HashSet;

use archicraft_model::glam::DVec2;
use archicraft_model::validate::validate_profile;
use archicraft_model::{
    MaterialId, ParametricShape, Profile, ProfileDimension, ProfileId, ProfileRegion, ProfileShape, ProfileUsage,
    Project,
};
use serde_json::{Map, Value, json};

use super::kinds::common::{material_or_first, point2_value};
use super::reader::{Object, Report, about, array, boolean, field, object, point_field, points, real, text, uuid};
use super::schema;
use crate::message::Message;
use crate::validate::Problem;

/// The `kind` of a custom profile's shape; a parametric shape's is its
/// code ([`ParametricShape::code`]).
const CUSTOM: &str = "custom";

/// The member of a shape's `dimensions` that holds `dimension`: its code
/// in camel case.
fn member(dimension: ProfileDimension) -> &'static str {
    match dimension {
        ProfileDimension::Width => "width",
        ProfileDimension::Height => "height",
        ProfileDimension::Diameter => "diameter",
        ProfileDimension::Thickness => "thickness",
        ProfileDimension::OuterRadius => "outerRadius",
        ProfileDimension::Web => "web",
        ProfileDimension::Flange => "flange",
        ProfileDimension::RootRadius => "rootRadius",
        ProfileDimension::ToeRadius => "toeRadius",
    }
}

/// The declaration of the profile records (IFX-061). Optional in the
/// project attribute, because files of format 4 and earlier have none. A
/// material is a string here, as in a tool's defaults slot, not a reference
/// as on an element: the writer writes a material node only for a material
/// an element uses (IFX-040), so a profile's material may have none.
pub(crate) fn schema() -> Value {
    let kinds = ParametricShape::ALL.iter().map(ParametricShape::code).chain([CUSTOM]);
    let dimensions: Vec<(&str, Value)> = ProfileDimension::ALL
        .into_iter()
        .map(|d| (member(d), schema::optional(schema::length())))
        .collect();
    let usage = ["columns", "beams", "walls", "railings"].map(|flag| (flag, schema::boolean()));
    let region = schema::object(&[
        ("material", schema::string()),
        ("outer", schema::array(schema::point2())),
        ("holes", schema::array(schema::array(schema::point2()))),
    ]);
    schema::array(schema::object(&[
        ("id", schema::string()),
        ("name", schema::string()),
        ("origin", schema::point2()),
        ("usage", schema::object(&usage)),
        ("catalogue", schema::optional(schema::string())),
        (
            "shape",
            schema::object(&[
                ("kind", schema::enumeration(kinds)),
                ("dimensions", schema::optional(schema::object(&dimensions))),
                ("material", schema::optional(schema::string())),
                ("regions", schema::optional(schema::array(region))),
            ]),
        ),
    ]))
}

fn ring(points: &[DVec2]) -> Value {
    points.iter().map(|p| point2_value(*p)).collect()
}

/// The record of `profile`; `catalogue` only for a catalogue profile.
fn record(profile: &Profile) -> Value {
    let shape = match &profile.shape {
        ProfileShape::Parametric { shape, material } => {
            let dimensions: Map<String, Value> = shape
                .dimensions()
                .into_iter()
                .map(|(d, value)| (member(d).to_owned(), json!(value)))
                .collect();
            json!({"kind": shape.code(), "dimensions": dimensions, "material": material.to_string()})
        }
        ProfileShape::Custom { regions } => {
            let regions: Vec<Value> = regions
                .iter()
                .map(|r| {
                    json!({
                        "material": r.material.to_string(),
                        "outer": ring(&r.outer),
                        "holes": r.holes.iter().map(|h| ring(h)).collect::<Vec<_>>(),
                    })
                })
                .collect();
            json!({"kind": CUSTOM, "regions": regions})
        }
    };
    let usage = profile.usage;
    let mut record = Map::new();
    record.insert("id".into(), json!(profile.id.to_string()));
    record.insert("name".into(), json!(profile.name));
    record.insert("origin".into(), point2_value(profile.origin));
    record.insert(
        "usage".into(),
        json!({"columns": usage.columns, "beams": usage.beams, "walls": usage.walls, "railings": usage.railings}),
    );
    if let Some(catalogue) = &profile.catalogue {
        record.insert("catalogue".into(), json!(catalogue));
    }
    record.insert("shape".into(), shape);
    Value::Object(record)
}

/// The `profiles` member: every profile in list order, an empty list for a
/// project without profiles.
pub(crate) fn write(profiles: &[Profile]) -> Value {
    profiles.iter().map(record).collect()
}

/// The `material` of a parametric shape or a region: the project's first
/// material, reported, when it is unreadable or names no material of
/// `project` (PRJ-008).
fn material(map: &Object, project: &Project, report: &mut Report<'_>) -> MaterialId {
    material_or_first(map, project, "ifcx-profile-material-missing", report)
}

/// Reports each of `keys` that `shape` has as left out: the members of the
/// other kind of shape, which nothing reads (IFX-083).
fn unused(shape: &Object, keys: impl IntoIterator<Item = impl AsRef<str>>, report: &mut Report<'_>) {
    for key in keys {
        let key = key.as_ref();
        if shape.contains_key(key) {
            report.repaired(about("ifcx-profile-member-unused", key));
        }
    }
}

/// A ring of points in the member `key`, its problem named with the member.
fn ring_of(key: &str, value: &Value) -> Result<Vec<DVec2>, Message> {
    points(value).map_err(|e| about("ifcx-field-point", key).nested("reason", e))
}

fn read_region(value: &Value, project: &Project, report: &mut Report<'_>) -> Result<ProfileRegion, Message> {
    let map = value.as_object().ok_or_else(|| Message::new("ifcx-not-object"))?;
    let outer = ring_of("outer", field(map, "outer")?)?;
    let holes = array(map, "holes")?
        .iter()
        .map(|hole| ring_of("holes", hole))
        .collect::<Result<_, _>>()?;
    Ok(ProfileRegion {
        outer,
        holes,
        material: material(map, project, report),
    })
}

/// The shape of a record: `Err` for a kind this release does not know, a
/// dimension the parametric shape needs that is missing or not a number,
/// or a region that cannot be read. A member of the other kind of shape
/// (the dimensions and material of a parametric shape, the regions of a
/// custom one) and a dimension the shape does not have are left out,
/// reported.
fn read_shape(map: &Object, project: &Project, report: &mut Report<'_>) -> Result<ProfileShape, Message> {
    let kind = text(map, "kind")?;
    if kind == CUSTOM {
        unused(map, ["dimensions", "material"], report);
        let regions = array(map, "regions")?
            .iter()
            .enumerate()
            .map(|(index, region)| {
                read_region(region, project, report).map_err(|e| {
                    Message::new("ifcx-profile-region")
                        .count("index", index)
                        .nested("reason", e)
                })
            })
            .collect::<Result<_, _>>()?;
        return Ok(ProfileShape::Custom { regions });
    }
    let Some(mut shape) = ParametricShape::ALL.into_iter().find(|s| s.code() == kind) else {
        return Err(about("ifcx-field-unknown-value", "kind").text("value", kind));
    };
    let dimensions = object(map, "dimensions")?;
    let needed = shape.dimensions();
    for (dimension, _) in &needed {
        if let Some(value) = shape.dimension_mut(*dimension) {
            *value = real(dimensions, member(*dimension))?;
        }
    }
    let other = ProfileDimension::ALL
        .into_iter()
        .filter(|d| !needed.iter().any(|(n, _)| n == d));
    unused(dimensions, other.map(member), report);
    unused(map, ["regions"], report);
    Ok(ProfileShape::Parametric {
        shape,
        material: material(map, project, report),
    })
}

/// Profile record `index` of the project at `path`, before the model's
/// rules are checked, with the message that names it. A record that is not
/// an object, has no identity or name, or no readable shape is skipped, and
/// so is a record of an identity among `loaded`, with that one report,
/// before the rest of it is read; the origin and usage flags take their
/// defaults, reported, and an unreadable catalogue designation is left out.
fn read_record(
    index: usize,
    value: &Value,
    project: &Project,
    loaded: &HashSet<ProfileId>,
    path: &str,
    problems: &mut Vec<Problem>,
) -> Option<(Profile, Message)> {
    let mut report = Report::new(
        problems,
        path,
        Message::new("ifcx-record-profile").count("index", index),
    );
    let Some(map) = value.as_object() else {
        report.skipped(Message::new("ifcx-not-object"));
        return None;
    };
    let identity = uuid(map, "id").and_then(|id| Ok((ProfileId::from_uuid(id), text(map, "name")?)));
    let (id, name) = match identity {
        Ok(identity) => identity,
        Err(e) => {
            report.skipped(e);
            return None;
        }
    };
    let named = Message::new("ifcx-record-profile-named").text("name", name.clone());
    let mut report = Report::new(problems, path, named.clone());
    if loaded.contains(&id) {
        report.skipped(Message::new("ifcx-profile-listed-twice"));
        return None;
    }
    let shape = match object(map, "shape").and_then(|shape| read_shape(shape, project, &mut report)) {
        Ok(shape) => shape,
        Err(e) => {
            report.skipped(e);
            return None;
        }
    };
    let defaults = ProfileUsage::default();
    let usage = match object(map, "usage") {
        Ok(flags) => ProfileUsage {
            columns: report.or(boolean(flags, "columns"), defaults.columns),
            beams: report.or(boolean(flags, "beams"), defaults.beams),
            walls: report.or(boolean(flags, "walls"), defaults.walls),
            railings: report.or(boolean(flags, "railings"), defaults.railings),
        },
        Err(e) => report.or(Err(e), defaults),
    };
    let profile = Profile {
        id,
        name,
        shape,
        origin: report.or(point_field(map, "origin"), DVec2::ZERO),
        usage,
        catalogue: report.optional(map, "catalogue", text),
    };
    Some((profile, named))
}

/// The profiles of the project at `path` from its `records` (PRF-004,
/// PRJ-008), checked against `project`, which has its materials: each
/// record as [`validate_profile`] normalises it, a record it rejects
/// skipped with the model's reason, and a later record of a loaded
/// profile's identity skipped as an error, so the first is kept, as for
/// groups: the later record's data is lost.
pub(crate) fn read(records: &[Value], project: &Project, path: &str, problems: &mut Vec<Problem>) -> Vec<Profile> {
    let mut profiles = Vec::new();
    let mut loaded = HashSet::new();
    for (index, value) in records.iter().enumerate() {
        let Some((profile, named)) = read_record(index, value, project, &loaded, path, problems) else {
            continue;
        };
        match validate_profile(project, &profile) {
            Ok(valid) => {
                loaded.insert(valid.id);
                profiles.push(valid);
            }
            Err(e) => Report::new(problems, path, named).skipped(e),
        }
    }
    profiles
}
