//! Contour detection in the application (INP-018, EDT-020): the Contour
//! detection command of the Design menu, its status-bar indicator, the key
//! held for it, the preview and the click in the plan view, and the cached
//! contour graph; the space tool's detection of rooms (SPC-001).

#![allow(clippy::unwrap_used)]

mod common;

use archicraft_app::ArchiCraftApp;
use archicraft_app::input::{Expect, InputMode};
use archicraft_app::tools::ToolKind;
use archicraft_app::workspace::{Pane, Request};
use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::WriteOptions;
use archicraft_model::glam::DVec2;
use archicraft_model::samples;
use common::{data_dir, harness};
use egui::accesskit::Toggled;
use egui::{Event, Key, Modifiers, PointerButton};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

/// The status-bar indicator while detection is on.
const INDICATOR: &str = "Contour detection on";

/// Opens two_storey_shell_with_shaft, written to an IFCX file under `name`,
/// with the tool `kind` active in the plan of the ground storey.
fn shaft_sample_with(name: &str, kind: ToolKind) -> Harness<'static, ArchiCraftApp> {
    let project = samples::two_storey_shell_with_shaft();
    let geometry = ModelGeometry::evaluate(&project);
    let options = WriteOptions {
        author: "contour test".into(),
        timestamp: "2026-11-04T00:00:00Z".into(),
        data_version: 1,
    };
    let (bytes, _) = archicraft_ifcx::save(&project, &geometry, &options, false);
    let dir = data_dir(name);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("two-storey-shell-with-shaft.ifcx");
    std::fs::write(&path, bytes).unwrap();
    let mut harness = harness(&format!("{name}-app"));
    harness.state_mut().open_path(&path);
    harness.run_steps(3);
    harness.state_mut().workspace().select_tool(kind);
    harness.run_steps(2);
    harness
}

/// The screen position of the plan point `p` in the active plan view.
fn screen_of(harness: &mut Harness<'static, ArchiCraftApp>, p: DVec2) -> egui::Pos2 {
    let ws = harness.state_mut().workspace();
    let state = &ws.plan_views[&ws.active_storey];
    state.camera.to_screen(state.last_rect, p)
}

fn hover(harness: &mut Harness<'static, ArchiCraftApp>, p: DVec2) {
    let pos = screen_of(harness, p);
    harness.hover_at(pos);
    harness.run_steps(2);
}

/// Clicks the primary button at the plan point `p`.
fn click(harness: &mut Harness<'static, ArchiCraftApp>, p: DVec2) {
    click_with(harness, p, Modifiers::NONE);
}

/// Clicks the primary button at the plan point `p` with `modifiers` held
/// from the hover on.
fn click_with(harness: &mut Harness<'static, ArchiCraftApp>, p: DVec2, modifiers: Modifiers) {
    harness.event(Event::ModifiersChanged(modifiers));
    hover(harness, p);
    let pos = screen_of(harness, p);
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

/// Whether contour detection is on and takes the active tool's clicks.
fn detecting(harness: &mut Harness<'static, ArchiCraftApp>) -> bool {
    harness.state_mut().workspace().tool.detecting()
}

fn shows(harness: &Harness<'static, ArchiCraftApp>, text: &str) -> bool {
    harness.query_all_by_label(text).next().is_some()
}

fn element_count(harness: &mut Harness<'static, ArchiCraftApp>) -> usize {
    harness.state_mut().workspace().doc.project.elements.len()
}

/// Opens the Design menu of the menu bar.
fn open_design_menu(harness: &mut Harness<'static, ArchiCraftApp>) {
    let design = harness
        .query_all_by_label("Design")
        .map(|n| n.rect())
        .min_by(|a, b| a.min.y.total_cmp(&b.min.y))
        .unwrap();
    harness.hover_at(design.center());
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: design.center(),
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run_steps(2);
}

/// Whether the Design menu checks Contour detection; closes the menu again
/// with a second click on its title, since Esc would also end the input.
fn menu_checked(harness: &mut Harness<'static, ArchiCraftApp>) -> bool {
    open_design_menu(harness);
    let checked = harness.get_by_label("Contour detection").accesskit_node().toggled() == Some(Toggled::True);
    open_design_menu(harness);
    assert!(
        harness.query_by_label("Contour detection").is_none(),
        "the menu is closed"
    );
    checked
}

/// EDT-020, INP-018: the Design menu has the Contour detection command; it
/// switches detection on, which the status bar shows while the tool's
/// clicks pick, and a click on the indicator switches it off again.
#[test]
fn the_design_menu_switches_contour_detection_and_the_status_bar_shows_it() {
    let mut harness = shaft_sample_with("contour-menu", ToolKind::Slab);
    assert!(!shows(&harness, INDICATOR));
    open_design_menu(&mut harness);
    harness.get_by_label("Contour detection").click();
    harness.run_steps(2);
    assert!(detecting(&mut harness), "the command switches detection on");
    assert!(shows(&harness, INDICATOR), "the status bar shows it");
    // Only while it takes the clicks: not with the selection tool.
    harness.state_mut().workspace().select_tool(ToolKind::Select);
    harness.run_steps(2);
    assert!(!shows(&harness, INDICATOR), "no indicator with the selection tool");
    harness.state_mut().workspace().select_tool(ToolKind::Slab);
    harness.run_steps(2);
    assert!(shows(&harness, INDICATOR), "the slab tool picks again");

    harness.get_by_label(INDICATOR).click();
    harness.run_steps(2);
    assert!(!detecting(&mut harness), "the indicator switches it off");
    assert!(!shows(&harness, INDICATOR));
}

/// INP-018, SLB-001: while Space is held, the slab tool previews the
/// region around (2, 3) on the ground storey and a click creates a slab of
/// that ring, 97.08875 m²; releasing Space returns to normal input, and
/// Esc ends a detection switched on by the command, neither changing the
/// project.
#[test]
fn holding_the_key_picks_a_region_and_release_or_esc_returns() {
    let mut harness = shaft_sample_with("contour-hold", ToolKind::Slab);
    let p = DVec2::new(2.0, 3.0);
    hover(&mut harness, p);
    harness.key_down(Key::Space);
    harness.run_steps(2);
    hover(&mut harness, p);
    assert!(detecting(&mut harness), "Space held switches to detection");
    let preview = {
        let ws = harness.state_mut().workspace();
        let step = ws.consumer().step().unwrap();
        assert_eq!(step.expects, Expect::Pick);
        assert!(shows(&harness, INDICATOR));
        let ws = harness.state_mut().workspace();
        ws.plan_views[&ws.active_storey].preview.clone()
    };
    assert_eq!(preview.len(), 1, "{preview:?}");

    let before = element_count(&mut harness);
    click(&mut harness, p);
    assert_eq!(element_count(&mut harness), before + 1);
    {
        let ws = harness.state_mut().workspace();
        let slab = ws.doc.project.elements.values().last().unwrap().as_slab().unwrap();
        assert_eq!(
            slab.outline.to_points().0,
            preview[0].0,
            "the slab has the previewed ring"
        );
        assert!((slab.outline.outer().signed_area() - 97.08875).abs() < 1e-6);
    }

    let revision = harness.state_mut().workspace().doc.project.revision;
    harness.key_up(Key::Space);
    harness.run_steps(2);
    assert!(!detecting(&mut harness), "the release ends detection");
    assert_eq!(harness.state_mut().workspace().doc.project.revision, revision);
    assert_eq!(
        harness.state_mut().workspace().consumer().step().unwrap().expects,
        Expect::Point
    );

    harness.state_mut().workspace().tool.detection.toggle();
    harness.run_steps(2);
    assert!(detecting(&mut harness));
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    let ws = harness.state_mut().workspace();
    assert!(!ws.tool.detection.is_on(), "Esc ends detection");
    assert_eq!(ws.tool.kind, ToolKind::Slab, "and keeps the tool");
    assert_eq!(ws.doc.project.revision, revision, "neither changes the project");
}

/// INP-018, INP-003: Esc while Space is held ends detection, and the
/// repeats of the still held key keep it off; pressing Space again
/// switches it back on.
#[test]
fn a_held_key_repeating_after_esc_keeps_detection_off() {
    let mut harness = shaft_sample_with("contour-repeat", ToolKind::Wall);
    hover(&mut harness, DVec2::new(2.0, 3.0));
    harness.key_down(Key::Space);
    harness.run_steps(2);
    assert!(detecting(&mut harness), "Space held switches to detection");
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert!(!detecting(&mut harness), "Esc ends detection");
    for _ in 0..3 {
        harness.event(Event::Key {
            key: Key::Space,
            physical_key: None,
            pressed: true,
            repeat: true,
            modifiers: Modifiers::NONE,
        });
        harness.run_steps(1);
    }
    assert!(!detecting(&mut harness), "the repeats of the held key keep it off");
    harness.key_up(Key::Space);
    harness.run_steps(2);
    harness.key_down(Key::Space);
    harness.run_steps(2);
    assert!(detecting(&mut harness), "a new press switches it on again");
    harness.key_up(Key::Space);
    harness.run_steps(2);
    assert!(!detecting(&mut harness));
}

/// INP-018, INP-020: a double-click and a triple-click in detection pick
/// once: the slab tool creates one slab at (2, 3), as one undo step, in
/// both input modes.
#[test]
fn a_double_click_in_detection_creates_one_slab() {
    for (mode, clicks) in [
        (InputMode::ClickClick, 2),
        (InputMode::DragRelease, 2),
        (InputMode::ClickClick, 3),
        (InputMode::DragRelease, 3),
    ] {
        let mut harness = shaft_sample_with(&format!("contour-clicks-{clicks}-{mode:?}"), ToolKind::Slab);
        harness.state_mut().workspace().prefs.input_mode = mode;
        harness.state_mut().workspace().tool.detection.toggle();
        let p = DVec2::new(2.0, 3.0);
        hover(&mut harness, p);
        let pos = screen_of(&mut harness, p);
        let (before, undo) = {
            let ws = harness.state_mut().workspace();
            (ws.doc.project.elements.len(), ws.doc.history.undo_len())
        };
        // Each click presses and releases in one frame, so the next one, a
        // frame (0.25 s) later, is the second click of a double-click and
        // the one after that the third of a triple-click.
        for _ in 0..clicks {
            let press_release = [true, false].map(|pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
            harness.input_mut().events.extend(press_release);
            harness.run_steps(1);
        }
        let seen = harness.ctx.input(|i| match clicks {
            2 => i.pointer.button_double_clicked(PointerButton::Primary),
            _ => i.pointer.button_triple_clicked(PointerButton::Primary),
        });
        assert!(seen, "{mode:?}: egui saw {clicks} clicks in a row");
        harness.run_steps(2);
        let ws = harness.state_mut().workspace();
        let what = format!("{mode:?}, {clicks} clicks");
        assert_eq!(ws.doc.project.elements.len(), before + 1, "{what}: one slab");
        assert_eq!(ws.doc.history.undo_len(), undo + 1, "{what}: one undo step");
    }
}

/// INP-018: the contour graph is built once per storey and plan revision:
/// hovering for many frames builds it once, and after an edit the next
/// hover builds it again.
#[test]
fn the_contour_graph_is_built_once_per_revision() {
    let mut harness = shaft_sample_with("contour-cache", ToolKind::Wall);
    hover(&mut harness, DVec2::new(2.0, 3.0));
    assert_eq!(harness.state_mut().workspace().doc.contour_builds(), 0, "built lazily");
    harness.state_mut().workspace().tool.detection.toggle();
    for i in 0..10 {
        hover(&mut harness, DVec2::new(2.0 + 0.1 * f64::from(i), 3.0));
    }
    assert_eq!(harness.state_mut().workspace().doc.contour_builds(), 1);

    {
        let ws = harness.state_mut().workspace();
        let storey = ws.active_storey;
        let wall = ws
            .doc
            .project
            .new_wall(storey, DVec2::new(1.0, 1.0), DVec2::new(3.0, 1.0), 0);
        assert!(ws.execute(archicraft_model::Command::AddElements(vec![wall])));
    }
    harness.run_steps(2);
    hover(&mut harness, DVec2::new(2.0, 3.0));
    hover(&mut harness, DVec2::new(2.5, 3.0));
    assert_eq!(
        harness.state_mut().workspace().doc.contour_builds(),
        2,
        "rebuilt after the edit"
    );
}

/// INP-018: with Alt held, the wall tool in detection previews only the
/// clicked edge at (2, 0.25), the room's south face up to the T wall, and
/// the click creates exactly one wall along it.
#[test]
fn an_alt_click_in_the_plan_creates_one_wall_along_the_edge() {
    let mut harness = shaft_sample_with("contour-alt", ToolKind::Wall);
    harness.state_mut().workspace().tool.detection.toggle();
    let p = DVec2::new(2.0, 0.25);
    harness.event(Event::ModifiersChanged(Modifiers::ALT));
    hover(&mut harness, p);
    let preview = {
        let ws = harness.state_mut().workspace();
        ws.plan_views[&ws.active_storey].preview.clone()
    };
    let edge = [DVec2::new(0.25, 0.25), DVec2::new(4.9425, 0.25)];
    assert_eq!(preview.len(), 2, "the edge and its wall's outline: {preview:?}");
    assert!(!preview[0].1);
    assert_eq!(preview[0].0.len(), 2, "the edge only: {preview:?}");
    for (q, e) in preview[0].0.iter().zip(edge) {
        assert!(q.distance(e) < 1e-5, "{preview:?}");
    }

    let before = element_count(&mut harness);
    click_with(&mut harness, p, Modifiers::ALT);
    assert_eq!(element_count(&mut harness), before + 1, "exactly one wall");
    let ws = harness.state_mut().workspace();
    let wall = ws.doc.project.elements.values().last().unwrap().as_wall().unwrap();
    assert_eq!(vec![wall.start, wall.end], preview[0].0);
}

/// SPC-001, INP-018, NFR-USE-003: A selects the space tool, which detects
/// rooms at once. Hovering inside the ground-floor room previews the
/// boundary the storey's regions give at the cursor, and the click creates
/// that space; outside the building the status line says the region is
/// open and a click creates nothing.
#[test]
fn the_space_tool_detects_the_room_under_the_cursor_in_the_plan() {
    let mut harness = shaft_sample_with("space-detect", ToolKind::Select);
    harness.key_press(Key::A);
    harness.run_steps(2);
    assert_eq!(harness.state_mut().workspace().tool.kind, ToolKind::Space);
    assert!(detecting(&mut harness));

    // East of the building, inside the plan whatever the height of the bars
    // below it: the plan is wider than the building.
    let outside = DVec2::new(12.6, 4.5);
    hover(&mut harness, outside);
    hover(&mut harness, outside);
    let prompt = harness.state_mut().workspace().consumer().step().unwrap().prompt;
    assert!(
        harness.query_all_by_label_contains("Region is open").next().is_some(),
        "the status line says why: {prompt}"
    );
    let before = element_count(&mut harness);
    click(&mut harness, outside);
    assert_eq!(element_count(&mut harness), before, "nothing outside the building");

    let p = DVec2::new(2.0, 3.0);
    hover(&mut harness, p);
    hover(&mut harness, p);
    let (preview, storey) = {
        let ws = harness.state_mut().workspace();
        (ws.plan_views[&ws.active_storey].preview.clone(), ws.active_storey)
    };
    let room = archicraft_geometry::space_region(&harness.state_mut().workspace().doc.project, storey, p).unwrap();
    assert_eq!(preview, [(room.to_points().0, true)], "the room's boundary");
    assert!(harness.query_all_by_label_contains("Region is open").next().is_none());

    click(&mut harness, p);
    assert_eq!(element_count(&mut harness), before + 1);
    let ws = harness.state_mut().workspace();
    let space = ws.doc.project.elements.values().last().unwrap().as_space().unwrap();
    assert_eq!(space.boundary.to_points().0, preview[0].0, "the previewed boundary");
    let archicraft_model::BoundarySource::Detected { seed, auto_update } = space.source else {
        panic!("a detected boundary")
    };
    assert!(auto_update && seed.distance(p) < 0.05, "{seed}");
    assert_eq!(space.long_name, "Room 1");
}

/// INP-018, SPC-001: the space tool detects rooms in its Detect method
/// without the shared toggle, so the status bar shows the indicator only
/// while detection is toggled or held, like the Design menu's check, in
/// every method; a click on the shown indicator switches detection off.
#[test]
fn the_space_tool_shows_the_indicator_only_while_detection_is_on() {
    use archicraft_app::tools::SpaceMode;
    let mut harness = shaft_sample_with("space-indicator", ToolKind::Space);
    for mode in [SpaceMode::Detect, SpaceMode::Polygon, SpaceMode::Rectangle] {
        harness.state_mut().workspace().tool.space_mode = mode;
        harness.run_steps(2);
        assert!(!shows(&harness, INDICATOR), "{mode:?}: detection off");
        harness.state_mut().workspace().tool.detection.toggle();
        harness.run_steps(2);
        assert!(shows(&harness, INDICATOR), "{mode:?}: detection toggled on");
        harness.get_by_label(INDICATOR).click();
        harness.run_steps(2);
        assert!(
            !harness.state_mut().workspace().tool.detection.is_on(),
            "{mode:?}: the click switches it off"
        );
        assert!(!shows(&harness, INDICATOR), "{mode:?}: detection off again");
    }
    harness.key_down(Key::Space);
    harness.run_steps(2);
    assert!(shows(&harness, INDICATOR), "Space held");
    harness.key_up(Key::Space);
    harness.run_steps(2);
    assert!(!shows(&harness, INDICATOR), "Space released");
}

/// INP-018: with a wall point pending, switching detection on from the
/// Design menu checks the menu item and shows the status-bar indicator,
/// which agree while the point stays pending; a click on the indicator
/// switches detection off and unchecks the menu item.
#[test]
fn the_indicator_and_the_menu_check_agree_while_points_are_pending() {
    let mut harness = shaft_sample_with("contour-pending", ToolKind::Wall);
    click(&mut harness, DVec2::new(2.0, 3.0));
    assert_eq!(
        harness.state_mut().workspace().tool.points.len(),
        1,
        "a point is pending"
    );
    assert!(!menu_checked(&mut harness));
    assert!(!shows(&harness, INDICATOR));

    open_design_menu(&mut harness);
    harness.get_by_label("Contour detection").click();
    harness.run_steps(2);
    assert_eq!(harness.state_mut().workspace().tool.points.len(), 1, "still pending");
    assert!(shows(&harness, INDICATOR), "the indicator shows with the point pending");
    assert!(menu_checked(&mut harness), "as the menu checks it");
    hover(&mut harness, DVec2::new(3.0, 3.0));
    assert!(shows(&harness, INDICATOR));

    // The wall tool's long prompt pushes the indicator past the right edge
    // of the 1400-point window (archicraft-djjoe), so it is clicked
    // through AccessKit.
    harness.get_by_label(INDICATOR).click_accesskit();
    harness.run_steps(2);
    assert!(!shows(&harness, INDICATOR));
    assert!(
        !menu_checked(&mut harness),
        "the indicator's click unchecks the menu item"
    );
}

/// Shows only the first 3D view: opens it and removes every plan from the
/// tab tree.
fn show_only_3d(harness: &mut Harness<'static, ArchiCraftApp>) {
    let (view, storeys) = {
        let ws = harness.state_mut().workspace();
        let view = ws.doc.project.views.first().unwrap().id;
        ws.requests.push(Request::OpenPane(Pane::View3d(view)));
        (
            view,
            ws.doc
                .project
                .storeys
                .as_slice()
                .iter()
                .map(|s| s.id)
                .collect::<Vec<_>>(),
        )
    };
    harness.run_steps(3);
    let tree = harness.state_mut().tree_mut();
    for storey in storeys {
        if let Some(tile) = tree.tiles.find_pane(&Pane::Plan(storey)) {
            tree.remove_recursively(tile);
        }
    }
    harness.state_mut().workspace().active_pane = Some(Pane::View3d(view));
    harness.run_steps(2);
}

/// INP-018: Space held in the plan and released while only a 3D view is
/// shown ends held detection, although no plan view draws the frame.
#[test]
fn releasing_the_key_while_only_a_3d_view_is_shown_ends_held_detection() {
    let mut harness = shaft_sample_with("contour-release-3d", ToolKind::Slab);
    hover(&mut harness, DVec2::new(2.0, 3.0));
    harness.key_down(Key::Space);
    harness.run_steps(2);
    assert!(harness.state_mut().workspace().tool.detection.held, "Space held");
    show_only_3d(&mut harness);
    assert!(harness.state_mut().workspace().tool.detection.held, "still held");
    harness.key_up(Key::Space);
    harness.run_steps(2);
    let ws = harness.state_mut().workspace();
    assert!(ws.plan_views.keys().all(|s| ws.active_pane != Some(Pane::Plan(*s))));
    assert!(!ws.tool.detection.held, "the release ends it");
    assert!(!ws.tool.detection.is_on());
}

/// INP-018: each storey keeps its own contour graph: hovering the upper
/// storey's plan in detection builds one more, and returning to the ground
/// storey's plan builds none.
#[test]
fn each_storey_keeps_its_contour_graph() {
    let mut harness = shaft_sample_with("contour-storeys", ToolKind::Wall);
    harness.state_mut().workspace().tool.detection.toggle();
    hover(&mut harness, DVec2::new(2.0, 3.0));
    hover(&mut harness, DVec2::new(2.5, 3.0));
    assert_eq!(
        harness.state_mut().workspace().doc.contour_builds(),
        1,
        "the ground storey's"
    );

    let (ground, upper) = {
        let ws = harness.state_mut().workspace();
        let storeys = ws.doc.project.storeys.as_slice();
        (storeys[0].id, storeys[1].id)
    };
    harness
        .state_mut()
        .workspace()
        .requests
        .push(Request::OpenPane(Pane::Plan(upper)));
    harness.run_steps(3);
    assert_eq!(harness.state_mut().workspace().active_storey, upper);
    hover(&mut harness, DVec2::new(2.0, 3.0));
    hover(&mut harness, DVec2::new(2.5, 3.0));
    assert_eq!(
        harness.state_mut().workspace().doc.contour_builds(),
        2,
        "one more for the upper storey"
    );

    harness
        .state_mut()
        .workspace()
        .requests
        .push(Request::OpenPane(Pane::Plan(ground)));
    harness.run_steps(3);
    assert_eq!(harness.state_mut().workspace().active_storey, ground);
    hover(&mut harness, DVec2::new(2.0, 3.0));
    hover(&mut harness, DVec2::new(2.5, 3.0));
    assert_eq!(
        harness.state_mut().workspace().doc.contour_builds(),
        2,
        "none on returning to the ground storey"
    );
}
