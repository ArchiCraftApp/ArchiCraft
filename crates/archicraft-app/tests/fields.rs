//! The properties strip and the settings dialog render every kind from its
//! declared FieldSpecs (UI-002, UI-003, ELM-001): no per-kind code in the
//! panel, the kind's fields shown for a selection and for the tool
//! defaults alike.

#![allow(clippy::unwrap_used)]

mod common;

use archicraft_app::ArchiCraftApp;
use archicraft_app::workspace::Request;
use archicraft_model::glam::DVec2;
use archicraft_model::units::format_length;
use archicraft_model::{Command, ElementKind, KindTag, TopLink};
use common::{data_dir, harness, open_strip_overflow, strip_overflow_rect};
use egui::Key;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{By, Queryable};

type App = Harness<'static, ArchiCraftApp>;

/// UI-002, UI-003, ELM-001: the panel code holds no per-kind code; every
/// kind's fields come from its descriptor.
#[test]
fn the_panel_code_has_no_per_kind_branches() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui");
    for file in ["properties.rs", "fields.rs"] {
        let source = std::fs::read_to_string(dir.join(file)).unwrap();
        for pattern in ["ElementKind::", "as_wall", "as_slab"] {
            assert!(!source.contains(pattern), "{file} contains {pattern}");
        }
    }
}

/// UI-002: the strip shows a selected wall's and a selected slab's
/// declared fields with today's labels, in its row or its overflow popup.
#[test]
fn the_strip_shows_the_wall_and_slab_fields() {
    let mut harness = harness("fields-wall-slab");
    let (wall, slab) = {
        let ws = harness.state_mut().workspace();
        let storey = ws.doc.project.storeys.ground();
        let wall = ws.doc.project.new_wall(storey, DVec2::ZERO, DVec2::new(4.0, 0.0), 0);
        let outline = vec![DVec2::new(0.0, 1.0), DVec2::new(4.0, 1.0), DVec2::new(4.0, 4.0)];
        let slab = ws.doc.project.new_slab(storey, outline, Vec::new(), 0);
        let ids = (wall.id, slab.id);
        assert!(ws.execute(Command::AddElements(vec![wall, slab])));
        ids
    };
    let wall_labels = [
        "Thickness",
        "Reference line",
        "Base offset",
        "Top",
        "Material",
        "Layer",
        "IFC class",
        "Position",
        "Structural function",
    ];
    for (id, labels) in [
        (wall, &wall_labels[..]),
        (slab, &["Thickness", "Reference plane", "Elevation offset"]),
    ] {
        let ws = harness.state_mut().workspace();
        ws.selection.clear();
        ws.selection.insert(id);
        harness.run_steps(2);
        open_strip_overflow(&mut harness);
        let strip = panel(&harness, "properties");
        let overflow = strip_overflow_rect(&harness).unwrap_or(egui::Rect::NOTHING);
        for label in labels {
            assert!(
                harness
                    .query_all_by_label(label)
                    .any(|n| strip.contains(n.rect().center()) || overflow.contains(n.rect().center())),
                "{label} is in the strip or its overflow popup"
            );
        }
    }
}

/// UI-002, EDT-017: a wall and a slab of different thicknesses show one
/// Thickness field with a mixed value, and a thickness typed into it is one
/// UpdateElements that changes both, each through its own kind's spec.
#[test]
fn a_shared_field_edits_every_kind_that_has_it() {
    let mut harness = harness("fields-shared-key");
    let (wall, slab, typed) = {
        let ws = harness.state_mut().workspace();
        let storey = ws.doc.project.storeys.ground();
        let mut wall = ws.doc.project.new_wall(storey, DVec2::ZERO, DVec2::new(4.0, 0.0), 0);
        wall.as_wall_mut().unwrap().thickness = 0.2;
        let outline = vec![DVec2::new(0.0, 1.0), DVec2::new(4.0, 1.0), DVec2::new(4.0, 4.0)];
        let mut slab = ws.doc.project.new_slab(storey, outline, Vec::new(), 0);
        slab.as_slab_mut().unwrap().thickness = 0.25;
        let ids = (wall.id, slab.id);
        assert!(ws.execute(Command::AddElements(vec![wall, slab])));
        ws.selection.extend([ids.0, ids.1]);
        let units = ws.doc.project.settings.units.clone();
        let typed = format_length(0.3, &units, &ws.prefs.number_format(), false);
        (ids.0, ids.1, typed)
    };
    harness.run_steps(2);
    let field = || By::new().role(Role::TextInput).label("Thickness");
    assert_eq!(harness.get(field()).value().as_deref(), Some(""), "a mixed thickness");
    let applied = harness.state_mut().workspace().doc.history.applied_commands().count();
    harness.get(field()).click();
    harness.run_steps(1);
    harness.get(field()).type_text(&typed);
    harness.key_press(Key::Enter);
    harness.run_steps(2);
    let ws = harness.state_mut().workspace();
    let commands: Vec<Command> = ws.doc.history.applied_commands().skip(applied).cloned().collect();
    assert!(
        matches!(commands.as_slice(), [Command::UpdateElements(e)] if e.len() == 2),
        "{commands:?}"
    );
    let project = &ws.doc.project;
    assert!((project.elements[&wall].as_wall().unwrap().thickness - 0.3).abs() < 1e-9);
    assert!((project.elements[&slab].as_slab().unwrap().thickness - 0.3).abs() < 1e-9);
}

/// MEA-003, UI-003: the settings dialog shows a selected wall's length and
/// volume at the end of its 'Geometry and positioning' section.
#[test]
fn the_dialog_shows_the_derived_values_with_the_geometry() {
    let mut harness = harness("fields-derived");
    {
        let ws = harness.state_mut().workspace();
        let storey = ws.doc.project.storeys.ground();
        let wall = ws.doc.project.new_wall(storey, DVec2::ZERO, DVec2::new(4.0, 0.0), 0);
        let id = wall.id;
        assert!(ws.execute(Command::AddElements(vec![wall])));
        ws.selection.insert(id);
        ws.requests.push(Request::Settings);
    }
    harness.run_steps(4);
    let strip = panel(&harness, "properties");
    let in_dialog = |label: &str| {
        harness
            .query_all_by_label(label)
            .map(|n| n.rect())
            .find(|r| !strip.contains(r.center()))
            .unwrap_or_else(|| panic!("{label} is in the dialog"))
    };
    let geometry = in_dialog("Geometry and positioning");
    let top_offset = in_dialog("Top offset");
    let material = in_dialog("Model and appearance");
    for label in ["Length", "Volume"] {
        let row = in_dialog(label);
        assert!(
            row.min.y > top_offset.min.y && row.min.y > geometry.min.y && row.max.y < material.min.y,
            "{label} ends the geometry section"
        );
    }
}

/// UI-002, UI-003: a selected element of the test kind, which the panel
/// code does not know, shows its descriptor's base and top fields in the
/// strip and in the settings dialog, and a height typed into the strip is
/// one UpdateElements of that element.
#[test]
fn a_dummy_shows_its_declared_fields_and_edits_them() {
    let mut harness = harness("fields-dummy");
    let (id, shown, typed) = {
        let ws = harness.state_mut().workspace();
        let storey = ws.doc.project.storeys.ground();
        let material = ws.doc.project.materials[0].id;
        let element = archicraft_model::kinds::Dummy::new(DVec2::ZERO, DVec2::ONE, material).into_element(
            &ws.doc.project,
            storey,
            0,
        );
        let id = element.id;
        assert!(ws.execute(Command::AddElements(vec![element])));
        ws.selection.insert(id);
        let units = ws.doc.project.settings.units.clone();
        let nf = ws.prefs.number_format();
        (
            id,
            format_length(1.0, &units, &nf, false),
            format_length(2.0, &units, &nf, false),
        )
    };
    harness.run_steps(2);
    let strip = panel(&harness, "properties");
    for label in ["Base offset", "Top", "Height"] {
        assert!(
            harness
                .query_all_by_label(label)
                .any(|n| strip.contains(n.rect().center())),
            "{label} is in the strip"
        );
    }

    let applied = harness.state_mut().workspace().doc.history.applied_commands().count();
    let height = || By::new().role(Role::TextInput).value(shown.as_str());
    harness
        .query_all(height())
        .find(|n| strip.contains(n.rect().center()))
        .expect("the height field in the strip")
        .click();
    harness.run_steps(1);
    harness
        .query_all(height())
        .find(|n| n.is_focused())
        .expect("the focused height field")
        .type_text(&typed);
    harness.key_press(Key::Enter);
    harness.run_steps(2);
    let ws = harness.state_mut().workspace();
    let commands: Vec<Command> = ws.doc.history.applied_commands().skip(applied).cloned().collect();
    assert!(
        matches!(commands.as_slice(), [Command::UpdateElements(e)] if e.len() == 1 && e[0].id == id),
        "{commands:?}"
    );
    let ElementKind::Dummy(dummy) = &ws.doc.project.elements[&id].kind else {
        panic!("still a dummy");
    };
    assert_eq!(dummy.link.top, TopLink::Fixed { height: 2.0 });

    ws.requests.push(Request::Settings);
    harness.run_steps(4);
    for label in ["Base offset", "Top", "Height"] {
        assert!(
            harness
                .query_all_by_label(label)
                .any(|n| !strip.contains(n.rect().center())),
            "{label} is in the settings dialog"
        );
    }
}

/// ELM-001: with the wall tool active and nothing selected, a thickness
/// typed into the strip is one SetToolDefaults that changes the wall slot
/// only; one undo restores it.
#[test]
fn a_thickness_for_the_wall_tool_is_one_set_tool_defaults() {
    let mut harness = harness("fields-defaults");
    let (before, applied) = select_wall_tool(&mut harness);
    type_thickness(&mut harness, 0.3);
    let ws = harness.state_mut().workspace();
    let commands: Vec<Command> = ws.doc.history.applied_commands().skip(applied).cloned().collect();
    assert!(
        matches!(commands.as_slice(), [Command::SetToolDefaults(_)]),
        "{commands:?}"
    );
    let defaults = &ws.doc.project.tool_defaults;
    assert!((wall_thickness(defaults.get(KindTag::Wall).map(|s| &s.kind)) - 0.3).abs() < 1e-9);
    assert_eq!(
        defaults.get(KindTag::Slab),
        before.get(KindTag::Slab),
        "the slab slot is unchanged"
    );
    ws.undo();
    assert_eq!(ws.doc.project.tool_defaults, before);
}

/// ELM-001, UI-002: a wall default edited through its FieldSpec is written
/// to the project's tool defaults, survives saving and reopening as IFCX
/// and is applied to the next wall made through `Project::new_element`.
#[test]
fn a_wall_default_from_the_strip_survives_an_ifcx_round_trip() {
    let mut app = harness("fields-defaults-roundtrip");
    select_wall_tool(&mut app);
    type_thickness(&mut app, 0.3);
    let path = data_dir("fields-defaults-roundtrip-file").join("defaults.ifcx");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    assert!(app.state_mut().save_to(&path));

    let mut reopened = harness("fields-defaults-reopened");
    reopened.state_mut().open_path(&path);
    reopened.run_steps(3);
    let project = &reopened.state_mut().workspace().doc.project;
    let slot = project.tool_defaults.get(KindTag::Wall).map(|s| &s.kind);
    assert!((wall_thickness(slot) - 0.3).abs() < 1e-9, "the reopened slot");
    let wall = project.new_element(KindTag::Wall, project.storeys.ground(), 0).unwrap();
    assert!((wall_thickness(Some(&wall.kind)) - 0.3).abs() < 1e-9, "the next wall");
}

/// Activates the wall tool with nothing selected; returns the tool
/// defaults and the number of applied commands before any edit.
fn select_wall_tool(harness: &mut App) -> (archicraft_model::project::ToolDefaults, usize) {
    harness.get_by_label("Wall").click();
    harness.run_steps(2);
    harness.get_by_label("Wall tool defaults");
    let ws = harness.state_mut().workspace();
    (
        ws.doc.project.tool_defaults.clone(),
        ws.doc.history.applied_commands().count(),
    )
}

/// Types `thickness` into the strip's Thickness field of the wall tool.
fn type_thickness(harness: &mut App, thickness: f64) {
    let (shown, typed) = {
        let ws = harness.state_mut().workspace();
        let slot = ws.doc.project.tool_defaults.get(KindTag::Wall).map(|s| &s.kind);
        let units = ws.doc.project.settings.units.clone();
        let nf = ws.prefs.number_format();
        (
            format_length(wall_thickness(slot), &units, &nf, false),
            format_length(thickness, &units, &nf, false),
        )
    };
    let field = || By::new().role(Role::TextInput).value(shown.as_str());
    harness.get(field()).click();
    harness.run_steps(1);
    harness.get(field()).type_text(&typed);
    harness.key_press(Key::Enter);
    harness.run_steps(2);
}

fn wall_thickness(kind: Option<&ElementKind>) -> f64 {
    let Some(ElementKind::Wall(wall)) = kind else {
        panic!("a wall: {kind:?}");
    };
    wall.thickness
}

/// The outer rectangle of the panel `name`.
fn panel(harness: &App, name: &str) -> egui::Rect {
    egui::containers::panel::PanelState::load(&harness.ctx, egui::Id::new(name))
        .unwrap()
        .outer_rect
}
