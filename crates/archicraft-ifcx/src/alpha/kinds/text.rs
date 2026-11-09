//! `archicraft::text`: the placeholder codec of texts (ANN-007). This
//! format version cannot store texts yet; they become entries of
//! `archicraft::annotations` with their parameters once their IFCX content
//! exists (archicraft-u4vm.2.2). Until then the codec is
//! [`KindCodec::PLACEHOLDER`]: the writer leaves texts, the text tool's
//! defaults slot and the text counter out of the file and says so in one
//! warning (PRJ-008), the file declares nothing for the kind, and the reader
//! takes the template's slot and a zero counter. It has no members, so
//! nothing here is ever written or read.

use archicraft_model::{KindTag, Text};
use serde_json::{Map, Value};

use super::{Fields, KindCodec, ReadContext};
use crate::alpha::reader::{Object, Report};
use crate::alpha::schema;
use crate::message::Message;

impl KindCodec for Text {
    const ATTRIBUTE: &'static str = "archicraft::text";

    const PLACEHOLDER: bool = true;

    const OPTIONAL_MATERIAL: bool = true;

    /// Nothing is stored yet (IFX-060 holds for texts once
    /// archicraft-u4vm.2.2 maps their fields).
    const FIELDS: Fields = &[];

    fn schema() -> Value {
        schema::object(&[])
    }

    fn write(&self) -> Map<String, Value> {
        Map::new()
    }

    /// A text entry in a file cannot be read yet: it is skipped with the
    /// reason.
    fn read(_attr: &Object, _context: &ReadContext<'_>, _report: &mut Report<'_>) -> Result<Self, Message> {
        Err(Message::new("ifcx-kind-unreadable").kind("kind", KindTag::Text))
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
