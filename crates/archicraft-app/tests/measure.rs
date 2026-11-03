//! The measure tool in the application (MEA-001, UI-001, UI-011): its
//! shortcut, toolbox and toolbar buttons, the readout in the status line and
//! the measure lines of the plan view.

#![allow(clippy::unwrap_used)]

mod common;

use archicraft_app::ArchiCraftApp;
use archicraft_app::tools::{MeasureMode, Measurement, ToolKind};
use archicraft_model::units::format_angle;
use common::harness;
use egui::accesskit::{Role, Toggled};
use egui::{Event, Key, Modifiers, PointerButton};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, NodeT, Queryable};

/// The label of the measure tool's toolbar button.
const TOOLBAR_LABEL: &str = "Measure tool";

fn tool(harness: &mut Harness<'static, ArchiCraftApp>) -> ToolKind {
    harness.state_mut().workspace().tool.kind
}

/// Selects the selection tool, as V does.
fn reset(harness: &mut Harness<'static, ArchiCraftApp>) {
    harness.state_mut().workspace().select_tool(ToolKind::Select);
    harness.run_steps(2);
}

/// Whether the toolbar's measure button shows as selected.
fn toolbar_selected(harness: &Harness<'static, ArchiCraftApp>) -> bool {
    let button = harness.get(By::new().role(Role::Button).label(TOOLBAR_LABEL));
    button.accesskit_node().toggled() == Some(Toggled::True)
}

/// The centre of the active plan view.
fn plan_centre(harness: &mut Harness<'static, ArchiCraftApp>) -> egui::Pos2 {
    let ws = harness.state_mut().workspace();
    ws.plan_views[&ws.active_storey].last_rect.center()
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

/// The readout the status line shows for the measurement now.
fn readout(harness: &mut Harness<'static, ArchiCraftApp>) -> String {
    let ws = harness.state_mut().workspace();
    let pairs = ws
        .tool
        .measure
        .readout(&ws.doc.project.settings.units, &ws.prefs.number_format());
    pairs
        .into_iter()
        .map(|(label, value)| format!("{label} {value}"))
        .collect::<Vec<_>>()
        .join("  ")
}

/// The measure lines the active plan view drew last frame.
fn measure_lines(harness: &mut Harness<'static, ArchiCraftApp>) -> usize {
    let ws = harness.state_mut().workspace();
    ws.plan_views[&ws.active_storey].measure.len()
}

/// Whether a widget of the window has a label containing `text`.
fn shows(harness: &Harness<'static, ArchiCraftApp>, text: &str) -> bool {
    harness.query_all(By::new().label_contains(text)).next().is_some()
}

/// UI-001, UI-011, MEA-001: M, the toolbox button and the toolbar button
/// each activate the measure tool, and the toolbar button then shows as
/// selected, as it does not with another tool.
#[test]
fn m_the_toolbox_and_the_toolbar_activate_the_measure_tool() {
    let mut harness = harness("measure-activate");
    assert!(!toolbar_selected(&harness));
    harness.key_press(Key::M);
    harness.run_steps(2);
    assert_eq!(tool(&mut harness), ToolKind::Measure, "M");
    assert!(toolbar_selected(&harness));

    reset(&mut harness);
    assert!(!toolbar_selected(&harness));
    harness.get(By::new().role(Role::Button).label("Measure")).click();
    harness.run_steps(2);
    assert_eq!(tool(&mut harness), ToolKind::Measure, "toolbox");

    reset(&mut harness);
    harness.get(By::new().role(Role::Button).label(TOOLBAR_LABEL)).click();
    harness.run_steps(2);
    assert_eq!(tool(&mut harness), ToolKind::Measure, "toolbar");
    assert!(toolbar_selected(&harness));
}

/// MEA-001: two clicks in the plan measure a distance: the status line shows
/// its total and its direction angle, and the plan draws the measure line.
/// The Angle mode shows the angle of three clicks.
#[test]
fn clicks_in_the_plan_show_the_readout() {
    let mut harness = harness("measure-readout");
    harness.key_press(Key::M);
    harness.run_steps(2);
    let centre = plan_centre(&mut harness);
    click_at(&mut harness, centre);
    click_at(&mut harness, centre + egui::vec2(150.0, -80.0));
    let text = readout(&mut harness);
    assert!(text.starts_with("Total "), "{text}");
    let direction = {
        let ws = harness.state_mut().workspace();
        let Some(Measurement::Distance { direction, .. }) = ws.tool.measure.measurement() else {
            panic!("two clicks measure a distance")
        };
        format_angle(
            direction,
            &ws.doc.project.settings.units,
            &ws.prefs.number_format(),
            true,
        )
    };
    assert!(text.ends_with(&direction), "{text} ends with {direction}");
    harness.get_by_label(&text);
    assert_eq!(measure_lines(&mut harness), 1);

    harness.get(By::new().role(Role::Button).label("Angle")).click();
    harness.run_steps(2);
    assert_eq!(harness.state_mut().workspace().tool.measure.mode, MeasureMode::Angle);
    assert!(!shows(&harness, "Total "), "a new mode starts a new measurement");
    for offset in [egui::vec2(0.0, 0.0), egui::vec2(150.0, 0.0), egui::vec2(0.0, -150.0)] {
        click_at(&mut harness, centre + offset);
    }
    let text = readout(&mut harness);
    assert!(text.starts_with("Angle "), "{text}");
    harness.get_by_label(&text);
}

/// MEA-001: Esc clears the measure lines and the readout, and so does
/// switching to the wall tool.
#[test]
fn esc_and_a_tool_switch_clear_the_measurement() {
    let mut harness = harness("measure-clear");
    harness.key_press(Key::M);
    harness.run_steps(2);
    let centre = plan_centre(&mut harness);
    let measure = |harness: &mut Harness<'static, ArchiCraftApp>| {
        click_at(harness, centre);
        click_at(harness, centre + egui::vec2(120.0, 0.0));
        assert_eq!(measure_lines(harness), 1);
        assert!(shows(harness, "Total "));
    };
    measure(&mut harness);
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert_eq!(measure_lines(&mut harness), 0);
    assert!(!shows(&harness, "Total "));
    assert_eq!(tool(&mut harness), ToolKind::Measure, "the first Esc only clears");

    measure(&mut harness);
    harness.get(By::new().role(Role::Button).label("Wall")).click();
    harness.run_steps(2);
    assert_eq!(tool(&mut harness), ToolKind::Wall);
    assert_eq!(measure_lines(&mut harness), 0);
    assert!(!shows(&harness, "Total "));
    assert!(harness.state_mut().workspace().tool.measure.points().is_empty());
}
