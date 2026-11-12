//! The project browser (VIEW-003, UI-001): a plan sheet at each storey, a
//! camera at each perspective view and a tall box at each parallel view,
//! tinted like the row's text, and the whole row still opens its view.
//! Saved camera views (VIEW-004) are saved from the 3D view and opened,
//! renamed, updated and deleted from their rows.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use archicraft_app::ArchiCraftApp;
use archicraft_app::commands::{Origin, Registry, Target, view3d};
use archicraft_app::persistence::prefs::ThemeChoice;
use archicraft_app::ui::icons;
use archicraft_app::views::camera::Camera3d;
use archicraft_app::workspace::Pane;
use archicraft_model::project::{Projection, View3d, ViewCamera};
use archicraft_model::{StoreyId, ViewId};
use common::harness;
use egui::accesskit::Role;
use egui::epaint::{Color32, Rect, Shape};
use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, NodeT, Queryable};

type App = Harness<'static, ArchiCraftApp>;

/// What the last frame painted inside a browser row: its images (the
/// texture's name, which egui takes from the icon's uri, where it was
/// painted and its tint) and the colours of its text.
#[derive(Debug, Default)]
struct Painted {
    images: Vec<(String, Rect, Color32)>,
    text: Vec<(Rect, Color32)>,
}

fn collect(h: &App, shape: &Shape, row: Rect, out: &mut Painted) {
    match shape {
        Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(h, s, row, out)),
        Shape::Mesh(mesh) => {
            let bounds = mesh.calc_bounds();
            if row.contains_rect(bounds) && !mesh.vertices.is_empty() {
                let name = h.ctx.tex_manager().read().meta(mesh.texture_id).map(|m| m.name.clone());
                out.images
                    .push((name.unwrap_or_default(), bounds, mesh.vertices[0].color));
            }
        }
        // egui paints an image as a rect filled with its texture.
        Shape::Rect(rect) if rect.brush.is_some() && row.contains_rect(rect.rect) => {
            let texture = rect.brush.as_ref().unwrap().fill_texture_id;
            let name = h.ctx.tex_manager().read().meta(texture).map(|m| m.name.clone());
            out.images.push((name.unwrap_or_default(), rect.rect, rect.fill));
        }
        Shape::Text(text) => {
            let bounds = text.visual_bounding_rect();
            if row.contains_rect(bounds) {
                out.text
                    .push((bounds, text.override_text_color.unwrap_or(text.fallback_color)));
            }
        }
        _ => {}
    }
}

fn painted_in(h: &App, row: Rect) -> Painted {
    let mut out = Painted::default();
    for clipped in &h.output().shapes {
        collect(h, &clipped.shape, row, &mut out);
    }
    out
}

/// The rect of the browser row whose label contains `text`: between the
/// browser's "Storeys" and "Project" headers, so neither a tab nor the
/// status line.
fn row(h: &App, text: &str) -> Rect {
    let browser = h.get_by_label("Storeys").rect();
    let project = h
        .query_all_by_label("Project")
        .map(|n| n.rect())
        .find(|r| r.min.x >= browser.min.x - 1.0 && r.min.y > browser.max.y)
        .expect("the browser has a Project header");
    let rows: Vec<Rect> = h
        .query_all_by_label_contains(text)
        .map(|n| n.rect())
        .filter(|r| r.min.x >= browser.min.x - 1.0 && r.min.y > browser.max.y && r.max.y < project.min.y)
        .collect();
    assert_eq!(rows.len(), 1, "browser rows with “{text}”: {rows:?}");
    rows[0]
}

/// The one icon of the row with `text`, after checking that it sits before
/// the text, at the text's height and in the text's colour.
fn icon_of(h: &App, text: &str) -> String {
    let rect = row(h, text);
    let painted = painted_in(h, rect);
    assert_eq!(painted.images.len(), 1, "“{text}” paints {painted:?}");
    let (name, icon, tint) = painted.images[0].clone();
    let (label, colour) = *painted.text.first().unwrap_or_else(|| panic!("“{text}” has no text"));
    assert!(icon.max.x <= label.min.x, "the icon of “{text}” is not before its text");
    assert_eq!(tint, colour, "the icon of “{text}” is not tinted like its text");
    assert!(
        (icon.center().y - rect.center().y).abs() <= 1.0,
        "the icon of “{text}” is not centred in its row: {icon:?} in {rect:?}"
    );
    assert!(
        icon.height() >= label.height() - 2.0 && icon.height() <= rect.height(),
        "the icon of “{text}” is not at row height: {icon:?}, text {label:?}, row {rect:?}"
    );
    name
}

fn uri(icon: egui::ImageSource<'static>) -> String {
    icon.uri().unwrap().to_owned()
}

/// Clicks the primary button at `pos`.
fn click_at(h: &mut App, pos: Pos2) {
    h.hover_at(pos);
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    h.run_steps(2);
}

fn storey_names(h: &mut App) -> Vec<(StoreyId, String)> {
    let storeys = &h.state_mut().workspace().doc.project.storeys;
    storeys.highest_first().map(|s| (s.id, s.name.clone())).collect()
}

fn view_named(h: &mut App, name: &str) -> ViewId {
    let views = &h.state_mut().workspace().doc.project.views;
    views
        .iter()
        .find(|v| v.name == name)
        .expect("the template has this view")
        .id
}

fn active_pane(h: &mut App) -> Option<Pane> {
    h.state_mut().workspace().active_pane
}

/// VIEW-003, UI-001: every storey row of the browser draws the plan sheet,
/// "3D perspective" the camera and "3D parallel" the parallel box, each
/// before its text, at row height and in the row's text colour, in the
/// light and the dark theme, selected or not. A click on the icon opens
/// the plan or the 3D view: the whole row is one target.
#[test]
fn browser_rows_show_the_plan_sheet_camera_and_parallel_box() {
    for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
        let mut h = harness(&format!("browser-icons-{theme:?}"));
        h.state_mut().workspace().prefs.theme = theme;
        h.run_steps(2);
        let storeys = storey_names(&mut h);
        assert!(storeys.len() >= 2, "{storeys:?}");
        for (_, name) in &storeys {
            assert_eq!(icon_of(&h, name), uri(icons::PLAN_SHEET), "{name}");
        }
        assert_eq!(icon_of(&h, "3D perspective"), uri(icons::CAMERA));
        assert_eq!(icon_of(&h, "3D parallel"), uri(icons::PARALLEL_BOX));

        for (id, name) in &storeys {
            let icon = painted_in(&h, row(&h, name)).images[0].1;
            click_at(&mut h, icon.center());
            assert_eq!(active_pane(&mut h), Some(Pane::Plan(*id)), "{theme:?}: {name}");
            // Selected now: still the plan sheet in the text's colour.
            assert_eq!(icon_of(&h, name), uri(icons::PLAN_SHEET), "{name}");
        }
        for (name, icon) in [("3D perspective", icons::CAMERA), ("3D parallel", icons::PARALLEL_BOX)] {
            let view = view_named(&mut h, name);
            let at = painted_in(&h, row(&h, name)).images[0].1;
            click_at(&mut h, at.center());
            assert_eq!(active_pane(&mut h), Some(Pane::View3d(view)), "{theme:?}: {name}");
            assert_eq!(icon_of(&h, name), uri(icon), "{name}");
        }
    }
}

/// VIEW-003, VIEW-020: a view's icon follows its projection. Edited in the
/// project, the closed "3D perspective" shows the parallel box; opened, its
/// view-control bar's view3d.toggle-projection switches what it shows, and
/// the icon with it.
#[test]
fn switching_a_views_projection_switches_its_icon() {
    let mut h = harness("browser-icons-projection");
    let view = view_named(&mut h, "3D perspective");
    assert_eq!(icon_of(&h, "3D perspective"), uri(icons::CAMERA));

    let ws = h.state_mut().workspace();
    ws.doc
        .project
        .views
        .iter_mut()
        .find(|v| v.id == view)
        .unwrap()
        .projection = Projection::Parallel;
    h.run_steps(2);
    assert_eq!(icon_of(&h, "3D perspective"), uri(icons::PARALLEL_BOX));

    let at = painted_in(&h, row(&h, "3D perspective")).images[0].1;
    click_at(&mut h, at.center());
    assert_eq!(active_pane(&mut h), Some(Pane::View3d(view)));
    assert_eq!(icon_of(&h, "3D perspective"), uri(icons::PARALLEL_BOX));
    for expected in [icons::CAMERA, icons::PARALLEL_BOX] {
        let ctx = h.ctx.clone();
        let app = h.state_mut();
        let target = Target::active(app.workspace());
        app.invoke(&ctx, view3d::TOGGLE_PROJECTION, target, Origin::Test);
        h.run_steps(2);
        assert_eq!(icon_of(&h, "3D perspective"), uri(expected));
    }
}

/// VIEW-003: the icon is chosen in one place, by projection, so saved
/// cameras show the same icons as the 3D views.
#[test]
fn the_view_icon_is_chosen_by_projection() {
    assert_eq!(uri(icons::view_icon(Projection::Perspective)), uri(icons::CAMERA));
    assert_eq!(uri(icons::view_icon(Projection::Parallel)), uri(icons::PARALLEL_BOX));
}

/// Drags with `button` from `pos` to `to` in four steps with `modifiers`
/// held.
fn drag(h: &mut App, pos: Pos2, to: Pos2, button: PointerButton, modifiers: Modifiers) {
    h.event(Event::ModifiersChanged(modifiers));
    h.hover_at(pos);
    h.run_steps(1);
    h.event(Event::PointerButton {
        pos,
        button,
        pressed: true,
        modifiers,
    });
    h.run_steps(1);
    for i in 1..=4 {
        h.hover_at(pos + (to - pos) * (i as f32 / 4.0));
        h.run_steps(1);
    }
    h.event(Event::PointerButton {
        pos: to,
        button,
        pressed: false,
        modifiers,
    });
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(2);
}

/// Orbits the open 3D view `view` with Shift and the middle button
/// (VIEW-021) and returns its new camera.
fn orbit(h: &mut App, view: ViewId, by: egui::Vec2) -> Camera3d {
    let (rect, before) = tab_camera(h, view).expect("the view is open");
    let center = rect.center();
    drag(h, center, center + by, PointerButton::Middle, Modifiers::SHIFT);
    let (_, after) = tab_camera(h, view).expect("the view is open");
    assert!((after.yaw - before.yaw).abs() > 1e-3, "the drag orbits the view");
    after
}

/// The rect and the camera of the open 3D view `view`, `None` without one.
fn tab_camera(h: &mut App, view: ViewId) -> Option<(Rect, Camera3d)> {
    let state = h.state_mut().workspace().views_3d.get(&view)?;
    Some((state.last_rect(), state.camera))
}

/// Opens the browser row with `text` with a click on it.
fn open_row(h: &mut App, text: &str) {
    let at = row(h, text).center();
    click_at(h, at);
}

/// Clicks the secondary button at `pos`, which opens a context menu.
fn secondary_click(h: &mut App, pos: Pos2) {
    h.hover_at(pos);
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    h.run_steps(2);
}

/// Opens the context menu of the browser row with `text`.
fn context_menu_of(h: &mut App, text: &str) {
    let at = row(h, text).center();
    secondary_click(h, at);
}

/// The saved view named `name`, if the project has one.
fn saved_view(h: &mut App, name: &str) -> Option<View3d> {
    let views = &h.state_mut().workspace().doc.project.views;
    views.iter().find(|v| v.name == name).cloned()
}

fn undo_len(h: &mut App) -> usize {
    h.state_mut().workspace().doc.history.undo_len()
}

fn has_tab(h: &mut App, view: ViewId) -> bool {
    h.state_mut().tree_mut().tiles.find_pane(&Pane::View3d(view)).is_some()
}

/// Asserts that `camera` shows the saved viewpoint `saved` within 1e-9:
/// target, distance, yaw, pitch and field of view.
fn assert_shows(camera: &Camera3d, saved: ViewCamera) {
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-9;
    assert!(
        (camera.target - saved.target).length() <= 1e-9
            && close(camera.distance, saved.distance)
            && close(camera.yaw, saved.yaw)
            && close(camera.pitch, saved.pitch)
            && close(camera.fov_y, saved.fov_y),
        "{camera:?} does not show {saved:?}"
    );
}

/// VIEW-004, VIEW-003, VIEW-020, UI-001: the 3D view-control bar's camera
/// icon button saves the viewpoint of the orbited 3D perspective as
/// "Camera 1", listed under 3D views with the camera icon and holding that
/// camera. Opened from the browser after another view, its tab shows the
/// saved viewpoint. Navigating adds no undo step, saving adds one
/// (UND-005).
#[test]
fn save_camera_lists_the_viewpoint_and_opening_it_shows_it() {
    let mut h = harness("browser-save-camera");
    open_row(&mut h, "3D perspective");
    let perspective = view_named(&mut h, "3D perspective");
    let steps = undo_len(&mut h);
    let orbited = orbit(&mut h, perspective, egui::vec2(60.0, -25.0));
    assert_eq!(undo_len(&mut h), steps, "navigation adds no undo step (UND-005)");

    let button = h.get_by_label("Save camera").rect();
    let painted = painted_in(&h, button);
    let images: Vec<&str> = painted.images.iter().map(|(name, _, _)| name.as_str()).collect();
    assert_eq!(images, [uri(icons::CAMERA)], "the bar's button is the camera icon");
    assert!(painted.text.is_empty(), "the bar's button has no text: {painted:?}");
    h.get_by_label("Save camera").click();
    h.run_steps(2);
    assert_eq!(undo_len(&mut h), steps + 1, "saving is one undo step");
    let camera = saved_view(&mut h, "Camera 1").expect("Camera 1 is saved");
    assert_eq!(camera.projection, Projection::Perspective);
    let saved = camera.camera.expect("Camera 1 has a camera");
    assert_shows(&orbited, saved);
    assert_eq!(icon_of(&h, "Camera 1"), uri(icons::CAMERA));
    assert_eq!(
        active_pane(&mut h),
        Some(Pane::View3d(perspective)),
        "saving keeps the tab"
    );

    open_row(&mut h, "3D parallel");
    assert_eq!(
        active_pane(&mut h),
        Some(Pane::View3d(view_named(&mut h, "3D parallel")))
    );
    open_row(&mut h, "Camera 1");
    assert_eq!(active_pane(&mut h), Some(Pane::View3d(camera.id)));
    let (_, shown) = tab_camera(&mut h, camera.id).expect("Camera 1 is open");
    assert_shows(&shown, saved);
    assert_eq!(shown.projection, Projection::Perspective);
    assert_eq!(undo_len(&mut h), steps + 1, "opening adds no undo step (UND-005)");
}

/// VIEW-004, UI-017: Save camera is also in the View menu and the context
/// menu of the browser's "3D views" header, greyed out while a plan is
/// active; from the header it saves the active 3D view's viewpoint.
#[test]
fn save_camera_is_in_the_view_menu_and_the_views_header() {
    let save = Registry::get().command(view3d::SAVE_CAMERA);
    assert_eq!(save.menu.map(|m| m.path), Some("view"));
    assert_eq!(save.icon.as_ref().and_then(|i| i.uri()), icons::CAMERA.uri());

    let mut h = harness("browser-save-camera-header");
    let header = h.get_by_label("3D views").rect().center();
    secondary_click(&mut h, header);
    assert!(
        h.get_by_label("Save camera").accesskit_node().is_disabled(),
        "no 3D view is active"
    );
    h.key_press(egui::Key::Escape);
    h.run_steps(2);

    open_row(&mut h, "3D parallel");
    let parallel = view_named(&mut h, "3D parallel");
    let (_, shown) = tab_camera(&mut h, parallel).expect("3D parallel is open");
    secondary_click(&mut h, header);
    // The menu item, beside the header; the view-control bar has the other.
    let item = h
        .query_all_by_label("Save camera")
        .min_by(|a, b| {
            let d = |n: &egui_kittest::Node<'_>| n.rect().center().distance(header);
            d(a).total_cmp(&d(b))
        })
        .expect("the header's menu has Save camera");
    assert!(
        item.rect().center().distance(header) < 200.0,
        "the menu item, not the bar's button: {:?}",
        item.rect()
    );
    item.click();
    h.run_steps(2);
    let camera = saved_view(&mut h, "Camera 1").expect("Camera 1 is saved");
    assert_eq!(camera.projection, Projection::Parallel);
    assert_shows(&shown, camera.camera.expect("a camera"));
    assert_eq!(icon_of(&h, "Camera 1"), uri(icons::PARALLEL_BOX));
}

/// VIEW-004, VIEW-003, UND-005: a camera view's context menu renames it,
/// updates its camera from its open tab (greyed out while it has none),
/// and deletes it with its tab; each is one undo step and undo brings the
/// deleted view back. Opening an open camera view returns its tab to the
/// saved viewpoint.
#[test]
fn camera_views_are_renamed_updated_and_deleted_from_the_browser() {
    let mut h = harness("browser-camera-menu");
    open_row(&mut h, "3D perspective");
    let ctx = h.ctx.clone();
    let app = h.state_mut();
    let target = Target::active(app.workspace());
    app.invoke(&ctx, view3d::SAVE_CAMERA, target, Origin::Test);
    h.run_steps(2);
    let id = saved_view(&mut h, "Camera 1").expect("Camera 1 is saved").id;

    context_menu_of(&mut h, "Camera 1");
    assert!(
        h.get_by_label("Update camera").accesskit_node().is_disabled(),
        "Camera 1 has no open tab"
    );
    let steps = undo_len(&mut h);
    h.get_by_label("Rename").click();
    h.run_steps(2);
    h.get(By::new().role(Role::TextInput)).type_text("Entrance");
    h.run_steps(1);
    h.key_press(egui::Key::Enter);
    h.run_steps(2);
    assert!(saved_view(&mut h, "Camera 1").is_none());
    assert_eq!(saved_view(&mut h, "Entrance").map(|v| v.id), Some(id));
    assert_eq!(undo_len(&mut h), steps + 1, "renaming is one undo step");
    row(&h, "Entrance");

    open_row(&mut h, "Entrance");
    let orbited = orbit(&mut h, id, egui::vec2(-50.0, 20.0));
    assert_eq!(undo_len(&mut h), steps + 1, "navigation adds no undo step (UND-005)");
    context_menu_of(&mut h, "Entrance");
    h.get_by_label("Update camera").click();
    h.run_steps(2);
    assert_eq!(undo_len(&mut h), steps + 2, "updating is one undo step");
    let updated = saved_view(&mut h, "Entrance").and_then(|v| v.camera).expect("a camera");
    assert_shows(&orbited, updated);

    open_row(&mut h, "3D parallel");
    open_row(&mut h, "Entrance");
    let (_, shown) = tab_camera(&mut h, id).expect("Entrance is open");
    assert_shows(&shown, updated);
    orbit(&mut h, id, egui::vec2(40.0, 10.0));
    open_row(&mut h, "Entrance");
    let (_, reset) = tab_camera(&mut h, id).expect("Entrance is open");
    assert_shows(&reset, updated);

    context_menu_of(&mut h, "Entrance");
    h.get_by_label("Delete").click();
    h.run_steps(3);
    assert!(saved_view(&mut h, "Entrance").is_none());
    assert_eq!(undo_len(&mut h), steps + 3, "deleting is one undo step");
    assert!(!has_tab(&mut h, id), "its tab is closed");
    assert_ne!(active_pane(&mut h), Some(Pane::View3d(id)));
    assert_eq!(h.query_all_by_label("Entrance").count(), 0, "its row is gone");

    h.state_mut().workspace().undo();
    h.run_steps(2);
    let back = saved_view(&mut h, "Entrance").expect("undo brings it back");
    assert_eq!((back.id, back.camera), (id, Some(updated)));
    row(&h, "Entrance");
}
