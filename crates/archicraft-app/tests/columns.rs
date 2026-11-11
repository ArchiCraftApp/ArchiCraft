//! A selected column's parameters in the properties strip (COL-001,
//! COL-003): its section, anchor, rotation and end cuts, drawn by the
//! generic field renderer from the column kind's declared fields.

#![allow(clippy::unwrap_used)]

mod common;

use archicraft_app::ArchiCraftApp;
use archicraft_model::glam::DVec2;
use archicraft_model::kinds::{Anchor, ColumnCut, CrossSection};
use archicraft_model::profile::{ParametricShape, Profile};
use archicraft_model::units::{format_area, format_length};
use archicraft_model::{Command, ElementId};
use common::{assert_present, harness};
use egui::Key;
use egui::accesskit::{Role, Toggled};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, NodeT, Queryable};

/// Adds a D.2 column at the origin of the ground storey, its section set
/// to `section`, and selects only it.
fn select_column(harness: &mut Harness<'static, ArchiCraftApp>, section: Option<CrossSection>) -> ElementId {
    let ws = harness.state_mut().workspace();
    let mut column = ws
        .doc
        .project
        .new_column(ws.doc.project.storeys.ground(), DVec2::ZERO, 0.0, 0);
    if let Some(section) = section {
        column.as_column_mut().unwrap().section = section;
    }
    let id = column.id;
    assert!(ws.execute(Command::AddElements(vec![column])));
    ws.selection.clear();
    ws.selection.insert(id);
    harness.run_steps(2);
    id
}

/// The number of commands in the history.
fn applied(harness: &mut Harness<'static, ArchiCraftApp>) -> usize {
    harness.state_mut().workspace().doc.history.applied_commands().count()
}

/// The commands added to the history since it held `applied` of them.
fn commands_since(harness: &mut Harness<'static, ArchiCraftApp>, applied: usize) -> Vec<Command> {
    let history = &harness.state_mut().workspace().doc.history;
    history.applied_commands().skip(applied).cloned().collect()
}

/// Asserts that `commands` is one UpdateElements of the element `id`.
fn assert_one_update(commands: &[Command], id: ElementId) {
    assert!(
        matches!(commands, [Command::UpdateElements(e)] if e.len() == 1 && e[0].id == id),
        "{commands:?}"
    );
}

/// Opens the combo box that shows `current`, clicked near its left end as
/// in ui.rs.
fn open_combo(harness: &mut Harness<'static, ArchiCraftApp>, current: &str) {
    let rect = harness.get(By::new().role(Role::ComboBox).value(current)).rect();
    let pos = egui::pos2(rect.min.x + 8.0, rect.center().y);
    harness.hover_at(pos);
    harness.run_steps(1);
    for pressed in [true, false] {
        harness.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    harness.run_steps(2);
}

/// COL-001: a rectangular column shows its width and depth, a circular one
/// its diameter; both show the cross-section area and, with the top linked
/// to a storey, the height beside the volume. Choosing 'Circular' is one
/// UpdateElements that keeps the width as the diameter.
#[test]
fn the_strip_shows_width_and_depth_or_the_diameter() {
    let mut harness = harness("column-section");
    let id = select_column(&mut harness, None);
    for label in [
        "Cross-section",
        "Width",
        "Depth",
        "Anchor point",
        "Rotation",
        "Top cut",
        "Material",
    ] {
        assert_present(&harness, label);
    }
    assert!(harness.query_by_label("Diameter").is_none());
    let (area, height) = {
        let ws = harness.state_mut().workspace();
        let units = ws.doc.project.settings.units.clone();
        let nf = ws.prefs.number_format();
        (format_area(0.09, &units, &nf), format_length(3.0, &units, &nf, true))
    };
    assert_present(&harness, "Cross-section area");
    assert_present(&harness, &area);
    assert_present(&harness, &height);

    let before = applied(&mut harness);
    open_combo(&mut harness, "Rectangular");
    harness.get_by_label("Circular").click();
    harness.run_steps(2);
    assert_one_update(&commands_since(&mut harness, before), id);
    let section = harness
        .state_mut()
        .workspace()
        .doc
        .project
        .element(id)
        .unwrap()
        .as_column()
        .unwrap()
        .section;
    assert_eq!(section, CrossSection::Circular { diameter: 0.3 });
    assert_present(&harness, "Diameter");
    for absent in ["Width", "Depth"] {
        assert!(harness.query_by_label(absent).is_none(), "{absent} is shown");
    }
}

/// COL-003: typing 15 into the top cut angle tilts the top cut by 15° in
/// exactly one UpdateElements; the bottom cut stays horizontal.
#[test]
fn a_top_cut_of_15_degrees_is_one_update() {
    let mut harness = harness("column-top-cut");
    let id = select_column(&mut harness, None);
    let field = || By::new().role(Role::TextInput).label("Top cut angle");
    let before = applied(&mut harness);
    harness.get(field()).click();
    harness.run_steps(1);
    harness.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    harness.get(field()).type_text("15");
    harness.key_press(Key::Enter);
    harness.run_steps(2);
    assert_one_update(&commands_since(&mut harness, before), id);
    let ws = harness.state_mut().workspace();
    let column = ws.doc.project.element(id).unwrap().as_column().unwrap().clone();
    let ColumnCut::Angle(angle) = column.top_cut else {
        panic!("an angled top cut: {:?}", column.top_cut)
    };
    assert!((angle - 15f64.to_radians()).abs() < 1e-12, "{angle}");
    assert_eq!(column.bottom_cut, ColumnCut::Horizontal);
    harness.run_steps(2);
    harness.get(By::new().role(Role::ComboBox).value("Angle"));
}

/// COL-001: the anchor is a grid of nine toggles named after their points,
/// the centre checked; clicking 'Bottom left' is one UpdateElements. A
/// rectangle offers no profile origin.
#[test]
fn the_anchor_grid_picks_one_of_nine_points() {
    let mut harness = harness("column-anchor");
    let id = select_column(&mut harness, None);
    let toggle = |label: &'static str| By::new().role(Role::Button).label(label);
    for label in [
        "Top left",
        "Top centre",
        "Top right",
        "Middle left",
        "Middle right",
        "Bottom left",
        "Bottom centre",
        "Bottom right",
    ] {
        assert_eq!(
            harness.get(toggle(label)).accesskit_node().toggled(),
            Some(Toggled::False),
            "{label}"
        );
    }
    assert_eq!(
        harness.get(toggle("Centre")).accesskit_node().toggled(),
        Some(Toggled::True)
    );
    assert!(harness.query_by_label("Profile origin").is_none());
    let before = applied(&mut harness);
    harness.get(toggle("Bottom left")).click();
    harness.run_steps(2);
    assert_one_update(&commands_since(&mut harness, before), id);
    let anchor = harness
        .state_mut()
        .workspace()
        .doc
        .project
        .element(id)
        .unwrap()
        .as_column()
        .unwrap()
        .anchor;
    assert_eq!(anchor, Anchor::BottomLeft);
    assert_eq!(
        harness.get(toggle("Bottom left")).accesskit_node().toggled(),
        Some(Toggled::True)
    );
}

/// PRF-004, COL-001: only profiles columns may use are offered, a profile
/// section only while there is one, and a profile column shows its
/// profile, the profile origin anchor and 'Materials from profile' in place
/// of the material.
#[test]
fn profiles_without_the_columns_flag_are_not_offered() {
    let mut harness = harness("column-profiles");
    select_column(&mut harness, None);
    open_combo(&mut harness, "Rectangular");
    harness.get_by_label("Circular");
    assert!(
        harness.query_by_label("Profile").is_none(),
        "a profile section without profiles"
    );
    harness.key_press(Key::Escape);
    harness.run_steps(2);

    let pier = {
        let ws = harness.state_mut().workspace();
        let material = ws.doc.project.materials[0].id;
        let pier = Profile::parametric("Pier", ParametricShape::ALL[0], material);
        let mut rail = Profile::parametric("Rail", ParametricShape::ALL[4], material);
        rail.usage.columns = false;
        let id = pier.id;
        assert!(ws.execute(Command::AddProfile(pier)));
        assert!(ws.execute(Command::AddProfile(rail)));
        id
    };
    select_column(&mut harness, Some(CrossSection::Profile(pier)));
    harness.get(By::new().role(Role::ComboBox).value("Profile"));
    assert_present(&harness, "Materials from profile");
    assert_present(&harness, "Profile origin");
    assert!(harness.query_by_label("Width").is_none());
    let piers = harness.query_all_by_label("Pier").count();
    open_combo(&mut harness, "Pier");
    assert_eq!(
        harness.query_all_by_label("Pier").count(),
        piers + 1,
        "the profile is offered"
    );
    assert!(
        harness.query_by_label("Rail").is_none(),
        "a beam-only profile is offered"
    );
}
