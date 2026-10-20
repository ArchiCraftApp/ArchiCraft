//! Regression net for the done application requirements (UI-006, UI-007,
//! STR-003, STR-004, NFR-USE-004, VIEW-002, VIEW-014, WAL-001,
//! NFR-PORT-005).
//!
//! Every test is named after the requirement it pins, so the traceability
//! table in docs/testing.md and the status script can find it. The tests run
//! the whole application without a GPU and find widgets by their accessible
//! labels (AccessKit). Tests that already exist elsewhere (the unit tests in
//! camera.rs and tools/mod.rs) are referenced in docs/testing.md rather than
//! repeated here.

mod common;

use archicraft_app::ArchiCraftApp;
use archicraft_app::input::{InputConsumer, Outcome};
use archicraft_app::tools::{ToolKind, WallMode};
use archicraft_app::views::camera::Camera2d;
use archicraft_app::workspace::{Pane, SceneKey};
use archicraft_model::glam::DVec2;
use archicraft_model::project::Projection;
use archicraft_model::{Command, ElementId, StoreyId, ViewId};
use common::{assert_present, harness, harness_at};
use egui::accesskit::Role;
use egui::{Event, Id, Key, Modifiers, MouseWheelUnit, PointerButton, Pos2, Rect, TouchPhase, Vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, NodeT, Queryable};
use egui_tiles::Tree;

type App = Harness<'static, ArchiCraftApp>;

/// Ctrl as egui-winit reports it on Windows and Linux, where it is also the
/// command key; the harness reports Linux.
const CTRL: Modifiers = Modifiers {
    ctrl: true,
    command: true,
    ..Modifiers::NONE
};

fn ground(h: &mut App) -> StoreyId {
    h.state_mut().workspace().doc.project.storeys.ground()
}

/// The 3D view of the template with `projection`.
fn view(h: &mut App, projection: Projection) -> ViewId {
    let views = &h.state_mut().workspace().doc.project.views;
    views
        .iter()
        .find(|v| v.projection == projection)
        .map(|v| v.id)
        .expect("the template has this 3D view")
}

/// Adds a wall on the ground floor through a command and returns its id.
fn add_wall(h: &mut App, start: DVec2, end: DVec2) -> ElementId {
    let ws = h.state_mut().workspace();
    let storey = ws.doc.project.storeys.ground();
    let wall = ws.doc.project.new_wall(storey, start, end, 0);
    let id = wall.id;
    assert!(ws.execute(Command::AddElements(vec![wall])));
    id
}

fn wall_ends(h: &mut App, id: ElementId) -> (DVec2, DVec2) {
    let element = h.state_mut().workspace().doc.project.element(id).cloned();
    let wall = element.as_ref().and_then(|e| e.as_wall()).expect("a wall");
    (wall.start, wall.end)
}

/// Where the plan of `storey` was drawn last frame, and its camera.
fn plan_camera(h: &mut App, storey: StoreyId) -> (Rect, Camera2d) {
    let ws = h.state_mut().workspace();
    let state = ws.plan_views.get(&storey).expect("the plan was drawn");
    (state.last_rect, state.camera)
}

fn active_pane(h: &mut App) -> Option<Pane> {
    h.state_mut().workspace().active_pane
}

/// Whether `pane` is the visible tab of its tab group.
fn is_active_tab(h: &mut App, pane: Pane) -> bool {
    let tree = h.state_mut().tree_mut();
    tree.tiles
        .find_pane(&pane)
        .is_some_and(|tile| tree.active_tiles().contains(&tile))
}

fn pointer_button(h: &App, pos: Pos2, button: PointerButton, pressed: bool) {
    h.event(Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: Modifiers::NONE,
    });
}

/// Moves the pointer to `pos`, then clicks there with the primary button.
fn click_at(h: &mut App, pos: Pos2) {
    h.hover_at(pos);
    h.run_steps(1);
    pointer_button(h, pos, PointerButton::Primary, true);
    pointer_button(h, pos, PointerButton::Primary, false);
    h.run_steps(1);
}

/// The browser row of the storey called `name` (its plan tab is called
/// "Plan: <name>").
fn browser_row_label(h: &App, name: &str) -> String {
    h.query_all_by_label_contains(name)
        .filter_map(|n| n.accesskit_node().label())
        .find(|label| !label.starts_with("Plan: "))
        .unwrap_or_else(|| panic!("no browser row for “{name}”"))
}

/// The tab of the view `title`: the button with a close button inside (the
/// browser and the status line name views too).
fn find_tab<'h>(h: &'h App, title: &'h str) -> Option<egui_kittest::Node<'h>> {
    let closes: Vec<Rect> = h.query_all_by_label("Close").map(|n| n.rect()).collect();
    h.query_all_by_label(title)
        .find(|n| n.accesskit_node().role() == Role::Button && closes.iter().any(|c| n.rect().contains_rect(*c)))
}

fn tab<'h>(h: &'h App, title: &'h str) -> egui_kittest::Node<'h> {
    find_tab(h, title).unwrap_or_else(|| panic!("no tab “{title}”"))
}

fn has_tab(h: &App, title: &str) -> bool {
    find_tab(h, title).is_some()
}

/// Opens the menu `title` of the menu bar (the browser has headers with the
/// same names).
fn open_menu(h: &mut App, title: &str) {
    let menu = h
        .query_all_by_label(title)
        .map(|n| n.rect())
        .min_by(|a, b| a.min.y.total_cmp(&b.min.y))
        .unwrap_or_else(|| panic!("no menu “{title}”"));
    click_at(h, menu.center());
    h.run_steps(1);
}

fn has_modal(h: &App) -> bool {
    h.ctx.memory(|m| m.top_modal_layer()).is_some()
}

/// UI-006: open views are tabs; Ctrl+Tab activates the next and
/// Ctrl+Shift+Tab the previous tab, and a closed plan tab reopens from the
/// project browser.
#[test]
fn ui_006_views_are_tabs_with_next_and_previous_tab_shortcuts() {
    let mut h = harness("ui-006");
    let storey = ground(&mut h);
    let perspective = view(&mut h, Projection::Perspective);
    let parallel = view(&mut h, Projection::Parallel);
    // Tabs: plan, 3D perspective, and 3D parallel opened from the browser.
    h.get_by_label("3D parallel").click();
    h.run_steps(2);
    assert_eq!(active_pane(&mut h), Some(Pane::View3d(parallel)));
    assert!(is_active_tab(&mut h, Pane::View3d(parallel)));

    let ctrl_shift = CTRL.plus(Modifiers::SHIFT);
    for (modifiers, expected) in [
        (CTRL, Pane::Plan(storey)),
        (CTRL, Pane::View3d(perspective)),
        (ctrl_shift, Pane::Plan(storey)),
        (ctrl_shift, Pane::View3d(parallel)),
        (ctrl_shift, Pane::View3d(perspective)),
    ] {
        h.key_press_modifiers(modifiers, Key::Tab);
        h.run_steps(2);
        assert_eq!(active_pane(&mut h), Some(expected), "after {modifiers:?}+Tab");
        assert!(is_active_tab(&mut h, expected), "{expected:?} is not the visible tab");
    }

    // Close the plan tab with its close button.
    let tab = tab(&h, "Plan: 00 Ground floor").rect();
    h.query_all_by_label("Close")
        .find(|close| tab.contains_rect(close.rect()))
        .expect("the plan tab has a close button")
        .click();
    h.run_steps(2);
    assert!(!has_tab(&h, "Plan: 00 Ground floor"));
    assert!(h.state_mut().tree_mut().tiles.find_pane(&Pane::Plan(storey)).is_none());

    // Reopen it from the project browser.
    let row = browser_row_label(&h, "00 Ground floor");
    h.get_by_label(&row).click();
    h.run_steps(2);
    assert!(has_tab(&h, "Plan: 00 Ground floor"));
    assert!(is_active_tab(&mut h, Pane::Plan(storey)));
}

/// UI-007: a plan and a 3D view side by side; selecting a wall in the plan
/// selects it in the 3D view too.
#[test]
fn ui_007_plan_and_3d_side_by_side_share_the_selection() {
    let mut h = harness("ui-007");
    let storey = ground(&mut h);
    let perspective = view(&mut h, Projection::Perspective);
    let wall = add_wall(&mut h, DVec2::ZERO, DVec2::new(4.0, 0.0));
    *h.state_mut().tree_mut() = Tree::new_horizontal(
        Id::new("archicraft-views"),
        vec![Pane::Plan(storey), Pane::View3d(perspective)],
    );
    h.run_steps(3);

    // Both panes are shown, the plan left of the 3D view.
    let plan_tab = tab(&h, "Plan: 00 Ground floor").rect();
    let view_tab = tab(&h, "3D perspective").rect();
    assert!(plan_tab.max.x < view_tab.min.x, "{plan_tab:?} {view_tab:?}");
    assert!(
        (plan_tab.min.y - view_tab.min.y).abs() < 1.0,
        "{plan_tab:?} {view_tab:?}"
    );
    assert!(is_active_tab(&mut h, Pane::Plan(storey)));
    assert!(is_active_tab(&mut h, Pane::View3d(perspective)));
    // Both panes were drawn: the 3D view was a hidden tab before the split,
    // so its state exists only because the split showed it.
    let (rect, camera) = plan_camera(&mut h, storey);
    let view_rect = h.state_mut().workspace().views_3d[&perspective].last_rect();
    assert!(rect.max.x <= view_rect.min.x, "{rect:?} {view_rect:?}");

    // Select the wall by clicking it in the plan.
    let pos = camera.to_screen(rect, DVec2::new(2.0, 0.0));
    assert!(rect.contains(pos), "the wall is visible in the plan");
    click_at(&mut h, pos);
    let ws = h.state_mut().workspace();
    assert_eq!(ws.selection.iter().copied().collect::<Vec<_>>(), vec![wall]);
    assert_eq!(SceneKey::of(ws).selection, vec![wall]);
}

/// Clicks the storey number `number` in the Storeys dialog.
fn select_in_storeys_dialog(h: &mut App, number: &str) {
    let rows: Vec<Rect> = h.query_all_by_label(number).map(|n| n.rect()).collect();
    // The number column is the left-most label with this text.
    let left = rows
        .into_iter()
        .min_by(|a, b| a.min.x.total_cmp(&b.min.x))
        .expect("the storey row");
    click_at(h, left.center());
}

fn storey_names(h: &mut App) -> Vec<String> {
    let storeys = &h.state_mut().workspace().doc.project.storeys;
    storeys.highest_first().map(|s| s.name.clone()).collect()
}

/// Inserts a storey next to the ground floor through the browser's context
/// menu entry `entry`; checks that the browser lists it in the next frame and
/// returns it.
fn insert_from_browser(h: &mut App, entry: &str) -> (StoreyId, String) {
    let ids = |h: &mut App| -> Vec<StoreyId> {
        let storeys = &h.state_mut().workspace().doc.project.storeys;
        storeys.highest_first().map(|s| s.id).collect()
    };
    let before = ids(h);
    let row = browser_row_label(h, "00 Ground floor");
    h.get_by_label(&row).click_secondary();
    h.run_steps(2);
    h.get_by_label(entry).click();
    h.run_steps(1);
    let id = ids(h)
        .into_iter()
        .find(|id| !before.contains(id))
        .unwrap_or_else(|| panic!("{entry} inserted no storey"));
    let name = h
        .state_mut()
        .workspace()
        .doc
        .project
        .storeys
        .get(id)
        .map(|s| s.name.clone())
        .expect("the storey");
    h.run_steps(1);
    browser_row_label(h, &name);
    (id, name)
}

/// STR-003: insert above, insert below, rename and delete in the Storeys
/// dialog and in the browser's context menu; the browser shows a new storey
/// in the next frame.
#[test]
fn str_003_storeys_are_inserted_renamed_and_deleted_from_dialog_and_browser() {
    let mut h = harness("str-003");
    open_menu(&mut h, "Project");
    h.get_by_label("Storeys…").click();
    h.run_steps(2);
    for label in ["Insert above", "Insert below", "Delete…"] {
        assert_present(&h, label);
    }

    // Insert above and below the first floor.
    select_in_storeys_dialog(&mut h, "1");
    h.get_by_label("Insert above").click();
    h.run_steps(1);
    h.run_steps(1);
    browser_row_label(&h, "02 Storey");
    h.get_by_label("Insert below").click();
    h.run_steps(1);
    h.run_steps(1);
    browser_row_label(&h, "01 Storey");
    assert_eq!(
        storey_names(&mut h),
        ["02 Storey", "01 First floor", "01 Storey", "00 Ground floor"]
    );

    // Rename in the name field.
    let name_field = || By::new().role(Role::TextInput).value("01 First floor");
    h.get(name_field()).click();
    h.run_steps(1);
    h.get(name_field()).type_text("Upper floor");
    h.key_press(Key::Enter);
    h.run_steps(2);
    h.run_steps(1);
    browser_row_label(&h, "Upper floor");
    assert_eq!(
        storey_names(&mut h),
        ["02 Storey", "Upper floor", "01 Storey", "00 Ground floor"]
    );

    // Delete asks first (STR-004), then removes the storey.
    select_in_storeys_dialog(&mut h, "3");
    h.get_by_label("Delete…").click();
    h.run_steps(2);
    h.get_by_label("Delete storey “02 Storey”?");
    h.get_by_label("Delete storey").click();
    h.run_steps(2);
    assert_eq!(storey_names(&mut h), ["Upper floor", "01 Storey", "00 Ground floor"]);
    assert!(h.query_all_by_label_contains("02 Storey").next().is_none());

    // The browser's context menu of a storey offers the same four (in a
    // fresh window, so that the dialog does not cover the browser).
    let mut h = harness("str-003-browser");
    let row = browser_row_label(&h, "00 Ground floor");
    h.get_by_label(&row).click_secondary();
    h.run_steps(2);
    for label in [
        "Insert storey above",
        "Insert storey below",
        "Rename, elevation, height…",
        "Delete storey…",
    ] {
        assert_present(&h, label);
    }
    h.key_press(Key::Escape);
    h.run_steps(2);
    let below = insert_from_browser(&mut h, "Insert storey below");
    let above = insert_from_browser(&mut h, "Insert storey above");
    let ids = h
        .state_mut()
        .workspace()
        .doc
        .project
        .storeys
        .highest_first()
        .map(|s| s.id)
        .collect::<Vec<_>>();
    assert_eq!(ids.last(), Some(&below.0), "inserted below the ground floor");
    assert_eq!(
        ids.get(ids.len() - 3),
        Some(&above.0),
        "inserted above the ground floor"
    );
    let row = browser_row_label(&h, &below.1);
    h.get_by_label(&row).click_secondary();
    h.run_steps(2);
    h.get_by_label("Delete storey…").click();
    h.run_steps(2);
    h.get_by_label(&format!("Delete storey “{}”?", below.1));
    h.get_by_label("Delete storey").click();
    h.run_steps(2);
    assert!(h.state_mut().workspace().doc.project.storeys.get(below.0).is_none());
    assert!(h.query_all_by_label(&row).next().is_none());
    // Rename from the context menu opens the dialog with the storey chosen.
    let row = browser_row_label(&h, "00 Ground floor");
    h.get_by_label(&row).click_secondary();
    h.run_steps(2);
    h.get_by_label("Rename, elevation, height…").click();
    h.run_steps(2);
    h.get(By::new().role(Role::TextInput).value("00 Ground floor"));
}

/// STR-004 and NFR-USE-004: deleting a storey with elements asks how to
/// handle them, deleting an element does not ask and is undone by Ctrl+Z,
/// and New with unsaved changes asks to save or discard.
#[test]
fn str_004_nfr_use_004_only_storey_deletion_and_unsaved_changes_ask() {
    let mut h = harness("str-004");
    let storey = ground(&mut h);
    let walls = [
        add_wall(&mut h, DVec2::ZERO, DVec2::new(4.0, 0.0)),
        add_wall(&mut h, DVec2::new(4.0, 0.0), DVec2::new(4.0, 3.0)),
        add_wall(&mut h, DVec2::new(4.0, 3.0), DVec2::new(0.0, 3.0)),
    ];
    h.run_steps(2);

    // Deleting an element: no prompt, undone by Ctrl+Z.
    h.state_mut().workspace().selection.insert(walls[0]);
    h.run_steps(1);
    h.key_press(Key::Delete);
    h.run_steps(2);
    assert_eq!(h.state_mut().workspace().doc.project.elements.len(), 2);
    assert!(!has_modal(&h));
    assert_eq!(h.state_mut().prompt_title(), None);
    h.key_press_modifiers(CTRL, Key::Z);
    h.run_steps(2);
    assert_eq!(h.state_mut().workspace().doc.project.elements.len(), 3);

    // Deleting the storey asks, showing the count and both choices.
    let before: Vec<_> = walls.iter().map(|w| wall_ends(&mut h, *w)).collect();
    let row = browser_row_label(&h, "00 Ground floor");
    h.get_by_label(&row).click_secondary();
    h.run_steps(2);
    h.get_by_label("Delete storey…").click();
    h.run_steps(2);
    assert!(has_modal(&h));
    h.get_by_label_contains("contains 3 element(s)");
    h.get_by_label_contains("Move them to another storey, keeping their position");
    h.get_by_label("Delete them");
    // The target storey choice shows the storey above.
    h.get_by_value("01 First floor");
    h.get_by_label("Delete storey").click();
    h.run_steps(2);
    assert!(!has_modal(&h));
    let ws = h.state_mut().workspace();
    let first = ws.doc.project.storeys.highest_first().next().map(|s| s.id);
    assert!(ws.doc.project.storeys.get(storey).is_none());
    for wall in walls {
        assert_eq!(ws.doc.project.element(wall).map(|e| Some(e.storey)), Some(first));
    }
    let after: Vec<_> = walls.iter().map(|w| wall_ends(&mut h, *w)).collect();
    assert_eq!(after, before, "the walls keep their plan position");

    // New with unsaved changes asks to save or discard.
    h.key_press_modifiers(CTRL, Key::N);
    h.run_steps(2);
    assert_eq!(h.state_mut().prompt_title().as_deref(), Some("Save changes?"));
    for label in ["Save", "Don't save", "Cancel"] {
        assert_present(&h, label);
    }
    h.get_by_label("Don't save").click();
    h.run_steps(2);
    assert_eq!(h.state_mut().prompt_title(), None);
    assert!(h.state_mut().workspace().doc.project.elements.is_empty());
}

/// VIEW-002: a new project has a plan per storey and one perspective and
/// one parallel 3D view, all listed in the project browser.
#[test]
fn view_002_a_new_project_has_a_plan_per_storey_and_two_3d_views() {
    let mut h = harness("view-002");
    let storeys: Vec<(StoreyId, String)> = {
        let project = &h.state_mut().workspace().doc.project;
        project
            .storeys
            .highest_first()
            .map(|s| (s.id, s.name.clone()))
            .collect()
    };
    assert_eq!(storeys.len(), 2);
    for (id, name) in storeys {
        let row = browser_row_label(&h, &name);
        h.get_by_label(&row).click();
        h.run_steps(2);
        assert!(has_tab(&h, &format!("Plan: {name}")));
        assert!(is_active_tab(&mut h, Pane::Plan(id)));
    }
    let projections: Vec<Projection> = {
        let project = &h.state_mut().workspace().doc.project;
        project.views.iter().map(|v| v.projection).collect()
    };
    assert_eq!(projections, [Projection::Perspective, Projection::Parallel]);
    assert_present(&h, "3D perspective");
    assert_present(&h, "3D parallel");
}

/// Moves the pointer over the plan to `pos`.
fn hover_plan(h: &mut App, pos: Pos2) {
    h.hover_at(pos);
    h.run_steps(1);
}

/// VIEW-014: middle-button pan, wheel and pinch zoom about the cursor, fit
/// to window and zoom to selection. Previous/next zoom with at least 20
/// steps is pinned by `zoom_history_keeps_20_steps` in camera.rs.
#[test]
fn view_014_plan_navigation_pans_zooms_and_fits() {
    let mut h = harness("view-014");
    let storey = ground(&mut h);
    let near = add_wall(&mut h, DVec2::ZERO, DVec2::new(4.0, 0.0));
    let far = add_wall(&mut h, DVec2::new(100.0, 50.0), DVec2::new(104.0, 50.0));
    h.run_steps(2);
    let (rect, start) = plan_camera(&mut h, storey);

    // Middle-button drag: the model point under the pointer follows it.
    let from = rect.center();
    let to = from + Vec2::new(60.0, -40.0);
    let grabbed = start.to_world(rect, from);
    hover_plan(&mut h, from);
    pointer_button(&h, from, PointerButton::Middle, true);
    h.run_steps(1);
    for i in 1..=4 {
        h.hover_at(from + (to - from) * (i as f32 / 4.0));
        h.run_steps(1);
    }
    pointer_button(&h, to, PointerButton::Middle, false);
    h.run_steps(1);
    let (_, panned) = plan_camera(&mut h, storey);
    assert_ne!(panned.center, start.center);
    assert!(
        panned.to_world(rect, to).distance(grabbed) < 1e-9,
        "{:?} is not under the pointer",
        grabbed
    );

    // Ctrl+wheel and pinch zoom keep the point under the cursor.
    let cursor = rect.center() + Vec2::new(-120.0, 70.0);
    hover_plan(&mut h, cursor);
    let zoom_events = [
        Event::MouseWheel {
            unit: MouseWheelUnit::Line,
            delta: Vec2::new(0.0, 2.0),
            phase: TouchPhase::Move,
            modifiers: CTRL,
        },
        Event::Zoom(0.5),
    ];
    for event in zoom_events {
        let (_, before) = plan_camera(&mut h, storey);
        let anchor = before.to_world(rect, cursor);
        h.event_modifiers(event.clone(), CTRL);
        h.run_steps(1);
        let (_, after) = plan_camera(&mut h, storey);
        assert_ne!(after.scale, before.scale, "{event:?} did not zoom");
        assert!(
            after.to_world(rect, cursor).distance(anchor) < 1e-9,
            "{event:?} moved the point under the cursor"
        );
    }

    // Fit to window (F) shows the whole model.
    let ends: Vec<DVec2> = [near, far]
        .into_iter()
        .flat_map(|w| {
            let (a, b) = wall_ends(&mut h, w);
            [a, b]
        })
        .collect();
    let (_, camera) = plan_camera(&mut h, storey);
    assert!(!ends.iter().all(|p| rect.contains(camera.to_screen(rect, *p))));
    h.remove_cursor();
    h.key_press(Key::F);
    h.run_steps(2);
    let (_, fitted) = plan_camera(&mut h, storey);
    for p in &ends {
        assert!(
            rect.contains(fitted.to_screen(rect, *p)),
            "{p} is outside the fitted view"
        );
    }

    // Zoom to selection (Shift+F) shows the selected wall, larger.
    h.state_mut().workspace().selection.insert(near);
    h.key_press_modifiers(Modifiers::SHIFT, Key::F);
    h.run_steps(2);
    let (_, zoomed) = plan_camera(&mut h, storey);
    assert!(zoomed.scale > fitted.scale);
    let (a, b) = wall_ends(&mut h, near);
    for p in [a, b] {
        assert!(rect.contains(zoomed.to_screen(rect, p)), "{p} is outside");
    }
    let (c, _) = wall_ends(&mut h, far);
    assert!(
        !rect.contains(zoomed.to_screen(rect, c)),
        "the view shows more than the selection"
    );
}

/// Runs the tool's outcome on the workspace.
fn run_tool(h: &mut App, storey: StoreyId, points: &[(f64, f64)]) {
    let ws = h.state_mut().workspace();
    for &(x, y) in points {
        let outcome = ws.tool.commit(&ws.doc.project, storey, DVec2::new(x, y));
        if let Outcome::Execute(command) = outcome {
            assert!(ws.execute(command));
        }
    }
}

/// The walls created since `count` elements existed, in creation order, as
/// (start, end).
fn new_walls(h: &mut App, count: usize) -> Vec<(DVec2, DVec2)> {
    let project = &h.state_mut().workspace().doc.project;
    project
        .elements
        .values()
        .skip(count)
        .map(|e| {
            let w = e.as_wall().expect("a wall");
            (w.start, w.end)
        })
        .collect()
}

/// WAL-001: the four wall modes are buttons of the wall tool; chained input
/// creates one wall per segment with shared ends, the rectangles four walls
/// sharing their corners. The tool's own tests (wall_chain_closes_into_joined_walls,
/// rectangle_tools in tools/mod.rs) pin names and orientation.
#[test]
fn wal_001_wall_modes_single_chained_rectangle_and_rotated_rectangle() {
    let mut h = harness("wal-001");
    let storey = ground(&mut h);
    h.get_by_label("Wall").click();
    h.run_steps(2);
    assert_eq!(h.state_mut().workspace().tool.kind, ToolKind::Wall);
    for mode in WallMode::ALL {
        h.get_by_label(mode.label()).click();
        h.run_steps(2);
        assert_eq!(h.state_mut().workspace().tool.wall_mode, mode);
    }

    h.state_mut().workspace().tool.wall_mode = WallMode::Single;
    run_tool(&mut h, storey, &[(0.0, 0.0), (5.0, 0.0)]);
    assert_eq!(new_walls(&mut h, 0), [(DVec2::ZERO, DVec2::new(5.0, 0.0))]);

    h.state_mut().workspace().tool.wall_mode = WallMode::Chain;
    run_tool(&mut h, storey, &[(0.0, 10.0), (5.0, 10.0), (5.0, 14.0), (2.0, 16.0)]);
    h.state_mut().workspace().tool.reset();
    let chain = new_walls(&mut h, 1);
    assert_eq!(chain.len(), 3, "one wall per segment");
    for pair in chain.windows(2) {
        assert_eq!(pair[0].1, pair[1].0, "consecutive walls share their end");
    }

    for (mode, points) in [
        (WallMode::Rectangle, vec![(20.0, 0.0), (26.0, 4.0)]),
        (WallMode::RotatedRectangle, vec![(30.0, 0.0), (34.0, 3.0), (31.0, 7.0)]),
    ] {
        let count = h.state_mut().workspace().doc.project.elements.len();
        h.state_mut().workspace().tool.wall_mode = mode;
        run_tool(&mut h, storey, &points);
        let walls = new_walls(&mut h, count);
        assert_eq!(walls.len(), 4, "{mode:?}");
        for i in 0..4 {
            assert_eq!(walls[i].1, walls[(i + 1) % 4].0, "{mode:?}: corner {i} is shared");
        }
    }
}

/// NFR-PORT-005: the main window has the parts of UI-001 at 100 %, 150 %
/// and 200 % scaling, without panicking.
#[test]
fn nfr_port_005_main_window_at_100_to_200_percent_scaling() {
    for (name, pixels_per_point) in [("ppp-100", 1.0), ("ppp-150", 1.5), ("ppp-200", 2.0)] {
        let mut h = harness_at(name, pixels_per_point);
        h.run_steps(2);
        assert_eq!(h.ctx.pixels_per_point(), pixels_per_point);
        for label in [
            "File",
            "Edit",
            "View",
            "Design",
            "Project",
            "Help",
            "Select",
            "Wall",
            "Slab",
            "Storeys",
            "3D views",
            "Plan: 00 Ground floor",
            "3D perspective",
            "No problems",
        ] {
            assert_present(&h, label);
        }
    }
}
