//! The project browser's panel (UI-004, UI-015, UI-001): its left edge
//! drags it wider or narrower between a minimum and half the window, a
//! toggle at the right end of the view tab row and the View menu hide and
//! show it, and its width and whether it is shown survive a restart.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use archicraft_app::ArchiCraftApp;
use archicraft_app::persistence::DataDirs;
use archicraft_app::persistence::prefs::{BROWSER_MIN_WIDTH, Preferences};
use archicraft_app::tr;
use archicraft_app::workspace::Pane;
use common::{data_dir, harness, harness_in};
use egui::accesskit::{Role, Toggled};
use egui::{Event, Modifiers, PointerButton, Pos2, Rect};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_tiles::Tree;

type App = Harness<'static, ArchiCraftApp>;

/// The accessible name of the toggle, the command's label.
const TOGGLE: &str = "Project browser";

/// The browser panel's rectangle as egui stored it.
fn browser(h: &App) -> Rect {
    egui::containers::panel::PanelState::load(&h.ctx, egui::Id::new("browser"))
        .expect("the browser panel")
        .outer_rect
}

/// Whether the browser was drawn in the last frame: its Storeys header.
fn browser_drawn(h: &App) -> bool {
    h.query_all_by_label(&tr!("browser-storeys")).next().is_some()
}

fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

/// A point on the browser's left edge below its rows, half a point inside
/// the panel: the views beside it take the pointer outside, and the panel
/// takes its width from where the pointer is.
fn on_edge(h: &App) -> Pos2 {
    let panel = browser(h);
    egui::pos2(panel.left() + 0.5, panel.bottom() - 40.0)
}

/// Drags the browser's left edge by `dx` points in eight steps.
fn drag_browser_edge(h: &mut App, dx: f32) {
    let start = on_edge(h);
    h.hover_at(start);
    h.run_steps(1);
    h.event(button(start, true));
    h.run_steps(1);
    for step in 1..=8 {
        h.hover_at(start + egui::vec2(dx * step as f32 / 8.0, 0.0));
        h.run_steps(1);
    }
    h.event(button(start + egui::vec2(dx, 0.0), false));
    h.run_steps(3);
}

/// The browser toggles: the buttons named [`TOGGLE`] (the View menu's
/// entry is a check box).
fn toggles(h: &App) -> Vec<Rect> {
    h.query_all_by_label(TOGGLE)
        .filter(|n| n.accesskit_node().role() == Role::Button)
        .map(|n| n.rect())
        .collect()
}

fn toggle(h: &App) -> Rect {
    match toggles(h)[..] {
        [rect] => rect,
        ref all => panic!("not exactly one browser toggle: {all:?}"),
    }
}

fn toggle_state(h: &App) -> Option<Toggled> {
    h.query_all_by_label(TOGGLE)
        .find(|n| n.accesskit_node().role() == Role::Button)
        .expect("the browser toggle")
        .accesskit_node()
        .toggled()
}

fn click(h: &mut App, rect: Rect) {
    h.hover_at(rect.center());
    h.run_steps(1);
    h.event(button(rect.center(), true));
    h.run_steps(1);
    h.event(button(rect.center(), false));
    h.run_steps(3);
}

/// The tab of the view `title`: the button with a close button inside (the
/// browser and the status line name views too).
fn tab(h: &App, title: &str) -> Rect {
    let closes: Vec<Rect> = h.query_all_by_label("Close").map(|n| n.rect()).collect();
    h.query_all_by_label(title)
        .find(|n| n.accesskit_node().role() == Role::Button && closes.iter().any(|c| n.rect().contains_rect(*c)))
        .unwrap_or_else(|| panic!("no tab “{title}”"))
        .rect()
}

fn plan_rect(h: &mut App) -> Rect {
    let ws = h.state_mut().workspace();
    let storey = ws.doc.project.storeys.ground();
    ws.plan_views.get(&storey).expect("the plan was drawn").last_rect
}

/// UI-015: dragging the browser's left edge 100 pt to the left makes it
/// 100 pt wider and 100 pt to the right narrower; the resize cursor shows on
/// the edge, and drags past the limits stop at the minimum and at half the
/// window.
#[test]
fn dragging_the_left_edge_resizes_the_browser_within_its_limits() {
    let mut h = harness("browser-resize");
    let start = browser(&h).width();

    let edge = on_edge(&h);
    h.hover_at(edge);
    h.run_steps(2);
    assert_eq!(
        h.output().platform_output.cursor_icon,
        egui::CursorIcon::ResizeHorizontal,
        "the resize cursor on the edge"
    );

    drag_browser_edge(&mut h, -100.0);
    let wider = browser(&h).width();
    assert!((wider - (start + 100.0)).abs() <= 1.0, "{start} -> {wider}");
    drag_browser_edge(&mut h, 100.0);
    let back = browser(&h).width();
    assert!((back - (wider - 100.0)).abs() <= 1.0, "{wider} -> {back}");
    drag_browser_edge(&mut h, 60.0);
    let narrower = browser(&h).width();
    assert!((narrower - (back - 60.0)).abs() <= 1.0, "{back} -> {narrower}");

    let window = h.ctx.content_rect().width();
    drag_browser_edge(&mut h, -2000.0);
    let widest = browser(&h).width();
    assert!((widest - window / 2.0).abs() <= 1.0, "{widest} is not half of {window}");
    drag_browser_edge(&mut h, 2000.0);
    let narrowest = browser(&h).width();
    assert!(
        (narrowest - BROWSER_MIN_WIDTH).abs() <= 1.0,
        "{narrowest} is not the minimum {BROWSER_MIN_WIDTH}"
    );
    // The storey row still fits at the minimum.
    assert!(browser_drawn(&h));
}

/// UI-004, UI-001: the rightmost widget of the view tab row is the browser
/// toggle, checked while the browser shows; a click hides the browser, so
/// it is not drawn and the views grow by its width, and a second click
/// shows it again at the width it had.
#[test]
fn the_tab_row_toggle_hides_and_shows_the_browser() {
    let mut h = harness("browser-toggle");
    drag_browser_edge(&mut h, -50.0);
    let width = browser(&h).width();
    let plan_tab = tab(&h, "Plan: 00 Ground floor");
    let view_tab = tab(&h, "3D perspective");
    let shown = toggle(&h);
    assert!(
        shown.left() >= plan_tab.right().max(view_tab.right()),
        "{shown:?} is not right of the tabs {plan_tab:?} {view_tab:?}"
    );
    assert!(
        (shown.center().y - plan_tab.center().y).abs() < 2.0,
        "{shown:?} is not in the tab row of {plan_tab:?}"
    );
    let row = plan_tab.y_range();
    let views_right = browser(&h).left() + 1.0;
    let rightmost = h
        .query_all(egui_kittest::kittest::By::new())
        .filter_map(|n| n.accesskit_node().bounding_box())
        .filter(|b| row.contains(((b.y0 + b.y1) / 2.0) as f32) && (b.x1 as f32) <= views_right)
        .map(|b| b.x1 as f32)
        .fold(f32::MIN, f32::max);
    assert!(
        (shown.right() - rightmost).abs() < 0.5,
        "{shown:?}, rightmost {rightmost}"
    );
    assert_eq!(toggle_state(&h), Some(Toggled::True));
    let views = plan_rect(&mut h).width();

    click(&mut h, shown);
    assert!(!browser_drawn(&h), "the browser is hidden");
    assert_eq!(toggle_state(&h), Some(Toggled::False));
    let hidden = toggle(&h);
    assert!(
        (hidden.right() - (shown.right() + width)).abs() <= 1.0,
        "{shown:?} -> {hidden:?}"
    );
    let grown = plan_rect(&mut h).width();
    assert!(
        (grown - (views + width)).abs() <= 1.0,
        "{views} -> {grown}, browser {width}"
    );

    click(&mut h, hidden);
    assert!(browser_drawn(&h));
    assert_eq!(toggle_state(&h), Some(Toggled::True));
    assert!((browser(&h).width() - width).abs() <= 1.0, "at its previous width");
    assert!((toggle(&h).right() - shown.right()).abs() <= 1.0);
}

/// Opens the menu `title` of the menu bar (the browser has headers with the
/// same names).
fn open_menu(h: &mut App, title: &str) {
    let menu = h
        .query_all_by_label(title)
        .map(|n| n.rect())
        .min_by(|a, b| a.min.y.total_cmp(&b.min.y))
        .unwrap_or_else(|| panic!("no menu “{title}”"));
    click(h, menu);
}

fn menu_entry(h: &App) -> egui_kittest::Node<'_> {
    h.query_all_by_label(TOGGLE)
        .find(|n| n.accesskit_node().role() == Role::CheckBox)
        .expect("the View menu's Project browser entry")
}

/// UI-004: the View menu's Project browser entry is checked while the
/// browser shows and hides and shows it like the toggle.
#[test]
fn the_view_menu_hides_and_shows_the_browser() {
    let mut h = harness("browser-menu");
    let width = browser(&h).width();
    open_menu(&mut h, "View");
    assert_eq!(menu_entry(&h).accesskit_node().toggled(), Some(Toggled::True));
    let entry = menu_entry(&h).rect();
    click(&mut h, entry);
    assert!(!browser_drawn(&h));
    assert_eq!(toggle_state(&h), Some(Toggled::False));

    open_menu(&mut h, "View");
    assert_eq!(menu_entry(&h).accesskit_node().toggled(), Some(Toggled::False));
    let entry = menu_entry(&h).rect();
    click(&mut h, entry);
    assert!(browser_drawn(&h));
    assert!((browser(&h).width() - width).abs() <= 1.0);
}

/// UI-004: with two tab bars side by side, and with two above each other,
/// there is exactly one toggle, at the right end of the tab bar in the
/// top right corner.
#[test]
fn split_views_have_one_toggle_in_the_top_right_tab_bar() {
    let mut h = harness("browser-split");
    let (storey, view) = {
        let ws = h.state_mut().workspace();
        (ws.doc.project.storeys.ground(), ws.doc.project.views[0].id)
    };
    let panes = vec![Pane::Plan(storey), Pane::View3d(view)];
    *h.state_mut().tree_mut() = Tree::new_horizontal(egui::Id::new("archicraft-views"), panes.clone());
    h.run_steps(3);
    let plan_tab = tab(&h, "Plan: 00 Ground floor");
    let view_tab = tab(&h, "3D perspective");
    assert!(plan_tab.right() < view_tab.left(), "side by side");
    let side_by_side = toggle(&h);
    assert!(side_by_side.left() >= view_tab.right(), "{side_by_side:?} {view_tab:?}");
    assert!((side_by_side.right() - (browser(&h).left())).abs() <= 2.0);

    *h.state_mut().tree_mut() = Tree::new_vertical(egui::Id::new("archicraft-views"), panes);
    h.run_steps(3);
    let plan_tab = tab(&h, "Plan: 00 Ground floor");
    let view_tab = tab(&h, "3D perspective");
    assert!(plan_tab.bottom() < view_tab.top(), "above each other");
    let stacked = toggle(&h);
    assert!(
        (stacked.center().y - plan_tab.center().y).abs() < 2.0,
        "in the top tab bar: {stacked:?} {plan_tab:?}"
    );
    assert!((stacked.right() - side_by_side.right()).abs() <= 1.0);
}

/// UI-015, UI-004: the browser's width and whether it is shown persist in
/// the preferences, and the browser opens so after a restart.
#[test]
fn the_browser_width_and_visibility_survive_a_restart() {
    let dir = data_dir("browser-restart");
    let mut h = harness_in(&dir, Preferences::default(), 1.0);
    drag_browser_edge(&mut h, -80.0);
    let width = browser(&h).width();
    drop(h);
    let saved = Preferences::load(&DataDirs::at(&dir).preferences_file());
    assert!(
        (saved.browser_width - width).abs() <= 1.0,
        "{} {width}",
        saved.browser_width
    );
    assert!(saved.browser_shown);

    let mut h = harness_in(&dir, saved, 1.0);
    assert!((browser(&h).width() - width).abs() <= 1.0, "restored width");
    let shown = toggle(&h);
    click(&mut h, shown);
    assert!(!browser_drawn(&h));
    drop(h);
    let saved = Preferences::load(&DataDirs::at(&dir).preferences_file());
    assert!(!saved.browser_shown);

    let mut h = harness_in(&dir, saved, 1.0);
    assert!(!browser_drawn(&h), "still hidden after a restart");
    let hidden = toggle(&h);
    click(&mut h, hidden);
    assert!(browser_drawn(&h));
    assert!((browser(&h).width() - width).abs() <= 1.0, "at the saved width");
}
