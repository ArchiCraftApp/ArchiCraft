//! UI tests without a GPU: widgets are found by their accessible labels
//! (AccessKit), as the technology choices plan for egui_kittest.

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use archicraft_app::ArchiCraftApp;
use archicraft_app::persistence::DataDirs;
use archicraft_app::persistence::crash::RecentLog;
use archicraft_app::persistence::prefs::Preferences;
use archicraft_app::views::tools::ToolKind;
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
