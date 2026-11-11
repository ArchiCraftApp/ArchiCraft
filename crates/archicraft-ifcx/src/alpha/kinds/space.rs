//! `archicraft::space`: the parameters of a space (IFX-060; SPC-001): the
//! boundary rings, the defaults slot's members (room name, number, base
//! offset and top link), where the boundary comes from and the stamp offset.
//!
//! The space's node is an `IfcSpace` (Table 5-1) whose
//! `bsi::ifc::prop::Name` shows the room name ([`KindCodec::node_name`]),
//! with a `Body` child without a material (IFX-017). The number stays here:
//! the pinned `prop@v5a` declares no `LongName`, so IFC's convention of the
//! number in `Name` and the room name in `LongName` waits for a standard
//! attribute, as predefined types do (IFX-024).

use archicraft_model::glam::DVec2;
use archicraft_model::kinds::VerticalLink;
use archicraft_model::{BoundarySource, Space, TopLink};
use serde_json::{Map, Value, json};

use super::common::{
    outline_schema, point2_value, read_outline, read_top_link, top_link_schema, top_link_value, write_outline,
};
use super::{Fields, KindCodec, ReadContext};
use crate::alpha::reader::{self, Object, Report, about};
use crate::alpha::schema;
use crate::message::Message;

/// Where the boundary comes from: `{"type": "drawn"}`, or `{"type":
/// "detected", "seed": [x, y], "autoUpdate": true}` for a region detected
/// around the seed (INP-018, SPC-002).
fn source_schema() -> Value {
    schema::object(&[
        ("type", schema::enumeration(["drawn", "detected"])),
        ("seed", schema::optional(schema::point2())),
        ("autoUpdate", schema::optional(schema::boolean())),
    ])
}

fn source_value(source: BoundarySource) -> Value {
    match source {
        BoundarySource::Drawn => json!({"type": "drawn"}),
        BoundarySource::Detected { seed, auto_update } => {
            json!({"type": "detected", "seed": point2_value(seed), "autoUpdate": auto_update})
        }
    }
}

fn read_source(map: &Object) -> Result<BoundarySource, Message> {
    match reader::text(map, "type")?.as_str() {
        "drawn" => Ok(BoundarySource::Drawn),
        "detected" => Ok(BoundarySource::Detected {
            seed: reader::field(map, "seed").and_then(reader::point)?,
            auto_update: reader::boolean(map, "autoUpdate")?,
        }),
        other => Err(about("ifcx-field-unknown-value", "type").text("value", other)),
    }
}

impl KindCodec for Space {
    const ATTRIBUTE: &'static str = "archicraft::space";

    const OPTIONAL_MATERIAL: bool = true;

    /// Format 1 left spaces out of the file (IFX-062).
    const SINCE_FORMAT: u32 = 2;

    const FIELDS: Fields = &[
        ("long_name", &["longName"]),
        ("number", &["number"]),
        // Ring 0 is the outline member, the other rings the holes member.
        ("boundary", &["outline", "holes"]),
        ("link", &["baseOffset", "top"]),
        ("source", &["source"]),
        ("stamp_offset", &["stampOffset"]),
    ];

    fn schema() -> Value {
        let mut fields = outline_schema().to_vec();
        fields.extend(Self::slot_schema());
        fields.push(("source", source_schema()));
        fields.push(("stampOffset", schema::point2()));
        schema::object(&fields)
    }

    fn write(&self) -> Map<String, Value> {
        let mut map = Map::new();
        write_outline(&mut map, &self.boundary);
        map.extend(self.write_slot());
        map.insert("source".into(), source_value(self.source));
        map.insert("stampOffset".into(), point2_value(self.stamp_offset));
        map
    }

    /// The room name, else the number (SPC-001), so that a viewer's tree
    /// shows the room; without either, the element's own name (`SP-001`).
    fn node_name(&self) -> Option<&str> {
        [&self.long_name, &self.number]
            .into_iter()
            .find(|name| !name.is_empty())
            .map(String::as_str)
    }

    /// Without a valid outline there is no space; invalid holes are left
    /// out and every other member is repaired with a report: no room name
    /// or number, the base at the storey, the top at the next storey, a
    /// drawn boundary and the stamp at the boundary's interior point. The
    /// rings are kept as read; validation orients them.
    fn read(attr: &Object, context: &ReadContext<'_>, report: &mut Report<'_>) -> Result<Self, Message> {
        let fallback = Space {
            long_name: String::new(),
            number: String::new(),
            boundary: read_outline(attr, report)?,
            link: VerticalLink {
                base_offset: 0.0,
                top: TopLink::Relative {
                    storeys: 1,
                    offset: 0.0,
                },
            },
            source: BoundarySource::Drawn,
            stamp_offset: DVec2::ZERO,
        };
        let mut space = Self::read_slot(attr, &fallback, context, report);
        space.source = report.or(reader::object(attr, "source").and_then(read_source), space.source);
        space.stamp_offset = report.or(
            reader::field(attr, "stampOffset").and_then(reader::point),
            space.stamp_offset,
        );
        Ok(space)
    }

    fn slot_schema() -> Vec<(&'static str, Value)> {
        vec![
            ("longName", schema::string()),
            ("number", schema::string()),
            ("baseOffset", schema::length()),
            ("top", top_link_schema()),
        ]
    }

    fn write_slot(&self) -> Map<String, Value> {
        let mut map = Map::new();
        map.insert("longName".into(), json!(self.long_name));
        map.insert("number".into(), json!(self.number));
        map.insert("baseOffset".into(), json!(self.link.base_offset));
        map.insert("top".into(), top_link_value(self.link.top));
        map
    }

    /// The slot keeps the default name, with `{n}` for the space's number
    /// in the name counter (ELM-019), and the default number; the source
    /// and the stamp offset belong to a placed space, like the boundary.
    fn read_slot(attr: &Object, default: &Self, _context: &ReadContext<'_>, report: &mut Report<'_>) -> Self {
        Space {
            long_name: report.or(reader::text(attr, "longName"), default.long_name.clone()),
            number: report.or(reader::text(attr, "number"), default.number.clone()),
            link: VerticalLink {
                base_offset: report.or(reader::real(attr, "baseOffset"), default.link.base_offset),
                top: report.or(reader::object(attr, "top").and_then(read_top_link), default.link.top),
            },
            ..default.clone()
        }
    }
}
