//! ELM-015: element groups in the user interface. Clicking one member selects
//! its outermost group, in the plan and in a 3D view; Group and Ungroup run
//! from the toolbar, the menus, the context menus and their shortcuts; groups
//! can be suspended for the session.

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::HashSet;

use archicraft_app::ArchiCraftApp;
use archicraft_app::commands::{Binding, Origin};
use archicraft_app::views::view3d::pick;
use archicraft_app::workspace::{Pane, Request};
use archicraft_model::glam::DVec2;
use archicraft_model::{Command, ElementId};
use common::{assert_present, data_dir, harness};
use egui::accesskit::Role;
use egui::os::OperatingSystem;
use egui::{Event, Key, Modifiers, PointerButton};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, NodeT, Queryable};

type App = Harness<'static, ArchiCraftApp>;

/// Adds horizontal walls 4 m long at the heights `ys` on the ground storey,
/// one change, and returns their ids.
fn add_walls(harness: &mut App, ys: &[f64]) -> Vec<ElementId> {
    let ws = harness.state_mut().workspace();
    let storey = ws.doc.project.storeys.ground();
    let walls: Vec<_> = ys
        .iter()
        .enumerate()
        .map(|(n, &y)| {
            ws.doc
                .project
                .new_wall(storey, DVec2::new(0.0, y), DVec2::new(4.0, y), n as u32)
        })
        .collect();
    let ids = walls.iter().map(|w| w.id).collect();
    assert!(ws.execute(Command::AddElements(walls)));
    harness.run_steps(2);
    ids
}

/// The screen position of the plan point `p` in the active plan view.
fn plan_point(harness: &mut App, p: DVec2) -> egui::Pos2 {
    let ws = harness.state_mut().workspace();
    let state = &ws.plan_views[&ws.active_storey];
    state.camera.to_screen(state.last_rect, p)
}

/// The middle of the wall at height `y` in the active plan view.
fn wall_at(harness: &mut App, y: f64) -> egui::Pos2 {
    plan_point(harness, DVec2::new(2.0, y))
}

/// Clicks `button` at `pos` with `modifiers`.
fn click_with(harness: &mut App, pos: egui::Pos2, button: PointerButton, modifiers: Modifiers) {
    harness.event(Event::ModifiersChanged(modifiers));
    harness.hover_at(pos);
    harness.run_steps(1);
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers,
        });
    }
    harness.run_steps(2);
    harness.event(Event::ModifiersChanged(Modifiers::NONE));
}

fn click(harness: &mut App, pos: egui::Pos2) {
    click_with(harness, pos, PointerButton::Primary, Modifiers::NONE);
}

fn shift_click(harness: &mut App, pos: egui::Pos2) {
    click_with(harness, pos, PointerButton::Primary, Modifiers::SHIFT);
}

/// The selection as a set.
fn selection(harness: &mut App) -> HashSet<ElementId> {
    harness.state_mut().workspace().selection.clone()
}

fn set(ids: &[ElementId]) -> HashSet<ElementId> {
    ids.iter().copied().collect()
}

/// Selects exactly `ids`, as clicks with groups suspended would.
fn select(harness: &mut App, ids: &[ElementId]) {
    harness.state_mut().workspace().selection = set(ids);
    harness.run_steps(2);
}

/// The toolbar's button labelled `label`, not a menu item or tooltip.
fn toolbar_button<'h>(harness: &'h App, label: &'h str) -> egui_kittest::Node<'h> {
    harness.get(By::new().role(Role::Button).label(label))
}

fn click_toolbar(harness: &mut App, label: &str) {
    toolbar_button(harness, label).click();
    harness.run_steps(2);
}

/// Presses `binding` with the modifier keys of this platform.
fn press(harness: &mut App, binding: Binding) {
    let mut modifiers = binding.for_os(OperatingSystem::from_target_os()).modifiers;
    modifiers.command = binding.primary;
    for pressed in [true, false] {
        harness.event(Event::Key {
            key: binding.key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        });
    }
    harness.run_steps(2);
}

/// Opens the menu `title` of the menu bar.
fn open_menu(harness: &mut App, title: &str) {
    let menu = harness
        .query_all_by_label(title)
        .map(|n| n.rect())
        .min_by(|a, b| a.min.y.total_cmp(&b.min.y))
        .unwrap();
    click(harness, menu.center());
}

/// The number of groups of the project.
fn groups(harness: &mut App) -> usize {
    harness.state_mut().workspace().doc.project.groups.len()
}

/// Opens the first 3D view and returns a screen point of it whose ray hits
/// `element` first.
fn open_3d_at(harness: &mut App, element: ElementId) -> egui::Pos2 {
    let ws = harness.state_mut().workspace();
    let view = ws.doc.project.views.first().unwrap().id;
    ws.requests.push(Request::OpenPane(Pane::View3d(view)));
    harness.run_steps(3);
    let ws = harness.state_mut().workspace();
    assert_eq!(ws.active_pane, Some(Pane::View3d(view)));
    let geometry = ws.doc.geometry();
    let state = &ws.views_3d[&view];
    let rect = state.last_rect();
    for iy in 1..60 {
        for ix in 1..60 {
            let p = rect.min + egui::vec2(rect.width() * ix as f32 / 60.0, rect.height() * iy as f32 / 60.0);
            let (origin, dir) = state.camera.ray(rect, p);
            if pick(&ws.doc.project, &geometry, origin, dir).is_some_and(|(id, _)| id == element) {
                return p;
            }
        }
    }
    panic!("no point of the 3D view shows {element:?}");
}

/// ELM-015 (epic: a group ungroups): two walls grouped with the toolbar's
/// Group are selected together by a click on one of them, in the plan and in
/// a 3D view; after the toolbar's Ungroup a click selects one wall again.
#[test]
fn a_click_on_a_member_selects_the_group_until_it_is_ungrouped() {
    let mut harness = harness("groups-click");
    let [a, b] = add_walls(&mut harness, &[0.0, 1.5])[..] else {
        unreachable!()
    };
    let (at_a, at_b) = (wall_at(&mut harness, 0.0), wall_at(&mut harness, 1.5));
    let empty = plan_point(&mut harness, DVec2::new(2.0, 0.75));
    click(&mut harness, at_a);
    shift_click(&mut harness, at_b);
    assert_eq!(selection(&mut harness), set(&[a, b]));
    click_toolbar(&mut harness, "Group");
    assert_eq!(groups(&mut harness), 1);
    assert_eq!(selection(&mut harness), set(&[a, b]), "the new group is selected");

    click(&mut harness, empty);
    assert!(selection(&mut harness).is_empty());
    click(&mut harness, at_a);
    assert_eq!(selection(&mut harness), set(&[a, b]), "plan click selects the group");

    let in_3d = open_3d_at(&mut harness, b);
    select(&mut harness, &[]);
    click(&mut harness, in_3d);
    assert_eq!(selection(&mut harness), set(&[a, b]), "3D click selects the group");

    click_toolbar(&mut harness, "Ungroup");
    assert_eq!(groups(&mut harness), 0);
    select(&mut harness, &[]);
    click(&mut harness, in_3d);
    assert_eq!(selection(&mut harness), set(&[b]), "3D click selects one wall");
    harness.state_mut().workspace().active_pane = None;
    harness.get_by_label("Plan/3D").click();
    harness.run_steps(3);
    click(&mut harness, at_a);
    assert_eq!(selection(&mut harness), set(&[a]), "plan click selects one wall");
}

/// ELM-015: grouping a group with a third wall nests it, so a click on a
/// member of the inner group selects all three; one Ungroup removes the
/// outer level only, after which the click selects the inner group again.
#[test]
fn nested_groups_select_the_outermost_group_and_ungroup_one_level() {
    let mut harness = harness("groups-nested");
    let [a, b, c] = add_walls(&mut harness, &[0.0, 1.5, 3.0])[..] else {
        unreachable!()
    };
    let at_a = wall_at(&mut harness, 0.0);
    select(&mut harness, &[a, b]);
    click_toolbar(&mut harness, "Group");
    click(&mut harness, at_a);
    let at = wall_at(&mut harness, 3.0);
    shift_click(&mut harness, at);
    assert_eq!(selection(&mut harness), set(&[a, b, c]));
    click_toolbar(&mut harness, "Group");
    assert_eq!(groups(&mut harness), 2);

    select(&mut harness, &[]);
    click(&mut harness, at_a);
    assert_eq!(selection(&mut harness), set(&[a, b, c]));
    click_toolbar(&mut harness, "Ungroup");
    assert_eq!(groups(&mut harness), 1);
    select(&mut harness, &[]);
    click(&mut harness, at_a);
    assert_eq!(selection(&mut harness), set(&[a, b]));
}

/// ELM-015: with groups suspended from the Edit menu a click selects the
/// member alone and the status bar says that groups are suspended; turning
/// suspension off restores group selection. The setting belongs to the
/// session: it neither makes the project dirty nor reaches the file.
#[test]
fn suspended_groups_select_single_members_for_the_session() {
    let mut harness = harness("groups-suspend");
    let [a, b] = add_walls(&mut harness, &[0.0, 1.5])[..] else {
        unreachable!()
    };
    let at_a = wall_at(&mut harness, 0.0);
    select(&mut harness, &[a, b]);
    click_toolbar(&mut harness, "Group");
    let dir = data_dir("groups-suspend-file");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("suspend.ifcx");
    harness.state_mut().workspace().doc.path = Some(path.clone());
    click_toolbar(&mut harness, "Save");
    assert!(!harness.state_mut().workspace().doc.is_dirty());
    assert!(harness.query_all_by_label("Groups suspended").next().is_none());

    open_menu(&mut harness, "Edit");
    harness.get_by_label("Suspend groups").click();
    harness.run_steps(2);
    assert!(harness.state_mut().workspace().groups_suspended);
    assert_present(&harness, "Groups suspended");
    assert!(
        !harness.state_mut().workspace().doc.is_dirty(),
        "suspending is no change"
    );
    select(&mut harness, &[]);
    click(&mut harness, at_a);
    assert_eq!(selection(&mut harness), set(&[a]));
    click_toolbar(&mut harness, "Save");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.to_lowercase().contains("suspend"), "{text}");

    open_menu(&mut harness, "Edit");
    harness.get_by_label("Suspend groups").click();
    harness.run_steps(2);
    assert!(!harness.state_mut().workspace().groups_suspended);
    assert!(harness.query_all_by_label("Groups suspended").next().is_none());
    select(&mut harness, &[]);
    click(&mut harness, at_a);
    assert_eq!(selection(&mut harness), set(&[a, b]));
    std::fs::remove_dir_all(&dir).unwrap();
}

/// ELM-015, SEL-003: groups survive saving and reopening; Shift-click adds
/// and removes a whole group; an area selection that catches one member
/// selects its whole group.
#[test]
fn groups_survive_reopening_and_shift_and_area_selection_take_whole_groups() {
    let mut harness = harness("groups-reopen");
    let [a, b, c] = add_walls(&mut harness, &[0.0, 1.5, 3.0])[..] else {
        unreachable!()
    };
    select(&mut harness, &[a, b]);
    click_toolbar(&mut harness, "Group");
    let dir = data_dir("groups-reopen-file");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("groups.ifcx");
    harness.state_mut().workspace().doc.path = Some(path.clone());
    click_toolbar(&mut harness, "Save");
    harness.state_mut().open_path(&path);
    harness.run_steps(3);
    assert_eq!(groups(&mut harness), 1);
    assert!(selection(&mut harness).is_empty());

    let at = wall_at(&mut harness, 3.0);

    click(&mut harness, at);
    let at = wall_at(&mut harness, 0.0);
    shift_click(&mut harness, at);
    assert_eq!(selection(&mut harness), set(&[a, b, c]), "Shift adds the group");
    let at = wall_at(&mut harness, 1.5);
    shift_click(&mut harness, at);
    assert_eq!(selection(&mut harness), set(&[c]), "Shift removes the group");

    // Right to left selects what the rectangle touches: here only wall A,
    // whose reference line it crosses.
    select(&mut harness, &[]);
    let start = plan_point(&mut harness, DVec2::new(3.0, 0.7));
    let end = plan_point(&mut harness, DVec2::new(1.0, -0.1));
    harness.hover_at(start);
    harness.run_steps(1);
    let button = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    harness.event(button(start, true));
    harness.run_steps(1);
    for step in 1..=6 {
        harness.hover_at(start + (end - start) * step as f32 / 6.0);
        harness.run_steps(1);
    }
    harness.event(button(end, false));
    harness.run_steps(2);
    assert_eq!(selection(&mut harness), set(&[a, b]), "the area takes the whole group");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// ELM-015, UI-011: Cmd/Ctrl+G groups and Cmd/Ctrl+Shift+G ungroups; with a
/// single ungrouped wall selected both are greyed out and their tooltips give
/// the reason; the properties strip names the selected group with its size,
/// and renaming it there is one undo step.
#[test]
fn shortcuts_disabled_reasons_and_the_group_in_the_properties_strip() {
    let mut harness = harness("groups-keys");
    let [a, b] = add_walls(&mut harness, &[0.0, 1.5])[..] else {
        unreachable!()
    };
    select(&mut harness, &[a]);
    for (label, reason) in [
        ("Group", "Select at least two elements or groups to group."),
        ("Ungroup", "No selected element belongs to a group."),
    ] {
        assert!(
            toolbar_button(&harness, label).accesskit_node().is_disabled(),
            "{label}"
        );
        toolbar_button(&harness, label).hover();
        harness.run_steps(4);
        assert_present(&harness, reason);
    }

    select(&mut harness, &[a, b]);
    press(&mut harness, Binding::primary(Key::G));
    assert_eq!(groups(&mut harness), 1);
    assert_present(&harness, "Group: Group 1 (2 elements)");
    assert!(!toolbar_button(&harness, "Ungroup").accesskit_node().is_disabled());

    let undo_steps = harness.state_mut().workspace().doc.history.undo_len();
    harness.get(By::new().role(Role::TextInput).value("Group 1")).click();
    harness.run_steps(1);
    harness
        .get(By::new().role(Role::TextInput).value("Group 1"))
        .type_text("Core");
    harness.run_steps(1);
    harness.key_press(Key::Enter);
    harness.run_steps(3);
    let ws = harness.state_mut().workspace();
    assert_eq!(ws.doc.history.undo_len(), undo_steps + 1, "renaming is one step");
    let name = ws.doc.project.groups.values().next().unwrap().name.clone();
    assert_eq!(name, "Core");
    harness.run_steps(2);
    assert_present(&harness, "Group: Core (2 elements)");
    harness.state_mut().workspace().undo();
    harness.run_steps(2);
    assert_present(&harness, "Group: Group 1 (2 elements)");

    press(&mut harness, Binding::primary(Key::G).shift());
    assert_eq!(groups(&mut harness), 0);
    assert!(
        harness
            .query_all_by_label("Group: Group 1 (2 elements)")
            .next()
            .is_none()
    );
}

/// The item `label` of the open context menu, which shows the shortcut
/// after the label, unlike the toolbar's button.
fn menu_item(harness: &App, label: &str) -> egui::Rect {
    let mut items = harness
        .query_all(By::new().role(Role::Button).label_contains(label))
        .filter(|n| {
            n.accesskit_node()
                .label()
                .is_some_and(|l| l.starts_with(&format!("{label} ")))
        });
    let item = items.next().unwrap_or_else(|| panic!("no menu item {label}"));
    assert!(items.next().is_none(), "two menu items {label}");
    item.rect()
}

/// UI-005, ELM-015: the context menus of the plan and of a 3D view offer
/// Group and Ungroup and invoke them with `Origin::ContextMenu`; a right
/// click on a grouped wall selects its group first.
#[test]
fn the_context_menus_group_and_ungroup() {
    let mut harness = harness("groups-context");
    let [a, b] = add_walls(&mut harness, &[0.0, 1.5])[..] else {
        unreachable!()
    };
    let at_a = wall_at(&mut harness, 0.0);
    select(&mut harness, &[a, b]);
    click_with(&mut harness, at_a, PointerButton::Secondary, Modifiers::NONE);
    let item = menu_item(&harness, "Group");
    click(&mut harness, item.center());
    assert_eq!(groups(&mut harness), 1);

    select(&mut harness, &[]);
    click_with(&mut harness, at_a, PointerButton::Secondary, Modifiers::NONE);
    assert_eq!(
        selection(&mut harness),
        set(&[a, b]),
        "the right click selects the group"
    );
    harness.state_mut().record_invocations();
    let item = menu_item(&harness, "Ungroup");
    click(&mut harness, item.center());
    let invoked: Vec<_> = harness
        .state_mut()
        .take_invocations()
        .into_iter()
        .map(|i| (i.id.as_str(), i.origin))
        .collect();
    assert_eq!(invoked, [("edit.ungroup", Origin::ContextMenu)]);

    let in_3d = open_3d_at(&mut harness, a);
    click_with(&mut harness, in_3d, PointerButton::Secondary, Modifiers::NONE);
    for label in ["Group", "Ungroup"] {
        menu_item(&harness, label);
    }
}
