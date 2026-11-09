//! Elements without a body, bound to a drafting view (ANN-002, IFX-060), in
//! IFCX, through the model's body-less test kind that the dev-dependency on
//! this crate with `test-kinds` provides. The note is a kind that is not an
//! IFCX node (ANN-010): its elements are entries of one
//! `archicraft::annotations` attribute per storey node, never nodes, so the
//! standard content of a file does not depend on them (IFX-063, IFX-068).
//! Notes round-trip through their codec bit for bit, and a malformed entry
//! is skipped with a report while the rest of the file loads (PRJ-008).

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::HashSet;

use archicraft_ifcx::alpha::schema::FORMAT_VERSION;
use archicraft_ifcx::validate::{self, Severity};
use archicraft_model::glam::DVec2;
use archicraft_model::kinds::Note;
use archicraft_model::{
    Command, DraftingView, Element, ElementId, GroupId, History, Position, Project, Relation, StoreyId, samples,
};
use serde_json::{Value, json};

use common::{edit, node_index, options, save};

const ANNOTATIONS: &str = "archicraft::annotations";

/// The ground and the upper storey of the two-storey shell.
fn storeys(p: &Project) -> (StoreyId, StoreyId) {
    let ground = p.storeys.ground();
    (ground, p.storeys.relative(ground, 1).unwrap().id)
}

/// The two-storey shell with three notes, at plan points that need every
/// digit of an `f64`, and their ids in model order: `NT-001` and `NT-003`
/// on the ground storey, `NT-002` on the upper one. `NT-003` sets every
/// optional member of the envelope (the last layer, a position, locked,
/// excluded from snapping); `NT-001` and `NT-002` are grouped.
fn shell_with_notes() -> (Project, Vec<ElementId>) {
    let mut p = samples::two_storey_shell();
    let (ground, upper) = storeys(&p);
    let mut notes = vec![
        Note::new(ground, DVec2::new(1.0 / 3.0, 2.0f64.sqrt()), "Living room").into_element(&p, ground, 0),
        Note::new(upper, DVec2::new(-0.1 - 0.2, 1e-7), "Bedroom «Nord»").into_element(&p, upper, 1),
        Note::new(ground, DVec2::new(5.5, -1.0 / 7.0), "Hall").into_element(&p, ground, 2),
    ];
    let last = &mut notes[2];
    last.layer = p.layers.last().unwrap().id;
    last.position = Position::Interior;
    last.locked = true;
    last.snap_excluded = true;
    let ids: Vec<ElementId> = notes.iter().map(|e| e.id).collect();
    let mut history = History::default();
    history.execute(&mut p, Command::AddElements(notes)).unwrap();
    history
        .execute(
            &mut p,
            Command::Group {
                group: GroupId::new(),
                elements: vec![ids[0], ids[1]],
            },
        )
        .unwrap();
    (p, ids)
}

/// What a loaded copy of `project` equals: the revision is not stored and
/// the data version is the save counter of [`options`].
fn as_loaded(project: &Project) -> Project {
    let mut expected = project.clone();
    expected.revision = 0;
    expected.data_version = options().data_version;
    expected
}

/// The `archicraft::annotations` value on the node of `storey`, if any.
fn annotations_of(file: &Value, storey: StoreyId) -> Option<&Value> {
    let node = file["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["path"] == storey.to_string().as_str())
        .unwrap();
    node["attributes"].get(ANNOTATIONS)
}

/// IFX-060, IFX-064, ANN-002, ANN-010: a project with notes on two storeys
/// loads back equal, without problems: each note's view, point (at full
/// precision) and text, and its envelope, groups included.
#[test]
fn notes_round_trip_bit_for_bit() {
    let (project, ids) = shell_with_notes();
    let loaded = archicraft_ifcx::load(&save(&project)).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert_eq!(loaded.project, as_loaded(&project));
    for id in ids {
        let note = loaded.project.element(id).unwrap();
        let home = note.storey;
        assert_eq!(note.as_note().unwrap().view, DraftingView::plan(home), "{}", note.name);
    }
}

/// ANN-010, IFX-060: notes are never nodes. Each storey with notes carries
/// one `archicraft::annotations` attribute whose entries are its notes in
/// model order: the envelope (id, kind, name, IFC class and the common
/// element data, optional members only when set) and the codec's members
/// under the kind code. The storey without a note, like every node of a
/// project without notes, has no such attribute, no node is a note, the
/// declarations include the attribute but no `archicraft::note` of its own,
/// and the file validates.
#[test]
fn notes_are_entries_of_one_annotations_attribute_on_their_storey() {
    let (project, ids) = shell_with_notes();
    let bytes = save(&project);
    let file: Value = serde_json::from_slice(&bytes).unwrap();
    let (ground, upper) = storeys(&project);
    for (storey, expected) in [(ground, vec![ids[0], ids[2]]), (upper, vec![ids[1]])] {
        let entries = annotations_of(&file, storey).unwrap().as_array().unwrap();
        let written: Vec<&str> = entries.iter().map(|e| e["id"].as_str().unwrap()).collect();
        let expected: Vec<String> = expected.iter().map(ToString::to_string).collect();
        assert_eq!(written, expected, "model order on {storey}");
    }
    let entry_of = |id: ElementId| {
        let element = project.element(id).unwrap();
        annotations_of(&file, element.storey)
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == id.to_string().as_str())
            .unwrap()
            .clone()
    };
    let note_member = |e: &Element| {
        let note = e.as_note().unwrap();
        json!({
            "view": {"type": "plan", "storey": e.storey.to_string()},
            "at": [note.at.x, note.at.y],
            "text": note.text,
        })
    };
    let plain = project.element(ids[0]).unwrap();
    assert_eq!(
        entry_of(ids[0]),
        json!({
            "id": plain.id.to_string(),
            "name": "NT-001",
            "ifcClass": "IfcAnnotation",
            "kind": "note",
            "layer": plain.layer.to_string(),
            "locked": false,
            "position": "undefined",
            "group": plain.group.unwrap().to_string(),
            "note": note_member(plain),
        })
    );
    let flagged = project.element(ids[2]).unwrap();
    assert_eq!(
        entry_of(ids[2]),
        json!({
            "id": flagged.id.to_string(),
            "name": "NT-003",
            "ifcClass": "IfcAnnotation",
            "kind": "note",
            "layer": flagged.layer.to_string(),
            "locked": true,
            "position": "interior",
            "snapExcluded": true,
            "note": note_member(flagged),
        })
    );

    let data = file["data"].as_array().unwrap();
    let note_paths: HashSet<String> = ids.iter().map(ToString::to_string).collect();
    for node in data {
        assert!(!note_paths.contains(node["path"].as_str().unwrap()), "{node}");
        for child in node["children"].as_object().into_iter().flatten() {
            assert!(!note_paths.contains(child.1.as_str().unwrap_or_default()), "{node}");
        }
        assert_ne!(node["attributes"]["archicraft::element"]["kind"], "note", "{node}");
        assert!(node["attributes"].get("archicraft::note").is_none(), "{node}");
        let storey_node = node["attributes"].get("archicraft::storey").is_some();
        assert!(storey_node || node["attributes"].get(ANNOTATIONS).is_none(), "{node}");
    }
    let schemas = file["schemas"].as_object().unwrap();
    assert!(schemas.contains_key(ANNOTATIONS));
    assert!(!schemas.contains_key("archicraft::note"));
    let parsed = validate::parse(&bytes).unwrap();
    let problems = validate::validate(&parsed);
    assert!(problems.is_empty(), "{problems:#?}");

    // A storey without notes, and every sample, carries and declares none.
    let mut one_storey = project.clone();
    one_storey
        .elements
        .retain(|_, e| e.storey == ground || e.as_note().is_none());
    let file: Value = serde_json::from_slice(&save(&one_storey)).unwrap();
    assert!(annotations_of(&file, ground).is_some());
    assert!(annotations_of(&file, upper).is_none());
    for sample in common::samples() {
        let text = String::from_utf8(save(&sample)).unwrap();
        assert!(!text.contains(ANNOTATIONS), "{}", sample.info.name);
    }
}

/// Checks that the value description `desc` describes every member of
/// `value`, through nested objects and arrays, and that every enumeration
/// value is one of its options; returns how many members it checked.
fn assert_described(desc: &Value, value: &Value, at: &str) -> usize {
    match desc["dataType"].as_str().unwrap() {
        "Object" => value
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, member)| {
                let field = &desc["objectRestrictions"]["values"][key];
                assert!(field.is_object(), "{at}.{key} is not described");
                1 + assert_described(field, member, &format!("{at}.{key}"))
            })
            .sum(),
        "Array" => value
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, item)| assert_described(&desc["arrayRestrictions"]["value"], item, &format!("{at}[{i}]")))
            .sum(),
        "Enum" => {
            let options = desc["enumRestrictions"]["options"].as_array().unwrap();
            assert!(options.contains(value), "{at}: {value} is an option of {options:?}");
            0
        }
        _ => 0,
    }
}

/// IFX-061: the declaration of `archicraft::annotations` describes every
/// member its entries carry, the envelope, the note's members and the
/// relations, so a file with notes validates on its own.
#[test]
fn the_annotations_declaration_describes_every_member_of_an_entry() {
    let (mut project, ids) = shell_with_notes();
    let wall = named(&project, "W-001").id;
    assert!(project.relations.insert(Relation::AnnotationBinding {
        annotation: ids[2],
        target: wall,
    }));
    let file: Value = serde_json::from_slice(&save(&project)).unwrap();
    let declaration = &file["schemas"][ANNOTATIONS]["value"];
    let (ground, upper) = storeys(&project);
    let mut members = 0;
    for storey in [ground, upper] {
        members += assert_described(declaration, annotations_of(&file, storey).unwrap(), ANNOTATIONS);
    }
    // Three envelopes of at least seven members, each with its note.
    assert!(members > 3 * 10, "{members}");
}

/// Removes every `archicraft::` attribute and schema declaration, leaving
/// what a consumer that ignores the ArchiCraft namespace reads, as bytes.
fn standard_content(bytes: &[u8]) -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(bytes).unwrap();
    for node in value["data"].as_array_mut().unwrap() {
        if let Some(attributes) = node.get_mut("attributes").and_then(Value::as_object_mut) {
            attributes.retain(|key, _| !key.starts_with("archicraft::"));
        }
    }
    value["schemas"]
        .as_object_mut()
        .unwrap()
        .retain(|key, _| !key.starts_with("archicraft::"));
    serde_json::to_vec(&value).unwrap()
}

/// IFX-068, IFX-063, ANN-010: the standard content does not depend on
/// annotations. The two-storey shell written with and without its notes
/// gives byte-identical files once every `archicraft::` attribute and
/// schema declaration is stripped, while the files themselves differ.
#[test]
fn ifx_068_notes_leave_the_standard_content_unchanged() {
    let (with_notes, _) = shell_with_notes();
    let mut without = with_notes.clone();
    without.elements.retain(|_, e| e.as_note().is_none());
    let (with_bytes, without_bytes) = (save(&with_notes), save(&without));
    assert_ne!(with_bytes, without_bytes);
    assert_eq!(standard_content(&with_bytes), standard_content(&without_bytes));
    assert_eq!(
        standard_content(&without_bytes),
        standard_content(&save(&samples::two_storey_shell()))
    );
}

/// IFX-007, IFX-064: saving a loaded project with notes writes the same
/// bytes again.
#[test]
fn a_project_with_notes_saves_the_same_bytes_after_loading() {
    let (project, _) = shell_with_notes();
    let bytes = save(&project);
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert_eq!(save(&loaded.project), bytes);
}

/// The element named `name`.
fn named<'p>(project: &'p Project, name: &str) -> &'p Element {
    project.elements.values().find(|e| e.name == name).unwrap()
}

/// SRS 3.3, IFX-060: the explicit relations of a note are written in its
/// entry and restored once every entry is read, so a binding to a note
/// later in the file resolves; a relation stored on a node whose second end
/// is a note waits for the entries too. The project loads equal and saves
/// the same bytes again.
#[test]
fn relations_of_notes_round_trip_after_every_entry() {
    let (mut project, ids) = shell_with_notes();
    let wall = named(&project, "W-001").id;
    let relations = [
        Relation::AnnotationBinding {
            annotation: ids[0],
            target: wall,
        },
        // NT-001 on the ground storey is bound to NT-002 on the upper one,
        // whose entry comes later in the file.
        Relation::AnnotationBinding {
            annotation: ids[0],
            target: ids[1],
        },
        // Stored on the wall's node; its second end is an entry.
        Relation::Trim {
            target: wall,
            by: ids[2],
        },
    ];
    for relation in relations {
        assert!(project.relations.insert(relation));
    }
    let bytes = save(&project);
    let file: Value = serde_json::from_slice(&bytes).unwrap();
    let (ground, _) = storeys(&project);
    let entry = &annotations_of(&file, ground).unwrap()[0];
    let mut written: Vec<Value> = entry["relations"].as_array().unwrap().clone();
    written.sort_by_key(ToString::to_string);
    let mut expected = vec![
        json!({"kind": "annotation_binding", "target": wall.to_string()}),
        json!({"kind": "annotation_binding", "target": ids[1].to_string()}),
    ];
    expected.sort_by_key(ToString::to_string);
    assert_eq!(written, expected);
    let node = &file["data"][node_index(&file, "W-001")];
    assert_eq!(
        node["attributes"]["archicraft::relations"],
        json!([{"kind": "trim", "target": ids[2].to_string()}])
    );

    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert_eq!(loaded.project, as_loaded(&project));
    assert_eq!(save(&loaded.project), bytes);
}

/// The problems whose message names the record `record`.
fn reports_about<'a>(problems: &'a [archicraft_ifcx::Problem], record: &str) -> Vec<&'a archicraft_ifcx::Problem> {
    problems
        .iter()
        .filter(|p| p.message.to_string().contains(record))
        .collect()
}

/// PRJ-008, IFX-083: a malformed entry is skipped with one report that
/// names it, at the storey node, and the rest of the file loads: an entry
/// without an id, one of an unknown kind, one whose note has an invalid
/// point, one without the note's members, one that is not an object and one
/// with neither a name nor an id. Every wall, the slab and the other notes
/// load, and only the validator reports the attribute besides.
#[test]
fn a_malformed_annotation_entry_is_skipped_with_a_report_and_the_rest_loads() {
    let (project, ids) = shell_with_notes();
    let (ground, _) = storeys(&project);
    let bytes = edit(&project, |v| {
        let i = v["data"]
            .as_array()
            .unwrap()
            .iter()
            .position(|n| n["path"] == ground.to_string().as_str())
            .unwrap();
        let entries = v["data"][i]["attributes"][ANNOTATIONS].as_array_mut().unwrap();
        let valid = entries[0].clone();
        let mut without_id = valid.clone();
        without_id.as_object_mut().unwrap().remove("id");
        without_id["name"] = json!("NT-101");
        let mut unknown_kind = valid.clone();
        unknown_kind["id"] = json!(ElementId::new().to_string());
        unknown_kind["name"] = json!("NT-102");
        unknown_kind["kind"] = json!("scribble");
        let mut invalid_point = valid.clone();
        invalid_point["id"] = json!(ElementId::new().to_string());
        invalid_point["name"] = json!("NT-103");
        invalid_point["note"]["at"] = json!("here");
        let mut no_members = valid.clone();
        no_members["id"] = json!(ElementId::new().to_string());
        no_members["name"] = json!("NT-104");
        no_members.as_object_mut().unwrap().remove("note");
        let mut anonymous = valid.clone();
        anonymous.as_object_mut().unwrap().remove("id");
        anonymous.as_object_mut().unwrap().remove("name");
        entries.splice(
            1..1,
            [
                without_id,
                unknown_kind,
                invalid_point,
                no_members,
                json!("not an entry"),
                anonymous,
            ],
        );
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert_eq!(loaded.project, as_loaded(&project), "every valid element loads");
    assert!(ids.iter().all(|id| loaded.project.element(*id).is_some()));

    let problems = &loaded.problems;
    for (record, reason) in [
        (
            r#"ifcx-record-element(name="NT-101")"#,
            r#"ifcx-field-missing(key="id")"#,
        ),
        (
            r#"ifcx-record-element(name="NT-102")"#,
            r#"ifcx-field-unknown-value(key="kind", value="scribble")"#,
        ),
        (r#"ifcx-record-element(name="NT-103")"#, "ifcx-point-not-list"),
        (r#"ifcx-record-element(name="NT-104")"#, "ifcx-parameters-missing"),
        (r#"ifcx-record-annotation(index=5)"#, "ifcx-not-object"),
        (r#"ifcx-record-annotation(index=6)"#, r#"ifcx-field-missing(key="id")"#),
    ] {
        let reports = reports_about(problems, record);
        assert_eq!(reports.len(), 1, "{record}: {problems:#?}");
        let report = reports[0];
        let text = report.message.to_string();
        assert!(text.starts_with("ifcx-skipped(record="), "{text}");
        assert!(text.contains(reason), "{record}: {text}");
        assert_eq!(report.path.as_deref(), Some(ground.to_string().as_str()), "{text}");
        assert_eq!(report.severity, Severity::Error, "a skipped record: {text}");
    }
    let others: Vec<String> = problems
        .iter()
        .map(|p| p.message.to_string())
        .filter(|m| !m.starts_with("ifcx-skipped("))
        .collect();
    assert!(
        others
            .iter()
            .all(|m| m.starts_with(r#"ifcx-attribute-invalid(id="archicraft::annotations""#)),
        "{others:#?}"
    );
}

/// PRJ-008: an `archicraft::annotations` value that is not a list loads
/// nothing from that storey, with one repair report at the storey, and the
/// storey and its walls load.
#[test]
fn an_annotations_value_that_is_not_a_list_is_reported_against_its_storey() {
    let (project, ids) = shell_with_notes();
    let (ground, _) = storeys(&project);
    let bytes = edit(&project, |v| {
        let i = v["data"]
            .as_array()
            .unwrap()
            .iter()
            .position(|n| n["path"] == ground.to_string().as_str())
            .unwrap();
        v["data"][i]["attributes"][ANNOTATIONS] = json!({"note": "?"});
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.project.element(ids[0]).is_none());
    assert!(loaded.project.element(ids[2]).is_none());
    assert!(loaded.project.element(ids[1]).is_some());
    let notes_lost = project.elements.len() - 2;
    assert_eq!(loaded.project.elements.len(), notes_lost);
    let reports = reports_about(&loaded.problems, "ifcx-record-storey");
    assert_eq!(reports.len(), 1, "{:#?}", loaded.problems);
    let text = reports[0].message.to_string();
    assert!(
        text.starts_with("ifcx-repaired(record=ifcx-record-storey(")
            && text.contains(r#"ifcx-field-not-list(key="archicraft::annotations")"#),
        "{text}"
    );
    assert_eq!(reports[0].path.as_deref(), Some(ground.to_string().as_str()));
}

/// PRJ-008: the entries are read after every element node, whatever their
/// place in the file: an entry on the ground storey that repeats the id of
/// a wall on the upper storey, whose node comes later in the file, is
/// skipped as a duplicate, and the wall loads.
#[test]
fn annotation_entries_are_read_after_every_element_node() {
    let (project, _) = shell_with_notes();
    let (ground, upper) = storeys(&project);
    let upper_wall = project
        .elements
        .values()
        .find(|e| e.storey == upper && e.as_wall().is_some())
        .unwrap();
    let bytes = edit(&project, |v| {
        let i = v["data"]
            .as_array()
            .unwrap()
            .iter()
            .position(|n| n["path"] == ground.to_string().as_str())
            .unwrap();
        v["data"][i]["attributes"][ANNOTATIONS][0]["id"] = json!(upper_wall.id.to_string());
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert_eq!(loaded.project.element(upper_wall.id), Some(upper_wall));
    assert_eq!(loaded.project.elements.len(), project.elements.len() - 1);
    let duplicates: Vec<String> = loaded
        .problems
        .iter()
        .map(|p| p.message.to_string())
        .filter(|m| m.contains("ifcx-duplicate"))
        .collect();
    assert_eq!(
        duplicates,
        [r#"ifcx-duplicate(name="NT-001")"#],
        "{:#?}",
        loaded.problems
    );
}

/// IFX-062: storing notes as entries is no change of the ArchiCraft format:
/// no release kind writes the attribute yet, so the format version stays
/// the one of the release kinds, 2 since spaces (the golden files and the
/// conformance run pin the release content).
#[test]
fn notes_as_entries_keep_the_format_version() {
    assert_eq!(FORMAT_VERSION, 2);
    let (project, _) = shell_with_notes();
    let file: Value = serde_json::from_slice(&save(&project)).unwrap();
    assert_eq!(
        file["data"][0]["attributes"]["archicraft::project"]["formatVersion"],
        "2"
    );
}
