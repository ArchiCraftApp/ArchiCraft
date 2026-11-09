//! Spaces in the application (SPC-001): the boundary and the stamp in the
//! plan view, picking a space by its stamp, and the properties of a selected
//! space.

#![allow(clippy::unwrap_used)]

mod common;

use archicraft_app::ArchiCraftApp;
use archicraft_app::workspace::Request;
use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::WriteOptions;
use archicraft_model::glam::DVec2;
use archicraft_model::units::format_area;
use archicraft_model::{Command, DrawingScale, ElementId, samples};
use common::{data_dir, harness};
use egui::accesskit::Role;
use egui::{Event, Modifiers, PointerButton};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, Queryable};

type App = Harness<'static, ArchiCraftApp>;

/// Opens two_storey_shell from an IFCX file under `name` and draws the
/// space "Kitchen", number "0.01", 3 m x 3 m in the room west of the
/// interior wall of the ground storey; returns its id.
fn shell_with_kitchen(name: &str) -> (App, ElementId) {
    let project = samples::two_storey_shell();
    let geometry = ModelGeometry::evaluate(&project);
    let options = WriteOptions {
        author: "spaces test".into(),
        timestamp: "2026-11-09T00:00:00Z".into(),
        data_version: 1,
    };
    let (bytes, _) = archicraft_ifcx::save(&project, &geometry, &options, false);
    let dir = data_dir(name);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("two-storey-shell.ifcx");
    std::fs::write(&path, bytes).unwrap();
    let mut harness = harness(&format!("{name}-app"));
    harness.state_mut().open_path(&path);
    harness.run_steps(3);
    let ws = harness.state_mut().workspace();
    let ground = ws.doc.project.storeys.ground();
    let square = vec![
        DVec2::new(1.0, 1.0),
        DVec2::new(4.0, 1.0),
        DVec2::new(4.0, 4.0),
        DVec2::new(1.0, 4.0),
    ];
    let mut space = ws.doc.project.new_space(ground, square, Vec::new(), 0);
    let fields = space.as_space_mut().unwrap();
    fields.long_name = "Kitchen".to_owned();
    fields.number = "0.01".to_owned();
    let id = space.id;
    assert!(ws.execute(Command::AddElements(vec![space])));
    assert_eq!(ws.active_storey, ground);
    harness.run_steps(3);
    (harness, id)
}

/// The stamp of `space` drawn last frame in the ground storey's plan: its
/// text size in points, its lines and its centre on screen.
fn stamp(harness: &mut App, space: ElementId) -> (f32, Vec<String>, egui::Pos2) {
    let ws = harness.state_mut().workspace();
    let state = &ws.plan_views[&ws.active_storey];
    let (_, text) = state.stamps.iter().find(|(id, _)| *id == space).unwrap();
    let lines = text.lines.iter().map(|l| l.galley.text().to_owned()).collect();
    let [a, _, c, _] = text.quad;
    (text.size, lines, a + (c - a) / 2.0)
}

fn choose_scale(harness: &mut App, scale: &str) {
    harness
        .get(By::new().role(Role::ComboBox).label("Drawing scale"))
        .click();
    harness.run_steps(2);
    harness.get_by_label(scale).click();
    harness.run_steps(3);
}

/// SPC-001, VIEW-013: the plan shows the space's stamp centred on its
/// anchor at 2.5 mm paper size at the view's drawing scale, one line each
/// for name, number and area in the project's area unit and the locale's
/// number format; at 1:50 the stamp is half as high on screen as at 1:100
/// with the same zoom (2.5 mm stand for half the model length).
#[test]
fn the_plan_shows_the_stamp_at_paper_size() {
    let (mut harness, space) = shell_with_kitchen("space-stamp");
    let (size, lines, centre) = stamp(&mut harness, space);
    let area = {
        let ws = harness.state_mut().workspace();
        format_area(9.0, &ws.doc.project.settings.units, &ws.prefs.number_format())
    };
    assert_eq!(lines, ["Kitchen", "0.01", area.as_str()]);
    assert!(area.ends_with(" m²"), "the default metric template's unit: {area}");
    let (points_per_metre, anchor) = {
        let ws = harness.state_mut().workspace();
        let state = &ws.plan_views[&ws.active_storey];
        assert_eq!(state.scale, DrawingScale::default());
        let plan = ws.doc.plan(ws.active_storey);
        let anchor = plan.stamp(space).unwrap().anchor;
        (state.camera.scale, state.camera.to_screen(state.last_rect, anchor))
    };
    let expected = DrawingScale::default().paper_mm_to_model(2.5) * points_per_metre;
    assert!((f64::from(size) - expected).abs() < 1e-3, "{size} is not {expected}");
    assert!(centre.distance(anchor) < 0.5, "{centre:?} is not on {anchor:?}");

    choose_scale(&mut harness, "1:50");
    let (half, ..) = stamp(&mut harness, space);
    assert_eq!(
        harness
            .state_mut()
            .workspace()
            .plan_views
            .values()
            .next()
            .unwrap()
            .camera
            .scale,
        points_per_metre,
        "the zoom stays"
    );
    assert!((half - size / 2.0).abs() < 1e-3, "{half} is not half of {size}");
    choose_scale(&mut harness, "1:100");
    assert!((stamp(&mut harness, space).0 - size).abs() < 1e-3);
}

/// SPC-001, SEL-001: clicking the stamp selects the space, though no line of
/// the plan lies under it.
#[test]
fn clicking_the_stamp_selects_the_space() {
    let (mut harness, space) = shell_with_kitchen("space-pick");
    harness.state_mut().workspace().selection.clear();
    let (_, _, centre) = stamp(&mut harness, space);
    harness.hover_at(centre);
    harness.run_steps(2);
    assert_eq!(harness.state_mut().workspace().hover, Some(space));
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos: centre,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run_steps(2);
    let ws = harness.state_mut().workspace();
    assert_eq!(ws.selection.iter().copied().collect::<Vec<_>>(), [space]);
}

/// SPC-001, UI-003: a selected space has no material, so neither the
/// properties strip nor the settings dialog shows a Material row, an empty
/// "Model and appearance" section or the note that cut fills come from the
/// material.
#[test]
fn a_selected_space_shows_no_material_settings() {
    let (mut harness, space) = shell_with_kitchen("space-properties");
    let ws = harness.state_mut().workspace();
    ws.selection.clear();
    ws.selection.insert(space);
    ws.requests.push(Request::Settings);
    harness.run_steps(3);
    harness.get_by_label("Geometry and positioning");
    for absent in ["Material", "Model and appearance", "Cut fills come from the material."] {
        assert!(harness.query_by_label(absent).is_none(), "{absent} is shown");
    }
}
