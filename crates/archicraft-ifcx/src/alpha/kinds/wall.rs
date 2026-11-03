//! `archicraft::wall`: the parameters of a wall (IFX-060; WAL-001, WAL-003,
//! ELM-006): the ends, the defaults slot's members and the material.

use archicraft_model::{ReferenceLine, TopLink, Wall};
use serde_json::{Map, Value, json};

use super::common::{
    point2_value, read_material, read_reference_line, read_top_link, reference_line_schema, top_link_schema,
    top_link_value, write_reference_line,
};
use super::{KindCodec, ReadContext};
use crate::alpha::reader::{self, Object, Report};
use crate::alpha::schema;
use crate::message::Message;

impl KindCodec for Wall {
    const ATTRIBUTE: &'static str = "archicraft::wall";

    fn schema() -> Value {
        let mut fields = vec![("start", schema::point2()), ("end", schema::point2())];
        fields.extend(Self::slot_schema());
        fields.push(("material", schema::reference()));
        schema::object(&fields)
    }

    fn write(&self) -> Map<String, Value> {
        let mut map = Map::new();
        map.insert("start".into(), point2_value(self.start));
        map.insert("end".into(), point2_value(self.end));
        map.extend(self.write_slot());
        map.insert("material".into(), json!(self.material.to_string()));
        map
    }

    /// Without valid ends there is no wall; every other member is repaired
    /// with a report: 0.25 m thick, the reference line outside, no base
    /// offset, the top on the storey above.
    fn read(attr: &Object, context: &ReadContext<'_>, report: &mut Report<'_>) -> Result<Self, Message> {
        let fallback = Wall {
            start: reader::field(attr, "start").and_then(reader::point)?,
            end: reader::field(attr, "end").and_then(reader::point)?,
            thickness: 0.25,
            reference_line: ReferenceLine::Outside,
            base_offset: 0.0,
            top: TopLink::Relative {
                storeys: 1,
                offset: 0.0,
            },
            material: context.project.materials[0].id,
        };
        let mut wall = Self::read_slot(attr, &fallback, report);
        wall.material = read_material(attr, context, report);
        Ok(wall)
    }

    fn slot_schema() -> Vec<(&'static str, Value)> {
        vec![
            ("thickness", schema::length()),
            ("referenceLine", reference_line_schema()),
            ("referenceOffset", schema::optional(schema::length())),
            ("baseOffset", schema::length()),
            ("top", top_link_schema()),
        ]
    }

    fn write_slot(&self) -> Map<String, Value> {
        let mut map = Map::new();
        map.insert("thickness".into(), json!(self.thickness));
        write_reference_line(&mut map, self.reference_line);
        map.insert("baseOffset".into(), json!(self.base_offset));
        map.insert("top".into(), top_link_value(self.top));
        map
    }

    fn read_slot(attr: &Object, default: &Self, report: &mut Report<'_>) -> Self {
        Wall {
            thickness: report.or(reader::real(attr, "thickness"), default.thickness),
            reference_line: report.or(read_reference_line(attr), default.reference_line),
            base_offset: report.or(reader::real(attr, "baseOffset"), default.base_offset),
            top: report.or(reader::object(attr, "top").and_then(read_top_link), default.top),
            ..default.clone()
        }
    }
}
