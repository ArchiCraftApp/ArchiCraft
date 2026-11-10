//! ELM-003: parameter pick-up and inject in the plan and 3D views. Alt-click
//! picks an element's settings up into the active tool's defaults,
//! Alt+Primary-click injects the tool's defaults into an element or the
//! selection; between kinds only the compatible settings go, and the status
//! line says what was skipped and why. A latched mode needs no modifiers;
//! the Edit menu and the toolbar latch it (EDT-020), and hovering an element
//! previews what a click would transfer.

#![allow(clippy::unwrap_used)]

mod common;

use archicraft_app::ArchiCraftApp;
use archicraft_app::tools::ToolKind;
use archicraft_app::views::transfer::TransferMode;
use archicraft_app::views::view3d::pick;
use archicraft_app::workspace::{Pane, Request, SceneKey};
use archicraft_model::glam::DVec2;
use archicraft_model::{Command, Element, ElementId, IfcClass, KindTag, LayerId, MaterialId};
use common::harness;
use egui::accesskit::{Role, Toggled};
use egui::os::OperatingSystem;
use egui::{CursorIcon, Event, Key, Modifiers, PointerButton};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, NodeT, Queryable};

type App = Harness<'static, ArchiCraftApp>;

/// Alt (Option on macOS), the pick-up gesture.
const ALT: Modifiers = Modifiers::ALT;

/// Alt+Primary, the inject gesture, with this platform's Primary key.
fn alt_primary() -> Modifiers {
    let mut m = Modifiers::ALT | Modifiers::COMMAND;
    if OperatingSystem::from_target_os().is_mac() {
        m.mac_cmd = true;
    } else {
        m.ctrl = true;
    }
    m
}

/// A 4 m wall from (0, `y`) to (4, `y`) on the ground storey.
fn wall(harness: &mut App, y: f64, nth: u32) -> Element {
    let p = &harness.state_mut().workspace().doc.project;
    p.new_wall(p.storeys.ground(), DVec2::new(0.0, y), DVec2::new(4.0, y), nth)
}

/// A 2 m square slab from (6, 0) on the ground storey.
fn slab(harness: &mut App) -> Element {
    let p = &harness.state_mut().workspace().doc.project;
    let outline = vec![
        DVec2::new(6.0, 0.0),
        DVec2::new(8.0, 0.0),
        DVec2::new(8.0, 2.0),
        DVec2::new(6.0, 2.0),
    ];
    p.new_slab(p.storeys.ground(), outline, Vec::new(), 0)
}

/// Adds `elements` in one change and returns their ids.
fn add(harness: &mut App, elements: Vec<Element>) -> Vec<ElementId> {
    let ids = elements.iter().map(|e| e.id).collect();
    assert!(harness.state_mut().workspace().execute(Command::AddElements(elements)));
    harness.run_steps(2);
    ids
}

/// A layer other than the wall defaults' own, and a material other than
/// the wall and the slab defaults' own, so that a transfer of either shows.
fn other_layer_and_material(harness: &mut App) -> (LayerId, MaterialId) {
    let p = &harness.state_mut().workspace().doc.project;
    let defaults = p.new_element(KindTag::Wall, p.storeys.ground(), 0).unwrap();
    let slab = p.new_element(KindTag::Slab, p.storeys.ground(), 0).unwrap();
    let layer = p.layers.iter().find(|l| l.id != defaults.layer).unwrap().id;
    let taken = [defaults.as_wall().unwrap().material, slab.as_slab().unwrap().material];
    let material = p.materials.iter().find(|m| !taken.contains(&m.id)).unwrap().id;
    (layer, material)
}

/// A 0.365 m wall at `y` on another layer and of another material than the
/// wall defaults.
fn thick_wall(harness: &mut App, y: f64) -> Element {
    let (layer, material) = other_layer_and_material(harness);
    let mut e = wall(harness, y, 0);
    e.layer = layer;
    let w = e.as_wall_mut().unwrap();
    w.thickness = 0.365;
    w.material = material;
    e
}

/// The screen position of the plan point `p` in the active plan view.
fn plan_point(harness: &mut App, p: DVec2) -> egui::Pos2 {
    let ws = harness.state_mut().workspace();
    let state = &ws.plan_views[&ws.active_storey];
    state.camera.to_screen(state.last_rect, p)
}

/// Clicks the primary button at `pos` with `modifiers` held, then releases
/// them.
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
    harness.run_steps(1);
}

fn select_tool(harness: &mut App, tool: ToolKind) {
    harness.state_mut().workspace().select_tool(tool);
    harness.run_steps(2);
}

fn element(harness: &mut App, id: ElementId) -> Element {
    harness.state_mut().workspace().doc.project.element(id).unwrap().clone()
}

/// The wall tool's defaults as an element.
fn wall_defaults(harness: &mut App) -> Element {
    let p = &harness.state_mut().workspace().doc.project;
    p.new_element(KindTag::Wall, p.storeys.ground(), 0).unwrap()
}

fn status(harness: &mut App) -> String {
    let ws = harness.state_mut().workspace();
    ws.status.as_ref().map(|s| s.text.clone()).unwrap_or_default()
}

/// ELM-003 (epic: pick-up and inject between walls): with the wall tool, an
/// Alt-click on a 0.365 m wall makes its thickness, material and layer the
/// wall defaults, and an Alt+Primary-click on another wall injects them;
/// both undo in one step each.
#[test]
fn alt_click_picks_up_a_wall_and_alt_primary_click_injects_it_into_another() {
    let mut harness = harness("transfer-walls");
    let source = thick_wall(&mut harness, 0.0);
    let (layer, material) = (source.layer, source.as_wall().unwrap().material);
    let target = wall(&mut harness, 3.0, 1);
    let target_thickness = target.as_wall().unwrap().thickness;
    assert_ne!(target_thickness, 0.365);
    let ids = add(&mut harness, vec![source, target]);
    select_tool(&mut harness, ToolKind::Wall);

    let on_source = plan_point(&mut harness, DVec2::new(2.0, 0.0));
    click_with(&mut harness, on_source, ALT);
    let defaults = wall_defaults(&mut harness);
    assert_eq!(defaults.as_wall().unwrap().thickness, 0.365);
    assert_eq!(defaults.as_wall().unwrap().material, material);
    assert_eq!(defaults.layer, layer);
    let name = element(&mut harness, ids[0]).name;
    let text = status(&mut harness);
    assert!(
        text.starts_with("Picked up ") && text.ends_with(&format!(" settings from {name}")),
        "{text}"
    );
    assert_eq!(harness.state_mut().workspace().tool.kind, ToolKind::Wall);
    assert!(
        harness.state_mut().workspace().selection.is_empty(),
        "the click selected nothing"
    );

    let on_target = plan_point(&mut harness, DVec2::new(2.0, 3.0));
    click_with(&mut harness, on_target, alt_primary());
    let injected = element(&mut harness, ids[1]);
    assert_eq!(injected.as_wall().unwrap().thickness, 0.365);
    assert_eq!(injected.as_wall().unwrap().material, material);
    assert_eq!(injected.layer, layer);
    let text = status(&mut harness);
    assert!(text.starts_with("Injected ") && text.contains(&injected.name), "{text}");
    assert_eq!(harness.state_mut().workspace().tool.kind, ToolKind::Wall);

    harness.state_mut().workspace().undo();
    assert_eq!(
        element(&mut harness, ids[1]).as_wall().unwrap().thickness,
        target_thickness
    );
    assert_ne!(element(&mut harness, ids[1]).layer, layer, "undo restores the target");
    harness.state_mut().workspace().undo();
    assert_ne!(wall_defaults(&mut harness).layer, layer, "undo restores the defaults");
}

/// ELM-003 (epic: only compatible settings transfer): the wall defaults
/// injected into a slab give it their thickness, material, layer and
/// position but not the IFC class, and the status line names the IFC class
/// as skipped.
#[test]
fn injecting_wall_defaults_into_a_slab_transfers_only_the_compatible_settings() {
    let mut harness = harness("transfer-slab");
    let source = thick_wall(&mut harness, 0.0);
    let s = slab(&mut harness);
    let ids = add(&mut harness, vec![source, s]);
    select_tool(&mut harness, ToolKind::Wall);
    let on_source = plan_point(&mut harness, DVec2::new(2.0, 0.0));
    click_with(&mut harness, on_source, ALT);
    let defaults = wall_defaults(&mut harness);
    let before = element(&mut harness, ids[1]);
    assert_ne!(before.as_slab().unwrap().material, defaults.as_wall().unwrap().material);
    assert_ne!(before.as_slab().unwrap().thickness, 0.365);
    assert_ne!(before.layer, defaults.layer);

    // The slab lies below the cut plane: it is picked at its outline.
    let on_slab = plan_point(&mut harness, DVec2::new(8.0, 1.0));
    click_with(&mut harness, on_slab, alt_primary());
    let after = element(&mut harness, ids[1]);
    let slab = after.as_slab().unwrap();
    assert_eq!(slab.thickness, 0.365);
    assert_eq!(slab.material, defaults.as_wall().unwrap().material);
    assert_eq!(after.layer, defaults.layer);
    assert_eq!(after.position, defaults.position);
    assert_eq!(after.ifc_class, before.ifc_class);
    assert_ne!(after.ifc_class, IfcClass::Wall);
    assert_eq!(slab.outline, before.as_slab().unwrap().outline, "the geometry stays");
    let text = status(&mut harness);
    assert!(
        text.starts_with("Injected ") && text.contains(&format!("settings into {}", after.name)),
        "{text}"
    );
    assert!(text.contains("IFC class not allowed"), "{text}");
}

/// ELM-003: a pick-up while the wall tool has a chain in progress keeps the
/// chain's points; the transfer state sits beside the tool.
#[test]
fn a_pick_up_keeps_the_input_in_progress() {
    let mut harness = harness("transfer-chain");
    let source = thick_wall(&mut harness, 0.0);
    add(&mut harness, vec![source]);
    select_tool(&mut harness, ToolKind::Wall);
    let start = plan_point(&mut harness, DVec2::new(1.0, 5.0));
    click_with(&mut harness, start, Modifiers::NONE);
    assert_eq!(harness.state_mut().workspace().tool.points.len(), 1);
    let on_source = plan_point(&mut harness, DVec2::new(2.0, 0.0));
    click_with(&mut harness, on_source, ALT);
    assert_eq!(wall_defaults(&mut harness).as_wall().unwrap().thickness, 0.365);
    assert_eq!(harness.state_mut().workspace().tool.points.len(), 1, "the chain stays");
}

/// ELM-003: inject into a selected element goes into every element of the
/// selection, and a locked element alone is refused with the lock's reason
/// (ELM-013).
#[test]
fn inject_into_the_selection_and_a_locked_element() {
    let mut harness = harness("transfer-selection");
    let source = thick_wall(&mut harness, 0.0);
    let a = wall(&mut harness, 3.0, 1);
    let b = wall(&mut harness, 6.0, 2);
    let mut locked = wall(&mut harness, 9.0, 3);
    locked.locked = true;
    let ids = add(&mut harness, vec![source, a, b, locked]);
    select_tool(&mut harness, ToolKind::Wall);
    let on_source = plan_point(&mut harness, DVec2::new(2.0, 0.0));
    click_with(&mut harness, on_source, ALT);

    harness.state_mut().workspace().selection = [ids[1], ids[2], ids[3]].into_iter().collect();
    harness.run_steps(2);
    let on_a = plan_point(&mut harness, DVec2::new(2.0, 3.0));
    click_with(&mut harness, on_a, alt_primary());
    for id in [ids[1], ids[2]] {
        assert_eq!(element(&mut harness, id).as_wall().unwrap().thickness, 0.365);
    }
    assert_ne!(element(&mut harness, ids[3]).as_wall().unwrap().thickness, 0.365);
    let text = status(&mut harness);
    assert!(
        text.contains("2 elements") && text.contains("1 locked element was skipped"),
        "{text}"
    );

    harness.state_mut().workspace().selection.clear();
    harness.run_steps(2);
    let on_locked = plan_point(&mut harness, DVec2::new(2.0, 9.0));
    click_with(&mut harness, on_locked, alt_primary());
    assert_ne!(element(&mut harness, ids[3]).as_wall().unwrap().thickness, 0.365);
    let ws = harness.state_mut().workspace();
    let status = ws.status.as_ref().unwrap();
    assert!(status.error && status.text.contains("locked"), "{}", status.text);
}

/// ELM-003: in a 3D view, an Alt-click with the selection tool picks the
/// wall up into the wall defaults through the handler the plan uses and
/// activates the wall tool.
#[test]
fn alt_click_in_a_3d_view_picks_up_into_the_clicked_kind() {
    let mut harness = harness("transfer-3d");
    let source = thick_wall(&mut harness, 0.0);
    let ids = add(&mut harness, vec![source]);
    let on_source = open_3d_at(&mut harness, ids[0]);
    click_with(&mut harness, on_source, ALT);
    assert_eq!(wall_defaults(&mut harness).as_wall().unwrap().thickness, 0.365);
    assert_eq!(harness.state_mut().workspace().tool.kind, ToolKind::Wall);
    assert!(harness.state_mut().workspace().selection.is_empty());

    let target = wall(&mut harness, 3.0, 1);
    let target = add(&mut harness, vec![target])[0];
    let on_target = open_3d_at(&mut harness, target);
    click_with(&mut harness, on_target, alt_primary());
    assert_eq!(element(&mut harness, target).as_wall().unwrap().thickness, 0.365);
}

/// ELM-003: a latched pick-up needs no modifiers and stays after its clicks
/// until Esc; then a click selects again.
#[test]
fn a_latched_mode_works_without_modifiers_until_esc() {
    let mut harness = harness("transfer-latch");
    let source = thick_wall(&mut harness, 0.0);
    let other = wall(&mut harness, 3.0, 1);
    let ids = add(&mut harness, vec![source, other]);
    select_tool(&mut harness, ToolKind::Wall);
    harness.state_mut().workspace().transfer.latch(TransferMode::PickUp);
    for _ in 0..2 {
        let on_source = plan_point(&mut harness, DVec2::new(2.0, 0.0));
        click_with(&mut harness, on_source, Modifiers::NONE);
        assert_eq!(wall_defaults(&mut harness).as_wall().unwrap().thickness, 0.365);
        assert_eq!(
            harness.state_mut().workspace().transfer.latched(),
            Some(TransferMode::PickUp)
        );
    }
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert_eq!(harness.state_mut().workspace().transfer.latched(), None);
    assert_eq!(
        harness.state_mut().workspace().tool.kind,
        ToolKind::Wall,
        "Esc only ends the latch"
    );

    select_tool(&mut harness, ToolKind::Select);
    let on_other = plan_point(&mut harness, DVec2::new(2.0, 3.0));
    click_with(&mut harness, on_other, Modifiers::NONE);
    assert!(harness.state_mut().workspace().selection.contains(&ids[1]));
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

/// The cursor icon of the last frame.
fn cursor(harness: &App) -> CursorIcon {
    harness.output().platform_output.cursor_icon
}

/// Holds `modifiers` with the pointer at `pos` for a few frames.
fn hover_holding(harness: &mut App, pos: egui::Pos2, modifiers: Modifiers) {
    harness.event(Event::ModifiersChanged(modifiers));
    harness.hover_at(pos);
    harness.run_steps(2);
}

/// ELM-003: holding a gesture over the plan shows its pointer, the
/// crosshair of the eyedropper for pick-up and the copy pointer for inject,
/// and releasing it returns to the selection tool's own.
#[test]
fn a_held_gesture_shows_its_pointer() {
    let mut harness = harness("transfer-cursor");
    let source = thick_wall(&mut harness, 0.0);
    add(&mut harness, vec![source]);
    let over = plan_point(&mut harness, DVec2::new(2.0, 0.0));
    hover_holding(&mut harness, over, Modifiers::NONE);
    let plain = cursor(&harness);
    assert!(!matches!(plain, CursorIcon::Crosshair | CursorIcon::Copy), "{plain:?}");
    hover_holding(&mut harness, over, ALT);
    assert_eq!(cursor(&harness), CursorIcon::Crosshair);
    hover_holding(&mut harness, over, alt_primary());
    assert_eq!(cursor(&harness), CursorIcon::Copy);
    hover_holding(&mut harness, over, Modifiers::NONE);
    assert_eq!(cursor(&harness), plain);
}

/// The labels of the toolbar's Pick up and Inject buttons.
const PICK_UP: &str = "Pick up parameters";
const INJECT: &str = "Inject parameters";

/// The toolbar's button labelled `label`, not a menu item or tooltip.
fn toolbar_button<'h>(harness: &'h App, label: &'h str) -> egui_kittest::Node<'h> {
    harness.get(By::new().role(Role::Button).label(label))
}

/// Whether the toolbar's button labelled `label` shows as selected.
fn toolbar_selected(harness: &App, label: &str) -> bool {
    toolbar_button(harness, label).accesskit_node().toggled() == Some(Toggled::True)
}

fn click_toolbar(harness: &mut App, label: &str) {
    toolbar_button(harness, label).click();
    harness.run_steps(2);
}

fn latched(harness: &mut App) -> Option<TransferMode> {
    harness.state_mut().workspace().transfer.latched()
}

/// Presses Esc with the pointer over `pos`, as a user in that view would.
fn escape_at(harness: &mut App, pos: egui::Pos2) {
    harness.hover_at(pos);
    harness.run_steps(1);
    harness.key_press(Key::Escape);
    harness.run_steps(2);
}

/// Whether a widget of the window has a label containing `text`.
fn shows(harness: &App, text: &str) -> bool {
    harness.query_all(By::new().label_contains(text)).next().is_some()
}

/// The gesture text a tooltip or menu gives for `modifiers`, as egui
/// writes them on this platform.
fn gesture_text(harness: &App, modifiers: Modifiers) -> String {
    format!("{}-click", harness.ctx.format_modifiers(modifiers))
}

/// UI-011, ELM-003: the toolbar shows Pick up and Inject right after Undo
/// and Redo, before Measure, Group and Ungroup.
#[test]
fn the_toolbar_shows_pick_up_and_inject_right_after_redo() {
    let harness = harness("transfer-toolbar-order");
    let order = ["Undo", "Redo", PICK_UP, INJECT, "Measure tool", "Group", "Ungroup"];
    let lefts: Vec<f32> = order
        .iter()
        .map(|label| toolbar_button(&harness, label).rect().left())
        .collect();
    assert!(lefts.is_sorted(), "{order:?}: {lefts:?}");
    let top = toolbar_button(&harness, "Redo").rect().top();
    for label in [PICK_UP, INJECT] {
        assert_eq!(
            toolbar_button(&harness, label).rect().top(),
            top,
            "{label} is in the toolbar"
        );
    }
}

/// UI-011, ELM-003 (EDT-020): the tooltips of the toolbar's Pick up and
/// Inject show their label, their gesture as shortcut and their
/// description.
#[test]
fn the_toolbar_tooltips_of_pick_up_and_inject_show_label_gesture_and_description() {
    let mut harness = harness("transfer-toolbar-tooltip");
    for (label, gesture, description) in [
        (PICK_UP, ALT, "Pick up the settings of the elements you click"),
        (
            INJECT,
            alt_primary(),
            "Inject the active tool's settings into the elements you click",
        ),
    ] {
        toolbar_button(&harness, label).hover();
        harness.run_steps(4);
        let gesture = gesture_text(&harness, gesture);
        assert!(shows(&harness, &gesture), "{label}: {gesture} is missing");
        assert!(shows(&harness, description), "{label}: {description} is missing");
        assert!(
            harness.query_all_by_label(label).count() >= 2,
            "{label}: the tooltip's title"
        );
    }
}

/// ELM-003 (Showcase 3, step 3): a click on the toolbar's Pick up latches
/// pick-up and selects the button; a click on a wall without modifiers then
/// picks up its settings, and Esc ends the mode and clears the button. The
/// same for Inject, whose click goes into another wall; it stays latched
/// for a second wall, and undo takes the injection back.
#[test]
fn the_toolbar_latches_pick_up_and_inject_until_esc() {
    let mut harness = harness("transfer-toolbar-latch");
    let source = thick_wall(&mut harness, 0.0);
    let a = wall(&mut harness, 3.0, 1);
    let b = wall(&mut harness, 6.0, 2);
    let before = a.as_wall().unwrap().thickness;
    let ids = add(&mut harness, vec![source, a, b]);
    select_tool(&mut harness, ToolKind::Wall);
    assert!(!toolbar_selected(&harness, PICK_UP));

    click_toolbar(&mut harness, PICK_UP);
    assert_eq!(latched(&mut harness), Some(TransferMode::PickUp));
    assert!(toolbar_selected(&harness, PICK_UP));
    assert!(!toolbar_selected(&harness, INJECT));
    let on_source = plan_point(&mut harness, DVec2::new(2.0, 0.0));
    click_with(&mut harness, on_source, Modifiers::NONE);
    assert_eq!(wall_defaults(&mut harness).as_wall().unwrap().thickness, 0.365);
    assert!(harness.state_mut().workspace().selection.is_empty());
    assert_eq!(latched(&mut harness), Some(TransferMode::PickUp));
    escape_at(&mut harness, on_source);
    assert_eq!(latched(&mut harness), None);
    assert!(!toolbar_selected(&harness, PICK_UP));

    click_toolbar(&mut harness, INJECT);
    assert_eq!(latched(&mut harness), Some(TransferMode::Inject));
    assert!(toolbar_selected(&harness, INJECT));
    for (id, y) in [(ids[1], 3.0), (ids[2], 6.0)] {
        let on_target = plan_point(&mut harness, DVec2::new(2.0, y));
        click_with(&mut harness, on_target, Modifiers::NONE);
        assert_eq!(element(&mut harness, id).as_wall().unwrap().thickness, 0.365);
        assert!(toolbar_selected(&harness, INJECT), "the latch stays");
    }
    let on_target = plan_point(&mut harness, DVec2::new(2.0, 6.0));
    escape_at(&mut harness, on_target);
    assert_eq!(latched(&mut harness), None);
    assert!(!toolbar_selected(&harness, INJECT));
    assert_eq!(harness.state_mut().workspace().tool.kind, ToolKind::Wall);

    harness.state_mut().workspace().undo();
    assert_ne!(element(&mut harness, ids[2]).as_wall().unwrap().thickness, 0.365);
    assert_eq!(element(&mut harness, ids[1]).as_wall().unwrap().thickness, 0.365);
    harness.state_mut().workspace().undo();
    assert_eq!(element(&mut harness, ids[1]).as_wall().unwrap().thickness, before);
}

/// ELM-003: a second click on the latched mode's button ends it, a click on
/// the other mode's button switches to it.
#[test]
fn a_second_invocation_ends_the_latch() {
    let mut harness = harness("transfer-toolbar-toggle");
    click_toolbar(&mut harness, PICK_UP);
    click_toolbar(&mut harness, INJECT);
    assert_eq!(latched(&mut harness), Some(TransferMode::Inject));
    assert!(!toolbar_selected(&harness, PICK_UP));
    click_toolbar(&mut harness, INJECT);
    assert_eq!(latched(&mut harness), None);
}

/// ELM-003, EDT-020: Pick up parameters from the Edit menu latches pick-up
/// without modifiers, shows a check mark there and its gesture as shortcut;
/// hovering a wall shows what would transfer before the click; Esc ends it.
/// Inject parameters from the menu, hovering a slab with the wall tool,
/// lists the settings that would transfer and the IFC class as skipped,
/// with the reason.
#[test]
fn the_edit_menu_latches_and_the_hover_tooltip_lists_transferred_and_skipped_settings() {
    let mut harness = harness("transfer-menu-preview");
    let source = thick_wall(&mut harness, 0.0);
    let s = slab(&mut harness);
    let ids = add(&mut harness, vec![source, s]);
    select_tool(&mut harness, ToolKind::Wall);
    let source_name = element(&mut harness, ids[0]).name;
    let slab_name = element(&mut harness, ids[1]).name;

    open_menu(&mut harness, "Edit");
    let gesture = gesture_text(&harness, ALT);
    assert!(shows(&harness, &gesture), "the menu shows the gesture {gesture}");
    menu_item(&harness, PICK_UP).click();
    harness.run_steps(2);
    assert_eq!(latched(&mut harness), Some(TransferMode::PickUp));
    open_menu(&mut harness, "Edit");
    assert_eq!(
        menu_item(&harness, PICK_UP).accesskit_node().toggled(),
        Some(Toggled::True)
    );
    open_menu(&mut harness, "Edit");
    assert_eq!(
        latched(&mut harness),
        Some(TransferMode::PickUp),
        "closing the menu keeps it"
    );

    let on_source = plan_point(&mut harness, DVec2::new(2.0, 0.0));
    hover_holding(&mut harness, on_source, Modifiers::NONE);
    harness.run_steps(4);
    assert_eq!(
        harness.state_mut().workspace().hover,
        Some(ids[0]),
        "the candidate is highlighted"
    );
    assert!(shows(&harness, &format!("Pick up from {source_name}")));
    assert!(shows(&harness, "Transfers: ") && shows(&harness, "Thickness"));
    assert_ne!(
        wall_defaults(&mut harness).as_wall().unwrap().thickness,
        0.365,
        "only a preview"
    );
    click_with(&mut harness, on_source, Modifiers::NONE);
    assert_eq!(wall_defaults(&mut harness).as_wall().unwrap().thickness, 0.365);
    escape_at(&mut harness, on_source);
    assert_eq!(latched(&mut harness), None);

    open_menu(&mut harness, "Edit");
    menu_item(&harness, INJECT).click();
    harness.run_steps(2);
    assert_eq!(latched(&mut harness), Some(TransferMode::Inject));
    let on_slab = plan_point(&mut harness, DVec2::new(8.0, 1.0));
    hover_holding(&mut harness, on_slab, Modifiers::NONE);
    harness.run_steps(4);
    assert!(shows(&harness, &format!("Inject into {slab_name}")));
    assert!(shows(&harness, "Thickness"));
    assert!(shows(&harness, "Skips: ") && shows(&harness, "IFC class not allowed"));
    assert_ne!(
        element(&mut harness, ids[1]).as_slab().unwrap().thickness,
        0.365,
        "only a preview"
    );
}

/// ELM-003, EDT-020: a held gesture shows the same preview as the latched
/// mode.
#[test]
fn a_held_gesture_shows_the_preview() {
    let mut harness = harness("transfer-gesture-preview");
    let source = thick_wall(&mut harness, 0.0);
    let ids = add(&mut harness, vec![source]);
    let name = element(&mut harness, ids[0]).name;
    let over = plan_point(&mut harness, DVec2::new(2.0, 0.0));
    hover_holding(&mut harness, over, ALT);
    harness.run_steps(4);
    assert!(shows(&harness, &format!("Pick up from {name}")));
    hover_holding(&mut harness, over, Modifiers::NONE);
    harness.run_steps(4);
    assert!(!shows(&harness, &format!("Pick up from {name}")));
}

/// ELM-003: in a 3D view, a latched pick-up works without modifiers,
/// highlights and previews the candidate, and Esc ends it there too.
#[test]
fn a_latched_mode_works_in_a_3d_view_and_ends_with_esc_there() {
    let mut harness = harness("transfer-3d-latch");
    let source = thick_wall(&mut harness, 0.0);
    let ids = add(&mut harness, vec![source]);
    let name = element(&mut harness, ids[0]).name;
    let on_source = open_3d_at(&mut harness, ids[0]);
    click_toolbar(&mut harness, PICK_UP);
    hover_holding(&mut harness, on_source, Modifiers::NONE);
    harness.run_steps(4);
    assert_eq!(SceneKey::of(harness.state_mut().workspace()).hover, Some(ids[0]));
    assert!(shows(&harness, &format!("Pick up from {name}")));
    click_with(&mut harness, on_source, Modifiers::NONE);
    assert_eq!(wall_defaults(&mut harness).as_wall().unwrap().thickness, 0.365);
    assert!(harness.state_mut().workspace().selection.is_empty());
    escape_at(&mut harness, on_source);
    assert_eq!(latched(&mut harness), None);
    assert!(!toolbar_selected(&harness, PICK_UP));
    hover_holding(&mut harness, on_source, Modifiers::NONE);
    assert_eq!(SceneKey::of(harness.state_mut().workspace()).hover, None);
}

/// The Edit menu's item of `label`, a check box (the toolbar's button has
/// the same label).
fn menu_item<'h>(harness: &'h App, label: &'h str) -> egui_kittest::Node<'h> {
    harness.get(By::new().role(Role::CheckBox).label_contains(label))
}

/// Opens the main menu `title` by clicking its button in the menu bar.
fn open_menu(harness: &mut App, title: &str) {
    let menu = harness
        .query_all_by_label(title)
        .map(|n| n.rect())
        .min_by(|a, b| a.min.y.total_cmp(&b.min.y))
        .unwrap();
    harness.hover_at(menu.center());
    harness.run_steps(1);
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: menu.center(),
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run_steps(2);
}
