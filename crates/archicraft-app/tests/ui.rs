//! UI tests without a GPU: widgets are found by their accessible labels
//! (AccessKit), as the technology choices plan for egui_kittest.

#![allow(clippy::unwrap_used)]

mod common;

use std::rc::Rc;

use archicraft_app::i18n::{self, Localizer};
use archicraft_app::input::{Field, InputConsumer, InputMode};
use archicraft_app::persistence::prefs::Preferences;
use archicraft_app::tools::{ToolKind, WallMode};
use archicraft_app::{ArchiCraftApp, tr};
use archicraft_model::glam::DVec2;
use archicraft_model::units::{AngleUnit, LengthUnit, ParseError, format_length};
use archicraft_model::validate::{MAX_THICKNESS, MIN_THICKNESS};
use archicraft_model::{Command, ProjectSettings};
use common::{assert_present, data_dir, harness, harness_in};
use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, PointerButton};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, Queryable};

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

/// SRS 3.2: an element of a kind without a panel of its own (the model's
/// test kind) shows the common fields only, and its missing geometry is the
/// only problem listed.
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
    for common in ["Locked", "Home storey"] {
        harness.get_by_label(common);
    }
    for specific in ["Thickness", "Reference line", "Length", "Area"] {
        assert!(harness.query_by_label(specific).is_none(), "{specific} is shown");
    }
    assert_eq!(harness.state_mut().workspace().doc.geometry().problems.len(), 1);
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
    // Menu titles and command labels are migrated with archicraft-e19 and
    // archicraft-5eu.
    open_menu(&mut harness, "Project");
    harness.get_by_label("Project settings…").click();
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
/// renders.
fn open_about(harness: &mut Harness<'static, ArchiCraftApp>, localizer: &Rc<Localizer>) {
    let help = i18n::with(Rc::clone(localizer), || tr!("menu-help"));
    harness.get_by_label(&help).click();
    harness.run_steps(2);
    // Command labels are migrated with the command registry (archicraft-5eu).
    harness.get_by_label("About ArchiCraft").click();
    harness.run_steps(2);
}

#[test]
fn about_window_text_comes_from_the_catalogue() {
    // NFR-L10N-001: the pilot migration of the About window and Help menu.
    let mut harness = harness("about");
    let english = Rc::new(Localizer::english());
    open_about(&mut harness, &english);
    let heading = i18n::with(Rc::clone(&english), || {
        tr!("dlg-about-heading", version = env!("CARGO_PKG_VERSION"))
    });
    assert_eq!(heading, format!("ArchiCraft {}", env!("CARGO_PKG_VERSION")));
    harness.get_by_label(&heading);
    harness.get_by_label("A desktop modeller for buildings that saves IFCX (IFC5).");
    harness.get_by_label(&format!("Writes {} files.", archicraft_ifcx::WRITTEN_VERSION));
    harness.get_by_label("Works offline. Sends no data anywhere; crash reports stay on this computer.");
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
        tr!("dlg-about-heading", version = env!("CARGO_PKG_VERSION"))
    });
    assert!(
        heading.starts_with(i18n::pseudo::OPEN) && heading.ends_with(i18n::pseudo::CLOSE),
        "{heading}"
    );
    assert!(heading.contains(env!("CARGO_PKG_VERSION")), "{heading}");
    harness.get_by_label(&heading);
    let title = i18n::with(Rc::clone(&pseudo), || tr!("dlg-about-title"));
    assert_eq!(title, "⟦Ábóút ÁrchíCráft⟧");
    assert_present(&harness, &title);
    // No English text is left in the window.
    for english in [
        format!("ArchiCraft {}", env!("CARGO_PKG_VERSION")),
        "A desktop modeller for buildings that saves IFCX (IFC5).".to_owned(),
    ] {
        assert!(harness.query_all_by_label(&english).next().is_none(), "{english}");
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
