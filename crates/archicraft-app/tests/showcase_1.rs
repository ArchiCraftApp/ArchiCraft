//! The exit check of Showcase 1 (archicraft-mn49k): the demo script walked
//! through the application with injected pointer and key events, one session
//! from a new project to a restart, asserting the model and what the window
//! shows after each step.
//!
//! 1. Save at the far left of the toolbar asks for a location on a new
//!    project and saves; a second click saves without asking (PRJ-003,
//!    UI-011).
//! 2. Measure from the toolbar or M, two clicks in the plan: distance and
//!    angle show; Esc clears them (MEA-001).
//! 3. Two walls selected, Group in the toolbar: a click on one wall selects
//!    both; Ungroup (ELM-015).
//! 4. Dragging the toolbox's edge switches between one and two icon columns;
//!    hovering a tool shows its name, shortcut and explanation; the column
//!    count survives a restart (UI-001, UI-011, UI-015).

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::HashSet;
use std::path::Path;

use archicraft_app::commands::Binding;
use archicraft_app::persistence::DataDirs;
use archicraft_app::persistence::prefs::Preferences;
use archicraft_app::tools::{Measurement, ToolKind, WallMode};
use archicraft_app::{ArchiCraftApp, tr};
use archicraft_model::glam::DVec2;
use archicraft_model::units::format_angle;
use archicraft_model::{Command, ElementId};
use common::{assert_present, data_dir, harness_in};
use egui::accesskit::{Role, Toggled};
use egui::{Event, Key, Modifiers, PointerButton};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, NodeT, Queryable};

type App = Harness<'static, ArchiCraftApp>;

/// The rectangle of the panel `name` as egui stored it.
fn panel(harness: &App, name: &str) -> egui::Rect {
    egui::containers::panel::PanelState::load(&harness.ctx, egui::Id::new(name))
        .unwrap_or_else(|| panic!("the {name} panel"))
        .outer_rect
}

/// The toolbar's button labelled `label`: the toolbar's labels end in
/// "tool" or name a command, unlike the toolbox's tool names.
fn toolbar_button<'h>(harness: &'h App, label: &'h str) -> egui_kittest::Node<'h> {
    let toolbox = panel(harness, "toolbox");
    harness
        .query_all(By::new().role(Role::Button).label(label))
        .find(|node| !toolbox.contains(node.rect().center()))
        .unwrap_or_else(|| panic!("no toolbar button “{label}”"))
}

/// The toolbox's button named `label`.
fn toolbox_button<'h>(harness: &'h App, label: &'h str) -> egui_kittest::Node<'h> {
    let toolbox = panel(harness, "toolbox");
    harness
        .query_all(By::new().role(Role::Button).label(label))
        .find(|node| toolbox.contains(node.rect().center()))
        .unwrap_or_else(|| panic!("no toolbox button “{label}”"))
}

/// The toolbox's tool buttons in group order (UI-001).
const TOOLBOX_TOOLS: [&str; 7] = ["Select", "Wall", "Slab", "Fit", "Orbit", "Plan/3D", "Measure"];

/// The number of columns the toolbox's tool buttons sit in.
fn toolbox_columns(harness: &App) -> usize {
    let mut lefts: Vec<f32> = TOOLBOX_TOOLS
        .iter()
        .map(|label| toolbox_button(harness, label).rect().left().round())
        .collect();
    lefts.sort_by(f32::total_cmp);
    lefts.dedup();
    lefts.len()
}

fn click_button(harness: &mut App, node: impl FnOnce(&App) -> egui_kittest::Node<'_>) {
    node(harness).click();
    harness.run_steps(2);
}

/// Clicks the primary button at `pos` with `modifiers`.
fn click_with(harness: &mut App, pos: egui::Pos2, modifiers: Modifiers) {
    harness.event(Event::ModifiersChanged(modifiers));
    harness.hover_at(pos);
    harness.run_steps(1);
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers,
        });
    }
    harness.run_steps(2);
    harness.event(Event::ModifiersChanged(Modifiers::NONE));
}

fn click(harness: &mut App, pos: egui::Pos2) {
    click_with(harness, pos, Modifiers::NONE);
}

fn press(harness: &mut App, key: Key) {
    harness.key_press(key);
    harness.run_steps(2);
}

/// The screen position of the plan point `p` in the active plan view.
fn plan_point(harness: &mut App, p: DVec2) -> egui::Pos2 {
    let ws = harness.state_mut().workspace();
    let state = &ws.plan_views[&ws.active_storey];
    state.camera.to_screen(state.last_rect, p)
}

/// Whether a widget of the window has a label containing `text`.
fn shows(harness: &App, text: &str) -> bool {
    harness.query_all(By::new().label_contains(text)).next().is_some()
}

fn selection(harness: &mut App) -> HashSet<ElementId> {
    harness.state_mut().workspace().selection.clone()
}

/// Step 1 (PRJ-003, UI-011): on a new project the toolbar's first button,
/// Save, invokes `file.save`, whose first save asks for the location: Cancel
/// saves nothing, a location is saved to. A second click, after renaming
/// the project, saves to that file without asking.
fn step_1_save(harness: &mut App, path: &Path) {
    assert!(harness.state_mut().workspace().doc.path.is_none(), "a new project");
    let lefts: Vec<f32> = ["Save", "Undo", "Redo", "Measure tool", "Group", "Ungroup"]
        .iter()
        .map(|label| toolbar_button(harness, label).rect().left())
        .collect();
    assert!(lefts.is_sorted(), "Save is the toolbar's first button: {lefts:?}");

    // The native file dialog cannot run in a test: its stand-in answers
    // `path`, or Cancel for `None`, and counts how often it is asked.
    harness.state_mut().stub_save_dialog(None);
    click_button(harness, |h| toolbar_button(h, "Save"));
    assert_eq!(harness.state().save_dialogs_asked(), 1);
    assert!(!path.exists(), "Cancel saves nothing");
    assert!(harness.state_mut().workspace().doc.path.is_none());
    harness.state_mut().stub_save_dialog(Some(path.to_owned()));
    click_button(harness, |h| toolbar_button(h, "Save"));
    assert_eq!(harness.state().save_dialogs_asked(), 1, "the first save asks");
    assert!(path.is_file());
    let ws = harness.state_mut().workspace();
    assert_eq!(ws.doc.path.as_deref(), Some(path));
    assert!(!ws.doc.is_dirty());
    assert_present(harness, &tr!("status-saved", path = path.display().to_string()));

    let ws = harness.state_mut().workspace();
    let mut info = ws.doc.project.info.clone();
    info.name = "Showcase 1".into();
    assert!(ws.execute(Command::SetInfo(info)));
    assert!(ws.doc.is_dirty());
    harness.run_steps(2);
    click_button(harness, |h| toolbar_button(h, "Save"));
    assert_eq!(harness.state().save_dialogs_asked(), 1, "the second save does not ask");
    assert!(!harness.state_mut().workspace().doc.is_dirty());
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains("Showcase 1"), "the second save wrote the file");
}

/// The readout the status line shows for the measurement now, as its
/// labels and values.
fn readout(harness: &mut App) -> String {
    let ws = harness.state_mut().workspace();
    let pairs = ws
        .tool
        .measure
        .readout(&ws.doc.project.settings.units, &ws.prefs.number_format());
    pairs
        .into_iter()
        .map(|(label, value)| format!("{label} {value}"))
        .collect::<Vec<_>>()
        .join("  ")
}

fn measure_lines(harness: &mut App) -> usize {
    let ws = harness.state_mut().workspace();
    ws.plan_views[&ws.active_storey].measure.len()
}

/// Step 2 (MEA-001): the toolbar's Measure and M both pick the measure tool;
/// two clicks in the plan show the distance with its direction angle in the
/// status line and a measure line in the plan; Esc clears both.
fn step_2_measure(harness: &mut App) {
    click_button(harness, |h| toolbar_button(h, "Measure tool"));
    assert_eq!(harness.state_mut().workspace().tool.kind, ToolKind::Measure);
    let selected = toolbar_button(harness, "Measure tool").accesskit_node().toggled();
    assert_eq!(selected, Some(Toggled::True), "the toolbar shows the tool selected");
    press(harness, Key::V);
    assert_eq!(harness.state_mut().workspace().tool.kind, ToolKind::Select);
    press(harness, Key::M);
    assert_eq!(harness.state_mut().workspace().tool.kind, ToolKind::Measure);

    let start = plan_point(harness, DVec2::new(0.0, -2.0));
    click(harness, start);
    click(harness, start + egui::vec2(150.0, -80.0));
    let ws = harness.state_mut().workspace();
    let Some(Measurement::Distance { direction, .. }) = ws.tool.measure.measurement() else {
        panic!("two clicks measure a distance")
    };
    let angle = format_angle(
        direction,
        &ws.doc.project.settings.units,
        &ws.prefs.number_format(),
        true,
    );
    let text = readout(harness);
    assert!(text.starts_with("Total ") && text.ends_with(&angle), "{text}");
    assert_present(harness, &text);
    assert_eq!(measure_lines(harness), 1);
    assert!(
        !harness.state_mut().workspace().doc.is_dirty(),
        "measuring changes nothing"
    );

    press(harness, Key::Escape);
    assert_eq!(measure_lines(harness), 0);
    assert!(!shows(harness, "Total "));
    assert_eq!(harness.state_mut().workspace().tool.kind, ToolKind::Measure);
}

/// Step 3 (ELM-015): two walls drawn with the wall tool, selected with a
/// click and a Shift-click and grouped from the toolbar; a click on one wall
/// selects both; after the toolbar's Ungroup a click selects one again.
fn step_3_group(harness: &mut App) {
    press(harness, Key::W);
    click_button(harness, |h| h.get_by_label(WallMode::Single.label()));
    for y in [0.0, 1.5] {
        for x in [0.0, 4.0] {
            let at = plan_point(harness, DVec2::new(x, y));
            click(harness, at);
        }
    }
    let ws = harness.state_mut().workspace();
    let walls: Vec<_> = ws
        .doc
        .project
        .elements
        .values()
        .filter_map(|e| e.as_wall().map(|w| (e.id, (w.start + w.end) / 2.0)))
        .collect();
    let [(a, mid_a), (b, mid_b)] = walls[..] else {
        panic!("two walls drawn: {walls:?}")
    };
    click_button(harness, |h| toolbox_button(h, "Select"));
    let at_a = plan_point(harness, mid_a);
    let at_b = plan_point(harness, mid_b);
    let empty = plan_point(harness, (mid_a + mid_b) / 2.0);
    click(harness, at_a);
    click_with(harness, at_b, Modifiers::SHIFT);
    assert_eq!(selection(harness), HashSet::from([a, b]));

    click_button(harness, |h| toolbar_button(h, "Group"));
    assert_eq!(harness.state_mut().workspace().doc.project.groups.len(), 1);
    assert_present(harness, "Group: Group 1 (2 elements)");
    click(harness, empty);
    assert!(selection(harness).is_empty());
    click(harness, at_a);
    assert_eq!(selection(harness), HashSet::from([a, b]), "one click selects the group");

    click_button(harness, |h| toolbar_button(h, "Ungroup"));
    assert!(harness.state_mut().workspace().doc.project.groups.is_empty());
    assert!(!shows(harness, "Group: Group 1"));
    click(harness, empty);
    click(harness, at_a);
    assert_eq!(selection(harness), HashSet::from([a]), "one click selects one wall");
}

/// Drags the toolbox's right edge `dx` points to the right and releases it.
fn drag_toolbox_edge(harness: &mut App, dx: f32) {
    let toolbox = panel(harness, "toolbox");
    let start = egui::pos2(toolbox.right() - 2.0, toolbox.bottom() - 40.0);
    let button = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    harness.hover_at(start);
    harness.run_steps(1);
    harness.event(button(start, true));
    harness.run_steps(1);
    for step in 1..=8 {
        harness.hover_at(start + egui::vec2(dx * step as f32 / 8.0, 0.0));
        harness.run_steps(1);
    }
    harness.event(button(start + egui::vec2(dx, 0.0), false));
    harness.run_steps(3);
}

/// The number of labels (not buttons) with the text `text`.
fn labels(harness: &App, text: &str) -> usize {
    harness.query_all(By::new().role(Role::Label).label(text)).count()
}

/// Step 4 (UI-001, UI-011, UI-015): dragging the toolbox's edge switches its
/// icon buttons from one column to two and back; hovering the Measure
/// button shows the tool's name, M and its explanation.
fn step_4_toolbox(harness: &mut App) {
    assert_eq!(toolbox_columns(harness), 1);
    drag_toolbox_edge(harness, 120.0);
    assert_eq!(toolbox_columns(harness), 2);
    drag_toolbox_edge(harness, -120.0);
    assert_eq!(toolbox_columns(harness), 1);
    drag_toolbox_edge(harness, 120.0);
    assert_eq!(toolbox_columns(harness), 2);

    let explanation = tr!("cmd-tool-measure-desc");
    let keys = harness.ctx.format_shortcut(&Binding::key(Key::M).shortcut());
    let (names, shortcuts) = (labels(harness, "Measure"), labels(harness, &keys));
    assert_eq!(labels(harness, &explanation), 0);
    toolbox_button(harness, "Measure").hover();
    harness.run_steps(4);
    assert_eq!(labels(harness, "Measure"), names + 1, "the tooltip's title");
    assert_eq!(labels(harness, &keys), shortcuts + 1, "the tooltip's shortcut");
    assert_eq!(labels(harness, &explanation), 1, "the tooltip's explanation");
}

/// PRJ-003, MEA-001, ELM-015, UI-001, UI-011, UI-015: the demo script of
/// Showcase 1 in one session, then a restart with the preferences that
/// session saved: the toolbox opens with two columns and the saved project is
/// the most recent one.
#[test]
fn the_showcase_1_demo_script_runs_from_a_new_project_to_a_restart() {
    let dir = data_dir("showcase-1");
    let project_dir = data_dir("showcase-1-project");
    std::fs::create_dir_all(&project_dir).unwrap();
    let path = project_dir.join("showcase.ifcx");
    let mut harness = harness_in(&dir, Preferences::default(), 1.0);

    step_1_save(&mut harness, &path);
    step_2_measure(&mut harness);
    step_3_group(&mut harness);
    step_4_toolbox(&mut harness);

    drop(harness);
    let saved = Preferences::load(&DataDirs::at(&dir).preferences_file());
    assert_eq!(saved.toolbox_columns, 2);
    assert_eq!(saved.recent.first().map(|p| p.as_path()), Some(path.as_path()));
    let harness = harness_in(&dir, saved, 1.0);
    assert_eq!(toolbox_columns(&harness), 2, "the columns survive the restart");
    std::fs::remove_dir_all(&project_dir).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
