//! The exit check of Showcase 2 (archicraft-8bsuj): the demo script walked
//! through the application with injected pointer and key events, one session
//! from a new project to a restart that reopens the saved file, asserting
//! the model and what the window shows after each step.
//!
//! 1. Four walls drawn around a room with the wall tool's rectangle; one
//!    click with the space tool inside the room: the plan shows the stamp
//!    "Room 1" with the area on the line below (SPC-001).
//! 2. The room renamed in the properties strip (SPC-001, UI-002).
//! 3. With the Text tool, a click and a typed note: the plan draws it at its
//!    2.5 mm paper size; its size changed in the properties (ANN-007).
//! 4. Saved from the toolbar and reopened from File ▸ Open recent after a
//!    restart: the stamp and the text are back, and the IFCX file has an
//!    `IfcSpace` (SPC-001, ANN-007, PRJ-003).

#![allow(clippy::unwrap_used)]

mod common;

use std::path::Path;

use archicraft_app::persistence::DataDirs;
use archicraft_app::persistence::prefs::Preferences;
use archicraft_app::tools::{ToolKind, WallMode};
use archicraft_app::views::annotation_draw::LaidOutText;
use archicraft_app::{ArchiCraftApp, tr};
use archicraft_model::glam::DVec2;
use archicraft_model::units::format_area;
use archicraft_model::{ElementId, ReferenceLine, Space, Text, Wall};
use common::{assert_present, data_dir, harness_in, in_strip, open_strip_overflow};
use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, Queryable};

type App = Harness<'static, ArchiCraftApp>;

/// The corners of the rectangle the four walls are drawn on.
const CORNERS: [DVec2; 2] = [DVec2::new(0.0, 0.0), DVec2::new(6.0, 4.0)];
/// Where the space tool clicks: the middle of the room.
const ROOM_CLICK: DVec2 = DVec2::new(3.0, 2.0);
/// Where the note's anchor lies: inside the room, clear of the walls and of
/// the stamp in the room's middle.
const NOTE_ANCHOR: DVec2 = DVec2::new(1.0, 0.8);

/// The rectangle of the panel `name` as egui stored it.
fn panel(harness: &App, name: &str) -> egui::Rect {
    egui::containers::panel::PanelState::load(&harness.ctx, egui::Id::new(name))
        .unwrap_or_else(|| panic!("the {name} panel"))
        .outer_rect
}

/// The toolbar's button labelled `label`, outside the toolbox.
fn toolbar_button<'h>(harness: &'h App, label: &'h str) -> egui_kittest::Node<'h> {
    let toolbox = panel(harness, "toolbox");
    harness
        .query_all(By::new().role(Role::Button).label(label))
        .find(|node| !toolbox.contains(node.rect().center()))
        .unwrap_or_else(|| panic!("no toolbar button “{label}”"))
}

/// Presses and releases the primary button at `pos` in one frame.
fn press_at(harness: &mut App, pos: Pos2) {
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
}

/// Moves the pointer to `pos` and clicks there.
fn click(harness: &mut App, pos: Pos2) {
    harness.hover_at(pos);
    harness.run_steps(2);
    press_at(harness, pos);
    harness.run_steps(3);
}

fn press(harness: &mut App, key: Key) {
    harness.key_press(key);
    harness.run_steps(2);
}

/// The screen position of the plan point `p` in the active plan view.
fn plan_point(harness: &mut App, p: DVec2) -> Pos2 {
    let ws = harness.state_mut().workspace();
    let state = &ws.plan_views[&ws.active_storey];
    state.camera.to_screen(state.last_rect, p)
}

/// The camera's points per metre in the active plan view.
fn points_per_metre(harness: &mut App) -> f64 {
    let ws = harness.state_mut().workspace();
    ws.plan_views[&ws.active_storey].camera.scale
}

fn tool(harness: &mut App) -> ToolKind {
    harness.state_mut().workspace().tool.kind
}

fn undo_len(harness: &mut App) -> usize {
    harness.state_mut().workspace().doc.history.undo_len()
}

/// The spaces of the project.
fn spaces(harness: &mut App) -> Vec<(ElementId, Space)> {
    let project = &harness.state_mut().workspace().doc.project;
    project
        .elements
        .values()
        .filter_map(|e| Some((e.id, e.as_space()?.clone())))
        .collect()
}

/// The texts of the project.
fn texts(harness: &mut App) -> Vec<(ElementId, Text)> {
    let project = &harness.state_mut().workspace().doc.project;
    project
        .elements
        .values()
        .filter_map(|e| Some((e.id, e.as_text()?.clone())))
        .collect()
}

/// The lines of the stamp of `space` the active plan drew last frame.
fn stamp_lines(harness: &mut App, space: ElementId) -> Vec<String> {
    let ws = harness.state_mut().workspace();
    let state = &ws.plan_views[&ws.active_storey];
    let (_, text) = state
        .stamps
        .iter()
        .find(|(id, _)| *id == space)
        .expect("the plan draws the space's stamp");
    text.lines.iter().map(|l| l.galley.text().to_owned()).collect()
}

/// The text `id` as the active plan laid it out last frame.
fn laid_out(harness: &mut App, id: ElementId) -> LaidOutText {
    let ws = harness.state_mut().workspace();
    let state = &ws.plan_views[&ws.active_storey];
    let (_, text) = state
        .texts
        .iter()
        .find(|(t, _)| *t == id)
        .expect("the plan draws the text");
    text.clone()
}

/// The area of `space` in the project's area unit, as the app shows it.
fn shown_area(harness: &mut App, space: ElementId) -> String {
    let ws = harness.state_mut().workspace();
    let area = ws.doc.project.elements[&space].as_space().unwrap().net_area();
    format_area(area, &ws.doc.project.settings.units, &ws.prefs.number_format())
}

/// Whether a node labelled `label` lies in the properties strip, in its
/// row or its overflow popup.
fn in_properties(harness: &App, label: &str) -> bool {
    harness.query_all_by_label(label).any(|n| in_strip(harness, n.rect()))
}

/// Types `text` into the text field at `rect` and commits it with Enter.
fn type_into(harness: &mut App, rect: egui::Rect, text: &str) {
    press_at(harness, rect.left_center() + egui::vec2(4.0, 0.0));
    harness.run_steps(2);
    harness.event(Event::Text(text.to_owned()));
    harness.key_press(Key::Enter);
    harness.run_steps(3);
}

/// Types `text` key by key as a keyboard does: the key's press, the typed
/// character and the release, one frame each character.
fn type_keys(harness: &mut App, text: &str) {
    for c in text.chars() {
        let key = match c {
            ' ' => Some(Key::Space),
            c => Key::from_name(&c.to_ascii_uppercase().to_string()),
        };
        let key_event = |key, pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        if let Some(key) = key {
            harness.event(key_event(key, true));
        }
        harness.event(Event::Text(c.to_string()));
        if let Some(key) = key {
            harness.event(key_event(key, false));
        }
        harness.run_steps(1);
    }
    harness.run_steps(1);
}

/// Step 1 (WAL-001, SPC-001, UI-002): W and the wall tool's Rectangle
/// method draw four walls with two clicks; A selects the space tool, whose
/// click in the middle of the room creates the space "Room 1" on the room's
/// inner faces. The plan shows its stamp with the area on the line below,
/// and the properties strip shows the same name and area. Returns the
/// space.
fn step_1_room(harness: &mut App) -> ElementId {
    press(harness, Key::W);
    assert_eq!(tool(harness), ToolKind::Wall);
    harness.get_by_label(WallMode::Rectangle.label()).click();
    harness.run_steps(2);
    assert_eq!(harness.state_mut().workspace().tool.wall_mode, WallMode::Rectangle);
    for corner in CORNERS {
        let at = plan_point(harness, corner);
        click(harness, at);
    }
    let walls: Vec<Wall> = harness
        .state_mut()
        .workspace()
        .doc
        .project
        .elements
        .values()
        .filter_map(|e| e.as_wall().cloned())
        .collect();
    assert_eq!(walls.len(), 4, "the rectangle draws four walls");
    // The template's walls: their reference line on the exterior face, so
    // the rectangle is the outline and the room lies a wall thickness inside.
    let thickness = walls[0].thickness;
    for wall in &walls {
        assert_eq!(wall.reference_line, ReferenceLine::Outside);
        assert_eq!(wall.thickness, thickness);
    }
    let [w, h] = (CORNERS[1] - CORNERS[0]).to_array();
    let inner_area = (w - 2.0 * thickness) * (h - 2.0 * thickness);

    press(harness, Key::Escape);
    press(harness, Key::A);
    assert_eq!(tool(harness), ToolKind::Space, "A selects the space tool");
    let steps = undo_len(harness);
    let at = plan_point(harness, ROOM_CLICK);
    click(harness, at);
    let created = spaces(harness);
    let [(room, space)] = created.as_slice() else {
        panic!("one click creates one space: {created:?}")
    };
    assert_eq!(space.long_name, "Room 1");
    assert_eq!(undo_len(harness), steps + 1, "one undo step");
    // The room's inner faces: 5.5 m x 3.5 m for the template's 0.25 m walls.
    let area = space.net_area();
    assert!(
        (area - inner_area).abs() < 1e-6,
        "the room's net area {area} is not {inner_area}"
    );
    let shown = shown_area(harness, *room);
    assert!(shown.ends_with(" m²"), "{shown}");
    assert_eq!(
        stamp_lines(harness, *room),
        ["Room 1", shown.as_str()],
        "the area on the line below the name"
    );
    open_strip_overflow(harness);
    let name_field = harness.get(By::new().role(Role::TextInput).value("Room 1")).rect();
    assert!(in_strip(harness, name_field), "the name is in the strip");
    assert!(in_properties(harness, &shown), "the area is in the strip");
    *room
}

/// Step 2 (SPC-001, UI-002, UND-001): "Living room" typed into the
/// strip's room name renames the space in one undo step, and the stamp
/// follows.
fn step_2_rename(harness: &mut App, room: ElementId) {
    let steps = undo_len(harness);
    let rect = harness.get(By::new().role(Role::TextInput).value("Room 1")).rect();
    type_into(harness, rect, "Living room");
    let ws = harness.state_mut().workspace();
    let space = ws.doc.project.elements[&room].as_space().unwrap();
    assert_eq!(space.long_name, "Living room");
    assert_eq!(ws.doc.history.undo_len(), steps + 1, "one undo step");
    let shown = shown_area(harness, room);
    assert_eq!(stamp_lines(harness, room), ["Living room", shown.as_str()]);
    assert!(
        harness
            .query(By::new().role(Role::TextInput).value("Living room"))
            .is_some(),
        "the strip shows the new name"
    );
}

/// Step 3 (ANN-007, VIEW-013, UI-002): T selects the Text tool; a click in
/// the room opens the inline editor there, the typed note commits with
/// Ctrl/Cmd+Enter as one undo step and the plan draws it at 2.5 mm paper
/// size at 1:100 (0.25 m times the camera's points per metre) with its
/// bottom-left corner where clicked. Selected with a click, a size of 5 mm
/// typed into the properties doubles its drawn height in one undo step.
/// Returns the text.
fn step_3_text(harness: &mut App) -> ElementId {
    press(harness, Key::T);
    assert_eq!(tool(harness), ToolKind::Text, "T selects the Text tool");
    let anchor = plan_point(harness, NOTE_ANCHOR);
    click(harness, anchor);
    harness.run_steps(1);
    assert!(harness.ctx.text_edit_focused(), "the editor has the keyboard");
    let steps = undo_len(harness);
    type_keys(harness, "Oak floor");
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
    harness.run_steps(3);
    assert!(harness.state_mut().workspace().tool.editor.is_none(), "committed");
    assert_eq!(undo_len(harness), steps + 1, "one undo step");
    let created = texts(harness);
    let [(note, text)] = created.as_slice() else {
        panic!("one text: {created:?}")
    };
    let note = *note;
    assert_eq!(text.content, "Oak floor");
    assert!(text.anchor.distance(NOTE_ANCHOR) < 0.05, "{}", text.anchor);

    let ppm = points_per_metre(harness);
    let drawn = laid_out(harness, note);
    let lines: Vec<&str> = drawn.lines.iter().map(|l| l.galley.text()).collect();
    assert_eq!(lines, ["Oak floor"]);
    assert!(
        (f64::from(drawn.size) - 0.25 * ppm).abs() < 1e-3,
        "drawn at 2.5 mm paper size: {} at {ppm} points per metre",
        drawn.size
    );
    assert!(
        drawn.quad[3].distance(anchor) < 1.0,
        "the bottom left corner is on the anchor"
    );

    press(harness, Key::V);
    assert_eq!(tool(harness), ToolKind::Select);
    let [a, _, c, _] = drawn.quad;
    click(harness, a + (c - a) / 2.0);
    let selection: Vec<ElementId> = harness.state_mut().workspace().selection.iter().copied().collect();
    assert_eq!(selection, [note], "a click on the note selects it");

    let steps = undo_len(harness);
    open_strip_overflow(harness);
    let rect = harness.get(By::new().role(Role::TextInput).label("Size (mm)")).rect();
    type_into(harness, rect, "5");
    assert_eq!(undo_len(harness), steps + 1, "the size is one undo step");
    let element = &harness.state_mut().workspace().doc.project.elements[&note];
    assert_eq!(element.as_text().unwrap().size_mm, Some(5.0));
    let doubled = laid_out(harness, note);
    assert!(
        (doubled.size - drawn.size * 2.0).abs() < 1e-3,
        "{} is not twice {}",
        doubled.size,
        drawn.size
    );
    // The galley's height is rounded to whole pixels per line.
    assert!(
        (doubled.extent.y - drawn.extent.y * 2.0).abs() <= 1.0,
        "the drawn height doubles: {} is not twice {}",
        doubled.extent.y,
        drawn.extent.y
    );
    note
}

/// Step 4, saving (PRJ-003, IFX-064): the toolbar's Save asks for the
/// location of the new project and writes the file; the file holds one
/// `IfcSpace` node named after the room, with a `Body` child, and the text.
fn step_4_save(harness: &mut App, path: &Path) {
    harness.state_mut().stub_save_dialog(Some(path.to_owned()));
    toolbar_button(harness, "Save").click();
    harness.run_steps(2);
    assert_eq!(harness.state().save_dialogs_asked(), 1, "the first save asks");
    assert!(!harness.state_mut().workspace().doc.is_dirty());
    assert_present(harness, &tr!("status-saved", path = path.display().to_string()));

    let file: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let nodes = file["data"].as_array().unwrap();
    let ifc_spaces: Vec<&serde_json::Value> = nodes
        .iter()
        .filter(|n| n["attributes"]["bsi::ifc::class"]["code"] == "IfcSpace")
        .collect();
    let [space] = ifc_spaces[..] else {
        panic!("one IfcSpace: {ifc_spaces:?}")
    };
    assert_eq!(space["attributes"]["bsi::ifc::prop::Name"], "Living room");
    assert!(space["children"]["Body"].is_string(), "the IfcSpace has a Body");
    let file_text = file.to_string();
    assert!(file_text.contains("Oak floor"), "the file holds the note");
}

/// Opens the menu titled `title` in the menu bar.
fn open_menu(harness: &mut App, title: &str) {
    let menu = harness
        .query_all_by_label(title)
        .map(|n| n.rect())
        .min_by(|a, b| a.min.y.total_cmp(&b.min.y))
        .unwrap_or_else(|| panic!("no menu “{title}”"));
    click(harness, menu.center());
}

/// SPC-001, ANN-007, PRJ-003: the demo script of Showcase 2 in one session,
/// then a restart that reopens the saved project from File ▸ Open recent:
/// the room's stamp shows the same name and area as before saving, and the
/// note is back with its content, anchor and 5 mm size, drawn at the same
/// size.
#[test]
fn the_showcase_2_demo_script_runs_from_a_new_project_to_a_reopened_file() {
    let dir = data_dir("showcase-2");
    let project_dir = data_dir("showcase-2-project");
    std::fs::create_dir_all(&project_dir).unwrap();
    let path = project_dir.join("showcase-2.ifcx");
    let mut harness = harness_in(&dir, Preferences::default(), 1.0);
    assert!(harness.state_mut().workspace().doc.path.is_none(), "a new project");

    let room = step_1_room(&mut harness);
    step_2_rename(&mut harness, room);
    let note = step_3_text(&mut harness);
    step_4_save(&mut harness, &path);
    let stamp = stamp_lines(&mut harness, room);
    let space = harness.state_mut().workspace().doc.project.elements[&room].clone();
    let text = harness.state_mut().workspace().doc.project.elements[&note].clone();
    let drawn = laid_out(&mut harness, note);
    let ppm = points_per_metre(&mut harness);

    // Step 4, reopening: a restart with the preferences the session saved
    // starts with a new project; File ▸ Open recent opens the saved file.
    drop(harness);
    let saved = Preferences::load(&DataDirs::at(&dir).preferences_file());
    let mut harness = harness_in(&dir, saved, 1.0);
    assert!(spaces(&mut harness).is_empty(), "the restart starts empty");
    open_menu(&mut harness, "File");
    harness.get_by_label_contains("Open recent").hover();
    harness.run_steps(2);
    harness.get_by_label(&path.display().to_string()).click();
    harness.run_steps(4);
    let ws = harness.state_mut().workspace();
    assert_eq!(ws.doc.path.as_deref(), Some(path.as_path()), "the saved file is open");
    assert_eq!(ws.active_storey, space.storey, "the plan of the room's storey");
    let reopened = ws.doc.project.elements.get(&room).expect("the room is back").clone();
    let (before, after) = (space.as_space().unwrap(), reopened.as_space().unwrap());
    assert_eq!(after.long_name, "Living room");
    assert_eq!(after.boundary, before.boundary);
    assert_eq!(after.net_area(), before.net_area());
    assert_eq!(stamp_lines(&mut harness, room), stamp, "the same stamp");

    let ws = harness.state_mut().workspace();
    let element = ws.doc.project.elements.get(&note).expect("the note is back").clone();
    let (t, s) = (element.as_text().unwrap(), text.as_text().unwrap());
    assert_eq!(t.content, "Oak floor");
    assert_eq!(t.anchor, s.anchor);
    assert_eq!(t.size_mm, Some(5.0));
    // The reopened project fits its own zoom: compare the paper size, the
    // drawn size per point per metre.
    let again = laid_out(&mut harness, note);
    let paper = |size: f32, ppm: f64| f64::from(size) / ppm;
    let (expected, got) = (
        paper(drawn.size, ppm),
        paper(again.size, points_per_metre(&mut harness)),
    );
    assert!((expected - 0.5).abs() < 1e-6, "5 mm at 1:100 is 0.5 m: {expected}");
    assert!((got - expected).abs() < 1e-6, "drawn at {got} m, not {expected} m");

    drop(harness);
    std::fs::remove_dir_all(&project_dir).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
