//! `archicraft::column`: the placeholder codec of columns (COL-001). This
//! format version cannot store columns yet; they become `IfcColumn` nodes
//! with a body, an axis and their parameters once their IFCX content exists
//! (archicraft-i4gl.3, after the profiles of archicraft-jrtb). Until then
//! the codec is [`KindCodec::PLACEHOLDER`]: the writer leaves columns, the
//! column tool's defaults slot and the column counter out of the file and
//! says so in one warning (PRJ-008), the file declares nothing for the kind,
//! and the reader takes the template's slot and a zero counter. It has no
//! members, so nothing here is ever written or read.

use archicraft_model::KindTag;
use archicraft_model::kinds::Column;
use serde_json::{Map, Value};

use super::{Fields, KindCodec, ReadContext};
use crate::alpha::reader::{Object, Report};
use crate::alpha::schema;
use crate::message::Message;

impl KindCodec for Column {
    const ATTRIBUTE: &'static str = "archicraft::column";

    const PLACEHOLDER: bool = true;

    /// Nothing is stored yet (IFX-060 holds for columns once
    /// archicraft-i4gl.3 maps their fields).
    const FIELDS: Fields = &[];

    fn schema() -> Value {
        schema::object(&[])
    }

    fn write(&self) -> Map<String, Value> {
        Map::new()
    }

    /// A column node in a file cannot be read yet: it is skipped with the
    /// reason.
    fn read(_attr: &Object, _context: &ReadContext<'_>, _report: &mut Report<'_>) -> Result<Self, Message> {
        Err(Message::new("ifcx-kind-unreadable").kind("kind", KindTag::Column))
    }

    fn slot_schema() -> Vec<(&'static str, Value)> {
        Vec::new()
    }

    fn write_slot(&self) -> Map<String, Value> {
        Map::new()
    }

    fn read_slot(_attr: &Object, default: &Self, _report: &mut Report<'_>) -> Self {
        default.clone()
    }
}
