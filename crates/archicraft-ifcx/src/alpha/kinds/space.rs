//! `archicraft::space`: the placeholder codec of spaces (SPC-001). This
//! format version cannot store spaces yet; they are written as `IfcSpace`
//! with their parameters once their IFCX content exists (archicraft-awj).
//! Until then the codec is [`KindCodec::PLACEHOLDER`]: the writer leaves
//! spaces, the space tool's defaults slot and the space counter out of the
//! file and says so in one warning (PRJ-008), the file declares nothing for
//! the kind, and the reader takes the template's slot and a zero counter.
//! It has no members, so nothing here is ever written or read.

use archicraft_model::{KindTag, Space};
use serde_json::{Map, Value};

use super::{Fields, KindCodec, ReadContext};
use crate::alpha::reader::{Object, Report};
use crate::alpha::schema;
use crate::message::Message;

impl KindCodec for Space {
    const ATTRIBUTE: &'static str = "archicraft::space";

    const PLACEHOLDER: bool = true;

    const OPTIONAL_MATERIAL: bool = true;

    /// Nothing is stored yet (IFX-060 holds for spaces once archicraft-awj
    /// maps their fields).
    const FIELDS: Fields = &[];

    fn schema() -> Value {
        schema::object(&[])
    }

    fn write(&self) -> Map<String, Value> {
        Map::new()
    }

    /// A space element in a file cannot be read yet: it is skipped with
    /// the reason.
    fn read(_attr: &Object, _context: &ReadContext<'_>, _report: &mut Report<'_>) -> Result<Self, Message> {
        Err(Message::new("ifcx-kind-unreadable").kind("kind", KindTag::Space))
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
