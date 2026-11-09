//! `archicraft::annotations`: the elements of the kinds that are not IFCX
//! nodes (ANN-010, SRS 3.2, IFX-060).
//!
//! Texts, markers and dimensions are stored in the ArchiCraft namespace only
//! ([`KindDescriptor::ifcx_node`](archicraft_model::kinds::KindDescriptor::ifcx_node)).
//! Their elements are written per home storey, in model order, as the
//! entries of one `archicraft::annotations` attribute on the storey node.
//! An entry is the element's envelope, its identity (`id`), `name`, IFC
//! class (`ifcClass`) and the common data that `archicraft::element` holds
//! for a node ([`writer::element_data`]), followed by the members the kind's
//! codec writes, under the kind code and at full precision, and by its
//! explicit relations under `relations`, as [`relations`] writes them for a
//! node. Such an element gets no node, so no body and no sub-node ids, and
//! the standard content never depends on it (IFX-063, IFX-068). An entry's
//! place among the storey's element nodes is not kept: on reading, the
//! entries follow the nodes.
//!
//! Like a kind's attribute, the declaration follows from the project: a file
//! declares `archicraft::annotations` only when it stores a kind whose
//! elements are entries ([`entry_kind`]), and then nests the members of those
//! kinds' codecs in the entry instead of declaring an attribute of their
//! own. Files of projects without such kinds do not change.
//!
//! Reading is lenient (IFX-083) and comes after every element node and the
//! nodes' relations, so that a binding read with the entry, such as a
//! dimension's, finds its target in the [`ReadContext`]. An entry without an
//! id, of an unknown kind or with invalid data is skipped, and reported at
//! the storey node against the entry, by its name if it has one (PRJ-008);
//! the other entries and the rest of the file load. The entries' relations
//! are recorded once every entry is read. An entry of a kind whose elements
//! are nodes is read like any other entry, and the next save writes it as a
//! node.
//!
//! [`ReadContext`]: super::ReadContext

use std::borrow::Cow;

use archicraft_model::relations::RelationStore;
use archicraft_model::{Element, ElementId, KindTag, Project};
use indexmap::IndexMap;
use serde_json::{Map, Value, json};

use super::{CODECS, Codec};
use crate::alpha::reader::{self, ElementRecord, Report, StoreyEntry, about};
use crate::alpha::relations::{self, Pending};
use crate::alpha::{schema, writer};
use crate::container::AttributeValue;
use crate::message::Message;
use crate::validate::{FlatNode, Problem};

pub const ATTRIBUTE: &str = "archicraft::annotations";

/// The member of an entry that holds its explicit relations.
const RELATIONS: &str = "relations";

/// Whether the elements of `kind` are entries of the attribute rather than
/// IFCX nodes (ANN-010).
pub(crate) fn entry_kind(kind: KindTag) -> bool {
    !kind.descriptor().ifcx_node
}

/// The value description of the attribute (IFX-061): a list of entries,
/// each the envelope with, optionally, the members of one of `codecs` under
/// its kind code, and the relations.
pub(crate) fn schema(codecs: &[&dyn Codec]) -> Value {
    let mut fields = vec![
        ("id", schema::string()),
        ("name", schema::string()),
        ("ifcClass", schema::string()),
    ];
    fields.extend(schema::element_data(codecs.iter().map(|c| c.tag().code())));
    fields.extend(codecs.iter().map(|c| (c.tag().code(), schema::optional(c.schema()))));
    fields.push((RELATIONS, schema::optional(relations::schema())));
    schema::array(schema::object(&fields))
}

/// The attribute for a storey node whose elements of the kinds that are not
/// nodes are `elements`, in model order; `None` without such elements, so
/// the node stays as it was. An element whose kind has no codec is written
/// without its parameters, with a warning, and skipped on reading, like a
/// node of such a kind.
pub(crate) fn write(elements: &[&Element], store: &RelationStore, warnings: &mut Vec<Problem>) -> Option<Value> {
    if elements.is_empty() {
        return None;
    }
    Some(elements.iter().map(|e| entry(e, store, warnings)).collect())
}

/// One element's entry: the envelope, the codec's members under the kind
/// code and the relations whose first end it is.
fn entry(e: &Element, store: &RelationStore, warnings: &mut Vec<Problem>) -> Value {
    let mut map = Map::new();
    map.insert("id".into(), json!(e.id.to_string()));
    map.insert("name".into(), json!(e.name));
    map.insert("ifcClass".into(), json!(e.ifc_class.code()));
    map.extend(writer::element_data(e));
    match CODECS.iter().find_map(|c| c.write(&e.kind)) {
        Some(members) => {
            map.insert(e.tag().code().into(), Value::Object(members));
        }
        None => warnings.push(Problem::warning(
            Some(&e.id.to_string()),
            Message::new("ifcx-kind-not-storable")
                .text("name", e.name.clone())
                .kind("kind", e.tag()),
        )),
    }
    if let Some(relations) = relations::write(store, e.id) {
        map.insert(RELATIONS.into(), relations);
    }
    Value::Object(map)
}

/// Names the entry at `index` in reports: by its name, else by its id, else
/// by its place in the list.
fn entry_record(entry: &Value, index: usize) -> Message {
    let named = ["name", "id"]
        .iter()
        .find_map(|key| entry.get(key)?.as_str().filter(|text| !text.is_empty()));
    match named {
        Some(name) => Message::new("ifcx-record-element").text("name", name),
        None => Message::new("ifcx-record-annotation").count("index", index),
    }
}

/// Reads the entries of the storeys read at `storeys` into `project`, once
/// every element node and the nodes' relations are in it, and returns the
/// entries' relations, to be settled once every entry is read. Each storey's
/// attribute is parsed as one tree while its entries are read and dropped
/// afterwards (NFR-SEC-003).
pub(crate) fn read<A: AttributeValue>(
    project: &mut Project,
    nodes: &IndexMap<String, FlatNode<A>>,
    storeys: &[StoreyEntry],
    problems: &mut Vec<Problem>,
) -> Vec<Pending> {
    let mut pending = Vec::new();
    for StoreyEntry { path, storey, .. } in storeys {
        let Some(value) = nodes.get(path.as_str()).and_then(|n| reader::attribute(n, ATTRIBUTE)) else {
            continue;
        };
        let Some(entries) = value.as_array() else {
            let record = Message::new("ifcx-record-storey").text("name", storey.name.clone());
            Report::new(problems, path, record)
                .repaired(Message::new("ifcx-left-out").nested("reason", about("ifcx-field-not-list", ATTRIBUTE)));
            continue;
        };
        let lost = project.storeys.get(storey.id).is_none();
        for (index, entry) in entries.iter().enumerate() {
            let record = entry_record(entry, index);
            let mut report = Report::new(problems, path, record.clone());
            if lost {
                report.skipped(Message::new("ifcx-storey-lost").text("name", storey.name.clone()));
                continue;
            }
            let Some(element) = read_entry(project, entry, storey.id, &mut report) else {
                continue;
            };
            if project.elements.contains_key(&element.id) {
                problems.push(Problem::error(
                    Some(path),
                    Message::new("ifcx-duplicate").text("name", element.name.clone()),
                ));
                continue;
            }
            if let Some(value) = entry.get(RELATIONS) {
                pending.extend(relations::parse(value, RELATIONS, element.id, path, record, problems));
            }
            project.elements.insert(element.id, element);
        }
    }
    pending
}

/// The element of one entry on `storey`. Without an identity or a kind this
/// version knows, there is no element; the rest is read as for a node.
fn read_entry(
    project: &Project,
    entry: &Value,
    storey: archicraft_model::StoreyId,
    report: &mut Report<'_>,
) -> Option<Element> {
    let Some(map) = entry.as_object() else {
        report.skipped(Message::new("ifcx-not-object"));
        return None;
    };
    let id = match reader::uuid(map, "id") {
        Ok(id) => ElementId::from_uuid(id),
        Err(e) => {
            report.skipped(e);
            return None;
        }
    };
    let kind = match reader::code(map, "kind", KindTag::from_code) {
        Ok(kind) => kind,
        Err(e) => {
            report.skipped(e);
            return None;
        }
    };
    let text = |key| map.get(key).and_then(Value::as_str).map(str::to_owned);
    let record = ElementRecord {
        kind,
        id,
        name: text("name"),
        class: text("ifcClass"),
        storey,
        common: map,
    };
    reader::read_element_record(
        project,
        record,
        |codec| map.get(codec.tag().code()).map(Cow::Borrowed),
        report,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The kind code names an entry's member, so no kind code may be one of
    /// the envelope's members.
    #[test]
    fn no_kind_code_is_a_member_of_the_envelope() {
        let declared = schema(&[]);
        let envelope = declared["arrayRestrictions"]["value"]["objectRestrictions"]["values"]
            .as_object()
            .unwrap();
        for kind in KindTag::ALL {
            assert!(!envelope.contains_key(kind.code()), "{kind:?}");
        }
    }
}
