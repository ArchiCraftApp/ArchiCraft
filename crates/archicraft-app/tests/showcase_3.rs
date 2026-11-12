//! The exit check of Showcase 3 (archicraft-tukkk): the demo script walked
//! through the application with injected pointer and key events, one session
//! from a new project to a restart that reopens the saved file, asserting
//! the model and what the window shows after each step.
//!
//! 1. With the column tool (C), one click places a column at the default
//!    size; in the Rotated method two clicks place a rotated one (COL-001,
//!    COL-002).
//! 2. The plan shows both columns cut; the 3D view shows them, and a click
//!    on one there selects it and shows its section, anchor and rotation in
//!    the properties strip (COL-001, COL-003).
//! 3. Pick up parameters in the toolbar and a click on a 0.30 m wall, then
//!    Inject parameters and a click on a 0.25 m wall: it becomes 0.30 m; the
//!    latch keeps Inject for two more walls; Undo restores the last one
//!    (ELM-003).
//! 4. Saved from the toolbar and reopened from File ▸ Open recent after a
//!    restart: the columns and walls are back, and the IFCX file has an
//!    `IfcColumn` with a `Body` and an `Axis` per column (COL-001, IFX-035,
//!    PRJ-003).

#![allow(clippy::unwrap_used)]

mod common;

use std::f64::consts::FRAC_PI_4;
use std::path::Path;

use archicraft_app::persistence::DataDirs;
use archicraft_app::persistence::prefs::Preferences;
use archicraft_app::tools::{ColumnMode, ToolKind, WallMode};
use archicraft_app::views::transfer::TransferMode;
use archicraft_app::views::view3d::pick;
use archicraft_app::workspace::{Pane, Request};
use archicraft_app::{ArchiCraftApp, tr};
use archicraft_geometry::geo::{Area, BoundingRect};
use archicraft_model::glam::DVec2;
use archicraft_model::kinds::{Anchor, CrossSection};
use archicraft_model::units::{format_angle, format_length};
use archicraft_model::{Element, ElementId};
use common::{assert_present, data_dir, harness_in, in_strip, open_strip_overflow};
use egui::accesskit::{Role, Toggled};
use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, NodeT, Queryable};

type App = Harness<'static, ArchiCraftApp>;

/// Where the one-click column stands.
const SINGLE_AT: DVec2 = DVec2::new(1.0, 1.0);
/// Where the rotated column stands, and the point of its second click, at
/// 45° from it.
const ROTATED_AT: [DVec2; 2] = [DVec2::new(4.0, 1.0), DVec2::new(5.0, 2.0)];
/// The y of the four 4 m walls of step 3, from x = 0 to x = 4: the first is
/// made 0.30 m thick, the others keep the template's 0.25 m.
const WALL_Y: [f64; 4] = [3.0, 4.5, 6.0, 7.5];

const PICK_UP: &str = "Pick up parameters";
const INJECT: &str = "Inject parameters";

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

fn click_toolbar(harness: &mut App, label: &str) {
    toolbar_button(harness, label).click();
    harness.run_steps(2);
}

/// Whether the toolbar's button labelled `label` shows as selected.
fn toolbar_selected(harness: &App, label: &str) -> bool {
    toolbar_button(harness, label).accesskit_node().toggled() == Some(Toggled::True)
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

/// Presses `key` with the pointer over `pos`, as a user in that view would.
fn press_over(harness: &mut App, pos: Pos2, key: Key) {
    harness.hover_at(pos);
    harness.run_steps(1);
    harness.key_press(key);
    harness.run_steps(2);
}

/// The screen position of the plan point `p` in the active plan view,
/// asserted to lie in the view.
fn plan_point(harness: &mut App, p: DVec2) -> Pos2 {
    let ws = harness.state_mut().workspace();
    let state = &ws.plan_views[&ws.active_storey];
    let pos = state.camera.to_screen(state.last_rect, p);
    assert!(state.last_rect.contains(pos), "{p} is outside the plan view");
    pos
}

fn tool(harness: &mut App) -> ToolKind {
    harness.state_mut().workspace().tool.kind
}

fn undo_len(harness: &mut App) -> usize {
    harness.state_mut().workspace().doc.history.undo_len()
}

fn element(harness: &mut App, id: ElementId) -> Element {
    harness.state_mut().workspace().doc.project.elements[&id].clone()
}

fn thickness(harness: &mut App, id: ElementId) -> f64 {
    element(harness, id).as_wall().unwrap().thickness
}

fn latched(harness: &mut App) -> Option<TransferMode> {
    harness.state_mut().workspace().transfer.latched()
}

/// The elements of the project for which `keep` holds, in id order.
fn elements(harness: &mut App, keep: impl Fn(&Element) -> bool) -> Vec<Element> {
    let project = &harness.state_mut().workspace().doc.project;
    let mut found: Vec<Element> = project.elements.values().filter(|e| keep(e)).cloned().collect();
    found.sort_by_key(|e| e.id);
    found
}

/// The columns of the project, in name order: C-001 first.
fn columns(harness: &mut App) -> Vec<Element> {
    let mut found = elements(harness, |e| e.as_column().is_some());
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// The length `metres` as the app shows it in a field, without its unit.
fn shown_length(harness: &mut App, metres: f64) -> String {
    let ws = harness.state_mut().workspace();
    format_length(metres, &ws.doc.project.settings.units, &ws.prefs.number_format(), false)
}

/// The value of the strip's text field labelled `label`.
fn strip_value(harness: &App, label: &str) -> String {
    harness
        .query_all(By::new().role(Role::TextInput).label(label))
        .find(|n| in_strip(harness, n.rect()))
        .unwrap_or_else(|| panic!("no field “{label}” in the strip"))
        .value()
        .unwrap_or_default()
}

/// Asserts that the plan of the active storey cuts `column` as its
/// 0.30 x 0.30 m section, turned by `rotation`, around `at`.
fn assert_cut(harness: &mut App, column: ElementId, at: DVec2, rotation: f64) {
    let ws = harness.state_mut().workspace();
    let storey = ws.active_storey;
    let plan = ws.doc.plan(storey);
    let cut = &plan.element(column).expect("the plan draws the column").cut;
    let [polygon] = cut.as_slice() else {
        panic!("one cut polygon: {cut:?}")
    };
    assert!(
        (polygon.unsigned_area() - 0.09).abs() < 1e-9,
        "{}",
        polygon.unsigned_area()
    );
    let bounds = polygon.bounding_rect().unwrap();
    // A square of side 0.3 turned by `rotation` spans 0.3 (|cos| + |sin|).
    let span = 0.3 * (rotation.cos().abs() + rotation.sin().abs());
    let centre = bounds.center();
    assert!(DVec2::new(centre.x, centre.y).distance(at) < 1e-9, "{centre:?}");
    assert!((bounds.width() - span).abs() < 1e-9, "{}", bounds.width());
    assert!((bounds.height() - span).abs() < 1e-9, "{}", bounds.height());
}

/// Step 1 (COL-001, COL-002): C selects the column tool in its Single
/// method; one click places C-001 at the D.2 default size, 0.30 x 0.30 m,
/// centred on the click and unrotated, in one undo step. In the Rotated
/// method a first click fixes C-002's position and a second click at 45°
/// from it sets its rotation. Returns both columns.
fn step_1_columns(harness: &mut App) -> [ElementId; 2] {
    press(harness, Key::C);
    assert_eq!(tool(harness), ToolKind::Column, "C selects the column tool");
    assert_eq!(harness.state_mut().workspace().tool.column_mode, ColumnMode::Single);

    let steps = undo_len(harness);
    let at = plan_point(harness, SINGLE_AT);
    click(harness, at);
    assert_eq!(undo_len(harness), steps + 1, "one click, one undo step");
    let placed = columns(harness);
    let [single] = placed.as_slice() else {
        panic!("one click places one column: {placed:?}")
    };
    assert_eq!(single.name, "C-001");
    let column = single.as_column().unwrap();
    assert_eq!(
        column.section,
        CrossSection::Rectangular {
            width: 0.3,
            height: 0.3
        },
        "the default size"
    );
    assert_eq!((column.anchor, column.rotation), (Anchor::Centre, 0.0));
    assert!(column.position.distance(SINGLE_AT) < 1e-9, "{}", column.position);
    let single = single.id;

    harness.get(By::new().role(Role::Button).label("Rotated")).click();
    harness.run_steps(2);
    assert_eq!(harness.state_mut().workspace().tool.column_mode, ColumnMode::Rotated);
    let [first, second] = ROTATED_AT.map(|p| plan_point(harness, p));
    click(harness, first);
    assert_eq!(columns(harness).len(), 1, "the first click only fixes the position");
    click(harness, second);
    assert_eq!(undo_len(harness), steps + 2, "the rotated column is one undo step");
    let placed = columns(harness);
    let [_, rotated] = placed.as_slice() else {
        panic!("two columns: {placed:?}")
    };
    assert_eq!(rotated.name, "C-002");
    let column = rotated.as_column().unwrap();
    assert!(column.position.distance(ROTATED_AT[0]) < 1e-9, "{}", column.position);
    assert!(
        (column.rotation - FRAC_PI_4).abs() < 1e-9,
        "rotated by {}",
        column.rotation
    );
    [single, rotated.id]
}

fn press(harness: &mut App, key: Key) {
    harness.key_press(key);
    harness.run_steps(2);
}

/// Step 2 (COL-001, COL-003, UI-007): the plan cuts both columns at their
/// 0.30 x 0.30 m section, the rotated one turned by 45°; the 3D view shows
/// both, and with the selection tool a click on the rotated column there
/// selects it and the strip shows its section kind, width, depth, anchor
/// and rotation and its end cuts.
fn step_2_views(harness: &mut App, [single, rotated]: [ElementId; 2]) {
    assert_cut(harness, single, SINGLE_AT, 0.0);
    assert_cut(harness, rotated, ROTATED_AT[0], FRAC_PI_4);

    press(harness, Key::Escape);
    press(harness, Key::V);
    assert_eq!(tool(harness), ToolKind::Select);
    let storey = harness.state_mut().workspace().active_storey;
    let on_rotated = open_3d_at(harness, rotated);
    // The 3D view shows the one-click column too.
    open_3d_at(harness, single);
    click(harness, on_rotated);
    let selection: Vec<ElementId> = harness.state_mut().workspace().selection.iter().copied().collect();
    assert_eq!(selection, [rotated], "a click in the 3D view selects the column");

    open_strip_overflow(harness);
    for label in ["Cross-section", "Anchor point", "Top cut", "Bottom cut"] {
        assert_present(harness, label);
    }
    let width = shown_length(harness, 0.3);
    assert_eq!(strip_value(harness, "Width"), width);
    assert_eq!(strip_value(harness, "Depth"), width);
    let angle = {
        let ws = harness.state_mut().workspace();
        format_angle(
            FRAC_PI_4,
            &ws.doc.project.settings.units,
            &ws.prefs.number_format(),
            false,
        )
    };
    assert_eq!(strip_value(harness, "Rotation"), angle);
    assert!(
        harness
            .query(By::new().role(Role::ComboBox).value("Rectangular"))
            .is_some(),
        "the section kind"
    );
    // In the strip's row the anchor grid opens from a combo box that shows
    // the point; in the overflow popup the grid shows as it is (UI-002).
    let combo = By::new().role(Role::ComboBox).label("Anchor point").value("Centre");
    if harness.query(combo).is_none() {
        let centre = harness.get(By::new().role(Role::Button).label("Centre"));
        assert_eq!(centre.accesskit_node().toggled(), Some(Toggled::True), "the anchor");
    }

    // Back to the plan for the walls, with nothing selected.
    let ws = harness.state_mut().workspace();
    ws.requests.push(Request::OpenPane(Pane::Plan(storey)));
    harness.run_steps(3);
    let free = plan_point(harness, DVec2::new(8.0, 1.0));
    press_over(harness, free, Key::Escape);
    assert!(harness.state_mut().workspace().selection.is_empty());
}

/// Opens the project's first 3D view and returns a point of it that shows
/// `element`, found by the view's own picking ray.
fn open_3d_at(harness: &mut App, element: ElementId) -> Pos2 {
    let ws = harness.state_mut().workspace();
    let view = ws.doc.project.views.first().unwrap().id;
    ws.requests.push(Request::OpenPane(Pane::View3d(view)));
    harness.run_steps(3);
    let ws = harness.state_mut().workspace();
    assert_eq!(ws.active_pane, Some(Pane::View3d(view)));
    let geometry = ws.doc.geometry();
    let state = &ws.views_3d[&view];
    let rect = state.last_rect();
    for iy in 1..80 {
        for ix in 1..80 {
            let p = rect.min + egui::vec2(rect.width() * ix as f32 / 80.0, rect.height() * iy as f32 / 80.0);
            let (origin, dir) = state.camera.ray(rect, p);
            if pick(&ws.doc.project, &geometry, origin, dir).is_some_and(|(id, _)| id == element) {
                return p;
            }
        }
    }
    panic!("no point of the 3D view shows {element:?}");
}

/// Draws a 4 m wall from (0, `y`) to (4, `y`) with the wall tool's Single
/// method and returns it.
fn draw_wall(harness: &mut App, y: f64) -> ElementId {
    let before: Vec<ElementId> = elements(harness, |e| e.as_wall().is_some())
        .iter()
        .map(|e| e.id)
        .collect();
    for x in [0.0, 4.0] {
        let at = plan_point(harness, DVec2::new(x, y));
        click(harness, at);
    }
    let after = elements(harness, |e| e.as_wall().is_some());
    let new: Vec<ElementId> = after.iter().map(|e| e.id).filter(|id| !before.contains(id)).collect();
    let [wall] = new[..] else {
        panic!("two clicks draw one wall: {new:?}")
    };
    wall
}

/// Step 3 (ELM-003, EDT-020, UND-001): four walls drawn with the wall
/// tool's Single method at the template's 0.25 m, the first set to 0.30 m
/// in the strip. With the selection tool active, Pick up parameters in the
/// toolbar latches pick-up; a click on the 0.30 m wall makes 0.30 m the
/// wall tool's thickness. Inject parameters switches the latch to inject; a
/// click on a 0.25 m wall makes it 0.30 m, and the latch stays for two more
/// clicks. Esc ends the latch, and Undo in the toolbar restores the last
/// wall to 0.25 m in one step, keeping the other injections.
fn step_3_transfer(harness: &mut App) -> [ElementId; 4] {
    press(harness, Key::W);
    assert_eq!(tool(harness), ToolKind::Wall);
    harness.get_by_label(WallMode::Single.label()).click();
    harness.run_steps(2);
    assert_eq!(harness.state_mut().workspace().tool.wall_mode, WallMode::Single);
    let walls = WALL_Y.map(|y| draw_wall(harness, y));
    for wall in walls {
        assert_eq!(thickness(harness, wall), 0.25, "the template's thickness");
    }

    press(harness, Key::Escape);
    press(harness, Key::V);
    let on = |harness: &mut App, i: usize| plan_point(harness, DVec2::new(2.0, WALL_Y[i]));
    let at = on(harness, 0);
    click(harness, at);
    let selection: Vec<ElementId> = harness.state_mut().workspace().selection.iter().copied().collect();
    assert_eq!(selection, [walls[0]]);
    let typed = shown_length(harness, 0.3);
    open_strip_overflow(harness);
    let field = harness
        .query_all(By::new().role(Role::TextInput).label("Thickness"))
        .find(|n| in_strip(harness, n.rect()))
        .unwrap()
        .rect();
    press_at(harness, field.left_center() + egui::vec2(4.0, 0.0));
    harness.run_steps(2);
    harness.event(Event::Text(typed));
    harness.key_press(Key::Enter);
    harness.run_steps(3);
    assert!((thickness(harness, walls[0]) - 0.3).abs() < 1e-9, "the 0.30 m wall");
    let free = plan_point(harness, DVec2::new(8.0, 1.0));
    press_over(harness, free, Key::Escape);
    assert!(harness.state_mut().workspace().selection.is_empty());

    click_toolbar(harness, PICK_UP);
    assert_eq!(latched(harness), Some(TransferMode::PickUp));
    assert!(toolbar_selected(harness, PICK_UP));
    let at = on(harness, 0);
    click(harness, at);
    assert_eq!(
        tool(harness),
        ToolKind::Wall,
        "picking up a wall activates the wall tool"
    );
    let defaults = {
        let p = &harness.state_mut().workspace().doc.project;
        p.new_wall(p.storeys.ground(), DVec2::ZERO, DVec2::X, 0)
    };
    assert!((defaults.as_wall().unwrap().thickness - 0.3).abs() < 1e-9, "picked up");
    assert!(harness.state_mut().workspace().selection.is_empty(), "nothing selected");

    click_toolbar(harness, INJECT);
    assert_eq!(latched(harness), Some(TransferMode::Inject));
    assert!(toolbar_selected(harness, INJECT) && !toolbar_selected(harness, PICK_UP));
    let steps = undo_len(harness);
    for (n, &wall) in walls.iter().enumerate().skip(1) {
        let at = on(harness, n);
        click(harness, at);
        assert!((thickness(harness, wall) - 0.3).abs() < 1e-9, "wall {n} becomes 0.30 m");
        assert_eq!(latched(harness), Some(TransferMode::Inject), "the latch stays");
        assert!(toolbar_selected(harness, INJECT));
    }
    assert_eq!(undo_len(harness), steps + 3, "one undo step per injection");
    let at = on(harness, 3);
    press_over(harness, at, Key::Escape);
    assert_eq!(latched(harness), None, "Esc ends the latch");
    assert!(!toolbar_selected(harness, INJECT));

    click_toolbar(harness, "Undo");
    assert_eq!(thickness(harness, walls[3]), 0.25, "undo restores the last wall");
    for &wall in &walls[..3] {
        assert!((thickness(harness, wall) - 0.3).abs() < 1e-9);
    }
    walls
}

/// Step 4, saving (PRJ-003, IFX-035): the toolbar's Save asks for the
/// location of the new project and writes the file; it holds an
/// `IfcColumn` node per column, each with a `Body` and an `Axis` child.
fn step_4_save(harness: &mut App, path: &Path) {
    harness.state_mut().stub_save_dialog(Some(path.to_owned()));
    click_toolbar(harness, "Save");
    assert_eq!(harness.state().save_dialogs_asked(), 1, "the first save asks");
    assert!(!harness.state_mut().workspace().doc.is_dirty());
    assert_present(harness, &tr!("status-saved", path = path.display().to_string()));

    let file: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let nodes = file["data"].as_array().unwrap();
    let ifc_columns: Vec<&serde_json::Value> = nodes
        .iter()
        .filter(|n| n["attributes"]["bsi::ifc::class"]["code"] == "IfcColumn")
        .collect();
    let mut names: Vec<&str> = ifc_columns
        .iter()
        .map(|n| n["attributes"]["bsi::ifc::prop::Name"].as_str().unwrap())
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["C-001", "C-002"], "an IfcColumn per column");
    for column in ifc_columns {
        assert!(column["children"]["Body"].is_string(), "{column}");
        assert!(column["children"]["Axis"].is_string(), "{column}");
    }
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

/// COL-001, COL-002, ELM-003, IFX-035, PRJ-003: the demo script of
/// Showcase 3 in one session, then a restart that reopens the saved project
/// from File ▸ Open recent: both columns and the four walls are back as
/// saved, and the plan cuts the columns as before.
#[test]
fn the_showcase_3_demo_script_runs_from_a_new_project_to_a_reopened_file() {
    let dir = data_dir("showcase-3");
    let project_dir = data_dir("showcase-3-project");
    std::fs::create_dir_all(&project_dir).unwrap();
    let path = project_dir.join("showcase-3.ifcx");
    let mut harness = harness_in(&dir, Preferences::default(), 1.0);
    assert!(harness.state_mut().workspace().doc.path.is_none(), "a new project");

    let placed = step_1_columns(&mut harness);
    step_2_views(&mut harness, placed);
    let walls = step_3_transfer(&mut harness);
    step_4_save(&mut harness, &path);
    let saved: Vec<Element> = elements(&mut harness, |e| e.as_column().is_some() || e.as_wall().is_some());
    assert_eq!(saved.len(), 6);

    // Step 4, reopening: a restart with the preferences the session saved
    // starts with a new project; File ▸ Open recent opens the saved file.
    drop(harness);
    let prefs = Preferences::load(&DataDirs::at(&dir).preferences_file());
    let mut harness = harness_in(&dir, prefs, 1.0);
    assert!(columns(&mut harness).is_empty(), "the restart starts empty");
    open_menu(&mut harness, "File");
    harness.get_by_label_contains("Open recent").hover();
    harness.run_steps(2);
    harness.get_by_label(&path.display().to_string()).click();
    harness.run_steps(4);
    let ws = harness.state_mut().workspace();
    assert_eq!(ws.doc.path.as_deref(), Some(path.as_path()), "the saved file is open");
    let reopened: Vec<Element> = elements(&mut harness, |e| e.as_column().is_some() || e.as_wall().is_some());
    assert_eq!(reopened.len(), saved.len());
    for (before, after) in saved.iter().zip(&reopened) {
        assert_eq!(after.id, before.id);
        assert_eq!(after.name, before.name);
        assert_eq!(after.kind, before.kind, "{} round-trips", before.name);
    }
    assert!((thickness(&mut harness, walls[2]) - 0.3).abs() < 1e-9);
    assert_eq!(thickness(&mut harness, walls[3]), 0.25);
    assert_cut(&mut harness, placed[0], SINGLE_AT, 0.0);
    assert_cut(&mut harness, placed[1], ROTATED_AT[0], FRAC_PI_4);

    drop(harness);
    std::fs::remove_dir_all(&project_dir).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
