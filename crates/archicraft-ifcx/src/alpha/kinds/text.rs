//! The parameters of a text (IFX-060; ANN-007): the drafting view, the
//! anchor at full precision, the anchor position by its code
//! ([`AnchorPos::code`]), the angle, the content, the style and the size
//! override, written only when the text has one. A text is not an IFCX node
//! (ANN-010, SRS 3.2), so they are the members `text` of its entry in its
//! storey's `archicraft::annotations` ([`super::annotations`]), not an
//! attribute of their own. The style refers to one of the project's text
//! styles, which `archicraft::project` holds as `textStyles`.
//!
//! The defaults slot of the text tool holds the anchor position, the angle,
//! the style and the size override; its view is a placeholder that a new
//! text replaces by the plan view of its storey, and its anchor and empty
//! content are filled in when the text is placed, so neither is written.

use archicraft_model::{AnchorPos, Text, TextStyleId};
use serde_json::{Map, Value, json};

use super::common::{drafting_view_schema, drafting_view_value, point2_value, read_drafting_view, read_text_style};
use super::{Fields, KindCodec, ReadContext};
use crate::alpha::reader::{self, Object, Report, about};
use crate::alpha::schema;
use crate::message::Message;

fn anchor_pos_schema() -> Value {
    schema::enumeration(AnchorPos::ALL.map(AnchorPos::code))
}

/// A size override from the member `key`: a positive number of paper
/// millimetres (ANN-007).
fn size_mm(map: &Object, key: &str) -> Result<f64, Message> {
    let size = reader::real(map, key)?;
    if size > 0.0 {
        Ok(size)
    } else {
        Err(about("ifcx-field-out-of-range", key))
    }
}

impl KindCodec for Text {
    const ATTRIBUTE: &'static str = "archicraft::text";

    const OPTIONAL_MATERIAL: bool = true;

    /// Formats 1 and 2 left texts out of the file (IFX-062).
    const SINCE_FORMAT: u32 = 3;

    const FIELDS: Fields = &[
        ("view", &["view"]),
        ("anchor", &["anchor"]),
        ("anchor_pos", &["anchorPos"]),
        ("angle", &["angle"]),
        ("content", &["content"]),
        ("style", &["style"]),
        ("size_mm", &["sizeMm"]),
    ];

    fn schema() -> Value {
        schema::object(&[
            ("view", drafting_view_schema()),
            ("anchor", schema::point2()),
            ("anchorPos", anchor_pos_schema()),
            ("angle", schema::angle()),
            ("content", schema::string()),
            ("style", schema::string()),
            ("sizeMm", schema::optional(schema::real())),
        ])
    }

    fn write(&self) -> Map<String, Value> {
        let mut map = Map::new();
        map.insert("view".into(), drafting_view_value(self.view));
        map.insert("anchor".into(), point2_value(self.anchor));
        map.insert("anchorPos".into(), json!(self.anchor_pos.code()));
        map.insert("angle".into(), json!(self.angle));
        map.insert("content".into(), json!(self.content));
        map.insert("style".into(), json!(self.style.to_string()));
        if let Some(size) = self.size_mm {
            map.insert("sizeMm".into(), json!(size));
        }
        map
    }

    /// Without a valid view, anchor and content there is no text; an
    /// unreadable anchor position or angle is repaired to the bottom left
    /// and 0, a style that is no identity or names no style of the project
    /// to the project's first style, and an unreadable or non-positive size
    /// override is left out, each with a report (PRJ-008).
    fn read(attr: &Object, context: &ReadContext<'_>, report: &mut Report<'_>) -> Result<Self, Message> {
        Ok(Text {
            view: reader::object(attr, "view").and_then(read_drafting_view)?,
            anchor: reader::field(attr, "anchor").and_then(reader::point)?,
            content: reader::text(attr, "content")?,
            anchor_pos: report.or(
                reader::code(attr, "anchorPos", AnchorPos::from_code),
                AnchorPos::default(),
            ),
            angle: report.or(reader::real(attr, "angle"), 0.0),
            style: read_text_style(attr, context, report),
            size_mm: report.optional(attr, "sizeMm", size_mm),
        })
    }

    fn slot_schema() -> Vec<(&'static str, Value)> {
        vec![
            ("anchorPos", anchor_pos_schema()),
            ("angle", schema::angle()),
            ("style", schema::string()),
            ("sizeMm", schema::optional(schema::real())),
        ]
    }

    fn write_slot(&self) -> Map<String, Value> {
        let mut map = Map::new();
        map.insert("anchorPos".into(), json!(self.anchor_pos.code()));
        map.insert("angle".into(), json!(self.angle));
        map.insert("style".into(), json!(self.style.to_string()));
        if let Some(size) = self.size_mm {
            map.insert("sizeMm".into(), json!(size));
        }
        map
    }

    /// The slot keeps `default`'s placeholder view, anchor and content.
    /// Whether its style exists is checked with the project's styles, once
    /// they are read (`reader::read_tool_defaults`).
    fn read_slot(attr: &Object, default: &Self, report: &mut Report<'_>) -> Self {
        Text {
            anchor_pos: report.or(
                reader::code(attr, "anchorPos", AnchorPos::from_code),
                default.anchor_pos,
            ),
            angle: report.or(reader::real(attr, "angle"), default.angle),
            style: report.or(
                reader::uuid(attr, "style").map(TextStyleId::from_uuid),
                default.style,
            ),
            size_mm: report.optional(attr, "sizeMm", size_mm),
            ..default.clone()
        }
    }
}
