//! Explicit relations in `archicraft::relations` (SRS 3.3, IFX-060): written
//! on the first end's node, declared only when a project has relations, and
//! read back leniently with every dropped entry reported (PRJ-008).

#![allow(clippy::unwrap_used)]

mod common;

use archicraft_ifcx::validate::{self, Severity};
use archicraft_model::{ElementId, Project, Relation, samples};
use serde_json::{Value, json};

use common::{edit, node_index, save};

const ATTRIBUTE: &str = "archicraft::relations";

/// The element named `name`.
fn id_of(project: &Project, name: &str) -> ElementId {
    project.elements.values().find(|e| e.name == name).unwrap().id
}

/// The walls-and-slab sample with a Host relation from W-001 to W-002,
/// inserted straight into the store (the relation commands are a later
/// slice), and the two ids.
fn sample_with_a_host_relation() -> (Project, ElementId, ElementId) {
    let mut project = samples::storey_with_walls_and_slab();
    let (hosted, host) = (id_of(&project, "W-001"), id_of(&project, "W-002"));
    assert!(project.relations.insert(Relation::Host { hosted, host }));
    (project, hosted, host)
}

/// The `archicraft::` declarations of a saved file, in order.
fn own_declarations(bytes: &[u8]) -> Vec<String> {
    let file: Value = serde_json::from_slice(bytes).unwrap();
    file["schemas"]
        .as_object()
        .unwrap()
        .keys()
        .filter(|k| k.starts_with("archicraft::"))
        .cloned()
        .collect()
}

/// IFX-060, IFX-064: a project with an explicit relation writes it as one
/// `{kind, target}` entry of `archicraft::relations` on the first end's
/// node, declares the attribute, validates, loads equal and writes the same
/// bytes again.
#[test]
fn an_explicit_relation_round_trips_through_archicraft_relations() {
    let (project, hosted, host) = sample_with_a_host_relation();
    let bytes = save(&project);
    let file: Value = serde_json::from_slice(&bytes).unwrap();
    let node = &file["data"][node_index(&file, "W-001")];
    assert_eq!(
        node["attributes"][ATTRIBUTE],
        json!([{"kind": "host", "target": host.to_string()}])
    );
    let other = &file["data"][node_index(&file, "W-002")];
    assert!(
        other["attributes"].get(ATTRIBUTE).is_none(),
        "the second end carries no copy: {other}"
    );
    assert!(own_declarations(&bytes).contains(&ATTRIBUTE.to_owned()));
    let parsed = validate::parse(&bytes).unwrap();
    let problems = validate::validate(&parsed);
    assert!(problems.is_empty(), "{problems:#?}");

    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    let mut expected = project.clone();
    expected.revision = 0;
    expected.data_version = 7;
    assert_eq!(loaded.project, expected);
    assert_eq!(loaded.project.relations.len(), 1);
    assert!(loaded.project.relations.contains(Relation::Host { hosted, host }.id()));
    assert_eq!(save(&loaded.project), bytes, "the same bytes again (IFX-007)");
}

/// IFX-061: the declaration follows from the project, like a kind's, so a
/// project without relations writes the same file as before.
#[test]
fn a_project_without_relations_declares_no_relations_attribute() {
    let project = samples::storey_with_walls_and_slab();
    assert!(project.relations.is_empty());
    let bytes = save(&project);
    assert!(!own_declarations(&bytes).contains(&ATTRIBUTE.to_owned()));
    assert!(!String::from_utf8(bytes).unwrap().contains(ATTRIBUTE));
}

/// PRJ-008, NFR-REL-004: a relation whose target names no element of the
/// file is dropped with one warning against the element node, and nothing
/// else is lost.
#[test]
fn a_relation_to_a_missing_element_is_dropped_with_one_warning() {
    let (project, _, _) = sample_with_a_host_relation();
    let stranger = ElementId::new();
    let bytes = edit(&project, |v| {
        let i = node_index(v, "W-001");
        v["data"][i]["attributes"][ATTRIBUTE][0]["target"] = json!(stranger.to_string());
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.project.relations.is_empty());
    assert_eq!(loaded.project.elements.len(), project.elements.len());
    assert_eq!(loaded.problems.len(), 1, "{:#?}", loaded.problems);
    let problem = &loaded.problems[0];
    assert_eq!(problem.severity, Severity::Warning);
    assert_eq!(
        problem.path.as_deref(),
        Some(id_of(&project, "W-001").to_string().as_str())
    );
    let text = problem.message.to_string();
    assert!(
        text.starts_with(r#"ifcx-repaired(record=ifcx-record-element(name="W-001")"#)
            && text.contains("ifcx-relation-target-missing")
            && text.contains(&stranger.to_string()),
        "{text}"
    );
    assert!(loaded.project.integrity_problems().is_empty());
}

/// PRJ-008, IFX-083: an entry whose kind this version does not know is
/// dropped with a repair report (the validator also reports the value
/// against the attribute), and an entry whose target is no identity too;
/// the other entries of the node are kept.
#[test]
fn a_relation_entry_with_an_unknown_kind_is_dropped_with_a_report() {
    let (project, hosted, host) = sample_with_a_host_relation();
    let trimmed_by = id_of(&project, "W-003");
    let bytes = edit(&project, |v| {
        let i = node_index(v, "W-001");
        v["data"][i]["attributes"][ATTRIBUTE] = json!([
            {"kind": "glue", "target": host.to_string()},
            {"kind": "trim", "target": "not-a-uuid"},
            {"kind": "trim", "target": trimmed_by.to_string()},
        ]);
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let kept: Vec<Relation> = loaded.project.relations.iter().map(|(_, r)| *r).collect();
    assert_eq!(
        kept,
        [Relation::Trim {
            target: hosted,
            by: trimmed_by
        }]
    );
    assert_eq!(loaded.project.elements.len(), project.elements.len());
    let repairs: Vec<String> = loaded
        .problems
        .iter()
        .filter(|p| p.severity == Severity::Warning)
        .map(|p| p.message.to_string())
        .collect();
    assert_eq!(repairs.len(), 2, "{repairs:#?}");
    assert!(
        repairs[0].starts_with(r#"ifcx-repaired(record=ifcx-record-element(name="W-001")"#)
            && repairs[0].contains(r#"ifcx-field-unknown-value(key="kind""#)
            && repairs[0].contains("glue"),
        "{}",
        repairs[0]
    );
    assert!(
        repairs[1].starts_with(r#"ifcx-repaired(record=ifcx-record-element(name="W-001")"#)
            && repairs[1].contains(r#"ifcx-field-not-uuid(key="target")"#),
        "{}",
        repairs[1]
    );
    let errors: Vec<String> = loaded
        .problems
        .iter()
        .filter(|p| p.severity == Severity::Error)
        .map(|p| p.message.to_string())
        .collect();
    assert_eq!(errors.len(), 1, "the validator's report on the enum value: {errors:#?}");
    assert!(
        errors[0].starts_with(r#"ifcx-attribute-invalid(id="archicraft::relations""#),
        "{errors:#?}"
    );
}

/// PRJ-008: a relation listed twice is read once and the second entry is
/// reported, so a file cannot grow the store silently.
#[test]
fn a_relation_listed_twice_is_read_once_and_reported() {
    let (project, hosted, host) = sample_with_a_host_relation();
    let bytes = edit(&project, |v| {
        let i = node_index(v, "W-001");
        let entry = v["data"][i]["attributes"][ATTRIBUTE][0].clone();
        v["data"][i]["attributes"][ATTRIBUTE] = json!([entry.clone(), entry]);
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert_eq!(loaded.project.relations.len(), 1);
    assert!(loaded.project.relations.contains(Relation::Host { hosted, host }.id()));
    assert_eq!(loaded.problems.len(), 1, "{:#?}", loaded.problems);
    let text = loaded.problems[0].message.to_string();
    assert!(
        text.starts_with(r#"ifcx-repaired(record=ifcx-record-element(name="W-001")"#)
            && text.contains("ifcx-relation-listed-twice"),
        "{text}"
    );
}
