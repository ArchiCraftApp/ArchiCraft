//! Spaces in the application (SPC-001): the boundary and the stamp in the
//! plan view, picking a space by its stamp, the space tool's click into a
//! room, and the properties and settings of spaces and the space tool.

#![allow(clippy::unwrap_used)]

mod common;

use archicraft_app::ArchiCraftApp;
use archicraft_app::tools::ToolKind;
use archicraft_app::workspace::Request;
use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::WriteOptions;
use archicraft_model::glam::DVec2;
use archicraft_model::units::{format_area, format_length};
use archicraft_model::{Command, DrawingScale, ElementId, ElementKind, KindTag, TopLink, samples};
use common::{close_strip_overflow, data_dir, harness, in_strip, open_strip_overflow};
use egui::accesskit::Role;
use egui::{Event, Modifiers, PointerButton};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, NodeT, Queryable};

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
    space_section(&harness);
    for absent in ["Material", "Model and appearance", "Cut fills come from the material."] {
        assert!(harness.query_by_label(absent).is_none(), "{absent} is shown");
    }
}

/// SPC-001, IFX-064 (demo step 4): a space added through the model and
/// renamed in the properties strip's Name field saves as IFCX and reopens
/// with the same name, room name, number, boundary and area, and its stamp
/// shows the same text as before saving.
#[test]
fn a_renamed_space_saves_and_reopens_with_the_same_stamp() {
    let (mut harness, space) = shell_with_kitchen("space-reopen");
    {
        let ws = harness.state_mut().workspace();
        ws.selection.clear();
        ws.selection.insert(space);
    }
    harness.run_steps(2);
    let name_field = || By::new().role(Role::TextInput).value("SP-001");
    harness.get(name_field()).click();
    harness.run_steps(1);
    harness.get(name_field()).type_text("KIT-1");
    harness.key_press(egui::Key::Enter);
    harness.run_steps(3);
    let before = harness.state_mut().workspace().doc.project.elements[&space].clone();
    assert_eq!(before.name, "KIT-1", "renamed in the properties strip");
    let (_, lines, _) = stamp(&mut harness, space);
    assert_eq!(lines[..2], ["Kitchen", "0.01"]);

    let path = data_dir("space-reopen-file").join("kitchen.ifcx");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    assert!(harness.state_mut().save_to(&path));
    let mut reopened = common::harness("space-reopen-again");
    reopened.state_mut().open_path(&path);
    reopened.run_steps(3);
    let after = reopened.state_mut().workspace().doc.project.elements[&space].clone();
    assert_eq!(after.name, "KIT-1");
    let (a, b) = (before.as_space().unwrap(), after.as_space().unwrap());
    assert_eq!(
        (a.long_name.as_str(), a.number.as_str()),
        (b.long_name.as_str(), b.number.as_str())
    );
    assert_eq!(a.boundary, b.boundary);
    assert_eq!(a.net_area(), b.net_area());
    assert_eq!(stamp(&mut reopened, space).1, lines);
}

/// Opens two_storey_shell_with_shaft from an IFCX file under `name` with the
/// space tool active in the plan of the ground storey.
fn shaft_sample_with_the_space_tool(name: &str) -> App {
    let project = samples::two_storey_shell_with_shaft();
    let geometry = ModelGeometry::evaluate(&project);
    let options = WriteOptions {
        author: "spaces test".into(),
        timestamp: "2026-11-10T00:00:00Z".into(),
        data_version: 1,
    };
    let (bytes, _) = archicraft_ifcx::save(&project, &geometry, &options, false);
    let dir = data_dir(name);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("two-storey-shell-with-shaft.ifcx");
    std::fs::write(&path, bytes).unwrap();
    let mut harness = harness(&format!("{name}-app"));
    harness.state_mut().open_path(&path);
    harness.run_steps(3);
    harness.state_mut().workspace().select_tool(ToolKind::Space);
    harness.run_steps(2);
    harness
}

/// Clicks the primary button at the plan point `p` of the active plan view
/// and returns the space the click created, if any.
fn click_in_plan(harness: &mut App, p: DVec2) -> Option<ElementId> {
    let before = harness.state_mut().workspace().doc.project.elements.len();
    let pos = {
        let ws = harness.state_mut().workspace();
        let state = &ws.plan_views[&ws.active_storey];
        state.camera.to_screen(state.last_rect, p)
    };
    for _ in 0..2 {
        harness.hover_at(pos);
        harness.run_steps(2);
    }
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    harness.run_steps(3);
    let ws = harness.state_mut().workspace();
    let elements = &ws.doc.project.elements;
    (elements.len() == before + 1)
        .then(|| elements.values().last().unwrap())
        .filter(|e| e.as_space().is_some())
        .map(|e| e.id)
}

/// Opens the plan of the upper storey from its row in the project browser.
fn open_upper_plan(harness: &mut App) {
    let (upper, name) = {
        let ws = harness.state_mut().workspace();
        let ground = ws.doc.project.storeys.ground();
        let upper = ws.doc.project.storeys.iter().find(|s| s.id != ground).unwrap();
        (upper.id, upper.name.clone())
    };
    let row = harness
        .query_all_by_label_contains(&name)
        .filter_map(|n| n.accesskit_node().label())
        .find(|label| !label.starts_with("Plan: "))
        .unwrap();
    harness.get_by_label(&row).click();
    harness.run_steps(4);
    assert_eq!(harness.state_mut().workspace().active_storey, upper);
}

/// The area of `space` in the project's area unit, as the app shows it.
fn shown_area(harness: &mut App, space: ElementId) -> String {
    let ws = harness.state_mut().workspace();
    let area = ws.doc.project.elements[&space].as_space().unwrap().net_area();
    format_area(area, &ws.doc.project.settings.units, &ws.prefs.number_format())
}

/// The text fields whose hint (AccessKit placeholder) is `hint`, as their
/// screen rectangles.
fn fields_with_hint(harness: &App, hint: &str) -> Vec<egui::Rect> {
    harness
        .query_all(By::new().role(Role::TextInput))
        .filter(|n| n.accesskit_node().placeholder() == Some(hint))
        .map(|n| n.rect())
        .collect()
}

/// Presses and releases the primary button at `pos`, which may lie beyond
/// a node's visible part (the strip can reach past the window).
fn press_at(harness: &mut App, pos: egui::Pos2) {
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
}

/// Types `text` into the text field at `rect` and commits it with Enter.
fn type_into(harness: &mut App, rect: egui::Rect, text: &str) {
    let pos = rect.left_center() + egui::vec2(4.0, 0.0);
    press_at(harness, pos);
    harness.run_steps(2);
    harness.event(Event::Text(text.to_owned()));
    harness.key_press(egui::Key::Enter);
    harness.run_steps(3);
}

/// SPC-001, UI-002 (Jakob Brandstötter's request of 2026-11-03): one click
/// with the space tool inside the ground-floor room creates the space "Room
/// 1"; the plan shows its stamp with the room's area on the line directly
/// below the name, and the properties strip shows the same name and area.
/// Renaming the room in the strip updates the stamp in one undo step; a
/// second space detected in the shaft on the upper storey is named "Room 2".
#[test]
fn one_click_in_a_room_names_it_and_the_properties_show_name_and_area() {
    let mut harness = shaft_sample_with_the_space_tool("space-jakob");
    let room = click_in_plan(&mut harness, DVec2::new(2.0, 3.0)).expect("the click creates a space");
    let area = shown_area(&mut harness, room);
    let (_, lines, _) = stamp(&mut harness, room);
    assert_eq!(lines, ["Room 1", area.as_str()], "the area directly below the name");

    open_strip_overflow(&mut harness);
    let name_field = harness.get(By::new().role(Role::TextInput).value("Room 1")).rect();
    assert!(in_strip(&harness, name_field), "the name is in the strip");
    let area_label = harness.get_by_label(&area).rect();
    assert!(in_strip(&harness, area_label), "the area is in the strip");

    let undo = harness.state_mut().workspace().doc.history.undo_len();
    let rect = harness.get(By::new().role(Role::TextInput).value("Room 1")).rect();
    type_into(&mut harness, rect, "Kitchen");
    let ws = harness.state_mut().workspace();
    assert_eq!(ws.doc.project.elements[&room].as_space().unwrap().long_name, "Kitchen");
    assert_eq!(ws.doc.history.undo_len(), undo + 1, "one undo step");
    assert_eq!(stamp(&mut harness, room).1, ["Kitchen", area.as_str()]);

    // The popup lies over the project browser.
    close_strip_overflow(&mut harness);
    open_upper_plan(&mut harness);
    let shaft = click_in_plan(&mut harness, DVec2::new(2.0, 7.25)).expect("the click creates a space in the shaft");
    let ws = harness.state_mut().workspace();
    let space = ws.doc.project.elements[&shaft].as_space().unwrap();
    assert_eq!(space.long_name, "Room 2");
    assert!(
        (space.net_area() - 2.610725).abs() < 1e-6,
        "the shaft's inner faces: {}",
        space.net_area()
    );
}

/// SPC-001, UI-003, ELM-001: with one space in the project and "Raum {n}"
/// typed as the default name into the 'Space' section of the settings
/// dialog while the space tool is active, the next detected room is named
/// "Raum 2". The empty default name shows the localised pattern as its hint.
#[test]
fn the_default_name_set_in_the_space_settings_names_the_next_room() {
    let mut harness = shaft_sample_with_the_space_tool("space-default-name");
    click_in_plan(&mut harness, DVec2::new(2.0, 3.0)).expect("the first space");
    harness.state_mut().workspace().selection.clear();
    harness.state_mut().workspace().requests.push(Request::Settings);
    harness.run_steps(4);
    let section = space_section(&harness);
    let properties = egui::containers::panel::PanelState::load(&harness.ctx, egui::Id::new("properties"))
        .unwrap()
        .outer_rect;
    let field = fields_with_hint(&harness, "Room {n}")
        .into_iter()
        .find(|r| !properties.contains(r.center()) && r.min.y > section.min.y)
        .expect("the default name in the dialog's Space section");
    type_into(&mut harness, field, "Raum {n}");
    let ws = harness.state_mut().workspace();
    let slot = ws.doc.project.tool_defaults.get(KindTag::Space).unwrap();
    let ElementKind::Space(defaults) = &slot.kind else {
        panic!("the space slot holds a space")
    };
    assert_eq!(defaults.long_name, "Raum {n}");
    close_settings(&mut harness);

    open_upper_plan(&mut harness);
    let shaft = click_in_plan(&mut harness, DVec2::new(2.0, 7.25)).expect("the next detected room");
    let ws = harness.state_mut().workspace();
    assert_eq!(ws.doc.project.elements[&shaft].as_space().unwrap().long_name, "Raum 2");
}

/// SPC-001, UI-002, UI-003, ELM-006: the properties of a selected space
/// without a number show its room name, the number field empty with the
/// suggested number 0.01 as its hint, the base offset, the top linked to
/// the home storey + 1 and its offset, and the read-only net area; editing
/// the room name, the number, the base offset, the top mode, the linked
/// storey and the top offset is one undo step each. The settings dialog
/// shows the same fields in its 'Space' section.
#[test]
fn a_selected_space_shows_and_edits_its_name_number_height_and_area() {
    let (mut harness, space) = shell_with_kitchen("space-fields");
    {
        let ws = harness.state_mut().workspace();
        let mut element = ws.doc.project.elements[&space].clone();
        element.as_space_mut().unwrap().number.clear();
        assert!(ws.execute(Command::UpdateElements(vec![element])));
        ws.selection.clear();
        ws.selection.insert(space);
    }
    harness.run_steps(3);
    open_strip_overflow(&mut harness);
    let number = fields_with_hint(&harness, "0.01")
        .into_iter()
        .find(|r| in_strip(&harness, *r))
        .expect("the number with the suggestion as hint");
    for label in [
        "Room name",
        "Number",
        "Base offset",
        "Top",
        "Linked storey",
        "Top offset",
        "Area",
    ] {
        assert!(
            harness.query_all_by_label(label).any(|n| in_strip(&harness, n.rect())),
            "{label} is in the strip"
        );
    }
    let area = shown_area(&mut harness, space);
    assert!(
        harness.query_all_by_label(&area).any(|n| in_strip(&harness, n.rect())),
        "the net area {area} is in the strip"
    );

    let space_of = |harness: &mut App| {
        harness.state_mut().workspace().doc.project.elements[&space]
            .as_space()
            .unwrap()
            .clone()
    };
    let undo_len = |harness: &mut App| harness.state_mut().workspace().doc.history.undo_len();
    let undo = undo_len(&mut harness);
    type_into(&mut harness, number, "0.07");
    assert_eq!(space_of(&mut harness).number, "0.07");
    assert_eq!(undo_len(&mut harness), undo + 1);

    let rect = harness.get(By::new().role(Role::TextInput).value("Kitchen")).rect();
    type_into(&mut harness, rect, "Pantry");
    assert_eq!(space_of(&mut harness).long_name, "Pantry");
    assert_eq!(undo_len(&mut harness), undo + 2);

    // The base offset and the top offset are the strip's two zero lengths
    // in the order of the rows: base offset first.
    let length = |harness: &mut App, v: f64| {
        let ws = harness.state_mut().workspace();
        format_length(v, &ws.doc.project.settings.units, &ws.prefs.number_format(), false)
    };
    let zero = length(&mut harness, 0.0);
    let zeros: Vec<egui::Rect> = harness
        .query_all(By::new().role(Role::TextInput).value(&zero))
        .map(|n| n.rect())
        .filter(|r| in_strip(&harness, *r))
        .collect();
    assert_eq!(zeros.len(), 2, "base offset and top offset: {zeros:?}");
    let typed = length(&mut harness, 0.1);
    type_into(&mut harness, zeros[0], &typed);
    assert!((space_of(&mut harness).link.base_offset - 0.1).abs() < 1e-12);
    assert_eq!(undo_len(&mut harness), undo + 3);
    let top_offset = harness
        .query_all(By::new().role(Role::TextInput).value(&zero))
        .map(|n| n.rect())
        .find(|r| in_strip(&harness, *r))
        .unwrap();
    let typed = length(&mut harness, 2.5);
    type_into(&mut harness, top_offset, &typed);
    assert_eq!(
        space_of(&mut harness).link.top,
        TopLink::Relative {
            storeys: 1,
            offset: 2.5
        }
    );
    assert_eq!(undo_len(&mut harness), undo + 4);

    // The top 2.5 m above the home storey itself.
    pick(&mut harness, "Home +1", "Home +0");
    assert_eq!(
        space_of(&mut harness).link.top,
        TopLink::Relative {
            storeys: 0,
            offset: 2.5
        }
    );
    assert_eq!(undo_len(&mut harness), undo + 5);
    pick(&mut harness, "Home storey + n", "Fixed height");
    assert!(matches!(space_of(&mut harness).link.top, TopLink::Fixed { .. }));
    assert_eq!(undo_len(&mut harness), undo + 6);

    harness.state_mut().workspace().requests.push(Request::Settings);
    harness.run_steps(4);
    let section = space_section(&harness);
    for label in ["Room name", "Number", "Base offset", "Top", "Height", "Area"] {
        assert!(
            harness
                .query_all_by_label(label)
                .any(|n| !in_strip(&harness, n.rect()) && n.rect().min.y > section.min.y),
            "{label} is in the dialog's Space section"
        );
    }
    assert!(
        harness.query_by_label("Geometry and positioning").is_none(),
        "all of a space's fields are in 'Space'"
    );
}

/// The header of the settings dialog's 'Space' section: the button
/// labelled "Space" that is not the toolbox's space tool, which is an
/// on/off button. The dialog may float over the toolbox, so its place does
/// not tell them apart.
fn space_section(harness: &App) -> egui::Rect {
    let headers: Vec<egui::Rect> = harness
        .query_all(By::new().role(Role::Button).label("Space"))
        .filter(|n| n.accesskit_node().toggled().is_none())
        .map(|n| n.rect())
        .collect();
    assert_eq!(headers.len(), 1, "one 'Space' section: {headers:?}");
    headers[0]
}

/// Closes the settings dialog with the close button in its title bar.
fn close_settings(harness: &mut App) {
    harness.get_by_label("Close window").click();
    harness.run_steps(3);
    assert!(harness.query_by_label("Settings").is_none(), "the dialog is closed");
}

/// Opens the combo box of the strip showing `shown` and picks `option`.
fn pick(harness: &mut App, shown: &str, option: &str) {
    let combo = harness.get(By::new().role(Role::ComboBox).value(shown)).rect();
    press_at(harness, combo.left_center() + egui::vec2(8.0, 0.0));
    harness.run_steps(2);
    harness.get_by_label(option).click();
    harness.run_steps(3);
}
