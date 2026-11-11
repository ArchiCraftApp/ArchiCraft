//! `archicraft::column`: the parameters of a column (IFX-060; COL-001,
//! COL-003, ELM-006): the position, the cross-section, the anchor, the
//! rotation, the base offset and top, the end cuts and the material.
//!
//! The column's node is an `IfcColumn`, or an `IfcMember` when chosen
//! (ELM-011), with its body in a `Body` child of its material, or of its
//! parametric profile's, or in `Region_1`, `Region_2`, … children of the
//! regions' materials for a custom profile, and its axis, the vertical line
//! through the position from the bottom to the top cut, in an `Axis` child
//! as `usd::usdgeom::basiscurves` (IFX-035); the writer writes the parts and
//! the axis the geometry gives for every kind. Angles are radians, declared with the plane
//! angle quantity kind. Columns are stored from format 6 on (IFX-062).
//!
//! A profile section names its profile by identity, as a `String`:
//! profiles are records of `archicraft::project`, not nodes (PRF-004). A
//! profile the project does not have gives the tool's template section,
//! reported.
//!
//! The tool's defaults slot holds the same parameters without the position,
//! which a new column gets where it is placed.

use archicraft_model::{ProfileId, TopLink};
use archicraft_model::kinds::{Anchor, Column, ColumnCut, CrossSection, MAX_CUT_ANGLE_DEG, VerticalLink};
use serde_json::{Map, Value, json};

use super::common::{point2_value, read_material, read_top_link, top_link_schema, top_link_value};
use super::{Fields, KindCodec, ReadContext};
use crate::alpha::reader::{self, Object, Report, about};
use crate::alpha::schema;
use crate::message::Message;

/// The anchors by their codes: the nine of [`Anchor::ALL`] in its order,
/// then a profile's insertion origin.
const ANCHORS: [(Anchor, &str); 10] = [
    (Anchor::TopLeft, "top-left"),
    (Anchor::TopCentre, "top-centre"),
    (Anchor::TopRight, "top-right"),
    (Anchor::Left, "left"),
    (Anchor::Centre, "centre"),
    (Anchor::Right, "right"),
    (Anchor::BottomLeft, "bottom-left"),
    (Anchor::BottomCentre, "bottom-centre"),
    (Anchor::BottomRight, "bottom-right"),
    (Anchor::ProfileOrigin, "profile-origin"),
];

fn anchor_code(anchor: Anchor) -> &'static str {
    ANCHORS
        .iter()
        .find(|(a, _)| *a == anchor)
        .map_or("centre", |(_, code)| code)
}

fn anchor_from_code(code: &str) -> Option<Anchor> {
    ANCHORS.iter().find(|(_, c)| *c == code).map(|(anchor, _)| *anchor)
}

fn anchor_schema() -> Value {
    schema::enumeration(ANCHORS.map(|(_, code)| code))
}

const RECTANGULAR: &str = "rectangular";
const CIRCULAR: &str = "circular";
const PROFILE: &str = "profile";

/// The cross-section (COL-001): `{"kind": "rectangular", "width", "height"}`,
/// `{"kind": "circular", "diameter"}` or `{"kind": "profile", "profile":
/// "<identity>"}`, the members of the other kinds left out.
fn section_schema() -> Value {
    schema::object(&[
        ("kind", schema::enumeration([RECTANGULAR, CIRCULAR, PROFILE])),
        ("width", schema::optional(schema::length())),
        ("height", schema::optional(schema::length())),
        ("diameter", schema::optional(schema::length())),
        ("profile", schema::optional(schema::string())),
    ])
}

fn section_value(section: CrossSection) -> Value {
    match section {
        CrossSection::Rectangular { width, height } => {
            json!({"kind": RECTANGULAR, "width": width, "height": height})
        }
        CrossSection::Circular { diameter } => json!({"kind": CIRCULAR, "diameter": diameter}),
        CrossSection::Profile(profile) => json!({"kind": PROFILE, "profile": profile.to_string()}),
    }
}

/// The section from its members; whether its dimensions are in range is
/// checked with the column (ELM-017), whether its profile exists by
/// [`known_profile`].
fn read_section(map: &Object) -> Result<CrossSection, Message> {
    match reader::text(map, "kind")?.as_str() {
        RECTANGULAR => Ok(CrossSection::Rectangular {
            width: reader::real(map, "width")?,
            height: reader::real(map, "height")?,
        }),
        CIRCULAR => Ok(CrossSection::Circular {
            diameter: reader::real(map, "diameter")?,
        }),
        PROFILE => Ok(CrossSection::Profile(ProfileId::from_uuid(reader::uuid(map, "profile")?))),
        other => Err(about("ifcx-field-unknown-value", "kind").text("value", other)),
    }
}

/// The column tool's section in the template (Appendix D.2), for a
/// section that cannot be read or names a profile the project does not
/// have.
const TEMPLATE_SECTION: CrossSection = CrossSection::Rectangular {
    width: 0.3,
    height: 0.3,
};

/// `section`, or [`TEMPLATE_SECTION`] when it names a profile the project
/// does not have, reported (PRJ-008). A profile anchor then becomes the
/// centre when the column is validated.
fn known_profile(section: CrossSection, context: &ReadContext<'_>, report: &mut Report<'_>) -> CrossSection {
    match section.profile() {
        Some(profile) if context.project.profile(profile).is_none() => {
            report.repaired(Message::new("ifcx-column-profile-missing"));
            TEMPLATE_SECTION
        }
        _ => section,
    }
}

const HORIZONTAL: &str = "horizontal";
const ANGLE: &str = "angle";

/// An end cut (COL-003): `{"kind": "horizontal"}` or `{"kind": "angle",
/// "angle": <radians>}`.
fn cut_schema() -> Value {
    schema::object(&[
        ("kind", schema::enumeration([HORIZONTAL, ANGLE])),
        ("angle", schema::optional(schema::angle())),
    ])
}

fn cut_value(cut: ColumnCut) -> Value {
    match cut {
        ColumnCut::Horizontal => json!({"kind": HORIZONTAL}),
        ColumnCut::Angle(angle) => json!({"kind": ANGLE, "angle": angle}),
    }
}

/// The cut in the member `key`: an unreadable cut, or an angle that is not
/// below [`MAX_CUT_ANGLE_DEG`] either way, is a horizontal cut, reported
/// (PRJ-008), so the column loads instead of failing its validation.
fn read_cut(attr: &Object, key: &str, report: &mut Report<'_>) -> ColumnCut {
    let read = reader::object(attr, key).and_then(|map| match reader::text(map, "kind")?.as_str() {
        HORIZONTAL => Ok(ColumnCut::Horizontal),
        ANGLE => Ok(ColumnCut::Angle(reader::real(map, "angle")?)),
        other => Err(about("ifcx-field-unknown-value", "kind").text("value", other)),
    });
    let max = MAX_CUT_ANGLE_DEG.to_radians();
    match report.or(read, ColumnCut::Horizontal) {
        ColumnCut::Angle(angle) if angle.abs() >= max => {
            report.repaired(
                Message::new("ifcx-column-cut-horizontal")
                    .text("key", key)
                    .angle("max", max),
            );
            ColumnCut::Horizontal
        }
        cut => cut,
    }
}

impl KindCodec for Column {
    const ATTRIBUTE: &'static str = "archicraft::column";

    /// Formats 1 to 5 left columns out of the file (IFX-062).
    const SINCE_FORMAT: u32 = 6;

    const FIELDS: Fields = &[
        ("position", &["position"]),
        ("section", &["section"]),
        ("material", &["material"]),
        ("anchor", &["anchor"]),
        ("rotation", &["rotation"]),
        ("link", &["baseOffset", "top"]),
        ("top_cut", &["topCut"]),
        ("bottom_cut", &["bottomCut"]),
    ];

    fn schema() -> Value {
        let mut fields = vec![("position", schema::point2())];
        fields.extend(Self::slot_schema());
        fields.push(("material", schema::reference()));
        schema::object(&fields)
    }

    fn write(&self) -> Map<String, Value> {
        let mut map = Map::new();
        map.insert("position".into(), point2_value(self.position));
        map.extend(self.write_slot());
        map.insert("material".into(), json!(self.material.to_string()));
        map
    }

    /// Without a valid position there is no column; every other member is
    /// repaired with a report to the column tool's template defaults
    /// (Appendix D.2): the template's section, centred, not turned, no base
    /// offset, the top on the storey above, horizontal cuts.
    fn read(attr: &Object, context: &ReadContext<'_>, report: &mut Report<'_>) -> Result<Self, Message> {
        let fallback = Column {
            position: reader::point_field(attr, "position")?,
            section: TEMPLATE_SECTION,
            material: context.project.materials[0].id,
            anchor: Anchor::Centre,
            rotation: 0.0,
            link: VerticalLink {
                base_offset: 0.0,
                top: TopLink::Relative {
                    storeys: 1,
                    offset: 0.0,
                },
            },
            top_cut: ColumnCut::Horizontal,
            bottom_cut: ColumnCut::Horizontal,
        };
        let mut column = Self::read_slot(attr, &fallback, context, report);
        column.material = read_material(attr, context, report);
        Ok(column)
    }

    fn slot_schema() -> Vec<(&'static str, Value)> {
        vec![
            ("section", section_schema()),
            ("anchor", anchor_schema()),
            ("rotation", schema::angle()),
            ("baseOffset", schema::length()),
            ("top", top_link_schema()),
            ("topCut", cut_schema()),
            ("bottomCut", cut_schema()),
        ]
    }

    fn write_slot(&self) -> Map<String, Value> {
        let mut map = Map::new();
        map.insert("section".into(), section_value(self.section));
        map.insert("anchor".into(), json!(anchor_code(self.anchor)));
        map.insert("rotation".into(), json!(self.rotation));
        map.insert("baseOffset".into(), json!(self.link.base_offset));
        map.insert("top".into(), top_link_value(self.link.top));
        map.insert("topCut".into(), cut_value(self.top_cut));
        map.insert("bottomCut".into(), cut_value(self.bottom_cut));
        map
    }

    /// The slot keeps `default`'s placeholder position; a cut that cannot
    /// be read or whose angle is beyond the limit is horizontal, and a
    /// profile the project does not have gives the template section, each
    /// reported.
    fn read_slot(attr: &Object, default: &Self, context: &ReadContext<'_>, report: &mut Report<'_>) -> Self {
        let section = report.or(reader::object(attr, "section").and_then(read_section), default.section);
        Column {
            section: known_profile(section, context, report),
            anchor: report.or(reader::code(attr, "anchor", anchor_from_code), default.anchor),
            rotation: report.or(reader::real(attr, "rotation"), default.rotation),
            link: VerticalLink {
                base_offset: report.or(reader::real(attr, "baseOffset"), default.link.base_offset),
                top: report.or(reader::object(attr, "top").and_then(read_top_link), default.link.top),
            },
            top_cut: read_cut(attr, "topCut", report),
            bottom_cut: read_cut(attr, "bottomCut", report),
            ..default.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every anchor has a code, the codes are distinct and read back.
    #[test]
    fn anchor_codes_cover_every_anchor() {
        for (anchor, (listed, code)) in Anchor::ALL.into_iter().chain([Anchor::ProfileOrigin]).zip(ANCHORS) {
            assert_eq!(anchor, listed);
            assert_eq!(anchor_code(anchor), code);
            assert_eq!(anchor_from_code(code), Some(anchor));
        }
        assert_eq!(anchor_from_code("middle"), None);
    }

    /// The section a damaged one is repaired to is the template's.
    #[test]
    fn the_repair_section_is_the_templates() {
        let template = archicraft_model::template::default_metric_project();
        let slot = template.tool_defaults.get(archicraft_model::KindTag::Column).unwrap();
        let archicraft_model::ElementKind::Column(column) = &slot.kind else {
            panic!("{slot:?}");
        };
        assert_eq!(column.section, TEMPLATE_SECTION);
    }
}
