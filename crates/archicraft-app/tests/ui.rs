//! UI tests without a GPU: widgets are found by their accessible labels
//! (AccessKit), as the technology choices plan for egui_kittest.

#![allow(clippy::unwrap_used)]

mod common;

use std::rc::Rc;

use archicraft_app::i18n::{self, Localizer};
use archicraft_app::input::InputConsumer;
use archicraft_app::tools::{ToolKind, WallMode};
use archicraft_app::{ArchiCraftApp, tr};
use archicraft_model::glam::DVec2;
use archicraft_model::units::{LengthUnit, format_length};
use archicraft_model::validate::{MAX_THICKNESS, MIN_THICKNESS};
use archicraft_model::{Command, ProjectSettings};
use common::{assert_present, harness};
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
        "Wall: click the start point of the reference line · R cycles the reference line",
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
        "Wall chain: click the next point or type a distance · Tab switches between distance and angle \
         · Shift constrains angles · Enter or double-click ends the chain · Backspace removes the last point \
         · Esc cancels the step",
    );
    let ws = harness.state_mut().workspace();
    assert_eq!(
        ws.tool.step().unwrap().status_line(),
        "Wall chain: click the next point or type a distance · Tab switches between distance and angle \
         · Shift constrains angles · Enter or double-click ends the chain · Backspace removes the last point \
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
        (ws.tool.points.len(), ws.input.typed.distance.clone(), ws.tool.kind)
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
        "Rotated wall rectangle: click the end of the base edge or type a distance · Tab switches between \
         distance and angle · Shift constrains angles · Backspace removes the last point · Esc cancels the step",
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
