//! The exit check of Showcase 3.5 (archicraft-2glnu): the demo script walked
//! through the application with injected pointer and key events, in a
//! window 1000 px wide, from opening a project to a restart that reopens it,
//! asserting the model and what the window shows after each step.
//!
//! 1. A wall selected in the opened project: the properties strip is one
//!    row, its groups separated, in the documented order; the chevron at its
//!    right end opens the units that do not fit. At this width the thickness
//!    fits into the row and is edited there, the top offset in the chevron's
//!    popup, each in one undo step (UI-002).
//! 2. The main toolbar shows only icons at the toolbox's size: Save (the
//!    floppy disk), Undo, Redo, Pick up, Inject, Measure, Group, Ungroup,
//!    then Layers, Materials and Settings; hovering Save shows its name,
//!    shortcut and description, and Layers opens the Layers dialog (UI-001,
//!    UI-011, PRJ-003).
//! 3. The project browser shows a plan sheet at each storey, a camera at the
//!    perspective and a tall box at the parallel view; dragging its edge
//!    makes it wider and narrower, and the toggle at the right end of the
//!    tab row hides it and shows it again at its width (VIEW-003, UI-015,
//!    UI-004).
//! 4. The view-control bar shows Zoom out, Zoom in and Fit (and in 3D
//!    Orbit) as icons; 150 typed into the zoom value zooms the plan to 150 %
//!    (UI-001, VIEW-013).
//! 5. The orbited perspective saved as a camera is listed as "Camera 1"
//!    with the camera icon and restores its viewpoint after another view,
//!    and again after saving and reopening the project (VIEW-004, PRJ-003).
//! 6. The texts of the toolbar, the properties strip, the view-control bar,
//!    the status line and the browser have one size (UI-001).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::Path;

use archicraft_app::persistence::DataDirs;
use archicraft_app::persistence::prefs::Preferences;
use archicraft_app::ui::command_widgets::{ICON, ICON_BUTTON};
use archicraft_app::ui::strip::RowLayout;
use archicraft_app::ui::{icons, properties, view_bar};
use archicraft_app::views::camera::Camera3d;
use archicraft_app::workspace::Pane;
use archicraft_app::{ArchiCraftApp, tr};
use archicraft_model::element::TopLink;
use archicraft_model::project::{Projection, ViewCamera};
use archicraft_model::units::format_length;
use archicraft_model::{ElementId, ViewId};
use common::{assert_present, data_dir, harness_in, strip_overflow_rect};
use egui::accesskit::{Role, Toggled};
use egui::epaint::{Color32, Shape};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, NodeT, Queryable};

type App = Harness<'static, ArchiCraftApp>;

/// The demo's window: about 1000 px wide.
const WINDOW: egui::Vec2 = egui::vec2(1000.0, 800.0);

/// The toolbar's buttons in the demo's order (step 2).
const TOOLBAR: [&str; 11] = [
    "Save",
    "Undo",
    "Redo",
    "Pick up parameters",
    "Inject parameters",
    "Measure tool",
    "Group",
    "Ungroup",
    "Layers…",
    "Materials…",
    "Project settings…",
];

/// The side of the toolbox's icons before Showcase 3.5, in points (UI-001:
/// the icons are a third larger than this).
const FORMER_ICON: f32 = 20.0;

/// The accessible name of the browser toggle in the tab row.
const BROWSER_TOGGLE: &str = "Project browser";

/// The rectangle of the panel `name` as egui stored it.
fn panel(h: &App, name: &str) -> Rect {
    egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new(name))
        .unwrap_or_else(|| panic!("the {name} panel"))
        .outer_rect
}

fn button_event(pos: Pos2, button: PointerButton, pressed: bool, modifiers: Modifiers) -> Event {
    Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers,
    }
}

/// Moves the pointer to `pos` and clicks the primary button there.
fn click(h: &mut App, pos: Pos2) {
    h.hover_at(pos);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(button_event(pos, PointerButton::Primary, pressed, Modifiers::NONE));
    }
    h.run_steps(3);
}

/// Drags with `button` from `from` to `to` in eight steps with `modifiers`
/// held.
fn drag(h: &mut App, from: Pos2, to: Pos2, button: PointerButton, modifiers: Modifiers) {
    h.event(Event::ModifiersChanged(modifiers));
    h.hover_at(from);
    h.run_steps(1);
    h.event(button_event(from, button, true, modifiers));
    h.run_steps(1);
    for step in 1..=8 {
        h.hover_at(from + (to - from) * (step as f32 / 8.0));
        h.run_steps(1);
    }
    h.event(button_event(to, button, false, modifiers));
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(3);
}

/// Opens the menu `title` of the menu bar: the topmost widget with that
/// label (the browser has headers with the same names).
fn open_menu(h: &mut App, title: &str) {
    let menu = h
        .query_all_by_label(title)
        .map(|n| n.rect())
        .min_by(|a, b| a.min.y.total_cmp(&b.min.y))
        .unwrap_or_else(|| panic!("no menu “{title}”"));
    click(h, menu.center());
}

/// Opens `path` from File ▸ Open recent.
fn open_recent(h: &mut App, path: &Path) {
    open_menu(h, "File");
    h.get_by_label_contains("Open recent").hover();
    h.run_steps(2);
    h.get_by_label(&path.display().to_string()).click();
    h.run_steps(4);
    let ws = h.state_mut().workspace();
    assert_eq!(ws.doc.path.as_deref(), Some(path), "the project is open");
}

fn undo_len(h: &mut App) -> usize {
    h.state_mut().workspace().doc.history.undo_len()
}

fn thickness(h: &mut App, wall: ElementId) -> f64 {
    h.state_mut().workspace().doc.project.elements[&wall]
        .as_wall()
        .unwrap()
        .thickness
}

/// The screen position of the plan point `p` in the active plan view,
/// asserted to lie in the view.
fn plan_point(h: &mut App, p: archicraft_model::glam::DVec2) -> Pos2 {
    let ws = h.state_mut().workspace();
    let state = &ws.plan_views[&ws.active_storey];
    let pos = state.camera.to_screen(state.last_rect, p);
    assert!(state.last_rect.contains(pos), "{p} is outside the plan view");
    pos
}

/// What the last frame painted inside `area`: its images (the texture's
/// name, which egui takes from the icon's uri, where it was painted and its
/// tint) and its texts (where, their colour and the sizes of their fonts).
#[derive(Debug, Default)]
struct Painted {
    images: Vec<(String, Rect, Color32)>,
    texts: Vec<(String, Rect, Color32, Vec<f32>)>,
}

fn collect(h: &App, shape: &Shape, area: Rect, out: &mut Painted) {
    let texture_name = |id| h.ctx.tex_manager().read().meta(id).map(|m| m.name.clone());
    match shape {
        Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(h, s, area, out)),
        Shape::Mesh(mesh) if !mesh.vertices.is_empty() && area.contains_rect(mesh.calc_bounds()) => {
            let name = texture_name(mesh.texture_id).unwrap_or_default();
            out.images.push((name, mesh.calc_bounds(), mesh.vertices[0].color));
        }
        // egui paints an image as a rect filled with its texture.
        Shape::Rect(rect) if rect.brush.is_some() && area.contains_rect(rect.rect) => {
            let texture = rect.brush.as_ref().unwrap().fill_texture_id;
            out.images
                .push((texture_name(texture).unwrap_or_default(), rect.rect, rect.fill));
        }
        Shape::Text(text) => {
            let bounds = text.visual_bounding_rect();
            if area.contains_rect(bounds) && !text.galley.text().trim().is_empty() {
                let sizes = text.galley.job.sections.iter().map(|s| s.format.font_id.size).collect();
                let colour = text.override_text_color.unwrap_or(text.fallback_color);
                out.texts.push((text.galley.text().to_owned(), bounds, colour, sizes));
            }
        }
        _ => {}
    }
}

fn painted_in(h: &App, area: Rect) -> Painted {
    let mut out = Painted::default();
    for clipped in &h.output().shapes {
        collect(h, &clipped.shape, area, &mut out);
    }
    out
}

fn uri(icon: &egui::ImageSource<'static>) -> String {
    icon.uri().unwrap().to_owned()
}

/// The toolbar's buttons, left to right: each one's rectangle and label.
/// Each shows no text.
fn toolbar_buttons(h: &App) -> Vec<(Rect, String)> {
    let bar = panel(h, "toolbar");
    let mut buttons: Vec<(Rect, String)> = h
        .query_all(By::new().role(Role::Button))
        .filter(|n| bar.contains(n.rect().center()))
        .map(|n| {
            assert!(
                n.children().all(|child| child.accesskit_node().role() != Role::TextRun),
                "{n:?} shows text"
            );
            (n.rect(), n.accesskit_node().label().unwrap_or_default())
        })
        .collect();
    buttons.sort_by(|a, b| a.0.min.x.total_cmp(&b.0.min.x));
    buttons
}

fn toolbar_button(h: &App, label: &str) -> Rect {
    let bar = panel(h, "toolbar");
    h.query_all(By::new().role(Role::Button).label(label))
        .find(|n| bar.contains(n.rect().center()))
        .unwrap_or_else(|| panic!("no toolbar button “{label}”"))
        .rect()
}

/// Asserts that `rect` is an icon button's square of side [`ICON_BUTTON`].
fn assert_icon_square(label: &str, rect: Rect) {
    assert!(
        (rect.width() - ICON_BUTTON).abs() < 0.5 && (rect.height() - ICON_BUTTON).abs() < 0.5,
        "{label} is not a {ICON_BUTTON} pt square: {rect:?}"
    );
}

/// The browser row whose label contains `text`: between the browser's
/// "Storeys" and "Project" headers, so neither a tab nor the status line.
fn row(h: &App, text: &str) -> Rect {
    let browser = panel(h, "browser");
    let storeys = h
        .query_all_by_label(&tr!("browser-storeys"))
        .map(|n| n.rect())
        .find(|r| browser.contains(r.center()))
        .expect("the browser's Storeys header");
    let project = h
        .query_all_by_label("Project")
        .map(|n| n.rect())
        .find(|r| browser.contains(r.center()) && r.min.y > storeys.max.y)
        .expect("the browser's Project header");
    let rows: Vec<Rect> = h
        .query_all_by_label_contains(text)
        .map(|n| n.rect())
        .filter(|r| browser.contains(r.center()) && r.min.y > storeys.max.y && r.max.y < project.min.y)
        .collect();
    assert_eq!(rows.len(), 1, "browser rows with “{text}”: {rows:?}");
    rows[0]
}

/// The one icon of the browser row with `text`, after checking that it
/// sits before the text and is tinted like it.
fn icon_of(h: &App, text: &str) -> String {
    let rect = row(h, text);
    let painted = painted_in(h, rect);
    let [(name, icon, tint)] = &painted.images[..] else {
        panic!("“{text}” paints {painted:?}")
    };
    let (_, label, colour, _) = painted.texts.first().unwrap_or_else(|| panic!("“{text}” has no text"));
    assert!(icon.max.x <= label.min.x, "the icon of “{text}” is not before its text");
    assert_eq!(tint, colour, "the icon of “{text}” is not tinted like its text");
    name.clone()
}

fn open_row(h: &mut App, text: &str) {
    let at = row(h, text).center();
    click(h, at);
}

fn view_named(h: &mut App, name: &str) -> Option<ViewId> {
    let views = &h.state_mut().workspace().doc.project.views;
    views.iter().find(|v| v.name == name).map(|v| v.id)
}

fn active_pane(h: &mut App) -> Option<Pane> {
    h.state_mut().workspace().active_pane
}

/// The rect and the camera of the open 3D view `view`.
fn view_camera(h: &mut App, view: ViewId) -> (Rect, Camera3d) {
    let state = &h.state_mut().workspace().views_3d[&view];
    (state.last_rect(), state.camera)
}

/// Asserts that `camera` shows the saved viewpoint `saved` within 1e-9.
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

/// The button labelled `label` in the view-control bar.
fn view_bar_button(h: &App, label: &str) -> Rect {
    let bar = panel(h, "view_bar");
    let node = h
        .query_all(By::new().role(Role::Button).label(label))
        .find(|n| bar.contains(n.rect().center()))
        .unwrap_or_else(|| panic!("no {label} button in the view-control bar"));
    assert!(
        node.children()
            .all(|child| child.accesskit_node().role() != Role::TextRun),
        "{label} shows text"
    );
    node.rect()
}

/// The browser toggle's rectangle and state: the one button named
/// [`BROWSER_TOGGLE`] (the View menu's entry is a check box).
fn browser_toggle(h: &App) -> (Rect, Option<Toggled>) {
    let toggles: Vec<_> = h
        .query_all_by_label(BROWSER_TOGGLE)
        .filter(|n| n.accesskit_node().role() == Role::Button)
        .map(|n| (n.rect(), n.accesskit_node().toggled()))
        .collect();
    match toggles[..] {
        [toggle] => toggle,
        ref all => panic!("not exactly one browser toggle: {all:?}"),
    }
}

fn browser_drawn(h: &App) -> bool {
    h.query_all_by_label(&tr!("browser-storeys")).next().is_some()
}

/// Drags the browser's left edge by `dx` points: from a point on the edge
/// below its rows, half a point inside the panel.
fn drag_browser_edge(h: &mut App, dx: f32) {
    let panel = panel(h, "browser");
    let start = egui::pos2(panel.left() + 0.5, panel.bottom() - 40.0);
    drag(
        h,
        start,
        start + egui::vec2(dx, 0.0),
        PointerButton::Primary,
        Modifiers::NONE,
    );
}

/// Types `metres`, as the app writes a length, into the strip's text field
/// labelled `label` that lies in `area` (the row or the chevron's popup)
/// and presses Enter: one undo step.
fn type_length(h: &mut App, label: &str, area: Rect, metres: f64) {
    let typed = {
        let ws = h.state_mut().workspace();
        format_length(metres, &ws.doc.project.settings.units, &ws.prefs.number_format(), false)
    };
    let field = h
        .query_all(By::new().role(Role::TextInput).label(label))
        .find(|n| area.contains(n.rect().center()))
        .unwrap_or_else(|| panic!("no field “{label}” in {area:?}"))
        .rect();
    let steps = undo_len(h);
    click(h, field.center());
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.event(Event::Text(typed));
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(undo_len(h), steps + 1, "{label}: one undo step");
}

/// Clicks W-001 in the active plan with the selection tool and returns it,
/// asserting that it is then the selection.
fn select_w001(h: &mut App) -> ElementId {
    let (wall, start, end, wall_thickness) = {
        let project = &h.state_mut().workspace().doc.project;
        let (id, wall) = project
            .elements
            .values()
            .find(|e| e.name == "W-001")
            .map(|e| (e.id, e.as_wall().unwrap().clone()))
            .expect("the project has W-001");
        (id, wall.start, wall.end, wall.thickness)
    };
    h.key_press(Key::V);
    h.run_steps(2);
    // W-001 runs from (0, 0) to (10, 0) with its outside face on that line:
    // its body lies on the left of it.
    let body = (end - start).perp().normalize() * wall_thickness / 2.0;
    let at = plan_point(h, (start + end) / 2.0 + body);
    click(h, at);
    let selection: Vec<ElementId> = h.state_mut().workspace().selection.iter().copied().collect();
    assert_eq!(selection, [wall], "a click selects W-001");
    wall
}

/// Step 1 (UI-002): with the selection tool, a click on the wall W-001
/// selects it. At 1000 px the strip is one row lower than two lines, its
/// units follow the documented order (the summary, Name and Home storey,
/// then the fields) with separators between the groups, and the chevron
/// is its last widget. The thickness fits into the row at this width, so
/// 0.30 m is typed there; the top offset does not, and the chevron's popup
/// holds it: 0.10 m typed there sets the wall's top offset. Each edit is one
/// undo step. Returns the wall.
fn step_1_strip(h: &mut App) -> ElementId {
    let wall = select_w001(h);

    let strip = panel(h, "properties");
    let line = h.ctx.global_style().spacing.interact_size.y;
    assert!(strip.height() < 2.0 * line, "one row: {strip:?}");
    let row = RowLayout::load(&h.ctx, properties::strip_id()).expect("the strip's layout");
    assert_eq!(row.names[..2], ["summary:Wall", "NameAndStorey"], "{:?}", row.names);
    assert!(row.names[2].starts_with("field:"), "{:?}", row.names);
    assert!(!row.separators.is_empty(), "the groups are separated");
    for pair in row.rects[..row.shown].windows(2) {
        assert!(pair[0].max.x <= pair[1].min.x + 0.5, "left to right: {pair:?}");
    }
    let at = |name: &str| row.names.iter().position(|n| n == name).unwrap();
    assert!(
        at("field:thickness") < row.shown,
        "the thickness is in the row: {row:?}"
    );
    assert!(at("field:top_offset") >= row.shown, "the top offset is not: {row:?}");
    let chevron = row.button.expect("the chevron");
    assert!(chevron.max.x <= WINDOW.x, "the chevron is inside the window");
    assert!(
        row.rects[..row.shown].iter().all(|r| r.max.x <= chevron.min.x),
        "the chevron is the strip's last widget"
    );

    type_length(h, "Thickness", strip, 0.3);
    assert!((thickness(h, wall) - 0.3).abs() < 1e-9, "{}", thickness(h, wall));

    click(h, chevron.center());
    let popup = strip_overflow_rect(h).expect("the chevron opens its popup");
    type_length(h, "Top offset", popup, 0.1);
    let top = h.state_mut().workspace().doc.project.elements[&wall]
        .as_wall()
        .unwrap()
        .top;
    assert!(
        matches!(top, TopLink::Relative { offset, .. } if (offset - 0.1).abs() < 1e-9),
        "{top:?}"
    );
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(strip_overflow_rect(h).is_none(), "Escape closes the popup");
    wall
}

/// Step 2 (UI-001, UI-011, PRJ-003): the toolbar's buttons are the demo's
/// icons without text, in that order, each a square of the toolbox
/// buttons' side, whose icons are a third larger than the former 20 pt
/// ones; Save paints the floppy disk. Hovering Save shows its name,
/// shortcut and description; a click on Layers opens the Layers dialog.
fn step_2_toolbar(h: &mut App) {
    let buttons = toolbar_buttons(h);
    let labels: Vec<&str> = buttons.iter().map(|(_, l)| l.as_str()).collect();
    assert_eq!(labels, TOOLBAR);
    for (rect, label) in &buttons {
        assert_icon_square(label, *rect);
    }
    let toolbox = panel(h, "toolbox");
    let wall_tool = h
        .query_all(By::new().role(Role::Button).label("Wall"))
        .find(|n| toolbox.contains(n.rect().center()))
        .expect("the toolbox's wall tool")
        .rect();
    assert_icon_square("The wall tool", wall_tool);
    let save = painted_in(h, toolbar_button(h, "Save"));
    let images: Vec<&str> = save.images.iter().map(|(name, ..)| name.as_str()).collect();
    assert_eq!(images, [uri(&icons::SAVE)], "Save is the floppy disk");
    let (_, icon, _) = &save.images[0];
    assert!((icon.width() - ICON).abs() < 0.5, "the icon is {ICON} pt: {icon:?}");
    assert!(
        icon.width() >= FORMER_ICON * 4.0 / 3.0 - 0.5,
        "a third larger than {FORMER_ICON} pt: {icon:?}"
    );

    h.hover_at(toolbar_button(h, "Save").center());
    h.run_steps(4);
    let keys = h
        .ctx
        .format_shortcut(&egui::KeyboardShortcut::new(Modifiers::COMMAND, Key::S));
    for text in ["Save", keys.as_str(), "Save the project to its file."] {
        assert_present(h, text);
    }

    let layers = By::new().role(Role::Window).label("Layers");
    assert!(h.query(layers.clone()).is_none());
    let at = toolbar_button(h, "Layers…").center();
    click(h, at);
    assert!(h.query(layers.clone()).is_some(), "Layers opens the Layers dialog");
    let window = h.get(layers.clone()).rect();
    let close = h
        .query_all_by_label("Close window")
        .map(|n| n.rect())
        .find(|r| window.contains(r.center()))
        .expect("the Layers dialog's close button");
    click(h, close.center());
    assert!(h.query(layers).is_none(), "the Layers dialog is closed");
}

/// Step 3 (VIEW-003, UI-015, UI-004): each storey row shows the plan sheet,
/// the perspective the camera and the parallel view the tall box, tinted
/// like their text. Dragging the browser's edge 80 pt to the left makes it
/// 80 pt wider, 40 pt back to the right 40 pt narrower. The toggle at the
/// right end of the tab row hides the browser and shows it again at that
/// width.
fn step_3_browser(h: &mut App) {
    let storeys: Vec<String> = {
        let storeys = &h.state_mut().workspace().doc.project.storeys;
        storeys.highest_first().map(|s| s.name.clone()).collect()
    };
    assert_eq!(storeys.len(), 2, "{storeys:?}");
    for storey in &storeys {
        assert_eq!(icon_of(h, storey), uri(&icons::PLAN_SHEET), "{storey}");
    }
    assert_eq!(icon_of(h, "3D perspective"), uri(&icons::CAMERA));
    assert_eq!(icon_of(h, "3D parallel"), uri(&icons::PARALLEL_BOX));

    let start = panel(h, "browser").width();
    drag_browser_edge(h, -80.0);
    let wider = panel(h, "browser").width();
    assert!((wider - (start + 80.0)).abs() <= 1.0, "{start} -> {wider}");
    drag_browser_edge(h, 40.0);
    let width = panel(h, "browser").width();
    assert!((width - (wider - 40.0)).abs() <= 1.0, "{wider} -> {width}");

    let (toggle, state) = browser_toggle(h);
    assert_eq!(state, Some(Toggled::True));
    let tabs = h
        .query_all(By::new().role(Role::Button).label_contains("Plan: "))
        .map(|n| n.rect())
        .find(|r| !panel(h, "browser").contains(r.center()))
        .expect("the plan's tab");
    assert!(toggle.left() >= tabs.right(), "the toggle is right of the tabs");
    assert!((toggle.center().y - tabs.center().y).abs() < 2.0, "in the tab row");
    click(h, toggle.center());
    assert!(!browser_drawn(h), "the browser is hidden");
    let (toggle, state) = browser_toggle(h);
    assert_eq!(state, Some(Toggled::False));
    click(h, toggle.center());
    assert!(browser_drawn(h), "the browser is back");
    assert_eq!(browser_toggle(h).1, Some(Toggled::True));
    let back = panel(h, "browser").width();
    assert!((back - width).abs() <= 1.0, "at its width: {width} -> {back}");
}

/// Step 4 (UI-001, VIEW-013): in the plan, Zoom out, Zoom in and Fit are
/// icon buttons; a click on the zoom value, 150 and Enter zoom the plan to
/// 150 % against its drawing scale, and the bar shows "150 %". In the 3D
/// view Orbit is an icon button too.
fn step_4_view_bar(h: &mut App) {
    for label in ["Zoom out", "Zoom in", "Fit"] {
        assert_icon_square(label, view_bar_button(h, label));
    }
    let zoom_factor = h.ctx.zoom_factor();
    let percent = |h: &mut App| {
        let ws = h.state_mut().workspace();
        let plan = &ws.plan_views[&ws.active_storey];
        view_bar::plan_zoom_percent(plan.camera.scale, zoom_factor, f64::from(plan.scale.denominator()))
    };
    let shown = {
        let now = percent(h);
        let nf = h.state_mut().workspace().prefs.number_format();
        tr!("view-bar-zoom", percent = nf.format_decimal(now, 0))
    };
    let readout = view_bar_button(h, &shown);
    click(h, readout.center());
    h.get(By::new().role(Role::TextInput)).type_text("150");
    h.run_steps(1);
    h.key_press(Key::Enter);
    h.run_steps(2);
    let now = percent(h);
    assert!((now / 150.0 - 1.0).abs() < 0.005, "{now} %");
    view_bar_button(h, "150 %");

    let perspective = view_named(h, "3D perspective").unwrap();
    open_row(h, "3D perspective");
    assert_eq!(active_pane(h), Some(Pane::View3d(perspective)));
    for label in ["Zoom out", "Zoom in", "Fit", "Orbit"] {
        assert_icon_square(label, view_bar_button(h, label));
    }
}

/// Step 5, saving the camera (VIEW-004): Shift and the middle button orbit
/// the 3D perspective to a new viewpoint; the bar's camera icon saves it as
/// "Camera 1", listed with the camera icon, in one undo step. Opening the
/// parallel view and then Camera 1 shows the saved viewpoint. Returns the
/// saved view and its camera.
fn step_5_camera(h: &mut App) -> (ViewId, ViewCamera) {
    let perspective = view_named(h, "3D perspective").unwrap();
    let (rect, before) = view_camera(h, perspective);
    let centre = rect.center();
    drag(
        h,
        centre,
        centre + egui::vec2(60.0, -25.0),
        PointerButton::Middle,
        Modifiers::SHIFT,
    );
    let (_, orbited) = view_camera(h, perspective);
    assert!((orbited.yaw - before.yaw).abs() > 1e-3, "the drag orbits the view");

    let steps = undo_len(h);
    let button = view_bar_button(h, "Save camera");
    let images: Vec<String> = painted_in(h, button).images.into_iter().map(|i| i.0).collect();
    assert_eq!(images, [uri(&icons::CAMERA)], "Save camera is the camera icon");
    click(h, button.center());
    assert_eq!(undo_len(h), steps + 1, "saving is one undo step");
    let camera = view_named(h, "Camera 1").expect("Camera 1 is saved");
    let saved = {
        let views = &h.state_mut().workspace().doc.project.views;
        let view = views.iter().find(|v| v.id == camera).unwrap();
        assert_eq!(view.projection, Projection::Perspective);
        view.camera.expect("Camera 1 has a camera")
    };
    assert_shows(&orbited, saved);
    assert_eq!(icon_of(h, "Camera 1"), uri(&icons::CAMERA));

    open_row(h, "3D parallel");
    assert_eq!(
        active_pane(h),
        Some(Pane::View3d(view_named(h, "3D parallel").unwrap()))
    );
    open_row(h, "Camera 1");
    assert_eq!(active_pane(h), Some(Pane::View3d(camera)));
    assert_shows(&view_camera(h, camera).1, saved);
    (camera, saved)
}

/// Step 5, saving the project (PRJ-003): the opened project has a path, so
/// the toolbar's Save writes it there without asking.
fn step_5_save(h: &mut App, path: &Path) {
    h.state_mut().stub_save_dialog(None);
    let at = toolbar_button(h, "Save").center();
    click(h, at);
    assert_eq!(
        h.state().save_dialogs_asked(),
        0,
        "a project with a path saves without asking"
    );
    assert!(!h.state_mut().workspace().doc.is_dirty());
    assert_present(h, &tr!("status-saved", path = path.display().to_string()));
}

/// The panels whose texts have one size (step 6), by their egui ids.
const TEXT_PANELS: [&str; 5] = ["toolbar", "properties", "view_bar", "status", "browser"];

/// Asserts that every text the last frame painted in the [`TEXT_PANELS`]
/// has the size `body`, and returns how many each panel painted.
fn assert_one_text_size(h: &App, at: &str, body: f32) -> [usize; 5] {
    TEXT_PANELS.map(|name| {
        let painted = painted_in(h, panel(h, name));
        for (text, _, _, sizes) in &painted.texts {
            assert!(
                sizes.iter().all(|&s| s == body),
                "{at}: “{text}” in the {name} panel is {sizes:?}, not {body} pt"
            );
        }
        painted.texts.len()
    })
}

/// Step 6 (UI-001): in the open 3D view, and in the plan, opened from the
/// browser, with W-001 selected again and the pointer over the plan (so
/// that the status line shows the cursor readout), every text painted in
/// the toolbar, the properties strip, the view-control bar, the status line
/// and the browser has the size of the Body style. The toolbar paints
/// none: its buttons are icons (step 2); each of the others paints some.
fn step_6_text_sizes(h: &mut App, wall: ElementId) {
    let body = h.ctx.global_style().text_styles[&egui::TextStyle::Body].size;
    let in_3d = assert_one_text_size(h, "3D view", body);

    let storey = {
        let ws = h.state_mut().workspace();
        ws.doc.project.storeys.get(ws.active_storey).unwrap().name.clone()
    };
    open_row(h, &storey);
    assert!(matches!(active_pane(h), Some(Pane::Plan(_))), "the plan is open");
    assert_eq!(select_w001(h), wall);
    let centre = {
        let ws = h.state_mut().workspace();
        ws.plan_views[&ws.active_storey].last_rect.center()
    };
    h.hover_at(centre);
    h.run_steps(3);
    let in_plan = assert_one_text_size(h, "plan", body);
    for (i, name) in TEXT_PANELS.iter().enumerate() {
        let painted = in_3d[i] + in_plan[i];
        if *name == "toolbar" {
            assert_eq!(painted, 0, "the toolbar shows no text");
        } else {
            assert!(painted > 0, "the {name} panel paints no text: {in_3d:?} {in_plan:?}");
        }
    }
}

/// UI-001, UI-002, UI-004, UI-011, UI-015, VIEW-003, VIEW-004, VIEW-013,
/// PRJ-003: the demo script of Showcase 3.5 in one session in a window
/// 1000 px wide, from opening a project with File ▸ Open recent to saving
/// it, then a restart that reopens it: the thickness edited in step 1 is
/// kept, Camera 1 is listed with the camera icon and opening it shows the
/// saved viewpoint.
#[test]
fn the_showcase_3_5_demo_script_runs_from_an_opened_project_to_a_reopened_camera() {
    let dir = data_dir("showcase-3-5");
    let project_dir = data_dir("showcase-3-5-project");
    std::fs::create_dir_all(&project_dir).unwrap();
    let path = project_dir.join("walls-and-slab.ifcx");
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join("../archicraft-ifcx/tests/golden/walls-and-slab.ifcx");
    std::fs::copy(golden, &path).unwrap();
    let prefs = Preferences {
        recent: vec![path.clone()],
        ..Preferences::default()
    };
    let mut h = harness_in(&dir, prefs, 1.0);
    h.set_size(WINDOW);
    h.run_steps(3);
    open_recent(&mut h, &path);
    assert!(matches!(active_pane(&mut h), Some(Pane::Plan(_))), "a plan is open");

    let wall = step_1_strip(&mut h);
    step_2_toolbar(&mut h);
    step_3_browser(&mut h);
    step_4_view_bar(&mut h);
    let (camera, saved) = step_5_camera(&mut h);
    step_6_text_sizes(&mut h, wall);
    step_5_save(&mut h, &path);

    // Step 5, reopening: a restart with the preferences the session saved
    // starts with a new project; File ▸ Open recent opens the saved one.
    drop(h);
    let prefs = Preferences::load(&DataDirs::at(&dir).preferences_file());
    let mut h = harness_in(&dir, prefs, 1.0);
    h.set_size(WINDOW);
    h.run_steps(3);
    assert!(view_named(&mut h, "Camera 1").is_none(), "the restart starts anew");
    open_recent(&mut h, &path);
    assert!((thickness(&mut h, wall) - 0.3).abs() < 1e-9, "the thickness is kept");
    assert_eq!(view_named(&mut h, "Camera 1"), Some(camera));
    assert_eq!(icon_of(&h, "Camera 1"), uri(&icons::CAMERA));
    open_row(&mut h, "Camera 1");
    assert_eq!(active_pane(&mut h), Some(Pane::View3d(camera)));
    assert_shows(&view_camera(&mut h, camera).1, saved);

    drop(h);
    std::fs::remove_dir_all(&project_dir).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
