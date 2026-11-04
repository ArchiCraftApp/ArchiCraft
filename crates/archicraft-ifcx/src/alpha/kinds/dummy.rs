//! `archicraft::dummy`: the parameters of the model's test kind (IFX-060;
//! ELM-006), behind the cargo feature `test-kinds` like the kind itself. It
//! proves that a kind's attribute is one codec module plus one line in the
//! codec list: the rectangle, the defaults slot's members (the vertical link
//! and the flags of the registry tests) and the material, which a dummy
//! without one leaves out (SPC-001, ANN-007).

use archicraft_model::kinds::{Dummy, VerticalLink};
use serde_json::{Map, Value, json};

use super::common::{point2_value, read_material, read_top_link, top_link_schema, top_link_value};
use super::{Fields, KindCodec, ReadContext};
use crate::alpha::reader::{self, Object, Report};
use crate::alpha::schema;
use crate::message::Message;

impl KindCodec for Dummy {
    const ATTRIBUTE: &'static str = "archicraft::dummy";

    const TEST_ONLY: bool = true;

    const OPTIONAL_MATERIAL: bool = true;

    const FIELDS: Fields = &[
        ("origin", &["origin"]),
        ("size", &["size"]),
        ("link", &["baseOffset", "top"]),
        ("material", &["material"]),
        ("parts", &["parts"]),
        ("panic_on_evaluate", &["panicOnEvaluate"]),
        ("site_level", &["siteLevel"]),
    ];

    fn schema() -> Value {
        let mut fields = vec![("origin", schema::point2()), ("size", schema::point2())];
        fields.extend(Self::slot_schema());
        fields.push(("material", schema::optional(schema::reference())));
        schema::object(&fields)
    }

    fn write(&self) -> Map<String, Value> {
        let mut map = Map::new();
        map.insert("origin".into(), point2_value(self.origin));
        map.insert("size".into(), point2_value(self.size));
        map.extend(self.write_slot());
        if let Some(material) = self.material {
            map.insert("material".into(), json!(material.to_string()));
        }
        map
    }

    /// Without a valid rectangle there is no element; every other member is
    /// repaired with a report to what [`Dummy::new`] gives: no base offset,
    /// a fixed top 1 m up, no flags. Without a material member the dummy
    /// has no material.
    fn read(attr: &Object, context: &ReadContext<'_>, report: &mut Report<'_>) -> Result<Self, Message> {
        let fallback = Dummy::without_material(
            reader::field(attr, "origin").and_then(reader::point)?,
            reader::field(attr, "size").and_then(reader::point)?,
        );
        let mut dummy = Self::read_slot(attr, &fallback, report);
        dummy.material = attr
            .contains_key("material")
            .then(|| read_material(attr, context, report));
        Ok(dummy)
    }

    fn slot_schema() -> Vec<(&'static str, Value)> {
        vec![
            ("baseOffset", schema::length()),
            ("top", top_link_schema()),
            ("parts", schema::boolean()),
            ("panicOnEvaluate", schema::boolean()),
            ("siteLevel", schema::boolean()),
        ]
    }

    fn write_slot(&self) -> Map<String, Value> {
        let mut map = Map::new();
        map.insert("baseOffset".into(), json!(self.link.base_offset));
        map.insert("top".into(), top_link_value(self.link.top));
        map.insert("parts".into(), json!(self.parts));
        map.insert("panicOnEvaluate".into(), json!(self.panic_on_evaluate));
        map.insert("siteLevel".into(), json!(self.site_level));
        map
    }

    fn read_slot(attr: &Object, default: &Self, report: &mut Report<'_>) -> Self {
        Dummy {
            link: VerticalLink {
                base_offset: report.or(reader::real(attr, "baseOffset"), default.link.base_offset),
                top: report.or(reader::object(attr, "top").and_then(read_top_link), default.link.top),
            },
            parts: report.or(reader::boolean(attr, "parts"), default.parts),
            panic_on_evaluate: report.or(reader::boolean(attr, "panicOnEvaluate"), default.panic_on_evaluate),
            site_level: report.or(reader::boolean(attr, "siteLevel"), default.site_level),
            ..default.clone()
        }
    }
}
