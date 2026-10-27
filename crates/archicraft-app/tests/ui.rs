//! UI tests without a GPU: widgets are found by their accessible labels
//! (AccessKit), as the technology choices plan for egui_kittest.

#![allow(clippy::unwrap_used)]

mod common;

use std::rc::Rc;

use archicraft_app::commands::{Binding, Origin, Registry, Target, view_keys};
use archicraft_app::i18n::{self, Localizer};
use archicraft_app::input::{
    ConstraintReference, DivisionMode, DivisionScope, Field, GuideOrigin, InputConsumer, InputMode, SnapKind,
};
use archicraft_app::persistence::DataDirs;
use archicraft_app::persistence::prefs::Preferences;
use archicraft_app::tools::{ToolKind, WallMode};
use archicraft_app::ui::icons;
use archicraft_app::workspace::{Consumer, Pane};
use archicraft_app::{ArchiCraftApp, tr};
use archicraft_model::glam::DVec2;
use archicraft_model::units::{AngleUnit, LengthUnit, ParseError, format_length};
use archicraft_model::validate::{MAX_THICKNESS, MIN_THICKNESS};
use archicraft_model::{
    Command, ElementId, ElementKind, IfcClass, KindTag, PredefinedType, ProjectSettings, ReferenceLine,
};
use common::{assert_present, data_dir, harness, harness_at, harness_in};
use egui::accesskit::Role;
use egui::os::OperatingSystem;
use egui::{Event, Key, Modifiers, PointerButton};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, NodeT, Queryable};

#[test]
fn main_window_has_the_parts_of_ui_001() {
    let harness = harness("parts");
    // Menu bar, tool palette, project browser, view tabs and status line.
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
        assert_present(&harness, label);
    }
}

#[test]
fn tool_buttons_and_properties_follow_each_other() {
    let mut harness = harness("tools");
    harness.get_by_label("Wall").click();
    harness.run_steps(2);
    assert_eq!(harness.state_mut().workspace().tool.kind, ToolKind::Wall);
    harness.get_by_label("Wall tool defaults");
    harness.get_by_label("Chain");
    harness.get_by_label("Slab").click();
    harness.run_steps(2);
    assert_eq!(harness.state_mut().workspace().tool.kind, ToolKind::Slab);
    harness.get_by_label("Slab tool defaults");
}

/// ELM-001: with the wall tool active and nothing selected, a thickness
/// typed into the properties strip becomes the thickness of the wall's
/// tool defaults slot, undoably; the slab slot stays as it was.
#[test]
fn a_thickness_in_the_strip_updates_the_wall_defaults_slot() {
    let mut harness = harness("defaults-thickness");
    harness.get_by_label("Wall").click();
    harness.run_steps(2);
    let (before, shown, typed) = {
        let ws = harness.state_mut().workspace();
        let before = ws.doc.project.tool_defaults.clone();
        let ElementKind::Wall(wall) = &before.get(KindTag::Wall).unwrap().kind else {
            panic!("the wall slot holds a wall");
        };
        let thickness = wall.thickness;
        let units = ws.doc.project.settings.units.clone();
        let nf = ws.prefs.number_format();
        (
            before,
            format_length(thickness, &units, &nf, false),
            format_length(0.3, &units, &nf, false),
        )
    };
    let field = || By::new().role(Role::TextInput).value(shown.as_str());
    harness.get(field()).click();
    harness.run_steps(1);
    harness.get(field()).type_text(&typed);
    harness.key_press(Key::Enter);
    harness.run_steps(2);
    let ws = harness.state_mut().workspace();
    let slot = ws.doc.project.tool_defaults.get(KindTag::Wall).unwrap();
    let ElementKind::Wall(wall) = &slot.kind else {
        panic!("the wall slot holds a wall");
    };
    assert!((wall.thickness - 0.3).abs() < 1e-9, "{}", wall.thickness);
    assert_eq!(
        ws.doc.project.tool_defaults.get(KindTag::Slab),
        before.get(KindTag::Slab)
    );
    ws.undo();
    assert_eq!(ws.doc.project.tool_defaults, before);
}

/// NFR-USE-003, INP-001: the status line names the next input of the wall
/// tool and lists the keys of that step, generated from its hints.
#[test]
fn status_line_shows_the_step_prompt_and_its_keys() {
    let mut harness = harness("prompt");
    harness.get_by_label("Wall").click();
    harness.run_steps(2);
    assert_present(
        &harness,
        "Wall: click the start point of the reference line · R cycles the reference line · 0–9 types the coordinates \
         · P switches polar and Cartesian",
    );
    let pos = {
        let ws = harness.state_mut().workspace();
        ws.plan_views[&ws.active_storey].last_rect.center()
    };
    harness.hover_at(pos);
    harness.run_steps(1);
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run_steps(2);
    assert_eq!(harness.state_mut().workspace().tool.points.len(), 1);
    assert_present(
        &harness,
        "Wall chain: click the next point or type a distance · Tab moves to the next field \
         · P switches polar and Cartesian · Shift+P switches relative and absolute · Shift constrains angles · Enter or double-click ends the chain · Backspace removes the last point \
         · Esc cancels the step",
    );
    let ws = harness.state_mut().workspace();
    assert_eq!(
        ws.tool.step().unwrap().status_line(),
        "Wall chain: click the next point or type a distance · Tab moves to the next field \
         · P switches polar and Cartesian · Shift+P switches relative and absolute · Shift constrains angles · Enter or double-click ends the chain · Backspace removes the last point \
         · Esc cancels the step",
        "the status line is the step's StepSpec"
    );
}

/// Clicks the primary button at `pos` in the plan view.
fn click_at(harness: &mut Harness<'static, ArchiCraftApp>, pos: egui::Pos2) {
    harness.hover_at(pos);
    harness.run_steps(1);
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run_steps(2);
}

/// Enters `points` of a wall chain on the active storey, as clicks would.
fn pending_chain(harness: &mut Harness<'static, ArchiCraftApp>, points: &[(f64, f64)]) {
    harness.get_by_label("Wall").click();
    harness.run_steps(2);
    let ws = harness.state_mut().workspace();
    assert_eq!(ws.tool.wall_mode, WallMode::Chain);
    let storey = ws.active_storey;
    for &(x, y) in points {
        let outcome = ws.tool.commit(&ws.doc.project, storey, DVec2::new(x, y));
        assert_eq!(outcome, archicraft_app::input::Outcome::Continue);
    }
}

/// UND-003, WAL-001: clicking a wall mode button while three chain points
/// are pending ends the chain: its two walls are created as one undo step
/// before the mode changes.
#[test]
fn a_wall_mode_button_commits_the_pending_chain() {
    let mut harness = harness("mode-commits-chain");
    pending_chain(&mut harness, &[(0.0, 0.0), (5.0, 0.0), (5.0, 4.0)]);
    harness.get_by_label(WallMode::Single.label()).click();
    harness.run_steps(2);
    let ws = harness.state_mut().workspace();
    assert_eq!(ws.tool.wall_mode, WallMode::Single);
    assert!(!ws.tool.is_drawing());
    let walls = ws
        .doc
        .project
        .elements
        .values()
        .filter(|e| e.as_wall().is_some())
        .count();
    assert_eq!(walls, 2);
    ws.undo();
    assert!(ws.doc.project.elements.is_empty(), "the chain is one undo step");
}

/// UND-003, WAL-007: the plan of the chain's storey previews the walls of
/// the pending chain and marks its points; once the plan of another storey
/// is active, that plan shows neither, and the chain is still pending.
#[test]
fn another_storeys_plan_shows_no_preview_of_the_pending_chain() {
    let mut harness = harness("chain-other-storey");
    pending_chain(&mut harness, &[(0.0, 0.0), (5.0, 0.0)]);
    let (ground, upper) = {
        let ws = harness.state_mut().workspace();
        let ground = ws.active_storey;
        let upper = ws.doc.project.storeys.iter().find(|s| s.id != ground).unwrap();
        (ground, (upper.id, upper.name.clone()))
    };
    let center = |harness: &mut Harness<'static, ArchiCraftApp>, storey| {
        harness.state_mut().workspace().plan_views[&storey].last_rect.center()
    };
    let pos = center(&mut harness, ground);
    harness.hover_at(pos);
    harness.run_steps(2);
    {
        let state = &harness.state_mut().workspace().plan_views[&ground];
        assert!(!state.preview.is_empty(), "the chain's own plan previews it");
        assert_eq!(state.pending.len(), 2);
    }

    let row = harness
        .query_all_by_label_contains(&upper.1)
        .filter_map(|n| n.accesskit_node().label())
        .find(|label| !label.starts_with("Plan: "))
        .unwrap();
    harness.get_by_label(&row).click();
    harness.run_steps(2);
    assert_eq!(harness.state_mut().workspace().active_storey, upper.0);
    let pos = center(&mut harness, upper.0);
    harness.hover_at(pos);
    harness.run_steps(2);
    let ws = harness.state_mut().workspace();
    let state = &ws.plan_views[&upper.0];
    assert!(state.glyph.is_some(), "the pointer is over the upper plan");
    assert_eq!(state.preview, [], "no preview of another storey's chain");
    assert_eq!(state.pending, []);
    assert_eq!(ws.tool.points.len(), 2, "the chain is still pending");
}

/// INP-003: in the plan view Esc cancels one step per press: the typed text,
/// then the last point, then the selection, then the drawing tool.
/// Backspace removes one typed character, then one point.
#[test]
fn escape_cancels_one_step_per_press() {
    let mut harness = harness("esc-ladder");
    harness.get_by_label("Wall").click();
    harness.run_steps(2);
    let pos = {
        let ws = harness.state_mut().workspace();
        ws.tool.wall_mode = WallMode::RotatedRectangle;
        let storey = ws.doc.project.storeys.ground();
        let far = DVec2::new(500.0, 500.0);
        let wall = ws.doc.project.new_wall(storey, far, far + DVec2::X, 0);
        let id = wall.id;
        assert!(ws.execute(Command::AddElements(vec![wall])));
        ws.selection.insert(id);
        ws.plan_views[&ws.active_storey].last_rect.center()
    };
    click_at(&mut harness, pos);
    click_at(&mut harness, pos + egui::vec2(120.0, 0.0));
    let state = |h: &mut Harness<'static, ArchiCraftApp>| {
        let ws = h.state_mut().workspace();
        (
            ws.tool.points.len(),
            ws.input.typed.text(Field::Distance).to_owned(),
            ws.tool.kind,
        )
    };
    assert_eq!(state(&mut harness), (2, String::new(), ToolKind::Wall));

    harness.event(Event::Text("12".into()));
    harness.run_steps(2);
    assert_eq!(state(&mut harness), (2, "12".into(), ToolKind::Wall));
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert_eq!(
        state(&mut harness),
        (2, String::new(), ToolKind::Wall),
        "Esc clears only the typed text"
    );
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert_eq!(
        state(&mut harness),
        (1, String::new(), ToolKind::Wall),
        "Esc returns to the second step"
    );
    assert_present(
        &harness,
        "Rotated wall rectangle: click the end of the base edge or type a distance · Tab moves to the next \
         field · P switches polar and Cartesian · Shift+P switches relative and absolute · Shift constrains angles · Backspace removes the last point · Esc cancels the step",
    );

    harness.event(Event::Text("12".into()));
    harness.run_steps(2);
    harness.key_press(Key::Backspace);
    harness.run_steps(2);
    assert_eq!(state(&mut harness), (1, "1".into(), ToolKind::Wall));
    harness.key_press(Key::Backspace);
    harness.run_steps(2);
    assert_eq!(state(&mut harness), (1, String::new(), ToolKind::Wall));
    harness.key_press(Key::Backspace);
    harness.run_steps(2);
    assert_eq!(state(&mut harness), (0, String::new(), ToolKind::Wall));
    assert_eq!(harness.state_mut().workspace().selection.len(), 1);

    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert!(harness.state_mut().workspace().selection.is_empty());
    assert_eq!(state(&mut harness), (0, String::new(), ToolKind::Wall));
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert_eq!(state(&mut harness), (0, String::new(), ToolKind::Select));
}

/// INP-003, UI-008: before the first point a key that does not start a value
/// stays a shortcut (S switches to the Slab tool); a digit starts typing the
/// first point, and the unit letters that follow in the same frame are part
/// of the value, not shortcuts: '3ft' leaves the view unfitted and the
/// tracker shows it in the X field.
#[test]
fn typing_the_first_point_keeps_shortcuts_until_a_value_starts() {
    let mut harness = harness("first-point");
    harness.get_by_label("Wall").click();
    harness.run_steps(2);
    let (storey, pos) = {
        let ws = harness.state_mut().workspace();
        let storey = ws.doc.project.storeys.ground();
        let far = DVec2::new(500.0, 500.0);
        let wall = ws.doc.project.new_wall(storey, far, far + DVec2::X, 0);
        assert!(ws.execute(Command::AddElements(vec![wall])));
        (storey, ws.plan_views[&storey].last_rect.center())
    };
    harness.hover_at(pos);
    harness.run_steps(2);

    harness.key_press(Key::S);
    harness.event(Event::Text("s".into()));
    harness.run_steps(2);
    assert_eq!(harness.state_mut().workspace().tool.kind, ToolKind::Slab);
    harness.key_press(Key::W);
    harness.event(Event::Text("w".into()));
    harness.run_steps(2);
    assert_eq!(harness.state_mut().workspace().tool.kind, ToolKind::Wall);

    let camera = |h: &mut Harness<'static, ArchiCraftApp>| h.state_mut().workspace().plan_views[&storey].camera;
    let before = camera(&mut harness);
    // Typed slowly (a frame per event) and fast (all keys in one frame).
    for same_frame in [false, true] {
        let mut events = Vec::new();
        for (key, text) in [(Key::Num3, "3"), (Key::F, "f"), (Key::T, "t")] {
            for pressed in [true, false] {
                events.push(Event::Key {
                    key,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                });
            }
            events.push(Event::Text(text.into()));
        }
        if same_frame {
            harness.input_mut().events.extend(events);
        } else {
            for event in events {
                harness.event(event);
            }
        }
        harness.run_steps(2);
        assert_eq!(camera(&mut harness), before, "F belongs to the typed value");
        let ws = harness.state_mut().workspace();
        assert_eq!(ws.input.typed.text(Field::X), "3ft", "same frame: {same_frame}");
        assert!(ws.tool.points.is_empty());
        let tracker = ws.plan_views[&storey].tracker.clone().unwrap_or_default();
        assert!(tracker.starts_with("▸X 3ft_\n"), "{tracker}");
        harness.key_press(Key::Escape);
        harness.run_steps(2);
        assert!(harness.state_mut().workspace().input.typed.is_empty());
    }

    // Without typed text F fits the view again.
    harness.key_press(Key::F);
    harness.run_steps(2);
    assert_ne!(camera(&mut harness), before);
}

/// INP-022: in the plan view, after the first wall point, X addresses the
/// ΔX field (switching to the Cartesian set) and 3+1 is typed into it as an
/// expression; Enter takes the chain's next point 4 m along X from the
/// first.
#[test]
fn typed_keys_address_a_field_and_take_an_expression() {
    let mut harness = harness("address-field");
    harness.get_by_label("Wall").click();
    harness.run_steps(2);
    let (storey, pos) = {
        let ws = harness.state_mut().workspace();
        (ws.active_storey, ws.plan_views[&ws.active_storey].last_rect.center())
    };
    click_at(&mut harness, pos);
    let first = harness.state_mut().workspace().tool.points[0];
    for (key, text) in [(Key::X, "X"), (Key::Num3, "3"), (Key::Plus, "+"), (Key::Num1, "1")] {
        harness.key_press(key);
        harness.event(Event::Text(text.into()));
        harness.run_steps(1);
    }
    harness.run_steps(1);
    let ws = harness.state_mut().workspace();
    assert_eq!(ws.input.typed.field, Field::X);
    assert_eq!(ws.input.typed.text(Field::X), "3+1");
    let tracker = ws.plan_views[&storey].tracker.clone().unwrap_or_default();
    assert!(tracker.starts_with("▸ΔX 3+1_\n"), "{tracker}");
    harness.key_press(Key::Enter);
    harness.run_steps(2);
    let ws = harness.state_mut().workspace();
    let points = &ws.tool.points;
    assert_eq!(points.len(), 2, "the chain took the typed point");
    assert!(
        (points[1] - (first + DVec2::new(4.0, 0.0))).length() < 1e-9,
        "{}",
        points[1]
    );
}

/// UI-008, HW-003: Help ▸ Keyboard shortcuts lists the commands with all
/// their default bindings from the registry, among them the second bindings
/// of Redo and Delete and the local keys of the plan view (HW-003).
#[test]
fn the_shortcuts_window_lists_every_binding() {
    let mut harness = harness("shortcuts-window");
    open_menu(&mut harness, "Help");
    harness.get_by_label("Keyboard shortcuts").click();
    harness.run_steps(2);
    let keys = |bindings: &[Binding]| {
        bindings
            .iter()
            .map(|b| harness.ctx.format_shortcut(&b.shortcut()))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let redo = keys(&[Binding::primary(Key::Z).shift(), Binding::primary(Key::Y)]);
    let delete = keys(&[Binding::key(Key::Delete), Binding::key(Key::Backspace)]);
    assert_eq!(delete, "Delete, Backspace");
    for (label, keys) in [("Redo", redo), ("Delete", delete), ("Wall tool", "W".to_owned())] {
        assert_present(&harness, label);
        assert_present(&harness, &keys);
    }
    let rows = Registry::get()
        .specs()
        .iter()
        .filter(|s| !s.default_bindings.is_empty())
        .count();
    // The 23 former actions and the 6 local commands of the plan view.
    assert_eq!(rows, 29);
    for spec in Registry::get().specs() {
        if !spec.default_bindings.is_empty() {
            assert_present(&harness, &keys(&spec.default_bindings));
        }
    }
    for id in view_keys::ALL {
        assert_present(&harness, &Registry::get().spec(id.as_str()).unwrap().label());
    }
}

#[test]
fn edit_menu_undoes_and_redoes() {
    let mut harness = harness("undo");
    {
        let ws = harness.state_mut().workspace();
        let storey = ws.doc.project.storeys.ground();
        let wall = ws.doc.project.new_wall(storey, DVec2::ZERO, DVec2::new(4.0, 0.0), 0);
        assert!(ws.execute(Command::AddElements(vec![wall])));
    }
    harness.run_steps(2);
    harness.get_by_label("Edit").click();
    harness.run_steps(2);
    harness.get_by_label_contains("Undo Create wall").click();
    harness.run_steps(2);
    assert!(harness.state_mut().workspace().doc.project.elements.is_empty());
    harness.get_by_label("Edit").click();
    harness.run_steps(2);
    harness.get_by_label_contains("Redo Create wall").click();
    harness.run_steps(2);
    assert_eq!(harness.state_mut().workspace().doc.project.elements.len(), 1);
}

/// Adds a wall on the ground storey, one change in the history.
fn add_wall(harness: &mut Harness<'static, ArchiCraftApp>) {
    let ws = harness.state_mut().workspace();
    let storey = ws.doc.project.storeys.ground();
    let wall = ws.doc.project.new_wall(storey, DVec2::ZERO, DVec2::new(4.0, 0.0), 0);
    assert!(ws.execute(Command::AddElements(vec![wall])));
    harness.run_steps(2);
}

/// UI-011: the toolbar lies directly below the menu bar and above the
/// properties strip, and its only buttons are Undo, then Redo, each its
/// command's icon and label.
#[test]
fn the_toolbar_lies_between_the_menu_bar_and_the_properties_strip() {
    let harness = harness("toolbar-place");
    let menu_bar = harness
        .query_all_by_label("File")
        .map(|n| n.rect())
        .min_by(|a, b| a.min.y.total_cmp(&b.min.y))
        .unwrap();
    let strip = harness
        .get_by_label("Nothing selected. Choose a tool, or select elements to edit them.")
        .rect();
    let mut buttons: Vec<(f32, String)> = harness
        .query_all(By::new().role(Role::Button))
        .filter(|n| n.rect().min.y > menu_bar.max.y && n.rect().max.y < strip.min.y)
        .map(|n| (n.rect().min.x, n.accesskit_node().label().unwrap_or_default()))
        .collect();
    buttons.sort_by(|a, b| a.0.total_cmp(&b.0));
    let labels: Vec<&str> = buttons.iter().map(|(_, label)| label.as_str()).collect();
    assert_eq!(labels, ["Undo", "Redo"]);
}

/// UI-001, NFR-PORT-005: the toolbox and the toolbar show their icons,
/// rasterised for the screen's pixels: 20 pt in the toolbox and 16 pt in the
/// toolbar, with twice as many pixels per side at 200 % scaling. egui names
/// an icon's texture by its uri.
#[test]
fn the_toolbox_and_the_toolbar_draw_their_icons_for_the_screen() {
    let toolbox = [
        icons::SELECT,
        icons::WALL,
        icons::SLAB,
        icons::FIT,
        icons::ORBIT,
        icons::PLAN_3D,
    ];
    let toolbar = [icons::UNDO, icons::REDO];
    for (pixels_per_point, name) in [(1.0, "icons-100"), (2.0, "icons-200")] {
        let harness = harness_at(name, pixels_per_point);
        let textures: Vec<(String, [usize; 2])> = harness
            .ctx
            .tex_manager()
            .read()
            .allocated()
            .map(|(_, meta)| (meta.name.clone(), meta.size))
            .collect();
        for (icons, points) in [(&toolbox[..], 20.0), (&toolbar[..], 16.0)] {
            let side = (points * pixels_per_point) as usize;
            for icon in icons {
                let uri = icon.uri().unwrap_or_default().to_owned();
                assert!(
                    textures.contains(&(uri.clone(), [side, side])),
                    "{uri} at {pixels_per_point}: {textures:?}"
                );
            }
        }
    }
}

/// The toolbar's button labelled `label`, not a tooltip with the same text.
fn toolbar_button<'h>(harness: &'h Harness<'static, ArchiCraftApp>, label: &'h str) -> egui_kittest::Node<'h> {
    harness.get(By::new().role(Role::Button).label(label))
}

/// UI-011, NFR-L10N-001: with the en-XA pseudo-locale installed, the labels
/// of the toolbar's buttons come out wrapped in the pseudo markers, so they
/// come from the catalogue.
#[cfg(debug_assertions)]
#[test]
fn the_toolbar_labels_come_from_the_catalogue() {
    let mut harness = harness("toolbar-pseudo");
    let pseudo = Rc::new(Localizer::pseudo());
    harness.state_mut().set_localizer(Rc::clone(&pseudo));
    harness.run_steps(2);
    for key in ["cmd-edit-undo-label", "cmd-edit-redo-label"] {
        let label = i18n::with(Rc::clone(&pseudo), || tr!(key));
        assert!(label.starts_with(i18n::pseudo::OPEN), "{label}");
        toolbar_button(&harness, &label);
    }
    assert!(harness.query_all_by_label("Undo").next().is_none());
}

/// UI-011, UND-001: with an empty history the toolbar's Undo and Redo are
/// greyed out; after a change Undo undoes it, then Redo redoes it, each
/// enabled only while it can run. Both invoke their commands by id with
/// `Origin::Toolbar`.
#[test]
fn the_toolbar_undoes_and_redoes() {
    let mut harness = harness("toolbar-undo");
    let enabled = |harness: &Harness<'static, ArchiCraftApp>| {
        ["Undo", "Redo"].map(|label| !toolbar_button(harness, label).accesskit_node().is_disabled())
    };
    let elements =
        |harness: &mut Harness<'static, ArchiCraftApp>| harness.state_mut().workspace().doc.project.elements.len();
    assert_eq!(enabled(&harness), [false, false]);
    add_wall(&mut harness);
    assert_eq!(enabled(&harness), [true, false]);
    toolbar_button(&harness, "Undo").click();
    harness.run_steps(2);
    assert_eq!(elements(&mut harness), 0);
    assert_eq!(enabled(&harness), [false, true]);
    toolbar_button(&harness, "Redo").click();
    harness.run_steps(2);
    assert_eq!(elements(&mut harness), 1);
    assert_eq!(enabled(&harness), [true, false]);

    harness.state_mut().record_invocations();
    toolbar_button(&harness, "Undo").click();
    harness.run_steps(2);
    assert_eq!(invoked(&mut harness), [("edit.undo", Origin::Toolbar)]);
    harness.state_mut().workspace().undo();
    harness.run_steps(2);
    toolbar_button(&harness, "Redo").click();
    harness.run_steps(2);
    assert_eq!(invoked(&mut harness), [("edit.redo", Origin::Toolbar)]);
}

/// UI-011: hovering a toolbar button shows the command's label in the
/// current state, its shortcut and its description; a greyed-out button also
/// gives the reason from the catalogue.
#[test]
fn the_toolbar_tooltips_show_label_shortcut_description_and_reason() {
    let mut harness = harness("toolbar-tooltips");
    add_wall(&mut harness);
    let undo_keys = harness.ctx.format_shortcut(&Binding::primary(Key::Z).shortcut());
    let redo_keys = harness
        .ctx
        .format_shortcut(&Binding::primary(Key::Z).shift().shortcut());
    toolbar_button(&harness, "Undo").hover();
    harness.run_steps(4);
    for text in [
        "Undo Create wall",
        undo_keys.as_str(),
        "Take back the last change to the model.",
    ] {
        assert_present(&harness, text);
    }
    assert!(harness.query_all_by_label("There is nothing to undo.").next().is_none());
    toolbar_button(&harness, "Redo").hover();
    harness.run_steps(4);
    for text in [
        "Redo",
        redo_keys.as_str(),
        "Repeat the last change that was undone.",
        "There is nothing to redo.",
    ] {
        assert_present(&harness, text);
    }
    assert!(harness.query_all_by_label("Undo Create wall").next().is_none());
}

/// The commands invoked since the last call, with their origin.
fn invoked(harness: &mut Harness<'static, ArchiCraftApp>) -> Vec<(&'static str, Origin)> {
    harness
        .state_mut()
        .take_invocations()
        .into_iter()
        .map(|i| (i.id.as_str(), i.origin))
        .collect()
}

/// UI-017, UI-001: the menus generated from the registry, the toolbox and the
/// status bar of a plan invoke their commands by id through
/// `ArchiCraftApp::invoke`, the menus with `Origin::Menu` and the toolbox and
/// the status bar with `Origin::Toolbar`.
#[test]
fn menus_toolbox_and_status_bar_invoke_commands_by_id() {
    let mut harness = harness("invoke-by-id");
    harness.state_mut().record_invocations();
    for (menu, item, id) in [
        ("Help", "About ArchiCraft", "help.about"),
        ("Design", "Wall tool", "tool.wall"),
        ("View", "Show wall reference lines", "view.reference-lines"),
        ("File", "Preferences…", "file.preferences"),
    ] {
        open_menu(&mut harness, menu);
        harness.get_by_label_contains(item).click();
        harness.run_steps(2);
        assert_eq!(invoked(&mut harness), [(id, Origin::Menu)], "{menu} ▸ {item}");
    }
    for (label, id) in [
        ("Select", "tool.select"),
        ("Wall", "tool.wall"),
        ("Slab", "tool.slab"),
        ("Orbit", "view.orbit"),
        ("Plan/3D", "view.switch-plan-3d"),
        ("Next zoom ▶", "view.next-zoom"),
        ("◀ Previous zoom", "view.previous-zoom"),
        ("No problems", "view.problems"),
    ] {
        harness.get_by_label(label).click();
        harness.run_steps(2);
        assert_eq!(invoked(&mut harness), [(id, Origin::Toolbar)], "{label}");
    }
    // Fit is in the toolbox and in the status bar.
    let fits: Vec<_> = harness.query_all_by_label("Fit").map(|n| n.rect().center()).collect();
    assert_eq!(fits.len(), 2);
    for pos in fits {
        click_at(&mut harness, pos);
        harness.run_steps(1);
        assert_eq!(invoked(&mut harness), [("view.fit-window", Origin::Toolbar)]);
    }
}

/// UI-017, VIEW-014: the status bar of a 3D view invokes Fit and Orbit by
/// id with `Origin::Toolbar`.
#[test]
fn the_3d_status_bar_invokes_its_commands_by_id() {
    let mut harness = harness("invoke-3d");
    harness.get_by_label("Plan/3D").click();
    harness.run_steps(2);
    assert!(matches!(
        harness.state_mut().workspace().active_pane,
        Some(Pane::View3d(_))
    ));
    harness.state_mut().record_invocations();
    for (label, id) in [("Fit", "view.fit-window"), ("Orbit", "view.orbit")] {
        // The status bar's button lies below the toolbox's.
        let pos = harness
            .query_all_by_label(label)
            .map(|n| n.rect())
            .max_by(|a, b| a.min.y.total_cmp(&b.min.y))
            .unwrap()
            .center();
        click_at(&mut harness, pos);
        harness.run_steps(1);
        assert_eq!(invoked(&mut harness), [(id, Origin::Toolbar)], "{label}");
    }
}

/// UI-017, UI-008: an on/off menu entry shows its shortcut like the other
/// entries; View ▸ Problems shows Primary+Shift+P.
#[test]
fn checked_menu_entries_show_their_shortcut() {
    let mut harness = harness("checked-shortcut");
    open_menu(&mut harness, "View");
    let shortcut = harness
        .ctx
        .format_shortcut(&Binding::primary(Key::P).shift().shortcut());
    harness.get_by_label(&format!("Problems {shortcut}"));
    harness.get_by_label("Show wall reference lines");
}

/// UI-011, PRJ-003: File ▸ Open recent follows Open… and lists the recent
/// projects, or says that there are none.
#[test]
fn open_recent_follows_open() {
    let mut harness = harness("open-recent");
    harness.state_mut().workspace().prefs.recent.clear();
    harness.run_steps(1);
    open_menu(&mut harness, "File");
    let top =
        |harness: &Harness<'static, ArchiCraftApp>, label: &str| harness.get_by_label_contains(label).rect().min.y;
    let (open, recent, save) = (
        top(&harness, "Open…"),
        top(&harness, "Open recent"),
        top(&harness, "Save as…"),
    );
    assert!(open < recent && recent < save, "{open} {recent} {save}");
    harness.get_by_label_contains("Open recent").hover();
    harness.run_steps(2);
    harness.get_by_label("No recent projects");
}

/// Presses and releases `binding` with the modifier keys `os` reports for
/// it: Cmd (with egui's logical `command`) on macOS, Ctrl elsewhere.
fn press(harness: &mut Harness<'static, ArchiCraftApp>, binding: Binding, os: OperatingSystem) {
    let mut modifiers = binding.for_os(os).modifiers;
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
    harness.run_steps(1);
}

/// The commands invoked by shortcuts since the last call.
fn invoked_by_shortcut(harness: &mut Harness<'static, ArchiCraftApp>) -> Vec<&'static str> {
    harness
        .state_mut()
        .take_invocations()
        .into_iter()
        .inspect(|i| assert_eq!(i.origin, Origin::Shortcut, "{i:?}"))
        .map(|i| i.id.as_str())
        .collect()
}

/// UI-008, HW-003, UI-017: every default binding of the registry, among them
/// the 23 shortcuts of the former action list, Primary+Y (redo) and
/// Backspace (delete), invokes its command when its keys are pressed, on
/// macOS and elsewhere. The plan view runs its local commands itself
/// (`the_plan_views_local_keys_run_their_commands`).
#[test]
fn every_default_shortcut_invokes_its_command() {
    let mut harness = harness("shortcuts");
    harness.state_mut().record_invocations();
    let mut pressed = 0;
    for os in [OperatingSystem::Mac, OperatingSystem::Nix] {
        for spec in Registry::get().specs().iter().filter(|s| !s.local) {
            for &binding in &spec.default_bindings {
                press(&mut harness, binding, os);
                assert_eq!(
                    invoked_by_shortcut(&mut harness),
                    [spec.id.as_str()],
                    "{} on {os:?}",
                    binding.display(os)
                );
                pressed += 1;
            }
        }
    }
    assert_eq!(pressed, 2 * 25);
}

/// UI-008, INP-003: while a tool takes point input the single-key shortcuts,
/// Backspace and Ctrl+Tab belong to the tool and invoke no command; the
/// shortcuts with Cmd or Ctrl still work.
#[test]
fn single_key_shortcuts_stay_off_while_a_tool_is_drawing() {
    let mut harness = harness("drawing-keys");
    harness.get_by_label("Wall").click();
    harness.run_steps(2);
    let pos = {
        let ws = harness.state_mut().workspace();
        ws.plan_views[&ws.active_storey].last_rect.center()
    };
    click_at(&mut harness, pos);
    assert_eq!(harness.state_mut().workspace().tool.points.len(), 1);
    harness.state_mut().record_invocations();
    let os = OperatingSystem::Nix;
    for binding in [Key::V, Key::W, Key::S, Key::F, Key::F3, Key::O, Key::Delete]
        .map(Binding::key)
        .into_iter()
        .chain([Binding::ctrl(Key::Tab)])
    {
        press(&mut harness, binding, os);
        assert_eq!(invoked_by_shortcut(&mut harness), Vec::<&str>::new(), "{binding:?}");
    }
    press(&mut harness, Binding::primary(Key::Z), os);
    assert_eq!(invoked_by_shortcut(&mut harness), ["edit.undo"]);
    press(&mut harness, Binding::key(Key::F).shift(), os);
    assert_eq!(invoked_by_shortcut(&mut harness), ["view.zoom-selection"]);
    assert_eq!(harness.state_mut().workspace().tool.points.len(), 1);
    // Backspace removes the point instead of deleting the selection.
    press(&mut harness, Binding::key(Key::Backspace), os);
    harness.run_steps(1);
    assert_eq!(invoked_by_shortcut(&mut harness), Vec::<&str>::new(), "Backspace");
    assert!(harness.state_mut().workspace().tool.points.is_empty());
}

/// The reference-line location of the wall tool's defaults.
fn wall_reference_line(harness: &mut Harness<'static, ArchiCraftApp>) -> ReferenceLine {
    let ws = harness.state_mut().workspace();
    let slot = ws.doc.project.tool_defaults.get(KindTag::Wall).unwrap();
    let ElementKind::Wall(wall) = &slot.kind else {
        panic!("the wall slot holds a wall");
    };
    wall.reference_line
}

/// Presses `key` with `modifiers` and types its character, as a keyboard
/// does, in one frame.
fn type_key(harness: &mut Harness<'static, ArchiCraftApp>, key: Key, modifiers: Modifiers, text: &str) {
    for pressed in [true, false] {
        harness.event(Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        });
    }
    harness.event(Event::Text(text.into()));
    harness.run_steps(2);
}

/// HW-003, NFR-A11Y-001, WAL-007, INP-003: the plan view's local keys run
/// their commands: with the wall tool active R cycles the reference line
/// before the first point and while drawing, with a status message; Tab
/// moves to the next typed field while drawing; Backspace removes the last
/// chain point. R typed into a focused text field is a letter and leaves the
/// reference line alone (UI-008).
#[test]
fn the_plan_views_local_keys_run_their_commands() {
    let mut harness = harness("local-keys");
    harness.get_by_label("Wall").click();
    harness.run_steps(2);
    let pos = {
        let ws = harness.state_mut().workspace();
        ws.plan_views[&ws.active_storey].last_rect.center()
    };
    harness.hover_at(pos);
    harness.run_steps(2);

    let first = wall_reference_line(&mut harness);
    type_key(&mut harness, Key::R, Modifiers::NONE, "r");
    let second = wall_reference_line(&mut harness);
    assert_eq!(second, first.next(), "R before the first point");
    let status = harness.state_mut().workspace().status.clone().unwrap();
    assert!(status.text.starts_with("Wall reference line: "), "{}", status.text);

    click_at(&mut harness, pos);
    click_at(&mut harness, pos + egui::vec2(120.0, 0.0));
    assert_eq!(harness.state_mut().workspace().tool.points.len(), 2);
    type_key(&mut harness, Key::R, Modifiers::NONE, "r");
    assert_eq!(wall_reference_line(&mut harness), second.next(), "R while drawing");

    type_key(&mut harness, Key::Num2, Modifiers::NONE, "2");
    let field = harness.state_mut().workspace().input.typed.field;
    harness.key_press(Key::Tab);
    harness.run_steps(2);
    let ws = harness.state_mut().workspace();
    assert_ne!(ws.input.typed.field, field, "Tab moved to the next field");
    assert_eq!(ws.tool.points.len(), 2);
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert!(harness.state_mut().workspace().input.typed.is_empty());
    harness.key_press(Key::Backspace);
    harness.run_steps(2);
    assert_eq!(harness.state_mut().workspace().tool.points.len(), 1, "Backspace");

    let before = wall_reference_line(&mut harness);
    harness.state_mut().workspace().doc.project.info.name = "Villa".to_owned();
    open_menu(&mut harness, "Project");
    harness.get_by_label("Project settings…").click();
    harness.run_steps(2);
    harness.get(By::new().role(Role::TextInput).value("Villa")).focus();
    harness.run_steps(2);
    assert!(harness.ctx.text_edit_focused());
    type_key(&mut harness, Key::R, Modifiers::NONE, "r");
    harness.get(By::new().role(Role::TextInput).value("Villar"));
    assert_eq!(wall_reference_line(&mut harness), before, "R typed into the field");
    assert_eq!(harness.state_mut().workspace().tool.points.len(), 1);
}

/// UI-009, HW-003: the plan view takes its local keys from the keymap: with
/// the reference-line cycle bound to Shift+R, Shift+R cycles it and plain R
/// does nothing.
#[test]
fn a_rebound_local_key_runs_its_command() {
    let mut harness = harness("rebound-local-key");
    harness
        .state_mut()
        .workspace()
        .keymap
        .set(view_keys::CYCLE_REFERENCE_LINE, vec![Binding::key(Key::R).shift()]);
    harness.get_by_label("Wall").click();
    harness.run_steps(2);
    let first = wall_reference_line(&mut harness);
    type_key(&mut harness, Key::R, Modifiers::NONE, "r");
    assert_eq!(wall_reference_line(&mut harness), first, "plain R");
    type_key(&mut harness, Key::R, Modifiers::SHIFT, "R");
    assert_eq!(wall_reference_line(&mut harness), first.next(), "Shift+R");
}

/// HW-003, NFR-A11Y-001: invoked other than by their key (as the command
/// palette will), the local commands act as their keys do: the reference
/// line cycles at once, and Remove last point and Cancel step reach the
/// active plan view in the next frame.
#[test]
fn invoked_local_commands_act_like_their_keys() {
    let mut harness = harness("invoked-local");
    harness.get_by_label("Wall").click();
    harness.run_steps(2);
    let pos = {
        let ws = harness.state_mut().workspace();
        ws.plan_views[&ws.active_storey].last_rect.center()
    };
    click_at(&mut harness, pos);
    click_at(&mut harness, pos + egui::vec2(120.0, 0.0));
    let invoke = |harness: &mut Harness<'static, ArchiCraftApp>, id| {
        let ctx = harness.ctx.clone();
        let app = harness.state_mut();
        let target = Target::active(app.workspace());
        app.invoke(&ctx, id, target, Origin::Test);
        harness.run_steps(1);
    };
    let first = wall_reference_line(&mut harness);
    invoke(&mut harness, view_keys::CYCLE_REFERENCE_LINE);
    assert_eq!(wall_reference_line(&mut harness), first.next());
    invoke(&mut harness, view_keys::REMOVE_LAST);
    assert_eq!(harness.state_mut().workspace().tool.points.len(), 1);
    invoke(&mut harness, view_keys::CANCEL_STEP);
    let ws = harness.state_mut().workspace();
    assert!(ws.tool.points.is_empty());
    assert!(ws.view_commands.is_empty());
}

/// UI-008: typing W into a text field of the Project settings dialog types
/// the letter and leaves the Selection tool active; the shortcuts that work
/// while typing still do.
#[test]
fn typing_into_a_text_field_does_not_switch_tools() {
    let mut harness = harness("typing-w");
    harness.state_mut().workspace().doc.project.info.name = "Villa Test".to_owned();
    open_menu(&mut harness, "Project");
    harness.get_by_label("Project settings…").click();
    harness.run_steps(2);
    harness.get(By::new().role(Role::TextInput).value("Villa Test")).focus();
    harness.run_steps(2);
    assert!(harness.ctx.text_edit_focused(), "the project name field has focus");
    harness.key_press(Key::W);
    harness.event(Event::Text("w".into()));
    harness.run_steps(2);
    assert_eq!(harness.state_mut().workspace().tool.kind, ToolKind::Select);
    harness.get(By::new().role(Role::TextInput).value("Villa Testw"));

    harness.state_mut().record_invocations();
    press(&mut harness, Binding::key(Key::W), OperatingSystem::Nix);
    press(&mut harness, Binding::primary(Key::A), OperatingSystem::Nix);
    assert_eq!(invoked_by_shortcut(&mut harness), Vec::<&str>::new());
    press(&mut harness, Binding::primary(Key::P).shift(), OperatingSystem::Nix);
    assert_eq!(invoked_by_shortcut(&mut harness), ["view.problems"]);
}

#[test]
fn selecting_an_element_shows_its_properties() {
    let mut harness = harness("select");
    {
        let ws = harness.state_mut().workspace();
        let storey = ws.doc.project.storeys.ground();
        let wall = ws.doc.project.new_wall(storey, DVec2::ZERO, DVec2::new(4.0, 0.0), 0);
        let id = wall.id;
        ws.execute(Command::AddElements(vec![wall]));
        ws.selection.insert(id);
    }
    harness.run_steps(2);
    harness.get_by_label("Locked");
    harness.get_by_label("Length");
    harness.get_by_label("Home storey");
}

/// INP-017: a selected wall offers a 'Snap targets' checkbox beside
/// 'Locked'; clearing it excludes the wall from snapping, undoably, while
/// it stays visible.
#[test]
fn a_selected_wall_can_be_excluded_from_snapping() {
    let mut harness = harness("snap-targets");
    let id = {
        let ws = harness.state_mut().workspace();
        let storey = ws.doc.project.storeys.ground();
        let wall = ws.doc.project.new_wall(storey, DVec2::ZERO, DVec2::new(4.0, 0.0), 0);
        let id = wall.id;
        ws.execute(Command::AddElements(vec![wall]));
        ws.selection.insert(id);
        id
    };
    harness.run_steps(2);
    harness.get_by_label("Snap targets").click();
    harness.run_steps(2);
    let ws = harness.state_mut().workspace();
    let wall = ws.doc.project.element(id).unwrap().clone();
    assert!(wall.snap_excluded);
    assert!(ws.doc.project.is_visible(&wall));
    assert!(!ws.doc.project.provides_snaps(&wall));
    ws.undo();
    assert!(!ws.doc.project.element(id).unwrap().snap_excluded);
}

/// INP-017: the Layers dialog has a 'Snaps' column whose checkbox excludes
/// the layer's elements from snapping without hiding them.
#[test]
fn the_layers_dialog_has_a_snaps_column() {
    let mut harness = harness("layer-snaps");
    // The project browser's button; the Project menu has the same command.
    harness.query_all_by_label("Layers…").next().unwrap().click();
    harness.run_steps(4);
    for header in ["Visible", "Locked", "Snaps", "Name", "Elements"] {
        harness.get_by_label(header);
    }
    let layer = harness.state_mut().workspace().doc.project.layers[0].id;
    // Each row has the Visible, Locked and Snaps checkboxes, in this order.
    harness
        .query_all(By::new().role(Role::CheckBox))
        .nth(2)
        .unwrap()
        .click();
    harness.run_steps(2);
    let ws = harness.state_mut().workspace();
    let layer = ws.doc.project.layer(layer).unwrap();
    assert!(layer.snap_excluded);
    assert!(layer.visible && !layer.locked);
}

/// SRS 3.2, ELM-011: an element of a kind without a panel of its own (the
/// model's test kind) shows the common fields only (name, layer, its kind's
/// IFC classes), and its missing geometry is the only problem listed.
#[test]
fn a_kind_without_its_own_panel_shows_the_common_fields() {
    let mut harness = harness("test-kind");
    {
        let ws = harness.state_mut().workspace();
        let storey = ws.doc.project.storeys.ground();
        let material = ws.doc.project.materials[0].id;
        let element = archicraft_model::kinds::Dummy::new(DVec2::ZERO, DVec2::ONE, material).into_element(
            &ws.doc.project,
            storey,
            0,
        );
        let id = element.id;
        let wall = ws
            .doc
            .project
            .new_wall(storey, DVec2::new(0.0, 3.0), DVec2::new(4.0, 3.0), 0);
        assert!(ws.execute(Command::AddElements(vec![element, wall])));
        ws.selection.insert(id);
    }
    harness.run_steps(2);
    for common in ["Name", "Layer", "IFC class", "Locked", "Home storey"] {
        harness.get_by_label(common);
    }
    for specific in ["Thickness", "Reference line", "Length", "Area"] {
        assert!(harness.query_by_label(specific).is_none(), "{specific} is shown");
    }
    assert_eq!(harness.state_mut().workspace().doc.geometry().problems.len(), 1);
}

/// Adds a 4 m wall and a 4 m × 3 m slab on the ground storey, `slab_type`
/// set on the slab, and returns their ids.
fn add_wall_and_slab(
    harness: &mut Harness<'static, ArchiCraftApp>,
    slab_type: Option<PredefinedType>,
) -> (ElementId, ElementId) {
    let ws = harness.state_mut().workspace();
    let storey = ws.doc.project.storeys.ground();
    let wall = ws.doc.project.new_wall(storey, DVec2::ZERO, DVec2::new(4.0, 0.0), 0);
    let outline = vec![
        DVec2::new(0.0, 1.0),
        DVec2::new(4.0, 1.0),
        DVec2::new(4.0, 4.0),
        DVec2::new(0.0, 4.0),
    ];
    let mut slab = ws.doc.project.new_slab(storey, outline, Vec::new(), 0);
    slab.predefined_type = slab_type;
    let ids = (wall.id, slab.id);
    assert!(ws.execute(Command::AddElements(vec![wall, slab])));
    ids
}

/// Selects only `id` and lets the panel follow.
fn select_only(harness: &mut Harness<'static, ArchiCraftApp>, id: ElementId) {
    let ws = harness.state_mut().workspace();
    ws.selection.clear();
    ws.selection.insert(id);
    harness.run_steps(2);
}

/// Opens the combo box that shows `current`. It is clicked near its left
/// end: the strip's first row reaches past the right edge of the test
/// window.
fn open_combo(harness: &mut Harness<'static, ArchiCraftApp>, current: &str) {
    let rect = harness.get(By::new().role(Role::ComboBox).value(current)).rect();
    click_at(harness, egui::pos2(rect.min.x + 8.0, rect.center().y));
}

/// The commands added to the history since it held `applied` of them.
fn commands_since(harness: &mut Harness<'static, ArchiCraftApp>, applied: usize) -> Vec<Command> {
    let history = &harness.state_mut().workspace().doc.history;
    history.applied_commands().skip(applied).cloned().collect()
}

/// Opens the combo box that shows `current` and clicks `option`, returning
/// the commands this added to the history.
fn choose_in_combo(harness: &mut Harness<'static, ArchiCraftApp>, current: &str, option: &str) -> Vec<Command> {
    let applied = harness.state_mut().workspace().doc.history.applied_commands().count();
    open_combo(harness, current);
    harness.get_by_label(option).click();
    harness.run_steps(2);
    commands_since(harness, applied)
}

/// IFX-024: a selected slab shows a "Predefined type" choice offering none
/// and the four slab types, and an IfcWall wall shows none; choosing "Base
/// slab" is one UpdateElements that sets the type.
#[test]
fn a_slab_offers_its_predefined_types_and_a_wall_none() {
    let mut harness = harness("predefined-types");
    let (wall, slab) = add_wall_and_slab(&mut harness, None);
    select_only(&mut harness, wall);
    harness.get_by_label("IFC class");
    assert!(harness.query_by_label("Predefined type").is_none());

    select_only(&mut harness, slab);
    harness.get_by_label("Predefined type");
    let nones = harness.query_all_by_label("None").count();
    let applied = harness.state_mut().workspace().doc.history.applied_commands().count();
    open_combo(&mut harness, "None");
    assert_eq!(
        harness.query_all_by_label("None").count(),
        nones + 1,
        "none is an option"
    );
    for option in ["Floor", "Roof", "Landing", "Base slab"] {
        harness.get_by_label(option);
    }
    for absent in ["Pad footing", "Strip footing"] {
        assert!(harness.query_by_label(absent).is_none(), "{absent} is offered");
    }
    harness.get_by_label("Base slab").click();
    harness.run_steps(2);
    let commands = commands_since(&mut harness, applied);
    assert!(
        matches!(commands.as_slice(), [Command::UpdateElements(e)] if e.len() == 1 && e[0].id == slab),
        "{commands:?}"
    );
    let ws = harness.state_mut().workspace();
    assert_eq!(
        ws.doc.project.element(slab).unwrap().predefined_type,
        Some(PredefinedType::BaseSlab)
    );
}

/// IFX-024: changing a floor slab's class to IfcFooting drops the floor
/// type, which footings do not offer, in the same UpdateElements, and the
/// type choice then offers the footing types.
#[test]
fn changing_the_class_clears_a_type_the_new_class_does_not_offer() {
    let mut harness = harness("predefined-type-class");
    let (_, slab) = add_wall_and_slab(&mut harness, Some(PredefinedType::Floor));
    select_only(&mut harness, slab);
    harness.get(By::new().role(Role::ComboBox).value("Floor"));

    let commands = choose_in_combo(&mut harness, "IfcSlab", "IfcFooting");
    assert!(
        matches!(commands.as_slice(), [Command::UpdateElements(e)] if e.len() == 1 && e[0].id == slab),
        "{commands:?}"
    );
    let element = harness
        .state_mut()
        .workspace()
        .doc
        .project
        .element(slab)
        .unwrap()
        .clone();
    assert_eq!((element.ifc_class, element.predefined_type), (IfcClass::Footing, None));
    open_combo(&mut harness, "None");
    harness.get_by_label("Pad footing");
    harness.get_by_label("Strip footing");
    assert!(harness.query_by_label("Floor").is_none());
}

/// The IFC classes offered in the open class choice, top to bottom: the
/// option labels of every known class, without the combo box itself.
fn offered_classes(harness: &Harness<'static, ArchiCraftApp>) -> Vec<&'static str> {
    let mut offered: Vec<(f32, &'static str)> = IfcClass::ALL
        .iter()
        .flat_map(|class| {
            harness
                .query_all_by_label(class.code())
                .filter(|node| node.accesskit_node().role() != Role::ComboBox)
                .map(|node| (node.rect().min.y, class.code()))
                .collect::<Vec<_>>()
        })
        .collect();
    offered.sort_by(|a, b| a.0.total_cmp(&b.0));
    offered.into_iter().map(|(_, code)| code).collect()
}

/// ELM-011: the IFC class choice offers exactly the selected kind's
/// classes: IfcWall and IfcFooting for a wall, IfcSlab, IfcFooting and
/// IfcCovering for a slab and the twenty Appendix D.5 classes in D.5 order
/// for the test kind, the stand-in for free-form solids and objects.
/// Choosing IfcFooting for the wall is one UpdateElements, and undo
/// restores IfcWall.
#[test]
fn the_class_choice_offers_exactly_the_kinds_classes() {
    let mut harness = harness("ifc-class-choice");
    let (wall, slab) = add_wall_and_slab(&mut harness, None);
    let dummy = {
        let ws = harness.state_mut().workspace();
        let storey = ws.doc.project.storeys.ground();
        let material = ws.doc.project.materials[0].id;
        let element = archicraft_model::kinds::Dummy::new(DVec2::new(6.0, 0.0), DVec2::ONE, material).into_element(
            &ws.doc.project,
            storey,
            0,
        );
        let id = element.id;
        assert!(ws.execute(Command::AddElements(vec![element])));
        id
    };
    for (id, current, expected) in [
        (dummy, "IfcBuildingElementProxy", &IfcClass::SELECTABLE_D5[..]),
        (slab, "IfcSlab", IfcClass::SLAB_CLASSES),
        (wall, "IfcWall", IfcClass::WALL_CLASSES),
    ] {
        select_only(&mut harness, id);
        open_combo(&mut harness, current);
        let expected: Vec<&str> = expected.iter().map(|c| c.code()).collect();
        assert_eq!(offered_classes(&harness), expected, "{current}");
        open_combo(&mut harness, current);
    }

    let commands = choose_in_combo(&mut harness, "IfcWall", "IfcFooting");
    assert!(
        matches!(commands.as_slice(), [Command::UpdateElements(e)] if e.len() == 1 && e[0].id == wall),
        "{commands:?}"
    );
    let class = |harness: &mut Harness<'static, ArchiCraftApp>| {
        harness
            .state_mut()
            .workspace()
            .doc
            .project
            .element(wall)
            .unwrap()
            .ifc_class
    };
    assert_eq!(class(&mut harness), IfcClass::Footing);
    harness.state_mut().workspace().undo();
    harness.run_steps(2);
    assert_eq!(class(&mut harness), IfcClass::Wall);
    harness.get(By::new().role(Role::ComboBox).value("IfcWall"));
}

/// IFX-024, ELM-001: with the slab tool active and nothing selected, a type
/// chosen in the strip goes into the slab tool's defaults slot through
/// SetToolDefaults.
#[test]
fn a_predefined_type_in_the_strip_updates_the_slab_defaults_slot() {
    let mut harness = harness("predefined-type-defaults");
    harness.get_by_label("Slab").click();
    harness.run_steps(2);
    harness.get_by_label("Slab tool defaults");
    let commands = choose_in_combo(&mut harness, "None", "Roof");
    assert!(
        matches!(commands.as_slice(), [Command::SetToolDefaults(_)]),
        "{commands:?}"
    );
    let ws = harness.state_mut().workspace();
    let slot = ws.doc.project.tool_defaults.get(KindTag::Slab).unwrap();
    assert_eq!(slot.predefined_type, Some(PredefinedType::Roof));
    let storey = ws.doc.project.storeys.ground();
    let slab = ws
        .doc
        .project
        .new_slab(storey, vec![DVec2::ZERO, DVec2::X, DVec2::ONE], Vec::new(), 0);
    assert_eq!(slab.predefined_type, Some(PredefinedType::Roof));
}

/// NFR-L10N-001: a value the model rejects reports the localized message in
/// the status line, with the limits in the project length unit.
#[test]
fn a_rejected_thickness_shows_its_limits_in_the_project_unit() {
    let mut harness = harness("rejected-thickness");
    let (shown, min, max) = {
        let ws = harness.state_mut().workspace();
        let mut settings: ProjectSettings = ws.doc.project.settings.clone();
        settings.units.length = LengthUnit::Centimeter;
        assert!(ws.execute(Command::SetSettings(settings)));
        let storey = ws.doc.project.storeys.ground();
        let wall = ws.doc.project.new_wall(storey, DVec2::ZERO, DVec2::new(4.0, 0.0), 0);
        let id = wall.id;
        let thickness = wall.as_wall().unwrap().thickness;
        assert!(ws.execute(Command::AddElements(vec![wall])));
        ws.selection.insert(id);
        let units = ws.doc.project.settings.units.clone();
        let nf = ws.prefs.number_format();
        (
            format_length(thickness, &units, &nf, false),
            format_length(MIN_THICKNESS, &units, &nf, true),
            format_length(MAX_THICKNESS, &units, &nf, true),
        )
    };
    harness.run_steps(2);
    let field = || By::new().role(Role::TextInput).value(shown.as_str());
    harness.get(field()).click();
    harness.run_steps(1);
    harness.get(field()).type_text("2000");
    harness.key_press(Key::Enter);
    harness.run_steps(2);
    let status = harness.state_mut().workspace().status.as_ref().unwrap().text.clone();
    assert!(
        status.contains(&format!("between {min} and {max}")),
        "{status} shows {min} and {max}"
    );
    assert!(min.ends_with("cm") && max.ends_with("cm"), "{min} {max}");
    assert!(!status.contains("model-error"), "{status}");
    harness.get_by_label_contains(&max);
}

/// Opens the menu `title` of the menu bar (the browser has headers with the
/// same names).
fn open_menu(harness: &mut Harness<'static, ArchiCraftApp>, title: &str) {
    let menu = harness
        .query_all_by_label(title)
        .map(|n| n.rect())
        .min_by(|a, b| a.min.y.total_cmp(&b.min.y))
        .unwrap_or_else(|| panic!("no menu “{title}”"));
    click_at(harness, menu.center());
    harness.run_steps(1);
}

/// NFR-L10N-001: the unit choices of the project settings come from the
/// catalogue: with the en-XA pseudo-locale they show the pseudo texts of the
/// unit label keys.
#[cfg(debug_assertions)]
#[test]
fn project_settings_unit_choices_come_from_the_catalogue() {
    let mut harness = harness("unit-choices");
    let pseudo = Rc::new(Localizer::pseudo());
    harness.state_mut().set_localizer(Rc::clone(&pseudo));
    harness.run_steps(2);
    let label = |key: &str| i18n::with(Rc::clone(&pseudo), || tr!(key));
    // Menu titles and command labels come from the catalogue.
    open_menu(&mut harness, &label("menu-project"));
    harness.get_by_label(&label("cmd-project-settings-label")).click();
    harness.run_steps(4);
    let metre = label(LengthUnit::Meter.label_key());
    assert!(metre.starts_with(i18n::pseudo::OPEN), "{metre}");
    harness
        .get(By::new().role(Role::ComboBox).value(metre.as_str()))
        .click();
    harness.run_steps(2);
    for unit in LengthUnit::ALL {
        assert_present(&harness, &label(unit.label_key()));
    }
    assert!(harness.query_all_by_label("Millimetre").next().is_none());
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    let degrees = label(AngleUnit::DecimalDegrees.label_key());
    harness
        .get(By::new().role(Role::ComboBox).value(degrees.as_str()))
        .click();
    harness.run_steps(2);
    for unit in AngleUnit::ALL {
        assert_present(&harness, &label(unit.label_key()));
    }
    assert!(harness.query_all_by_label("Gradians").next().is_none());
}

/// NFR-L10N-001, INP-005: a length the properties strip cannot parse
/// reports the localized parse error in the status line.
#[cfg(debug_assertions)]
#[test]
fn an_unparsable_length_reports_the_localized_parse_error() {
    let mut harness = harness("unparsable-length");
    let pseudo = Rc::new(Localizer::pseudo());
    harness.state_mut().set_localizer(Rc::clone(&pseudo));
    let shown = {
        let ws = harness.state_mut().workspace();
        let storey = ws.doc.project.storeys.ground();
        let wall = ws.doc.project.new_wall(storey, DVec2::ZERO, DVec2::new(4.0, 0.0), 0);
        let id = wall.id;
        let thickness = wall.as_wall().unwrap().thickness;
        assert!(ws.execute(Command::AddElements(vec![wall])));
        ws.selection.insert(id);
        let units = ws.doc.project.settings.units.clone();
        format_length(thickness, &units, &ws.prefs.number_format(), false)
    };
    harness.run_steps(2);
    let field = || By::new().role(Role::TextInput).value(shown.as_str());
    harness.get(field()).click();
    harness.run_steps(1);
    harness.get(field()).type_text("1/0");
    harness.key_press(Key::Enter);
    harness.run_steps(2);
    let status = harness.state_mut().workspace().status.as_ref().unwrap().text.clone();
    let expected = {
        let ws = harness.state_mut().workspace();
        let message = ParseError::InvalidLength("1/0".into()).message();
        i18n::with(Rc::clone(&pseudo), || ws.render(&message))
    };
    assert!(expected.starts_with(i18n::pseudo::OPEN), "{expected}");
    assert_eq!(status, expected);
}

/// Opens Help ▸ About, finding the Help menu by the title `localizer`
/// renders, and lets the new window settle: egui lays it out left-aligned
/// in its first frame and ignores clicks in it for a few frames.
fn open_about(harness: &mut Harness<'static, ArchiCraftApp>, localizer: &Rc<Localizer>) {
    let help = i18n::with(Rc::clone(localizer), || tr!("menu-help"));
    harness.get_by_label(&help).click();
    harness.run_steps(2);
    let about = i18n::with(Rc::clone(localizer), || tr!("cmd-help-about-label"));
    harness.get_by_label(&about).click();
    harness.run_steps(4);
}

/// The version as the About window shows it: major.minor, plus the patch
/// when it is not 0 or the version has a pre-release part.
fn shown_version() -> String {
    if env!("CARGO_PKG_VERSION_PATCH") == "0" && env!("CARGO_PKG_VERSION_PRE").is_empty() {
        format!(
            "{}.{}",
            env!("CARGO_PKG_VERSION_MAJOR"),
            env!("CARGO_PKG_VERSION_MINOR")
        )
    } else {
        env!("CARGO_PKG_VERSION").to_owned()
    }
}

/// The links of the About window: their text and the URL they open.
const ABOUT_LINKS: [(&str, &str); 3] = [
    ("https://archicraft.app", "https://archicraft.app"),
    (
        "https://creativecommons.org/licenses/by/4.0/",
        "https://creativecommons.org/licenses/by/4.0/",
    ),
    (
        "Source code: https://github.com/ArchiCraftApp/ArchiCraft",
        "https://github.com/ArchiCraftApp/ArchiCraft",
    ),
];

/// The About window as Jakob Brandstötter asked for it (archicraft-pb3f2):
/// centred, top to bottom, the app icon with space above it, the name with
/// the version and release stage, what ArchiCraft is, the copyright, the
/// website, a gap, the CC BY 4.0 notice (licence link, warranty disclaimer,
/// source code), and last the file format and the privacy note. The text
/// comes from the catalogue (NFR-L10N-001, the pilot migration).
#[test]
fn about_window_shows_icon_version_copyright_website_and_licence() {
    let mut harness = harness("about");
    let english = Rc::new(Localizer::english());
    open_about(&mut harness, &english);
    let heading = i18n::with(Rc::clone(&english), || {
        tr!("dlg-about-heading", version = shown_version())
    });
    assert_eq!(heading, format!("ArchiCraft {} (alpha)", shown_version()));
    let format = format!("Writes {} files.", archicraft_ifcx::WRITTEN_VERSION);
    let lines = [
        "ArchiCraft app icon",
        heading.as_str(),
        "ArchiCraft is an open-source architectural CAD and BIM tool.",
        "Copyright © 2026 Jakob Brandstötter and Alexander Partsch",
        ABOUT_LINKS[0].0,
        "ArchiCraft is open source, licensed under the Creative Commons Attribution 4.0 International licence \
         (CC BY 4.0).",
        ABOUT_LINKS[1].0,
        "To the extent possible, ArchiCraft is provided as is and as available, without warranties of any kind \
         (section 5 of the licence).",
        ABOUT_LINKS[2].0,
        format.as_str(),
        "Works offline. Sends no data anywhere; crash reports stay on this computer.",
    ];
    let rects: Vec<egui::Rect> = lines.iter().map(|line| harness.get_by_label(line).rect()).collect();
    for (pair, names) in rects.windows(2).zip(lines.windows(2)) {
        assert!(
            pair[0].bottom() <= pair[1].top(),
            "not top to bottom: {names:?} {pair:?}"
        );
    }
    let axis = rects[0].center().x;
    for (rect, line) in rects.iter().zip(lines) {
        assert!(
            (rect.center().x - axis).abs() < 1.0,
            "not centred: {line} {rect:?}, axis {axis}"
        );
    }
    // The icon, readable at 128 pt, with space between it and the title bar
    // (the window's one node of unknown role, between its Hide and Close
    // buttons).
    let icon = harness.get_by_role_and_label(Role::Image, lines[0]);
    assert_eq!(icon.rect().size(), egui::vec2(128.0, 128.0));
    let window = harness.get_by_role_and_label(Role::Window, "About ArchiCraft");
    let title_bar = window.get_by_role(Role::Unknown).rect();
    let close = window.get_by_label("Close window").rect();
    assert!(
        title_bar.y_range().contains(close.center().y),
        "{title_bar:?} {close:?}"
    );
    assert!(
        rects[0].top() - title_bar.bottom() >= 16.0,
        "title bar {title_bar:?}, icon {:?}",
        rects[0]
    );
    // A gap like an empty paragraph between the website and the licence.
    let line_gap = rects[4].top() - rects[3].bottom();
    let gap = rects[5].top() - rects[4].bottom();
    assert!(gap >= line_gap + 12.0, "gap {gap}, line gap {line_gap}");
}

/// Each link of the About window asks the platform to open its URL, which
/// the desktop app does in the browser (eframe's `links` feature, below).
#[test]
fn about_window_links_open_their_urls() {
    let mut harness = harness("about-links");
    open_about(&mut harness, &Rc::new(Localizer::english()));
    for (text, url) in ABOUT_LINKS {
        harness.get_by_label(text).click();
        harness.step();
        let opened: Vec<&str> = harness
            .output()
            .platform_output
            .commands
            .iter()
            .filter_map(|command| match command {
                egui::OutputCommand::OpenUrl(open) => Some(open.url.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(opened, [url], "{text}");
    }
}

/// Without eframe's `links` feature egui-winit only logs that it cannot
/// open a link, so the About window's links would do nothing.
#[test]
fn the_desktop_app_opens_links_in_the_browser() {
    let manifest = include_str!("../../../Cargo.toml");
    let start = manifest
        .find("\neframe = {")
        .expect("eframe in the workspace dependencies");
    let end = start + manifest[start..].find("] }").expect("the end of eframe's features");
    assert!(manifest[start..end].contains("\"links\""), "{}", &manifest[start..end]);
}

/// NFR-L10N-001: with the en-XA pseudo-locale installed, the Help menu title
/// and the About window come out wrapped in the pseudo markers, so they come
/// from the catalogue.
#[cfg(debug_assertions)]
#[test]
fn about_window_follows_the_pseudo_locale() {
    let mut harness = harness("about-pseudo");
    let pseudo = Rc::new(Localizer::pseudo());
    harness.state_mut().set_localizer(Rc::clone(&pseudo));
    harness.run_steps(2);
    assert!(harness.query_all_by_label("Help").next().is_none());
    open_about(&mut harness, &pseudo);
    let heading = i18n::with(Rc::clone(&pseudo), || {
        tr!("dlg-about-heading", version = shown_version())
    });
    assert!(
        heading.starts_with(i18n::pseudo::OPEN) && heading.ends_with(i18n::pseudo::CLOSE),
        "{heading}"
    );
    assert!(heading.contains(&shown_version()), "{heading}");
    harness.get_by_label(&heading);
    let title = i18n::with(Rc::clone(&pseudo), || tr!("dlg-about-title"));
    assert_eq!(title, "⟦Ábóút ÁrchíCráft⟧");
    assert_present(&harness, &title);
    // No English text is left in the window; the URLs stay as they are.
    for english in [
        format!("ArchiCraft {} (alpha)", shown_version()),
        "ArchiCraft app icon".to_owned(),
        "ArchiCraft is an open-source architectural CAD and BIM tool.".to_owned(),
        "Copyright © 2026 Jakob Brandstötter and Alexander Partsch".to_owned(),
        "ArchiCraft is open source, licensed under the Creative Commons Attribution 4.0 International licence \
         (CC BY 4.0)."
            .to_owned(),
        "To the extent possible, ArchiCraft is provided as is and as available, without warranties of any kind \
         (section 5 of the licence)."
            .to_owned(),
        ABOUT_LINKS[2].0.to_owned(),
        format!("Writes {} files.", archicraft_ifcx::WRITTEN_VERSION),
        "Works offline. Sends no data anywhere; crash reports stay on this computer.".to_owned(),
    ] {
        assert!(harness.query_all_by_label(&english).next().is_none(), "{english}");
    }
    for (text, _) in &ABOUT_LINKS[..2] {
        assert_present(&harness, text);
    }
}

/// A file that cannot be opened, because it does not exist or is larger than
/// the size limit (lowered here instead of writing 2 GB), shows the “Cannot
/// open the project” message instead of panicking (NFR-SEC-003, PRJ-008).
#[test]
fn unreadable_or_oversized_file_shows_cannot_open() {
    let dir = std::env::temp_dir().join(format!("archicraft-ui-limits-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let oversized = dir.join("oversized.ifcx");
    std::fs::write(&oversized, vec![b' '; 2048]).unwrap();
    for (path, reason) in [
        (dir.join("missing.ifcx"), "the file cannot be read"),
        (oversized, "the file is larger than the limit of 1 KB"),
    ] {
        let mut harness = harness("cannot-open");
        harness.state_mut().set_file_size_limit(1024);
        harness.state_mut().open_path(&path);
        harness.run_steps(2);
        harness.get_by_label("Cannot open the project");
        harness.get_by_label_contains(reason);
        harness.get_by_label("OK").click();
        harness.run_steps(2);
        assert!(harness.query_by_label("Cannot open the project").is_none());
    }
}

/// A damaged file opens, and the load report lists what was skipped or
/// repaired (PRJ-008): here a layer record that is not an object.
#[test]
fn a_damaged_file_opens_with_the_load_report() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../archicraft-ifcx/tests/corrupt/non-object-layer.ifcx");
    let mut harness = harness("load-report");
    harness.state_mut().open_path(&path);
    harness.run_steps(2);
    harness.get_by_label("The project was opened with problems");
    harness.get_by_label_contains("layer record 1 skipped");
    harness.get_by_label("OK").click();
    harness.run_steps(2);
    assert!(harness.query_by_label("The project was opened with problems").is_none());
}

/// PRJ-008, NFR-L10N-001: the load report and the problems panel show the
/// problems of a damaged file as sentences from the catalogue, the model's
/// reason rendered in place, and never a message key.
#[test]
fn load_problems_show_as_localized_text() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../archicraft-ifcx/tests/corrupt/invalid-wall.ifcx");
    let no_key_form = |harness: &Harness<'static, ArchiCraftApp>| {
        for prefix in ["model-", "ifcx-"] {
            let keys: Vec<String> = harness
                .query_all_by_label_contains(prefix)
                .map(|node| format!("{node:?}"))
                .collect();
            assert!(keys.is_empty(), "{keys:#?}");
        }
    };
    let expected = "element W-002 skipped: Thickness must be between";
    let mut harness = harness("load-report-text");
    harness.state_mut().open_path(&path);
    harness.run_steps(2);
    harness.get_by_label("The project was opened with problems");
    harness.get_by_label_contains(expected);
    no_key_form(&harness);
    harness.get_by_label("OK").click();
    harness.run_steps(2);

    press(&mut harness, Binding::primary(Key::P).shift(), OperatingSystem::Nix);
    harness.run_steps(2);
    harness.get_by_label_contains("from opening the file");
    harness.get_by_label_contains(expected);
    no_key_form(&harness);
}

/// UI-012, NFR-L10N-001: the problems panel shows each element without
/// geometry as its name and a sentence from the catalogue: the kind as its
/// noun, the model's reason for a rejected outline in place, and never a
/// message key.
#[test]
fn geometry_problems_show_as_localized_text() {
    let mut harness = harness("geometry-problem-text");
    {
        let ws = harness.state_mut().workspace();
        let project = &mut ws.doc.project;
        let storey = project.storeys.ground();
        let material = project.materials[0].id;
        let dummy =
            archicraft_model::kinds::Dummy::new(DVec2::ZERO, DVec2::ONE, material).into_element(project, storey, 0);
        // A wall without length, which the commands refuse, put into the
        // project directly so that its outline reaches the geometry.
        let mut wall = project.new_wall(storey, DVec2::new(2.0, 2.0), DVec2::new(2.0, 2.0), 0);
        wall.name = "Zero wall".to_owned();
        for element in [dummy, wall] {
            project.elements.insert(element.id, element);
        }
        project.revision += 1;
    }
    press(&mut harness, Binding::primary(Key::P).shift(), OperatingSystem::Nix);
    harness.run_steps(2);
    harness.get_by_label_contains("2 geometry");
    harness.get_by_label_contains(": elements of the kind “test element” have no geometry yet");
    harness.get_by_label_contains("Zero wall: the outline is invalid: A polygon needs at least three distinct points.");
    for prefix in ["model-", "geometry-"] {
        let keys: Vec<String> = harness
            .query_all_by_label_contains(prefix)
            .map(|node| format!("{node:?}"))
            .collect();
        assert!(keys.is_empty(), "{keys:#?}");
    }
}

/// NFR-REL-007, NFR-SEC-003: every fuzz regression and every corrupt fixture
/// of the IFCX crate goes through the application's open path without a
/// panic, and the application answers each with the load report or the
/// “Cannot open the project” message.
#[test]
fn malformed_files_open_with_a_report_or_a_message() {
    let ifcx_tests = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../archicraft-ifcx/tests");
    let mut files: Vec<_> = ["fuzz-regressions", "corrupt"]
        .iter()
        .flat_map(|dir| std::fs::read_dir(ifcx_tests.join(dir)).unwrap())
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .collect();
    files.sort();
    assert!(files.len() > 10, "{files:?}");
    let mut harness = harness("malformed-files");
    for path in files {
        harness.state_mut().open_path(&path);
        harness.run_steps(2);
        let answer = ["The project was opened with problems", "Cannot open the project"]
            .into_iter()
            .find(|label| harness.query_by_label(label).is_some());
        assert!(
            answer.is_some(),
            "{} opened without a report or a message",
            path.display()
        );
        harness.get_by_label("OK").click();
        harness.run_steps(2);
    }
}

/// IFX-008: the Preferences window offers indented, human-readable IFCX;
/// confirming it with OK makes the next save indented.
#[test]
fn preferences_offer_indented_ifcx_output() {
    let mut harness = harness("indented-ifcx");
    assert!(!harness.state_mut().workspace().prefs.indented_ifcx);
    harness.get_by_label("File").click();
    harness.run_steps(2);
    harness.get_by_label_contains("Preferences").click();
    // A new window takes clicks only once it has been laid out and shown.
    harness.run_steps(4);
    harness.get_by_label("Indented IFCX (human-readable)").click();
    harness.run_steps(2);
    harness.get_by_label("OK").click();
    harness.run_steps(2);
    assert!(harness.state_mut().workspace().prefs.indented_ifcx);

    let dir = std::env::temp_dir().join(format!("archicraft-ui-indented-save-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("indented.ifcx");
    assert!(harness.state_mut().save_to(&path));
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("{\n  \"header\": {\n"), "{}", &text[..40]);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// INP-009: in the plan view Tab cycles the wall tool's snap candidates
/// while nothing is typed: the view keeps the key from focus navigation,
/// the resolved point moves to the next candidate and no point is placed.
#[test]
fn tab_cycles_snap_candidates_in_the_plan_view() {
    let mut harness = harness("snap-cycle");
    harness.get_by_label("Wall").click();
    harness.run_steps(2);
    let pos = {
        let ws = harness.state_mut().workspace();
        let storey = ws.active_storey;
        let state = &ws.plan_views[&storey];
        let (rect, camera) = (state.last_rect, state.camera);
        let left = camera.to_world(rect, rect.center() - egui::vec2(3.0, 0.0));
        let right = camera.to_world(rect, rect.center() + egui::vec2(3.0, 0.0));
        let walls = vec![
            ws.doc.project.new_wall(storey, left - DVec2::new(2.0, 0.0), left, 0),
            ws.doc.project.new_wall(storey, right, right + DVec2::new(2.0, 0.0), 1),
        ];
        assert!(ws.execute(Command::AddElements(walls)));
        rect.center()
    };
    // A middle click activates the view without placing a point.
    harness.hover_at(pos);
    harness.run_steps(1);
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos,
            button: PointerButton::Middle,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run_steps(2);
    let before = harness.state_mut().workspace().input.last().cloned().unwrap();
    assert!(before.candidates.len() >= 2, "{:?}", before.candidates);
    assert_eq!(before.snap, before.candidates[0]);

    harness.key_press(Key::Tab);
    harness.run_steps(2);
    let ws = harness.state_mut().workspace();
    let after = ws.input.last().cloned().unwrap();
    assert_eq!(after.snap, before.candidates[1], "Tab chose the second candidate");
    assert!(ws.tool.points.is_empty());
    assert!(ws.input.typed.is_empty());
}

/// SET-001, INP-020: the Preferences window gathers snap radius, angle
/// increment and the input mode in its "Input" group; choosing
/// press-drag-release with OK applies it, and the status line names it.
#[test]
fn preferences_offer_the_input_mode_in_the_input_group() {
    let mut harness = harness("input-mode");
    assert_eq!(harness.state_mut().workspace().prefs.input_mode, InputMode::ClickClick);
    harness.get_by_label("File").click();
    harness.run_steps(2);
    harness.get_by_label_contains("Preferences").click();
    harness.run_steps(4);
    for label in [
        "Input",
        "Snap radius (points)",
        "Angle increment (°)",
        "Input mode",
        "Click-click",
        "Press-drag-release",
    ] {
        assert_present(&harness, label);
    }
    harness.get_by_label("Press-drag-release").click();
    harness.run_steps(2);
    harness.get_by_label("OK").click();
    harness.run_steps(2);
    assert_eq!(harness.state_mut().workspace().prefs.input_mode, InputMode::DragRelease);

    harness.get_by_label("Wall").click();
    harness.run_steps(2);
    assert_present(
        &harness,
        "Wall: click the start point of the reference line · R cycles the reference line \
         · 0–9 types the coordinates · P switches polar and Cartesian \
         · Press-drag-release: press at a point, drag and release at the next",
    );
}

/// INP-006: holding Shift while drawing a wall holds the cursor on the
/// constraint direction: the preview's end lies on the horizontal through
/// the start point, the label names the constrained point and the plan view
/// draws the constraint guide; releasing Shift frees the cursor again.
#[test]
fn shift_constrains_the_wall_preview() {
    let mut harness = harness("shift-constrains");
    harness.get_by_label("Wall").click();
    harness.run_steps(2);
    let (start, storey) = {
        let ws = harness.state_mut().workspace();
        ws.tool.wall_mode = WallMode::Single;
        let storey = ws.active_storey;
        (ws.plan_views[&storey].last_rect.center(), storey)
    };
    click_at(&mut harness, start);
    let first = harness.state_mut().workspace().tool.points[0];
    harness.event(Event::ModifiersChanged(Modifiers::SHIFT));
    harness.hover_at(start + egui::vec2(160.0, -14.0));
    harness.run_steps(2);
    let ws = harness.state_mut().workspace();
    let resolved = ws.input.last().cloned().expect("a resolved point");
    assert_eq!(resolved.snap.kind, SnapKind::Constrained);
    assert!(
        (resolved.point.y - first.y).abs() < 1e-9,
        "{} vs {first}",
        resolved.point
    );
    assert!(resolved.point.x > first.x);
    let state = &ws.plan_views[&storey];
    assert!(
        state.label.as_deref().is_some_and(|l| l.ends_with("constrained")),
        "{:?}",
        state.label
    );
    assert_eq!(state.guides.len(), 1);
    assert_eq!(state.guides[0].origin, GuideOrigin::Constraint);

    harness.event(Event::ModifiersChanged(Modifiers::NONE));
    harness.hover_at(start + egui::vec2(160.0, -15.0));
    harness.run_steps(2);
    let ws = harness.state_mut().workspace();
    assert_ne!(ws.input.last().map(|r| r.snap.kind), Some(SnapKind::Constrained));
    assert!(ws.plan_views[&storey].guides.is_empty());
}

/// INP-006, Appendix D.1: the Preferences window offers 15° and 45° next
/// to the angle increment, and the reference of the constraint
/// directions; the choices apply with OK.
#[test]
fn preferences_offer_quick_angle_increments_and_the_constraint_reference() {
    let mut harness = harness("angle-increment");
    harness.get_by_label("File").click();
    harness.run_steps(2);
    harness.get_by_label_contains("Preferences").click();
    harness.run_steps(4);
    for label in ["15°", "45°", "Constrain directions from", "X axis", "Grid orientation"] {
        assert_present(&harness, label);
    }
    harness.get_by_label("15°").click();
    harness.run_steps(2);
    harness.get_by_label("Grid orientation").click();
    harness.run_steps(2);
    harness.get_by_label("OK").click();
    harness.run_steps(2);
    let prefs = &harness.state_mut().workspace().prefs;
    assert_eq!(prefs.angle_increment_deg, 15.0);
    assert_eq!(prefs.constraint_reference, ConstraintReference::Frame);

    harness.get_by_label("File").click();
    harness.run_steps(2);
    harness.get_by_label_contains("Preferences").click();
    harness.run_steps(4);
    harness.get_by_label("45°").click();
    harness.run_steps(2);
    harness.get_by_label("OK").click();
    harness.run_steps(2);
    assert_eq!(harness.state_mut().workspace().prefs.angle_increment_deg, 45.0);
}

/// SET-001, INP-002: the Preferences window offers the tracker behaviour in
/// its "Input" group; turning on X/Y/Z with OK stores it, and the plan
/// view's tracker then adds the absolute X, Y and Z beside the distance and
/// angle from the previous point.
#[test]
fn preferences_turn_on_xyz_in_the_tracker() {
    let mut harness = harness("tracker-xyz");
    assert!(!harness.state_mut().workspace().prefs.tracker_xyz);
    harness.get_by_label("File").click();
    harness.run_steps(2);
    harness.get_by_label_contains("Preferences").click();
    harness.run_steps(4);
    assert_present(&harness, "Tracker");
    harness.get_by_label("Show X, Y and Z").click();
    harness.run_steps(2);
    harness.get_by_label("OK").click();
    harness.run_steps(2);
    assert!(harness.state_mut().workspace().prefs.tracker_xyz);

    harness.get_by_label("Wall").click();
    harness.run_steps(2);
    let (start, storey) = {
        let ws = harness.state_mut().workspace();
        ws.tool.wall_mode = WallMode::Single;
        let storey = ws.active_storey;
        (ws.plan_views[&storey].last_rect.center(), storey)
    };
    click_at(&mut harness, start);
    assert_eq!(
        harness.state_mut().workspace().tool.points.len(),
        1,
        "the click took the start point"
    );
    harness.hover_at(start + egui::vec2(40.0, -30.0));
    harness.run_steps(2);
    let ws = harness.state_mut().workspace();
    let tracker = ws.plan_views[&storey].tracker.clone().unwrap_or_default();
    let lines: Vec<&str> = tracker.lines().collect();
    assert_eq!(lines.len(), 5, "{tracker}");
    for (line, label) in lines.iter().zip(["▸d ", " a ", " X ", " Y ", " Z "]) {
        assert!(line.starts_with(label), "{tracker}");
    }
}

/// INP-020: with press-drag-release chosen, pressing in the plan view,
/// dragging 120 px and releasing draws a wall from the press to the release;
/// a plain click still takes one point.
#[test]
fn press_drag_release_draws_a_wall_in_the_plan_view() {
    let prefs = Preferences {
        input_mode: InputMode::DragRelease,
        ..Preferences::default()
    };
    let mut harness = harness_in(&data_dir("drag-release"), prefs, 1.0);
    harness.get_by_label("Wall").click();
    harness.run_steps(2);
    let (start, scale) = {
        let ws = harness.state_mut().workspace();
        ws.tool.wall_mode = WallMode::Single;
        let state = &ws.plan_views[&ws.active_storey];
        (state.last_rect.center(), state.camera.scale)
    };
    let end = start + egui::vec2(120.0, 0.0);
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
        harness.hover_at(start + egui::vec2(20.0 * step as f32, 0.0));
        harness.run_steps(1);
    }
    let ws = harness.state_mut().workspace();
    assert_eq!(ws.tool.points.len(), 1, "the press took the start point");
    assert!(ws.doc.project.elements.is_empty());
    harness.event(button(end, false));
    harness.run_steps(2);
    let ws = harness.state_mut().workspace();
    let walls: Vec<_> = ws.doc.project.elements.values().filter_map(|e| e.as_wall()).collect();
    assert_eq!(walls.len(), 1, "the release took the end point");
    let length = walls[0].start.distance(walls[0].end);
    // Both ends snap to the invisible grid (INP-013).
    let grid = ws.prefs.snap_step;
    assert!((length - 120.0 / scale).abs() <= grid, "{length} vs {}", 120.0 / scale);
    assert!(ws.tool.points.is_empty());

    click_at(&mut harness, start + egui::vec2(0.0, 80.0));
    assert_eq!(
        harness.state_mut().workspace().tool.points.len(),
        1,
        "a click takes one point"
    );
}

/// STR-009, NFR-L10N-003: the slope widget shows a pitch in the project's
/// slope format and commits any typed slope notation as a pitch angle.
#[test]
fn the_slope_widget_shows_the_slope_format_and_commits_typed_slopes() {
    use archicraft_app::ui::widgets::{self, Edited};
    use archicraft_model::units::{NumberFormat, SlopeFormat, UnitSettings};
    let units = UnitSettings {
        slope: SlopeFormat::Percent,
        ..UnitSettings::default()
    };
    let nf = NumberFormat::default();
    let mut h = Harness::new_ui_state(
        move |ui, pitch: &mut f64| {
            if let Edited::Changed(v) = widgets::slope(ui, egui::Id::new("slope"), Some(*pitch), &units, &nf, 120.0) {
                *pitch = v;
            }
        },
        30f64.to_radians(),
    );
    h.run();
    let field = |value: &'static str| By::new().role(Role::TextInput).value(value);
    h.get(field("57.74 %")).click();
    h.run();
    h.get(field("57.74 %")).type_text("6:12");
    h.key_press(Key::Enter);
    h.run();
    assert!((h.state() - 0.5f64.atan()).abs() < 1e-12, "{}", h.state());
    h.get(field("50.00 %"));
}

/// The view-control bar's readout of the cursor at `pos` in the plan, from
/// the user origin (INP-014).
fn readout_at(harness: &mut Harness<'static, ArchiCraftApp>, pos: egui::Pos2) -> String {
    harness.hover_at(pos);
    harness.run_steps(2);
    let ws = harness.state_mut().workspace();
    let cursor = ws.plan_views[&ws.active_storey]
        .cursor
        .expect("the pointer is over the plan");
    let p = ws.frames.user_origin.to_local(cursor);
    let units = &ws.doc.project.settings.units;
    let nf = ws.prefs.number_format();
    format!(
        "X {}  Y {}",
        format_length(p.x, units, &nf, true),
        format_length(p.y, units, &nf, true)
    )
}

/// INP-014, INP-004: the View menu lists 'Place user origin' and 'Reset user
/// origin'; placing it with two clicks in the plan, the origin and a point
/// on its X axis, changes the view-control bar's X/Y readout of the same
/// cursor position and leaves the model unchanged; resetting it brings the
/// readout from the project origin back.
#[test]
fn the_user_origin_changes_the_coordinate_readout() {
    let mut harness = harness("user-origin");
    let revision = harness.state_mut().workspace().doc.project.revision;
    let centre = {
        let ws = harness.state_mut().workspace();
        ws.plan_views[&ws.active_storey].last_rect.center()
    };
    let probe = centre + egui::vec2(-60.0, 45.0);
    let from_project = readout_at(&mut harness, probe);
    assert_present(&harness, &from_project);

    let open_view_menu = |harness: &mut Harness<'static, ArchiCraftApp>| {
        // The menu bar is above the browser, which has a "View" header too.
        harness
            .query_all_by_label("View")
            .min_by(|a, b| a.rect().min.y.total_cmp(&b.rect().min.y))
            .unwrap()
            .click();
        harness.run_steps(2);
    };
    open_view_menu(&mut harness);
    assert_present(&harness, "Place user origin");
    assert_present(&harness, "Reset user origin");
    harness.get_by_label("Place user origin").click();
    harness.run_steps(2);
    assert_present(
        &harness,
        "User origin: click the new origin · 0–9 types the coordinates · Esc cancels the step",
    );
    click_at(&mut harness, centre + egui::vec2(40.0, -30.0));
    assert_present(
        &harness,
        "User origin: click a point on the X axis or type its angle · Enter keeps the axes unrotated \
         · Shift constrains angles · Esc cancels the step",
    );
    click_at(&mut harness, centre + egui::vec2(40.0, -130.0));
    let frame = harness.state_mut().workspace().frames.user_origin;
    assert!(
        (frame.rotation - 90f64.to_radians()).abs() < 1e-9,
        "the X axis points up the screen: {frame:?}"
    );

    let from_user = readout_at(&mut harness, probe);
    assert_ne!(from_user, from_project);
    assert_present(&harness, &from_user);
    assert!(harness.query_all_by_label(&from_project).next().is_none());
    assert_eq!(harness.state_mut().workspace().doc.project.revision, revision);

    open_view_menu(&mut harness);
    harness.get_by_label("Reset user origin").click();
    harness.run_steps(2);
    assert_eq!(readout_at(&mut harness, probe), from_project);
    assert_present(&harness, &from_project);
}

/// Right-clicks at `pos` in the plan view.
fn right_click_at(harness: &mut Harness<'static, ArchiCraftApp>, pos: egui::Pos2) {
    harness.hover_at(pos);
    harness.run_steps(1);
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run_steps(2);
}

/// INP-014: the plan's context menu offers 'Place user origin' and, once one
/// is placed, 'Reset user origin'. While the origin is being placed, the
/// menu over a wall offers only the origin items and 'Cancel input': it
/// neither selects the wall nor offers to delete it, and 'Cancel input' goes
/// back to the tool.
#[test]
fn the_plan_context_menu_places_the_user_origin() {
    let mut harness = harness("user-origin-menu");
    let wall_at = {
        let ws = harness.state_mut().workspace();
        let storey = ws.active_storey;
        let wall = ws.doc.project.new_wall(storey, DVec2::ZERO, DVec2::new(4.0, 0.0), 0);
        assert!(ws.execute(Command::AddElements(vec![wall])));
        let state = &ws.plan_views[&storey];
        state.camera.to_screen(state.last_rect, DVec2::new(2.0, 0.0))
    };
    harness.run_steps(2);
    let empty = wall_at + egui::vec2(0.0, -120.0);

    right_click_at(&mut harness, empty);
    assert!(
        !harness.get_by_label("Place user origin").accesskit_node().is_disabled(),
        "placing is always offered"
    );
    assert!(
        harness.get_by_label("Reset user origin").accesskit_node().is_disabled(),
        "nothing to reset before an origin is placed"
    );
    harness.get_by_label("Place user origin").click();
    harness.run_steps(2);
    assert_present(
        &harness,
        "User origin: click the new origin · 0–9 types the coordinates · Esc cancels the step",
    );

    right_click_at(&mut harness, wall_at);
    assert_present(&harness, "Cancel input");
    assert!(harness.query_by_label("Delete").is_none());
    assert!(harness.state_mut().workspace().selection.is_empty());
    harness.get_by_label("Cancel input").click();
    harness.run_steps(2);
    let ws = harness.state_mut().workspace();
    assert_eq!(ws.active_consumer, Consumer::Tool);
    assert!(ws.selection.is_empty());
    assert_eq!(ws.tool.kind, ToolKind::Select);

    // Placed with two clicks, the origin can be reset from the menu.
    right_click_at(&mut harness, empty);
    harness.get_by_label("Place user origin").click();
    harness.run_steps(2);
    click_at(&mut harness, empty);
    click_at(&mut harness, empty + egui::vec2(60.0, 0.0));
    assert!(harness.state_mut().workspace().frames.user_origin.is_placed());
    right_click_at(&mut harness, empty);
    assert!(!harness.get_by_label("Reset user origin").accesskit_node().is_disabled());
    harness.get_by_label("Reset user origin").click();
    harness.run_steps(2);
    assert!(!harness.state_mut().workspace().frames.user_origin.is_placed());
}

/// INP-010: with the wall tool active the tool-mode row has a 'Division'
/// control: the mode, its value field and the scope. Choosing four equal
/// parts replaces the hovered wall's tick at its midpoint with three ticks
/// on the next frame, a value typed into the field divides it anew, and
/// the choices are kept in the preferences file.
#[test]
fn the_division_control_chooses_the_mode_its_value_and_the_scope() {
    let dir = data_dir("division-control");
    let mut harness = harness_in(&dir, Preferences::default(), 1.0);
    harness.get_by_label("Wall").click();
    harness.run_steps(2);
    let (storey, on_wall) = {
        let ws = harness.state_mut().workspace();
        let storey = ws.active_storey;
        let wall = ws.doc.project.new_wall(storey, DVec2::ZERO, DVec2::new(4.0, 0.0), 0);
        assert!(ws.execute(Command::AddElements(vec![wall])));
        let state = &ws.plan_views[&storey];
        (storey, state.camera.to_screen(state.last_rect, DVec2::new(1.3, 0.0)))
    };
    let ticks = |h: &mut Harness<'static, ArchiCraftApp>| {
        h.hover_at(on_wall);
        h.run_steps(2);
        h.state_mut().workspace().plan_views[&storey]
            .ticks
            .iter()
            .map(|p| (p.x * 1e6).round() / 1e6)
            .collect::<Vec<_>>()
    };
    assert_eq!(ticks(&mut harness), [2.0], "half by default");

    assert_present(&harness, &tr!("tool-division"));
    harness
        .get(By::new().role(Role::ComboBox).value(tr!("tool-division-half").as_str()))
        .click();
    harness.run_steps(2);
    harness.get_by_label(&tr!("tool-division-equal")).click();
    harness.run_steps(2);
    assert_eq!(
        harness.state_mut().workspace().prefs.input.division,
        DivisionMode::Equal(4)
    );
    assert_eq!(ticks(&mut harness), [1.0, 2.0, 3.0]);

    // The value field of the mode.
    harness.get(By::new().role(Role::SpinButton).value("4")).click();
    harness.run_steps(2);
    harness.event(Event::Text("8".into()));
    harness.run_steps(1);
    harness.key_press(Key::Enter);
    harness.run_steps(2);
    assert_eq!(
        harness.state_mut().workspace().prefs.input.division,
        DivisionMode::Equal(8)
    );
    assert_eq!(ticks(&mut harness), [0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5]);

    // The scope.
    harness
        .get(
            By::new()
                .role(Role::ComboBox)
                .value(tr!("tool-division-scope-nodes").as_str()),
        )
        .click();
    harness.run_steps(2);
    harness.get_by_label(&tr!("tool-division-scope-element")).click();
    harness.run_steps(2);
    let ws = harness.state_mut().workspace();
    assert_eq!(ws.prefs.input.scope, DivisionScope::WholeElement);

    // Kept in the preferences file.
    let saved = Preferences::load(&DataDirs::at(&dir).preferences_file());
    assert_eq!(saved.input.division, DivisionMode::Equal(8));
    assert_eq!(saved.input.scope, DivisionScope::WholeElement);
}

/// The text field that shows `value`.
fn text_field(value: &str) -> By<'_> {
    By::new().role(Role::TextInput).value(value)
}

/// INP-010: the value field follows the mode: none for half, a percentage
/// for a percentage and a length in the project's units for the two
/// distance modes, which keep the distance when switching between them.
#[test]
fn the_division_value_field_follows_the_mode() {
    let mut harness = harness("division-value");
    harness.get_by_label("Wall").click();
    harness.run_steps(2);
    let choose = |h: &mut Harness<'static, ArchiCraftApp>, from: &str, to: &str| {
        h.get(By::new().role(Role::ComboBox).value(tr!(from).as_str())).click();
        h.run_steps(2);
        h.get_by_label(&tr!(to)).click();
        h.run_steps(2);
        h.state_mut().workspace().prefs.input.division
    };
    assert!(harness.query_by_label(&tr!("tool-division-percent-suffix")).is_none());
    assert_eq!(
        choose(&mut harness, "tool-division-half", "tool-division-percent"),
        DivisionMode::Percent(50.0)
    );
    harness.get(By::new().role(Role::SpinButton).value("50"));
    harness.get_by_label(&tr!("tool-division-percent-suffix"));

    assert_eq!(
        choose(&mut harness, "tool-division-percent", "tool-division-repeated"),
        DivisionMode::Repeated(1.0)
    );
    let shown = {
        let ws = harness.state_mut().workspace();
        let nf = ws.prefs.number_format();
        (
            format_length(1.0, &ws.doc.project.settings.units, &nf, false),
            format_length(0.6, &ws.doc.project.settings.units, &nf, false),
        )
    };
    harness.get(text_field(&shown.0)).click();
    harness.run_steps(1);
    harness.get(text_field(&shown.0)).type_text(&shown.1);
    harness.key_press(Key::Enter);
    harness.run_steps(2);
    assert_eq!(
        harness.state_mut().workspace().prefs.input.division,
        DivisionMode::Repeated(0.6)
    );
    assert_eq!(
        choose(&mut harness, "tool-division-repeated", "tool-division-best-equal"),
        DivisionMode::BestEqualNear(0.6)
    );
    harness.get(text_field(&shown.1));
    assert_eq!(
        choose(&mut harness, "tool-division-best-equal", "tool-division-half"),
        DivisionMode::Half
    );
    assert!(harness.query(text_field(&shown.1)).is_none());
}
