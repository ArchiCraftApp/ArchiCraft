//! UI tests without a GPU: widgets are found by their accessible labels
//! (AccessKit), as the technology choices plan for egui_kittest.

#![allow(clippy::unwrap_used)]

use std::rc::Rc;
use std::sync::Arc;

use archicraft_app::i18n::{self, Localizer};
use archicraft_app::persistence::DataDirs;
use archicraft_app::persistence::crash::RecentLog;
use archicraft_app::persistence::prefs::Preferences;
use archicraft_app::views::tools::ToolKind;
use archicraft_app::{ArchiCraftApp, tr};
use archicraft_model::Command;
use archicraft_model::glam::DVec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

fn harness(name: &str) -> Harness<'static, ArchiCraftApp> {
    let dir = std::env::temp_dir().join(format!("archicraft-ui-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let app = ArchiCraftApp::new(
        DataDirs::at(&dir),
        Preferences::default(),
        Arc::new(RecentLog::default()),
        None,
    );
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .build_eframe(move |_| app);
    harness.run_steps(3);
    harness
}

fn assert_present(harness: &Harness<'static, ArchiCraftApp>, label: &str) {
    assert!(
        harness.query_all_by_label(label).next().is_some(),
        "“{label}” is missing"
    );
}

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
