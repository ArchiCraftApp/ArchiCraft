//! UI tests without a GPU: widgets are found by their accessible labels
//! (AccessKit), as the technology choices plan for egui_kittest.

#![allow(clippy::unwrap_used)]

mod common;

use std::rc::Rc;

use archicraft_app::i18n::{self, Localizer};
use archicraft_app::tools::ToolKind;
use archicraft_app::{ArchiCraftApp, tr};
use archicraft_model::Command;
use archicraft_model::glam::DVec2;
use common::{assert_present, harness};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

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
