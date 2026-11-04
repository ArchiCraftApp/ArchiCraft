//! `archicraft::slab`: the parameters of a slab (IFX-060; SLB-001…003): the
//! rings, the defaults slot's members and the material.

use archicraft_model::{Slab, SlabReference};
use serde_json::{Map, Value, json};

use super::common::{point2_value, read_material};
use super::{Fields, KindCodec, ReadContext};
use crate::alpha::reader::{self, Object, Report};
use crate::alpha::schema;
use crate::message::Message;

fn ring_value(ring: &[archicraft_model::glam::DVec2]) -> Value {
    ring.iter().map(|p| point2_value(*p)).collect()
}

impl KindCodec for Slab {
    const ATTRIBUTE: &'static str = "archicraft::slab";

    const FIELDS: Fields = &[
        ("outline", &["outline"]),
        ("holes", &["holes"]),
        ("thickness", &["thickness"]),
        ("reference", &["reference"]),
        ("offset", &["offset"]),
        ("material", &["material"]),
    ];

    fn schema() -> Value {
        let mut fields = vec![
            ("outline", schema::array(schema::point2())),
            ("holes", schema::array(schema::array(schema::point2()))),
        ];
        fields.extend(Self::slot_schema());
        fields.push(("material", schema::reference()));
        schema::object(&fields)
    }

    fn write(&self) -> Map<String, Value> {
        let mut map = Map::new();
        map.insert("outline".into(), ring_value(&self.outline));
        map.insert("holes".into(), self.holes.iter().map(|h| ring_value(h)).collect());
        map.extend(self.write_slot());
        map.insert("material".into(), json!(self.material.to_string()));
        map
    }

    /// Without a valid outline there is no slab; invalid holes are left out
    /// and every other member is repaired with a report: 0.20 m thick, the
    /// top at the storey elevation.
    fn read(attr: &Object, context: &ReadContext<'_>, report: &mut Report<'_>) -> Result<Self, Message> {
        let outline = reader::field(attr, "outline").and_then(reader::points)?;
        let holes = match reader::array(attr, "holes").and_then(|h| h.iter().map(reader::points).collect()) {
            Ok(h) => h,
            Err(e) => {
                report.repaired(Message::new("ifcx-holes-left-out").nested("reason", e));
                Vec::new()
            }
        };
        let fallback = Slab {
            outline,
            holes,
            thickness: 0.2,
            reference: SlabReference::Top,
            offset: 0.0,
            material: context.project.materials[0].id,
        };
        let mut slab = Self::read_slot(attr, &fallback, report);
        slab.material = read_material(attr, context, report);
        Ok(slab)
    }

    fn slot_schema() -> Vec<(&'static str, Value)> {
        vec![
            ("thickness", schema::length()),
            (
                "reference",
                schema::enumeration([SlabReference::Top.code(), SlabReference::Bottom.code()]),
            ),
            ("offset", schema::length()),
        ]
    }

    fn write_slot(&self) -> Map<String, Value> {
        let mut map = Map::new();
        map.insert("thickness".into(), json!(self.thickness));
        map.insert("reference".into(), json!(self.reference.code()));
        map.insert("offset".into(), json!(self.offset));
        map
    }

    fn read_slot(attr: &Object, default: &Self, report: &mut Report<'_>) -> Self {
        Slab {
            thickness: report.or(reader::real(attr, "thickness"), default.thickness),
            reference: report.or(
                reader::code(attr, "reference", SlabReference::from_code),
                default.reference,
            ),
            offset: report.or(reader::real(attr, "offset"), default.offset),
            ..default.clone()
        }
    }
}
