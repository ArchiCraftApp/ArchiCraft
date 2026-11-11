//! The parameters of the model's body-less test kind (IFX-060; ANN-002),
//! behind the cargo feature `test-kinds` like the kind itself: the drafting
//! view, the plan point at full precision and the text. A note is not an
//! IFCX node (ANN-010), so they are the members `note` of its entry in its
//! storey's `archicraft::annotations` ([`super::annotations`]), not an
//! attribute of their own. The kind has no material and no tool, so its
//! defaults slot has no members of its own.

use archicraft_model::kinds::Note;
use serde_json::{Map, Value, json};

use super::common::{drafting_view_schema, drafting_view_value, point2_value, read_drafting_view};
use super::{Fields, KindCodec, ReadContext};
use crate::alpha::reader::{self, Object, Report};
use crate::alpha::schema;
use crate::message::Message;

impl KindCodec for Note {
    const ATTRIBUTE: &'static str = "archicraft::note";

    const TEST_ONLY: bool = true;

    const OPTIONAL_MATERIAL: bool = true;

    const FIELDS: Fields = &[("view", &["view"]), ("at", &["at"]), ("text", &["text"])];

    fn schema() -> Value {
        schema::object(&[
            ("view", drafting_view_schema()),
            ("at", schema::point2()),
            ("text", schema::string()),
        ])
    }

    fn write(&self) -> Map<String, Value> {
        let mut map = Map::new();
        map.insert("view".into(), drafting_view_value(self.view));
        map.insert("at".into(), point2_value(self.at));
        map.insert("text".into(), json!(self.text));
        map
    }

    /// Without a valid view and point there is no note; a missing or
    /// invalid text is repaired to an empty one, with a report.
    fn read(attr: &Object, _context: &ReadContext<'_>, report: &mut Report<'_>) -> Result<Self, Message> {
        Ok(Note {
            view: reader::object(attr, "view").and_then(read_drafting_view)?,
            at: reader::field(attr, "at").and_then(reader::point)?,
            text: report.or(reader::text(attr, "text"), String::new()),
        })
    }

    fn slot_schema() -> Vec<(&'static str, Value)> {
        Vec::new()
    }

    fn write_slot(&self) -> Map<String, Value> {
        Map::new()
    }

    fn read_slot(_attr: &Object, default: &Self, _context: &ReadContext<'_>, _report: &mut Report<'_>) -> Self {
        default.clone()
    }
}
