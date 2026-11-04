//! Elements without a body, bound to a drafting view (ANN-002, IFX-060), in
//! IFCX, through the model's body-less test kind that the dev-dependency on
//! this crate with `test-kinds` provides: notes round-trip through their
//! codec bit for bit, and their nodes carry no body.

#![allow(clippy::unwrap_used)]

mod common;

use archicraft_model::glam::DVec2;
use archicraft_model::kinds::Note;
use archicraft_model::{Command, DraftingView, ElementId, History, Project, samples};
use serde_json::Value;

use common::{node_index, save};

/// The two-storey shell with a note on each storey, at plan points that
/// need every digit of an `f64`, and the notes' ids.
fn shell_with_notes() -> (Project, Vec<ElementId>) {
    let mut p = samples::two_storey_shell();
    let ground = p.storeys.ground();
    let upper = p.storeys.relative(ground, 1).unwrap().id;
    let notes = vec![
        Note::new(ground, DVec2::new(1.0 / 3.0, 2.0f64.sqrt()), "Living room").into_element(&p, ground, 0),
        Note::new(upper, DVec2::new(-0.1 - 0.2, 1e-7), "Bedroom «Nord»").into_element(&p, upper, 1),
    ];
    let ids = notes.iter().map(|e| e.id).collect();
    History::default().execute(&mut p, Command::AddElements(notes)).unwrap();
    (p, ids)
}

/// IFX-060, IFX-064, ANN-002: a project with notes on two storeys loads
/// back equal, without problems: each note's view, point (at full
/// precision) and text, and its envelope.
#[test]
fn notes_round_trip_bit_for_bit() {
    let (project, ids) = shell_with_notes();
    let loaded = archicraft_ifcx::load(&save(&project)).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    let mut expected = project.clone();
    expected.revision = 0;
    expected.data_version = 7;
    assert_eq!(loaded.project, expected);
    for id in ids {
        let note = loaded.project.element(id).unwrap();
        let home = note.storey;
        assert_eq!(note.as_note().unwrap().view, DraftingView::plan(home), "{}", note.name);
    }
}

/// IFX-060, ANN-002: a note's parameters are written through its codec,
/// `archicraft::note` with the view in the model's tagged form, and a note
/// gets no body or axis node, because it has no body.
#[test]
fn a_note_is_written_through_its_codec_without_a_body() {
    let (project, ids) = shell_with_notes();
    let file: Value = serde_json::from_slice(&save(&project)).unwrap();
    for id in ids {
        let element = project.element(id).unwrap();
        let note = element.as_note().unwrap();
        let node = &file["data"][node_index(&file, &element.name)];
        assert_eq!(node["attributes"]["archicraft::element"]["kind"], "note");
        let attribute = &node["attributes"]["archicraft::note"];
        assert_eq!(
            attribute["view"],
            serde_json::json!({"type": "plan", "storey": element.storey.to_string()})
        );
        assert_eq!(attribute["at"], serde_json::json!([note.at.x, note.at.y]));
        assert_eq!(attribute["text"], note.text.as_str());
        assert!(
            node.get("children").is_none_or(|c| c.as_object().unwrap().is_empty()),
            "{node}"
        );
    }
}
