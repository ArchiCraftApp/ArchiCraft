//! Regression net for the done application requirements (UI-006, UI-007,
//! STR-003, STR-004, NFR-USE-004, VIEW-002, VIEW-014, WAL-001,
//! NFR-PORT-005, PRJ-004, NFR-REL-002, PRJ-005, NFR-REL-001, NFR-REL-003,
//! IFX-001, IFX-002, UND-001, ELM-004, NFR-ACC-005, XCH-008, HW-001, UND-005).
//!
//! Every test is named after the requirement it pins, so the traceability
//! table in docs/testing.md and the status script can find it. The tests run
//! the whole application without a GPU and find widgets by their accessible
//! labels (AccessKit). Tests that already exist elsewhere (the unit tests in
//! camera.rs, tools/mod.rs and persistence/) are referenced in docs/testing.md
//! rather than repeated here.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use archicraft_app::ArchiCraftApp;
use archicraft_app::app::{IFCX_EXTENSIONS, with_ifcx_extension};
use archicraft_app::input::{InputConsumer, Outcome};
use archicraft_app::persistence::autosave::{ManualClock, RecoveryInfo, Session, write_autosave};
use archicraft_app::persistence::prefs::{Preferences, SeparatorChoice};
use archicraft_app::persistence::{DataDirs, backup_path};
use archicraft_app::tools::{ToolKind, WallMode};
use archicraft_app::views::camera::{Camera2d, Camera3d};
use archicraft_app::workspace::{Pane, SceneKey};
use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::IfcxFile;
use archicraft_ifcx::container::Header;
use archicraft_model::glam::DVec2;
use archicraft_model::project::Projection;
use archicraft_model::{Command, ElementId, ElementKind, StoreyId, ViewId};
use common::{assert_present, data_dir, harness, harness_at, harness_in, harness_with_clock};
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
    // Without a GPU the 3D pane draws no scene, so SceneKey::of stands for
    // it: view3d.rs builds every 3D view's scene from SceneKey::of(ws), whose
    // selection is the workspace's, so this repeats the assertion above and
    // pins that the 3D views take the plan's selection.
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
    h.run_steps(2);
    browser_row_label(&h, "02 Storey");
    h.get_by_label("Insert below").click();
    h.run_steps(2);
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
    // Enter commits the field, the next frame runs the rename and the one
    // after shows it in the browser.
    h.run_steps(3);
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
/// handle them, deleting an element does not ask, is undone by Ctrl+Z and
/// redone by Ctrl+Shift+Z (UI-008), and New with unsaved changes asks to save
/// or discard.
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
    // Ctrl+Shift+Z redoes the deletion, it does not undo adding the last
    // wall (UI-008).
    h.key_press_modifiers(CTRL.plus(Modifiers::SHIFT), Key::Z);
    h.run_steps(2);
    let project = &h.state_mut().workspace().doc.project;
    assert!(project.element(walls[0]).is_none(), "Ctrl+Shift+Z did not redo");
    assert!(project.element(walls[2]).is_some(), "Ctrl+Shift+Z undid");
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

    // A wheel, Ctrl+wheel (a touchpad's scroll in points too) and the pinch,
    // which comes without modifiers, zoom and keep the point under the
    // cursor.
    let cursor = rect.center() + Vec2::new(-120.0, 70.0);
    hover_plan(&mut h, cursor);
    let wheel = |unit, delta, modifiers| Event::MouseWheel {
        unit,
        delta,
        phase: TouchPhase::Move,
        modifiers,
    };
    let zoom_events = [
        (
            wheel(MouseWheelUnit::Line, Vec2::new(0.0, 2.0), Modifiers::NONE),
            Modifiers::NONE,
        ),
        (wheel(MouseWheelUnit::Line, Vec2::new(0.0, 2.0), CTRL), CTRL),
        (wheel(MouseWheelUnit::Point, Vec2::new(0.0, -60.0), CTRL), CTRL),
        (Event::Zoom(0.5), Modifiers::NONE),
    ];
    for (event, modifiers) in zoom_events {
        let (_, before) = plan_camera(&mut h, storey);
        let anchor = before.to_world(rect, cursor);
        h.event_modifiers(event.clone(), modifiers);
        h.run_steps(1);
        let (_, after) = plan_camera(&mut h, storey);
        assert_ne!(after.scale, before.scale, "{event:?} did not zoom");
        assert!(
            after.to_world(rect, cursor).distance(anchor) < 1e-9,
            "{event:?} moved the point under the cursor"
        );
    }

    // A wheel in points without Ctrl (a touchpad's two-finger scroll) pans:
    // the point under the cursor moves with the scroll.
    let (_, before) = plan_camera(&mut h, storey);
    let anchor = before.to_world(rect, cursor);
    let delta = Vec2::new(30.0, -20.0);
    h.event(wheel(MouseWheelUnit::Point, delta, Modifiers::NONE));
    h.run_steps(1);
    let (_, after) = plan_camera(&mut h, storey);
    assert_eq!(after.scale, before.scale, "the scroll zoomed");
    assert!(
        after.to_world(rect, cursor + delta).distance(anchor) < 1e-9,
        "the scroll did not pan by its delta"
    );

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

/// Drags with `button` from `from` to `to` in four steps, holding
/// `modifiers`.
fn drag(h: &mut App, from: Pos2, to: Pos2, button: PointerButton, modifiers: Modifiers) {
    h.event(Event::ModifiersChanged(modifiers));
    h.hover_at(from);
    h.run_steps(1);
    h.event(Event::PointerButton {
        pos: from,
        button,
        pressed: true,
        modifiers,
    });
    h.run_steps(1);
    for i in 1..=4 {
        h.hover_at(from + (to - from) * (i as f32 / 4.0));
        h.run_steps(1);
    }
    h.event(Event::PointerButton {
        pos: to,
        button,
        pressed: false,
        modifiers,
    });
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(1);
}

/// One notch of a mouse wheel (line units, no modifier) at `pos`.
fn wheel_notch(h: &mut App, pos: Pos2) {
    h.hover_at(pos);
    h.run_steps(1);
    h.event(Event::MouseWheel {
        unit: MouseWheelUnit::Line,
        delta: Vec2::new(0.0, 1.0),
        phase: TouchPhase::Move,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(1);
}

/// Clicks the secondary button at `pos`.
fn secondary_click(h: &mut App, pos: Pos2) {
    h.hover_at(pos);
    h.run_steps(1);
    pointer_button(h, pos, PointerButton::Secondary, true);
    pointer_button(h, pos, PointerButton::Secondary, false);
    h.run_steps(2);
}

/// A plan of the ground floor left of the perspective view, with one wall,
/// both drawn once; returns the storey and the view.
fn plan_beside_3d(name: &str) -> (App, StoreyId, ViewId) {
    let mut h = harness(name);
    let storey = ground(&mut h);
    let perspective = view(&mut h, Projection::Perspective);
    add_wall(&mut h, DVec2::ZERO, DVec2::new(4.0, 0.0));
    *h.state_mut().tree_mut() = Tree::new_horizontal(
        Id::new("archicraft-views"),
        vec![Pane::Plan(storey), Pane::View3d(perspective)],
    );
    h.run_steps(3);
    (h, storey, perspective)
}

fn camera_3d(h: &mut App, view: ViewId) -> (Rect, Camera3d) {
    let state = &h.state_mut().workspace().views_3d[&view];
    (state.last_rect(), state.camera)
}

/// HW-001: the mouse navigates as VIEW-014 and VIEW-021 ask and opens context
/// menus with the secondary button. In the plan the middle button pans and
/// the scroll wheel zooms; in the 3D view the middle button pans, Shift and
/// the middle button orbit and the wheel zooms; a secondary click opens the
/// context menu of either view.
#[test]
fn hw_001_middle_button_and_wheel_navigate_and_the_secondary_button_opens_context_menus() {
    let (mut h, storey, perspective) = plan_beside_3d("hw-001");

    let (rect, start) = plan_camera(&mut h, storey);
    drag(
        &mut h,
        rect.center(),
        rect.center() + Vec2::new(40.0, 30.0),
        PointerButton::Middle,
        Modifiers::NONE,
    );
    let (_, panned) = plan_camera(&mut h, storey);
    assert_ne!(panned.center, start.center, "the middle button pans the plan");
    wheel_notch(&mut h, rect.center());
    let (_, zoomed) = plan_camera(&mut h, storey);
    assert_ne!(zoomed.scale, panned.scale, "the wheel zooms the plan");

    assert!(h.query_by_label("Select all").is_none());
    secondary_click(&mut h, rect.center() + Vec2::new(-50.0, 60.0));
    assert_present(&h, "Select all");
    assert_present(&h, "Fit to window");
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(h.query_by_label("Select all").is_none(), "Escape closes the menu");

    let (view_rect, start) = camera_3d(&mut h, perspective);
    let center = view_rect.center();
    drag(
        &mut h,
        center,
        center + Vec2::new(40.0, 30.0),
        PointerButton::Middle,
        Modifiers::NONE,
    );
    let (_, panned) = camera_3d(&mut h, perspective);
    assert_ne!(panned.target, start.target, "the middle button pans the 3D view");
    assert_eq!((panned.yaw, panned.pitch), (start.yaw, start.pitch));
    drag(
        &mut h,
        center,
        center + Vec2::new(40.0, 30.0),
        PointerButton::Middle,
        Modifiers::SHIFT,
    );
    let (_, orbited) = camera_3d(&mut h, perspective);
    assert_ne!(orbited.yaw, panned.yaw, "Shift and the middle button orbit");
    assert_eq!(orbited.target, panned.target);
    wheel_notch(&mut h, center);
    let (_, zoomed) = camera_3d(&mut h, perspective);
    assert_ne!(zoomed.distance, orbited.distance, "the wheel zooms the 3D view");

    assert!(h.query_by_label("Orbit mode").is_none());
    secondary_click(&mut h, center);
    assert_present(&h, "Orbit mode");
}

/// UND-005: panning, zooming, fitting, previous and next zoom in the plan and
/// panning, orbiting and zooming in the 3D view leave the project and the
/// undo stack unchanged, so Ctrl+Z still undoes the last edit.
#[test]
fn und_005_navigation_creates_no_undo_steps() {
    let (mut h, storey, perspective) = plan_beside_3d("und-005");
    let (undo_len, project) = {
        let ws = h.state_mut().workspace();
        (ws.doc.history.undo_len(), ws.doc.project.clone())
    };
    assert_eq!(undo_len, 1, "adding the wall is the only step");

    let (rect, start) = plan_camera(&mut h, storey);
    drag(
        &mut h,
        rect.center(),
        rect.center() + Vec2::new(40.0, 30.0),
        PointerButton::Middle,
        Modifiers::NONE,
    );
    wheel_notch(&mut h, rect.center());
    h.event_modifiers(Event::Zoom(0.5), CTRL);
    h.run_steps(1);
    let (_, before_fit) = plan_camera(&mut h, storey);
    h.remove_cursor();
    h.key_press(Key::F);
    h.run_steps(2);
    let (_, fitted) = plan_camera(&mut h, storey);
    assert_ne!(fitted, before_fit, "F fits the plan");
    hover_plan(&mut h, rect.center());
    h.key_press_modifiers(CTRL, Key::OpenBracket);
    h.run_steps(2);
    let (_, previous) = plan_camera(&mut h, storey);
    assert_eq!(previous, before_fit, "previous zoom returns to the view before the fit");
    h.key_press_modifiers(CTRL, Key::CloseBracket);
    h.run_steps(2);
    let (_, moved) = plan_camera(&mut h, storey);
    assert_eq!(moved, fitted, "next zoom returns to the fitted view");
    assert_ne!(moved, start, "the plan was navigated");

    let (view_rect, start) = camera_3d(&mut h, perspective);
    let center = view_rect.center();
    for modifiers in [Modifiers::NONE, Modifiers::SHIFT] {
        drag(
            &mut h,
            center,
            center + Vec2::new(40.0, 30.0),
            PointerButton::Middle,
            modifiers,
        );
    }
    wheel_notch(&mut h, center);
    let (_, moved) = camera_3d(&mut h, perspective);
    assert_ne!(moved, start, "the 3D view was navigated");

    let ws = h.state_mut().workspace();
    assert_eq!(ws.doc.history.undo_len(), undo_len, "navigation added no undo step");
    assert_eq!(ws.doc.project, project, "navigation changed nothing in the project");

    h.remove_cursor();
    h.key_press_modifiers(CTRL, Key::Z);
    h.run_steps(2);
    let ws = h.state_mut().workspace();
    assert_eq!(ws.doc.history.undo_len(), 0);
    assert!(ws.doc.project.elements.is_empty(), "Ctrl+Z undid the wall");
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
    assert_eq!(
        new_walls(&mut h, 1),
        [],
        "a chain creates its walls when it ends (UND-003)"
    );
    let ws = h.state_mut().workspace();
    if let Outcome::Execute(command) = ws.tool.finish(&ws.doc.project) {
        assert!(ws.execute(command));
    }
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

/// A fresh, empty folder for the project files of the test `name`.
fn project_folder(name: &str) -> PathBuf {
    let folder = data_dir(&format!("{name}-files"));
    fs::create_dir_all(&folder).expect("the project folder");
    folder
}

/// The names of the files in `folder`, sorted.
fn file_names(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .expect("the folder")
        .map(|e| e.expect("an entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The header of the IFCX file at `path`.
fn header(path: &Path) -> Header {
    let file: IfcxFile = serde_json::from_slice(&fs::read(path).expect("the saved file")).expect("an IFCX file");
    file.header
}

/// Saves with Ctrl+S, which writes to the project's own path without a
/// dialog once it has one.
fn save_with_shortcut(h: &mut App) {
    assert!(
        h.state_mut().workspace().doc.path.is_some(),
        "the project has no path yet"
    );
    h.key_press_modifiers(CTRL, Key::S);
    h.run_steps(2);
}

/// Waits up to ten seconds for the autosave thread to write `path`.
fn wait_for(path: &Path) -> bool {
    let until = Instant::now() + Duration::from_secs(10);
    while Instant::now() < until {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    path.exists()
}

/// PRJ-004 and NFR-REL-002: an autosave left in the recovery folder by a
/// session that ended abnormally is offered at the next start, and Recover
/// opens it.
#[test]
fn prj_004_nfr_rel_002_an_abandoned_autosave_is_offered_at_start() {
    let dir = data_dir("prj-004");
    let recovery = DataDirs::at(&dir).recovery();
    let project = archicraft_model::samples::storey_with_walls_and_slab();
    let info = RecoveryInfo {
        project_name: "Crashed house".into(),
        original_path: None,
        saved_at: "2026-10-20T08:00:00Z".into(),
    };
    let crashed = Session::start(&recovery).expect("a session");
    write_autosave(
        crashed.dir(),
        &project,
        &ModelGeometry::evaluate(&project),
        &info,
        "Test",
    )
    .expect("the autosave");
    let crashed_dir = crashed.dir().to_owned();
    // Releases the lock without removing the folder, as a crash would. The
    // new name keeps it apart from the session the application starts.
    drop(crashed);
    let abandoned = recovery.join("session-abandoned");
    fs::rename(crashed_dir, &abandoned).expect("the abandoned session");

    let mut h = harness_in(&dir, Preferences::default(), 1.0);
    assert_present(&h, "Recover unsaved work");
    h.get_by_label("Crashed house — autosaved 2026-10-20T08:00:00Z (never saved)");
    h.get_by_label("Recover").click();
    h.run_steps(2);
    assert!(h.query_all_by_label("Recover unsaved work").next().is_none());
    let ws = h.state_mut().workspace();
    assert_eq!(ws.doc.project.elements.len(), project.elements.len());
    assert!(!abandoned.exists(), "the recovered session is removed");
}

/// PRJ-004 and NFR-REL-002: the application autosaves to its own session
/// folder under the recovery folder, never next to the project, once the
/// interval of `prefs.autosave_minutes` has passed.
#[test]
fn prj_004_nfr_rel_002_autosaves_follow_the_preference_interval() {
    let dir = data_dir("prj-004-interval");
    let mut h = harness_in(&dir, Preferences::default(), 1.0);
    assert_eq!(h.state_mut().workspace().prefs.autosave_minutes, 5);
    let sessions: Vec<PathBuf> = fs::read_dir(DataDirs::at(&dir).recovery())
        .expect("the recovery folder")
        .map(|e| e.expect("an entry").path())
        .collect();
    let [session] = sessions.as_slice() else {
        panic!("one session folder expected: {sessions:?}");
    };
    let autosave = session.join("autosave.json");

    // An edit within the five minutes is not autosaved yet.
    add_wall(&mut h, DVec2::ZERO, DVec2::new(4.0, 0.0));
    h.run_steps(3);
    std::thread::sleep(Duration::from_millis(200));
    assert!(!autosave.exists(), "autosaved before the interval passed");

    // With an interval of zero minutes the next frame autosaves.
    h.state_mut().workspace().prefs.autosave_minutes = 0;
    h.run_steps(1);
    assert!(wait_for(&autosave), "no autosave in {}", session.display());
    let model = fs::read(session.join("autosave.ifcx")).expect("the autosaved model");
    let saved = archicraft_ifcx::load(&model).expect("a readable autosave");
    assert_eq!(saved.project.elements.len(), 1);
    assert_eq!(h.state_mut().workspace().doc.path, None, "no project file was written");
}

/// How long a test waits for the autosave thread.
const AUTOSAVE_TIMEOUT: Duration = Duration::from_secs(10);

fn start_with_clock(name: &str, prefs: Preferences) -> (App, ManualClock, PathBuf) {
    let dir = data_dir(name);
    let clock = ManualClock::new();
    let h = harness_with_clock(&dir, prefs, 1.0, Arc::new(clock.clone()));
    (h, clock, dir)
}

/// Moves the clock by `by` and lets the application run a few frames, then
/// returns the number of autosaves written so far.
fn advance_clock(h: &mut App, clock: &ManualClock, by: Duration) -> usize {
    clock.advance(by);
    h.run_steps(2);
    h.state()
        .autosaves_written(AUTOSAVE_TIMEOUT)
        .expect("the queued autosaves are written in time")
}

/// PRJ-004 and NFR-REL-002: after an edit at 0:00 nothing is autosaved at
/// 4:59, one autosave is written at 5:00 and none follows while nothing
/// changes.
#[test]
fn prj_004_nfr_rel_002_autosave_is_written_at_five_minutes_and_not_before() {
    let (mut h, clock, dir) = start_with_clock("autosave-clock-default", Preferences::default());
    assert_eq!(h.state_mut().workspace().prefs.autosave_minutes, 5);
    add_wall(&mut h, DVec2::ZERO, DVec2::new(4.0, 0.0));

    let written = advance_clock(&mut h, &clock, Duration::from_secs(4 * 60 + 59));
    assert_eq!(written, 0, "autosaved at 4:59");
    let written = advance_clock(&mut h, &clock, Duration::from_secs(1));
    assert_eq!(written, 1, "no autosave at 5:00");
    let written = advance_clock(&mut h, &clock, Duration::from_secs(5 * 60));
    assert_eq!(written, 1, "autosaved again at 10:00 without a change");
    let written = advance_clock(&mut h, &clock, Duration::from_secs(60 * 60));
    assert_eq!(written, 1, "autosaved again at 70:00 without a change");

    drop(h);
    let _ = std::fs::remove_dir_all(dir);
}

/// PRJ-004: the interval comes from the preferences; with 2 minutes the
/// autosave is written at 2:00, not at 1:59.
#[test]
fn prj_004_autosave_interval_follows_the_preferences() {
    let prefs = Preferences {
        autosave_minutes: 2,
        ..Preferences::default()
    };
    let (mut h, clock, dir) = start_with_clock("autosave-clock-two", prefs);
    add_wall(&mut h, DVec2::ZERO, DVec2::new(4.0, 0.0));

    let written = advance_clock(&mut h, &clock, Duration::from_secs(60 + 59));
    assert_eq!(written, 0, "autosaved at 1:59");
    let written = advance_clock(&mut h, &clock, Duration::from_secs(1));
    assert_eq!(written, 1, "no autosave at 2:00");

    // A later change is autosaved two minutes after the previous autosave.
    add_wall(&mut h, DVec2::new(0.0, 3.0), DVec2::new(4.0, 3.0));
    let written = advance_clock(&mut h, &clock, Duration::from_secs(60));
    assert_eq!(written, 1, "autosaved one minute after the last autosave");
    let written = advance_clock(&mut h, &clock, Duration::from_secs(60));
    assert_eq!(written, 2, "the change is not autosaved at 4:00");

    drop(h);
    let _ = std::fs::remove_dir_all(dir);
}

/// PRJ-005, NFR-REL-001 and NFR-REL-003: every save replaces the file
/// atomically and keeps the previous version as a backup, as many as
/// `prefs.backups` (default 1); no temporary file is left, and the backups
/// do not have the extension the open dialog lists.
#[test]
fn prj_005_nfr_rel_001_nfr_rel_003_saves_are_atomic_with_configurable_backups() {
    let mut h = harness("prj-005");
    assert_eq!(h.state_mut().workspace().prefs.backups, 1);
    let folder = project_folder("prj-005");
    let path = folder.join("house.ifcx");
    let (bak, bak2) = (folder.join("house.ifcx.bak"), folder.join("house.ifcx.bak2"));
    assert_eq!(
        (backup_path(&path, 1), backup_path(&path, 2)),
        (bak.clone(), bak2.clone())
    );

    add_wall(&mut h, DVec2::ZERO, DVec2::new(4.0, 0.0));
    assert!(h.state_mut().save_to(&path));
    assert_eq!(file_names(&folder), ["house.ifcx"]);
    let first = fs::read(&path).expect("the first save");

    add_wall(&mut h, DVec2::new(4.0, 0.0), DVec2::new(4.0, 3.0));
    save_with_shortcut(&mut h);
    let second = fs::read(&path).expect("the second save");
    assert_ne!(second, first);
    assert_eq!(file_names(&folder), ["house.ifcx", "house.ifcx.bak"]);
    assert_eq!(fs::read(&bak).expect("the backup"), first);

    h.state_mut().workspace().prefs.backups = 2;
    add_wall(&mut h, DVec2::new(4.0, 3.0), DVec2::new(0.0, 3.0));
    save_with_shortcut(&mut h);
    assert_eq!(file_names(&folder), ["house.ifcx", "house.ifcx.bak", "house.ifcx.bak2"]);
    assert_eq!(fs::read(&bak).expect("the newest backup"), second);
    assert_eq!(fs::read(&bak2).expect("the older backup"), first);

    // The open dialog lists only IFCX files, which the backups are not.
    assert_eq!(IFCX_EXTENSIONS, ["ifcx"]);
    let listed = |p: &Path| {
        p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| IFCX_EXTENSIONS.contains(&e))
    };
    assert!(listed(&path));
    assert!(!listed(&bak) && !listed(&bak2), "backups are listed by the open dialog");
}

/// IFX-001: a file name chosen without an extension is saved with .ifcx; a
/// name with an extension is kept.
#[test]
fn ifx_001_save_adds_the_ifcx_extension() {
    let folder = Path::new("projects");
    assert_eq!(with_ifcx_extension(PathBuf::from("house")), PathBuf::from("house.ifcx"));
    assert_eq!(with_ifcx_extension(folder.join("house")), folder.join("house.ifcx"));
    assert_eq!(
        with_ifcx_extension(folder.join("house.ifcx")),
        folder.join("house.ifcx")
    );
    assert_eq!(
        with_ifcx_extension(folder.join("house.v2.ifcx")),
        folder.join("house.v2.ifcx")
    );
}

/// IFX-002: every save writes the next dataVersion, and the author is the
/// author of the preferences, by default "ArchiCraft user".
#[test]
fn ifx_002_every_save_increments_data_version_and_writes_the_author() {
    let mut h = harness("ifx-002");
    assert_eq!(h.state_mut().workspace().prefs.author, "ArchiCraft user");
    let n = h.state_mut().workspace().doc.project.data_version;
    let path = project_folder("ifx-002").join("house.ifcx");

    assert!(h.state_mut().save_to(&path));
    let first = header(&path);
    assert_eq!(first.data_version, (n + 1).to_string());
    assert_eq!(first.author, "ArchiCraft user");

    h.state_mut().workspace().prefs.author = "Jane Architect".into();
    save_with_shortcut(&mut h);
    let second = header(&path);
    assert_eq!(second.data_version, (n + 2).to_string());
    assert_eq!(second.author, "Jane Architect");
}

/// UND-001: saving does not clear the history; Ctrl+Z after a save undoes
/// the last edit.
#[test]
fn und_001_saving_keeps_the_undo_history() {
    let mut h = harness("und-001");
    let kept = add_wall(&mut h, DVec2::ZERO, DVec2::new(4.0, 0.0));
    let last = add_wall(&mut h, DVec2::new(4.0, 0.0), DVec2::new(4.0, 3.0));
    let stack = |h: &mut App| {
        let history = &h.state_mut().workspace().doc.history;
        (history.undo_len(), history.undo_label())
    };
    let before = stack(&mut h);
    assert_eq!(before.0, 2);

    assert!(h.state_mut().save_to(&project_folder("und-001").join("house.ifcx")));
    h.run_steps(2);
    assert_eq!(stack(&mut h), before, "saving changed the undo stack");

    h.key_press_modifiers(CTRL, Key::Z);
    h.run_steps(2);
    let project = &h.state_mut().workspace().doc.project;
    assert!(project.element(last).is_none(), "Ctrl+Z did not undo the last edit");
    assert!(project.element(kept).is_some());
}

/// The text field of the properties strip that shows `value`.
fn strip_field(value: &str) -> By<'_> {
    By::new().role(Role::TextInput).value(value)
}

/// ELM-004: the properties strip of one selected wall has an editable Name
/// field, and committing it renames the wall through UpdateElements; with
/// two walls selected there is no Name field.
#[test]
fn elm_004_the_properties_strip_renames_a_single_wall() {
    let mut h = harness("elm-004");
    let wall = add_wall(&mut h, DVec2::ZERO, DVec2::new(4.0, 0.0));
    let other = add_wall(&mut h, DVec2::new(4.0, 0.0), DVec2::new(4.0, 3.0));
    let name_of = |h: &mut App, id: ElementId| {
        let project = &h.state_mut().workspace().doc.project;
        project.element(id).map(|e| e.name.clone()).expect("the wall")
    };
    let name = name_of(&mut h, wall);
    h.state_mut().workspace().selection.insert(wall);
    h.run_steps(2);
    assert_present(&h, "Name");

    h.get(strip_field(&name)).click();
    h.run_steps(1);
    h.get(strip_field(&name)).type_text("North wall");
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(name_of(&mut h, wall), "North wall");
    let history = &h.state_mut().workspace().doc.history;
    assert!(
        matches!(history.applied_commands().last(), Some(Command::UpdateElements(e)) if e.len() == 1 && e[0].id == wall),
        "the rename is not an UpdateElements of the wall"
    );

    let other_name = name_of(&mut h, other);
    h.state_mut().workspace().selection.insert(other);
    h.run_steps(2);
    assert!(h.query_all(strip_field("North wall")).next().is_none());
    assert!(h.query_all(strip_field(&other_name)).next().is_none());
}

/// NFR-ACC-005: the properties strip shows a wall thickness of 0.123456 m
/// rounded to 0.123, and committing the field unedited leaves the stored
/// value bit-identical.
#[test]
fn nfr_acc_005_property_fields_round_only_the_display() {
    let prefs = Preferences {
        decimal_separator: SeparatorChoice::Point,
        ..Preferences::default()
    };
    let mut h = harness_in(&data_dir("nfr-acc-005"), prefs, 1.0);
    let thickness = 0.123456;
    let wall = {
        let ws = h.state_mut().workspace();
        let storey = ws.doc.project.storeys.ground();
        let mut wall = ws.doc.project.new_wall(storey, DVec2::ZERO, DVec2::new(4.0, 0.0), 0);
        if let ElementKind::Wall(w) = &mut wall.kind {
            w.thickness = thickness;
        }
        let id = wall.id;
        assert!(ws.execute(Command::AddElements(vec![wall])));
        ws.selection.insert(id);
        id
    };
    h.run_steps(2);
    let stored = |h: &mut App| {
        let project = &h.state_mut().workspace().doc.project;
        project
            .element(wall)
            .and_then(|e| e.as_wall())
            .map(|w| w.thickness.to_bits())
    };
    assert_eq!(stored(&mut h), Some(thickness.to_bits()));
    let undo_len = h.state_mut().workspace().doc.history.undo_len();

    h.get(strip_field("0.123")).click();
    h.run_steps(1);
    h.key_press(Key::Enter);
    h.run_steps(2);
    assert_eq!(stored(&mut h), Some(thickness.to_bits()), "the shown value was stored");
    assert_eq!(h.state_mut().workspace().doc.history.undo_len(), undo_len);
    h.get(strip_field("0.123"));
}

/// XCH-008: a saved file opens through the application without load
/// problems, and its elements accept UpdateElements.
#[test]
fn xch_008_a_saved_file_opens_editable_without_problems() {
    let path = project_folder("xch-008").join("house.ifcx");
    let wall = {
        let mut h = harness("xch-008-save");
        let wall = add_wall(&mut h, DVec2::ZERO, DVec2::new(4.0, 0.0));
        assert!(h.state_mut().save_to(&path));
        wall
    };

    let mut h = harness("xch-008-open");
    h.state_mut().open_path(&path);
    h.run_steps(2);
    assert_eq!(h.state_mut().prompt_title(), None, "the file opened with a prompt");
    assert_present(&h, "No problems");
    let ws = h.state_mut().workspace();
    assert_eq!(ws.doc.path.as_deref(), Some(path.as_path()));
    let mut renamed = ws.doc.project.element(wall).cloned().expect("the saved wall");
    renamed.name = "Opened wall".into();
    assert!(ws.execute(Command::UpdateElements(vec![renamed])));
    assert_eq!(
        ws.doc.project.element(wall).map(|e| e.name.as_str()),
        Some("Opened wall")
    );
}
