//! The properties strip as one ordered row (UI-001, UI-002): it never
//! wraps, its groups are separated, and what does not fit into the window
//! is in the popup of a chevron at its right end.

#![allow(clippy::unwrap_used)]

mod common;

use std::rc::Rc;

use archicraft_app::i18n::{self, Localizer};
use archicraft_app::tools::ToolKind;
use archicraft_app::ui::properties;
use archicraft_app::ui::strip::{self, RowLayout};
use archicraft_app::{ArchiCraftApp, tr};
use archicraft_model::fields::{Section, fields_of};
use archicraft_model::glam::DVec2;
use archicraft_model::units::format_length;
use archicraft_model::{Command, ElementId};
use common::harness;
use egui::accesskit::Role;
use egui::{Key, Modifiers, Rect};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, NodeT, Queryable};

type App = Harness<'static, ArchiCraftApp>;

/// The smallest window the application allows (main.rs).
const MIN_WIDTH: f32 = 800.0;

/// Adds a 5 m wall on the ground storey and selects only it.
fn select_wall(harness: &mut App) -> ElementId {
    let ws = harness.state_mut().workspace();
    let wall = ws
        .doc
        .project
        .new_wall(ws.doc.project.storeys.ground(), DVec2::ZERO, DVec2::new(5.0, 0.0), 0);
    select(harness, wall)
}

/// Adds a 4 m x 3 m slab on the ground storey and selects only it.
fn select_slab(harness: &mut App) -> ElementId {
    let ws = harness.state_mut().workspace();
    let outline = vec![
        DVec2::ZERO,
        DVec2::new(4.0, 0.0),
        DVec2::new(4.0, 3.0),
        DVec2::new(0.0, 3.0),
    ];
    let slab = ws
        .doc
        .project
        .new_slab(ws.doc.project.storeys.ground(), outline, Vec::new(), 0);
    select(harness, slab)
}

fn select(harness: &mut App, element: archicraft_model::Element) -> ElementId {
    let ws = harness.state_mut().workspace();
    let id = element.id;
    assert!(ws.execute(Command::AddElements(vec![element])));
    ws.selection.clear();
    ws.selection.insert(id);
    harness.run_steps(3);
    id
}

fn set_width(harness: &mut App, width: f32) {
    harness.set_size(egui::vec2(width, 900.0));
    harness.run_steps(3);
}

fn layout(harness: &App) -> RowLayout {
    RowLayout::load(&harness.ctx, properties::strip_id()).expect("the strip's layout")
}

/// The rectangle of the properties panel.
fn panel(harness: &App) -> Rect {
    egui::containers::panel::PanelState::load(&harness.ctx, egui::Id::new("properties"))
        .unwrap()
        .outer_rect
}

/// The rectangles of the widgets of the strip, with their roles.
fn strip_widgets(harness: &App) -> Vec<(Role, Rect)> {
    let strip = panel(harness);
    harness
        .query_all(By::new().predicate(|_| true))
        .filter(|n| n.accesskit_node().bounding_box().is_some())
        .map(|n| (n.accesskit_node().role(), n.rect()))
        .filter(|(role, r)| *role != Role::Window && strip.contains(r.center()))
        .collect()
}

/// The visible label of the unit `name` of the strip of the selected
/// element `id`; `None` for the units before the summary and the summary.
fn unit_label(harness: &mut App, id: ElementId, name: &str) -> Option<String> {
    let element = harness.state_mut().workspace().doc.project.element(id).unwrap().clone();
    if ["Mode(", "Measure(", "division:", "summary:"]
        .iter()
        .any(|p| name.starts_with(p))
    {
        return None;
    }
    Some(match name {
        // The first label of each; Home storey and Snap targets follow.
        "NameAndStorey" => tr!("dlg-properties-name"),
        "Lock" => tr!("dlg-properties-locked-toggle"),
        "Settings" => tr!("dlg-properties-settings"),
        _ => match (name.strip_prefix("field:"), name.strip_prefix("derived:")) {
            (Some(key), _) => tr!(fields_of(&element).find(|s| s.key == key).unwrap().label),
            (_, Some(label)) => label.to_owned(),
            _ => panic!("no label for {name}"),
        },
    })
}

/// The section rank of the field unit `name` in the strip's order:
/// geometry, classification, the rest.
fn section_rank(harness: &mut App, id: ElementId, name: &str) -> Option<u8> {
    let key = name.strip_prefix("field:")?;
    let element = harness.state_mut().workspace().doc.project.element(id).unwrap().clone();
    let spec = fields_of(&element).find(|s| s.key == key).unwrap();
    Some(match spec.section {
        Section::Geometry => 0,
        Section::Classification => 1,
        _ => 2,
    })
}

/// Clicks the chevron at the right end of the strip.
fn click_more(harness: &mut App) {
    harness
        .get(By::new().role(Role::Button).label(&tr!("dlg-properties-more")))
        .click();
    harness.run_steps(3);
}

/// Whether `node` lies in the overflow popup: the popup's area is one of
/// its ancestors.
fn in_popup(node: &egui_kittest::Node<'_>) -> bool {
    let popup = strip::overflow_id(properties::strip_id()).accesskit_id();
    let mut ancestor = node.accesskit_node().parent();
    while let Some(n) = ancestor {
        if n.locate().0 == popup {
            return true;
        }
        ancestor = n.parent();
    }
    false
}

/// The overflow popup's rectangle, while it is open.
fn popup(harness: &App) -> Option<Rect> {
    let id = strip::overflow_id(properties::strip_id());
    harness
        .ctx
        .memory(|m| m.area_rect(id))
        .filter(|_| harness.ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false))
}

/// UI-002, archicraft-g8c1: at 1400 x 900 and at the smallest window, with
/// a wall and with a slab selected, the strip is one row: every widget of
/// it lies within the panel, which is lower than two rows, and every combo
/// box and text field lies fully inside the window.
#[test]
fn the_strip_is_one_row_inside_the_window() {
    for select in [select_wall, select_slab] {
        for width in [1400.0, MIN_WIDTH] {
            let mut harness = harness("strip-one-row");
            select(&mut harness);
            set_width(&mut harness, width);
            let strip = panel(&harness);
            let line = harness.ctx.global_style().spacing.interact_size.y;
            assert!(strip.height() < 2.0 * line, "{width}: the strip is {strip:?}");
            let widgets = strip_widgets(&harness);
            assert!(widgets.len() > 5, "{width}: {widgets:?}");
            for (role, rect) in widgets {
                assert!(
                    rect.top() >= strip.top() && rect.bottom() <= strip.bottom(),
                    "{width}: {role:?} at {rect:?} is outside the row {strip:?}"
                );
                if matches!(role, Role::ComboBox | Role::TextInput) {
                    assert!(
                        rect.max.x <= width,
                        "{width}: {role:?} at {rect:?} reaches past the window"
                    );
                }
            }
        }
    }
}

/// UI-002: at the demo's width of 1000 px, with the wall tool active and a
/// wall selected, the units do not all fit; the chevron is the strip's
/// last widget and opens a popup with exactly the hidden units in strip
/// order, the thickness among them. The thickness typed there is one undo
/// step.
#[test]
fn the_chevron_opens_the_hidden_units_and_edits_there_are_one_undo_step() {
    let mut harness = harness("strip-overflow");
    harness.state_mut().workspace().select_tool(ToolKind::Wall);
    let wall = select_wall(&mut harness);
    set_width(&mut harness, 4000.0);
    let full = layout(&harness);
    assert_eq!(full.shown, full.names.len(), "everything fits at 4000 px");
    let width = 1000.0;
    set_width(&mut harness, width);
    let row = layout(&harness);
    assert_eq!(row.names, full.names);
    let thickness = full.names.iter().position(|n| n == "field:thickness").unwrap();
    assert!(
        row.shown <= thickness,
        "the thickness is hidden: {:?}",
        &row.names[..row.shown]
    );
    let button = row.button.expect("the chevron");
    let rightmost = strip_widgets(&harness)
        .into_iter()
        .map(|(_, r)| r)
        .max_by(|a, b| a.max.x.total_cmp(&b.max.x))
        .unwrap();
    assert_eq!(rightmost, button, "the chevron is the strip's last widget");
    assert!(button.max.x <= width);
    assert!(popup(&harness).is_none());

    click_more(&mut harness);
    let area = popup(&harness).expect("the popup is open");
    let hidden: Vec<String> = row.hidden().to_vec();
    assert_eq!(
        &full.names[row.shown..],
        &hidden[..],
        "the popup's units are the row's tail"
    );
    let mut tops = Vec::new();
    for name in &hidden {
        let label = unit_label(&mut harness, wall, name).unwrap();
        let nodes: Vec<Rect> = harness.query_all_by_label(&label).map(|n| n.rect()).collect();
        let inside: Vec<Rect> = harness
            .query_all_by_label(&label)
            .filter(in_popup)
            .map(|n| n.rect())
            .collect();
        assert!(inside.iter().all(|r| area.contains(r.center())));
        assert_eq!(inside.len(), 1, "{name} ({label}) in the popup: {nodes:?}");
        tops.push(inside[0].top());
    }
    assert!(
        tops.is_sorted(),
        "the popup lists them in strip order: {hidden:?} at {tops:?}"
    );
    for name in &full.names[..row.shown] {
        let Some(label) = unit_label(&mut harness, wall, name) else {
            continue;
        };
        assert!(
            harness
                .query_all_by_label(&label)
                .all(|n| !area.contains(n.rect().center())),
            "{name} is shown in the row and the popup"
        );
    }

    let (shown, typed) = {
        let ws = harness.state_mut().workspace();
        let units = ws.doc.project.settings.units.clone();
        let nf = ws.prefs.number_format();
        let thickness = ws.doc.project.element(wall).unwrap().as_wall().unwrap().thickness;
        (
            format_length(thickness, &units, &nf, false),
            format_length(0.3, &units, &nf, false),
        )
    };
    let field = || By::new().role(Role::TextInput).label("Thickness");
    assert!(in_popup(&harness.get(field())));
    assert_eq!(harness.get(field()).value().as_deref(), Some(shown.as_str()));
    let before = harness.state_mut().workspace().doc.history.applied_commands().count();
    harness.get(field()).click();
    harness.run_steps(1);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::A);
    harness.get(field()).type_text(&typed);
    harness.key_press(Key::Enter);
    harness.run_steps(3);
    let ws = harness.state_mut().workspace();
    let commands: Vec<Command> = ws.doc.history.applied_commands().skip(before).cloned().collect();
    assert!(
        matches!(commands.as_slice(), [Command::UpdateElements(e)] if e.len() == 1 && e[0].id == wall),
        "{commands:?}"
    );
    let thickness =
        |ws: &archicraft_app::workspace::Workspace| ws.doc.project.element(wall).unwrap().as_wall().unwrap().thickness;
    assert!((thickness(ws) - 0.3).abs() < 1e-9);
    assert!(popup(&harness).is_some(), "the popup stays open after the edit");
    let ws = harness.state_mut().workspace();
    ws.undo();
    assert!((thickness(ws) - 0.3).abs() > 1e-3, "one undo step");

    // Escape closes the popup, and the chevron opens it again.
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert!(popup(&harness).is_none());
}

/// UI-002: at 1400 px with a wall selected, the units of the row follow
/// the documented order (summary | Name, Home storey | geometry fields |
/// classification fields | the other fields | Locked, Snap targets |
/// derived values | Settings…), each widget right of the one before, and a
/// separator lies between neighbouring groups.
#[test]
fn the_row_follows_the_documented_order_with_separators() {
    let mut harness = harness("strip-order");
    let wall = select_wall(&mut harness);
    // Wide enough for every unit, to check the whole order.
    for width in [1400.0, 4000.0] {
        set_width(&mut harness, width);
        let row = layout(&harness);
        let rank = |harness: &mut App, name: &str| -> u8 {
            match name {
                n if n.starts_with("summary:") => 0,
                "NameAndStorey" => 1,
                n if n.starts_with("field:") => 2 + section_rank(harness, wall, n).unwrap(),
                "Lock" => 5,
                n if n.starts_with("derived:") => 6,
                "Settings" => 7,
                other => panic!("an unexpected unit {other}"),
            }
        };
        let ranks: Vec<u8> = row.names.iter().map(|n| rank(&mut harness, n)).collect();
        assert!(ranks.is_sorted(), "{:?}", row.names);
        assert_eq!(row.names[..2], ["summary:Wall", "NameAndStorey"]);
        if width > 2000.0 {
            assert_eq!(row.shown, row.names.len(), "{:?}", row.names);
            assert!(row.button.is_none());
            assert_eq!(
                row.names[row.names.len() - 4..],
                ["Lock", "derived:Length", "derived:Volume", "Settings"]
            );
        }
        // The widgets lie left to right in that order.
        let mut right = f32::NEG_INFINITY;
        for (i, name) in row.names[..row.shown].iter().enumerate() {
            if name.starts_with("summary:") {
                continue;
            }
            let label = unit_label(&mut harness, wall, name).unwrap();
            let rect = harness
                .query_all_by_label(&label)
                .map(|n| n.rect())
                .find(|r| row.rects[i].contains(r.center()))
                .unwrap_or_else(|| panic!("{name} ({label}) in its unit"));
            assert!(rect.min.x >= right, "{name} starts left of the unit before");
            right = rect.max.x;
        }
        // One separator between each pair of neighbouring groups, none
        // inside a group.
        let mut separators = row.separators.iter();
        for (i, pair) in ranks[..row.shown].windows(2).enumerate() {
            let (a, b) = (row.rects[i], row.rects[i + 1]);
            let between = row
                .separators
                .iter()
                .filter(|s| s.min.x >= a.max.x && s.max.x <= b.min.x)
                .count();
            let new_group = pair[0] != pair[1]
                || (row.names[i].starts_with("field:")
                    && section_rank(&mut harness, wall, &row.names[i])
                        != section_rank(&mut harness, wall, &row.names[i + 1]));
            assert_eq!(
                between,
                usize::from(new_group),
                "between {} and {}",
                row.names[i],
                row.names[i + 1]
            );
            if new_group {
                separators.next();
            }
        }
        assert!(separators.next().is_none(), "no further separators");
    }
}

/// UI-002: shrinking the window by single pixels moves exactly one unit
/// from the row into the popup at each split point, and growing it by one
/// pixel moves it back; two frames at the same width give the same split.
#[test]
fn single_pixel_resizes_move_one_unit_at_a_split_point() {
    let mut harness = harness("strip-resize");
    select_wall(&mut harness);
    let mut width = 1400.0;
    set_width(&mut harness, width);
    let mut splits = 0;
    while splits < 3 && width > MIN_WIDTH {
        let before = layout(&harness);
        width -= 1.0;
        harness.set_size(egui::vec2(width, 900.0));
        harness.run_steps(1);
        let after = layout(&harness);
        harness.run_steps(1);
        assert_eq!(
            layout(&harness).shown,
            after.shown,
            "{width}: the same split in the next frame"
        );
        if after.shown == before.shown {
            continue;
        }
        splits += 1;
        assert_eq!(after.shown + 1, before.shown, "{width}: one unit moves");
        assert_eq!(after.hidden()[1..], *before.hidden(), "{width}");
        harness.set_size(egui::vec2(width + 1.0, 900.0));
        harness.run_steps(1);
        assert_eq!(
            layout(&harness).shown,
            before.shown,
            "{width}: one pixel more moves it back"
        );
        harness.set_size(egui::vec2(width, 900.0));
        harness.run_steps(1);
        assert_eq!(layout(&harness).shown, after.shown);
    }
    assert_eq!(splits, 3, "three split points above {MIN_WIDTH} px");
}

/// NFR-L10N-001: the strip's texts come from the catalogue: with the en-XA
/// pseudo-locale, the empty state, the summary, Name, Home storey, Locked,
/// Snap targets, the derived values, Settings… and the chevron show their
/// pseudo texts.
#[cfg(debug_assertions)]
#[test]
fn the_strip_texts_come_from_the_catalogue() {
    let mut harness = harness("strip-pseudo");
    let pseudo = Rc::new(Localizer::pseudo());
    harness.state_mut().set_localizer(Rc::clone(&pseudo));
    harness.run_steps(3);
    let label = |key: &str| i18n::with(Rc::clone(&pseudo), || tr!(key));
    let empty = label("dlg-properties-nothing-selected");
    assert!(empty.starts_with(i18n::pseudo::OPEN), "{empty}");
    assert!(harness.query_by_label(&empty).is_some(), "the empty state");

    select_wall(&mut harness);
    set_width(&mut harness, MIN_WIDTH);
    click_more(&mut harness);
    for key in [
        "dlg-properties-name",
        "dlg-properties-home-storey",
        "dlg-properties-locked-toggle",
        "dlg-properties-snap-targets",
        "dlg-properties-length",
        "dlg-properties-volume",
        "dlg-properties-settings",
        "dlg-properties-more",
    ] {
        let text = label(key);
        assert!(harness.query_all_by_label(&text).next().is_some(), "{key}: “{text}”");
    }
    for english in ["Name", "Home storey", "Locked", "Snap targets", "Settings…", "Length"] {
        assert!(
            harness.query_all_by_label(english).next().is_none(),
            "“{english}” is English"
        );
    }
}
