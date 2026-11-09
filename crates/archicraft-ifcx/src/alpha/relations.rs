//! `archicraft::relations`: the explicit relations between elements (SRS
//! 3.3, IFX-060, IFX-061).
//!
//! Each explicit relation is stored on the node of its first end, the
//! dependent element, as one entry of an array: `{ kind, target }`, the
//! variant's code and the identity of the second end. An element that is not
//! a node keeps the same array as `relations` in its annotation entry
//! ([`super::kinds::annotations`]). The entry is an open object, so the epic
//! that owns a relation can add its parameters (junction settings, for
//! example) as optional members without a format change.
//! Derived storey links are not stored here: the kind codecs write them with
//! the element's parameters.
//!
//! Like a kind's attribute, the declaration follows from the project
//! ([`present`]): a file declares `archicraft::relations` only when the
//! project has explicit relations, so files of projects without them do not
//! change. Reading is lenient (IFX-083): an entry that cannot be read, names
//! an unknown kind, targets no element of the file or repeats another is
//! dropped and reported against its element (PRJ-008). A relation is
//! recorded once both its ends are read: the nodes' relations after every
//! element node, those that name an annotation entry and the entries' own
//! after every entry.

use archicraft_model::relations::EXPLICIT_CODES;
use archicraft_model::{ElementId, Project, Relation, RelationStore};
use indexmap::IndexMap;
use serde_json::{Value, json};

use super::reader::{self, Report, about};
use super::schema;
use crate::container::{AttributeValue, IfcxFile};
use crate::message::Message;
use crate::validate::{FlatNode, Problem};

pub const ATTRIBUTE: &str = "archicraft::relations";

/// True if the project stores explicit relations, so that the file must
/// declare the attribute (IFX-061).
pub fn present(project: &Project) -> bool {
    !project.relations.is_empty()
}

/// True if a node of `file` carries the attribute: what [`present`] is for
/// a project, for a migrated file that declares its schemas again (IFX-062).
pub(crate) fn present_in(file: &IfcxFile) -> bool {
    file.data.iter().any(|node| node.attributes.contains_key(ATTRIBUTE))
}

/// The value description of the attribute: a list of entries with the
/// relation's kind and the identity of its second end.
pub(crate) fn schema() -> Value {
    schema::array(schema::object(&[
        ("kind", schema::enumeration(EXPLICIT_CODES.iter().copied())),
        ("target", schema::reference()),
    ]))
}

/// The attribute for the node of `element`: one entry per explicit relation
/// whose first end it is, in store order; `None` when it has none, so the
/// node stays as it was.
pub(crate) fn write(relations: &RelationStore, element: ElementId) -> Option<Value> {
    let entries: Vec<Value> = relations
        .starting_at(element)
        .filter_map(|relation| {
            let (_, target) = relation.ends()?;
            Some(json!({"kind": relation.code(), "target": target.to_string()}))
        })
        .collect();
    (!entries.is_empty()).then(|| Value::Array(entries))
}

/// An explicit relation read from the file, held until its second end is
/// known: a relation on a node whose target is an annotation entry waits for
/// the entries ([`super::kinds::annotations`]), and the relations of the
/// entries for every entry. It keeps the node and the record to report it
/// against.
pub(crate) struct Pending {
    path: String,
    record: Message,
    relation: Relation,
    kind: String,
    target: ElementId,
}

/// The relations of `value`, the attribute of an element node or the
/// `relations` of an annotation entry (its member `key`), whose first end is
/// `first`. An entry that cannot be read or names an unknown kind is dropped
/// and reported at `path` against `record` as a repair.
pub(crate) fn parse(
    value: &Value,
    key: &str,
    first: ElementId,
    path: &str,
    record: Message,
    problems: &mut Vec<Problem>,
) -> Vec<Pending> {
    let mut report = Report::new(problems, path, record.clone());
    let Some(entries) = value.as_array() else {
        report.repaired(Message::new("ifcx-left-out").nested("reason", about("ifcx-field-not-list", key)));
        return Vec::new();
    };
    let mut pending = Vec::new();
    for entry in entries {
        let Some(map) = entry.as_object() else {
            report.repaired(Message::new("ifcx-left-out").nested("reason", Message::new("ifcx-not-object")));
            continue;
        };
        let parsed = reader::text(map, "kind").and_then(|kind| {
            let target = ElementId::from_uuid(reader::uuid(map, "target")?);
            let relation = Relation::from_code(&kind, first, target)
                .ok_or_else(|| about("ifcx-field-unknown-value", "kind").text("value", kind.clone()))?;
            Ok((relation, kind, target))
        });
        match parsed {
            Ok((relation, kind, target)) => pending.push(Pending {
                path: path.to_owned(),
                record: record.clone(),
                relation,
                kind,
                target,
            }),
            Err(e) => report.repaired(Message::new("ifcx-left-out").nested("reason", e)),
        }
    }
    pending
}

/// Restores the relations of the element nodes read at `elements` (their
/// node path and identity, in file order) into `project`, once every element
/// node is known. A relation whose target is no element node is returned, to
/// be [`settle`]d once the annotation entries are read.
pub(crate) fn read<A: AttributeValue>(
    project: &mut Project,
    nodes: &IndexMap<String, FlatNode<A>>,
    elements: &[(String, ElementId)],
    problems: &mut Vec<Problem>,
) -> Vec<Pending> {
    let mut waiting = Vec::new();
    for (path, first) in elements {
        let Some(node) = nodes.get(path.as_str()) else {
            continue;
        };
        let Some(value) = reader::attribute(node, ATTRIBUTE) else {
            continue;
        };
        let pending = parse(
            &value,
            ATTRIBUTE,
            *first,
            path,
            reader::element_record(path, node),
            problems,
        );
        waiting.extend(insert(project, pending, problems));
    }
    waiting
}

/// Records the `pending` relations once every element is read. One whose
/// target is still no element is dropped and reported against its element as
/// a repair.
pub(crate) fn settle(project: &mut Project, pending: impl IntoIterator<Item = Pending>, problems: &mut Vec<Problem>) {
    for unknown in insert(project, pending, problems) {
        Report::new(problems, &unknown.path, unknown.record).repaired(
            Message::new("ifcx-relation-target-missing")
                .text("relation", unknown.kind)
                .text("text", unknown.target.to_string()),
        );
    }
}

/// Records each pending relation whose target is an element of `project`,
/// reporting one listed twice, and returns the others.
fn insert(
    project: &mut Project,
    pending: impl IntoIterator<Item = Pending>,
    problems: &mut Vec<Problem>,
) -> Vec<Pending> {
    let mut unknown = Vec::new();
    for p in pending {
        if project.element(p.target).is_none() {
            unknown.push(p);
        } else if !project.relations.insert(p.relation) {
            Report::new(problems, &p.path, p.record)
                .repaired(Message::new("ifcx-relation-listed-twice").text("relation", p.kind));
        }
    }
    unknown
}
