//! Body-less elements bound to a drafting view (ANN-002), through the
//! body-less test kind, which stands in for 2D drafting elements, markers
//! and dimensions: the view's serde form, the binding to the home
//! storey's plan, the rebinding when the element moves to another storey
//! and the flat vertical extent.

#![allow(clippy::unwrap_used)]
#![cfg(feature = "test-kinds")]

use archicraft_model::command::{StoreyElements, relink_to_storey};
use archicraft_model::drafting_view::zero_height_extent;
use archicraft_model::glam::DVec2;
use archicraft_model::kinds::Note;
use archicraft_model::template::default_metric_project;
use archicraft_model::{
    Arg, Command, CommandError, DraftingView, Element, History, Message, Project, StoreyId, samples,
};
use serde_json::json;

fn ground(p: &Project) -> StoreyId {
    p.storeys.ground()
}

/// The default project's upper storey, 3 m above the ground storey.
fn upper(p: &Project) -> StoreyId {
    p.storeys.as_slice()[1].id
}

/// A note on `home` in the plan view of `view`.
fn note(p: &Project, home: StoreyId, view: StoreyId) -> Element {
    Note::new(view, DVec2::new(1.5, -2.25), "Living room").into_element(p, home, 0)
}

/// The explanation of a rejected element: the error itself when it was
/// added, the reason nested in `model-error-element-invalid` when it was
/// edited.
fn explanation(error: CommandError) -> Message {
    let CommandError::Invalid(message) = error else {
        panic!("expected CommandError::Invalid, got {error:?}");
    };
    if message.key() != "model-error-element-invalid" {
        return message;
    }
    match message.arg("reason") {
        Some(Arg::Nested(reason)) => (**reason).clone(),
        other => panic!("expected a nested reason, got {other:?}"),
    }
}

/// ANN-002: a drafting view is stored tagged by its type,
/// `{ "type": "plan", "storey": "<uuid>" }`, and reads back equal, also as
/// the view of an element.
#[test]
fn a_drafting_view_serialises_tagged_by_its_type() {
    let p = default_metric_project();
    let storey = upper(&p);
    let view = DraftingView::plan(storey);
    let value = serde_json::to_value(view).unwrap();
    assert_eq!(value, json!({"type": "plan", "storey": storey.to_string()}));
    assert_eq!(serde_json::from_value::<DraftingView>(value).unwrap(), view);
    assert_eq!(view.plan_storey(), Some(storey));

    let element = note(&p, storey, storey);
    let text = serde_json::to_string(&element).unwrap();
    assert_eq!(serde_json::from_str::<Element>(&text).unwrap(), element);
}

/// ANN-002: a note in the plan view of its home storey is added and
/// undone like any element, although its extent has no height.
#[test]
fn a_note_in_its_home_storeys_plan_is_added() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let before = p.clone();
    let element = note(&p, upper(&p), upper(&p));
    let id = element.id;
    assert_eq!(element.name, "NT-001");
    h.execute(&mut p, Command::AddElements(vec![element])).unwrap();
    assert!(p.element(id).is_some());
    h.undo(&mut p).unwrap();
    assert_eq!(Project { revision: 0, ..p }, before);
}

/// ANN-002: a note whose view shows another storey than its home storey is
/// rejected with an explanation naming both storeys, and so is one whose
/// view shows a storey that does not exist; the project stays unchanged.
#[test]
fn a_note_on_another_storeys_view_is_rejected() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let before = p.clone();

    let elsewhere = note(&p, ground(&p), upper(&p));
    let error = h.execute(&mut p, Command::AddElements(vec![elsewhere])).unwrap_err();
    let explained = explanation(error);
    assert_eq!(explained.key(), "model-error-view-not-home-storey");
    assert_eq!(explained.arg("view"), Some(&Arg::Text("01 First floor".into())));
    assert_eq!(explained.arg("home"), Some(&Arg::Text("00 Ground floor".into())));
    assert_eq!(p, before);

    let unknown = note(&p, ground(&p), StoreyId::new());
    let error = h.execute(&mut p, Command::AddElements(vec![unknown])).unwrap_err();
    assert_eq!(explanation(error).key(), "model-error-view-storey-missing");
    assert_eq!(p, before);

    // An edit that moves the note's view away from its home storey is
    // rejected the same way.
    let element = note(&p, ground(&p), ground(&p));
    let id = element.id;
    h.execute(&mut p, Command::AddElements(vec![element])).unwrap();
    let added = p.clone();
    let mut edited = p.element(id).unwrap().clone();
    edited.as_note_mut().unwrap().view = DraftingView::plan(upper(&p));
    let error = h.execute(&mut p, Command::UpdateElements(vec![edited])).unwrap_err();
    assert_eq!(explanation(error).key(), "model-error-view-not-home-storey");
    assert_eq!(p, added);
}

/// ANN-002, ELM-005: moving a note to another storey rebinds its view to
/// that storey's plan and keeps its plan position; undo restores both.
#[test]
fn relinking_a_note_rebinds_its_view() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let element = note(&p, ground(&p), ground(&p));
    let id = element.id;
    h.execute(&mut p, Command::AddElements(vec![element])).unwrap();
    let moved = relink_to_storey(&p, p.element(id).unwrap(), upper(&p)).unwrap();
    h.execute(&mut p, Command::UpdateElements(vec![moved])).unwrap();
    let after = p.element(id).unwrap();
    assert_eq!(after.storey, upper(&p));
    let after = after.as_note().unwrap();
    assert_eq!(after.view, DraftingView::plan(upper(&p)));
    assert_eq!(after.at, DVec2::new(1.5, -2.25));
    h.undo(&mut p).unwrap();
    assert_eq!(
        p.element(id).unwrap().as_note().unwrap().view,
        DraftingView::plan(ground(&p))
    );
}

/// ANN-002, STR-004: deleting a note's storey and moving its elements to
/// another storey rebinds the note's view to the target's plan.
#[test]
fn deleting_a_notes_storey_moves_it_to_the_targets_plan() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let (ground, upper) = (ground(&p), upper(&p));
    let element = note(&p, upper, upper);
    let id = element.id;
    h.execute(&mut p, Command::AddElements(vec![element])).unwrap();
    h.execute(
        &mut p,
        Command::DeleteStorey {
            storey: upper,
            elements: StoreyElements::MoveTo(ground),
        },
    )
    .unwrap();
    let moved = p.element(id).unwrap();
    assert_eq!(moved.storey, ground);
    assert_eq!(moved.as_note().unwrap().view, DraftingView::plan(ground));
    h.undo(&mut p).unwrap();
    assert_eq!(
        p.element(id).unwrap().as_note().unwrap().view,
        DraftingView::plan(upper)
    );
}

/// ANN-002: a note's vertical extent is the zero-height extent at its home
/// storey's elevation, following the storey; without the storey there is
/// none, as for every kind.
#[test]
fn a_notes_vertical_extent_is_flat_at_its_home_storey() {
    let mut p = samples::two_storey_shell();
    let mut h = History::default();
    let upper = p.storeys.relative(p.storeys.ground(), 1).unwrap().id;
    let elevation = p.storeys.get(upper).unwrap().elevation;
    assert!(elevation > 0.0);
    let element = note(&p, upper, upper);
    let id = element.id;
    h.execute(&mut p, Command::AddElements(vec![element])).unwrap();
    assert_eq!(p.vertical_extent(p.element(id).unwrap()), Some((elevation, elevation)));
    assert_eq!(zero_height_extent(upper, &p.storeys), Some((elevation, elevation)));

    h.execute(
        &mut p,
        Command::SetStoreyElevation {
            storey: upper,
            elevation: elevation + 0.5,
            shift_above: true,
        },
    )
    .unwrap();
    let raised = elevation + 0.5;
    assert_eq!(p.vertical_extent(p.element(id).unwrap()), Some((raised, raised)));

    let kind = &p.element(id).unwrap().kind;
    assert_eq!(kind.vertical_extent(StoreyId::new(), &p.storeys), None);
    assert_eq!(zero_height_extent(StoreyId::new(), &p.storeys), None);
}
