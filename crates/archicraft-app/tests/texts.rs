//! Texts in the application (ANN-007): a text block drawn in the plan view
//! at its paper size, picked by a click inside it, deleted, and edited in
//! the properties strip.

#![allow(clippy::unwrap_used)]

mod common;

use archicraft_app::ArchiCraftApp;
use archicraft_app::views::annotation_draw::LaidOutText;
use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::WriteOptions;
use archicraft_model::glam::DVec2;
use archicraft_model::{AnchorPos, Command, ElementId, samples};
use common::{data_dir, harness};
use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, Queryable};

type App = Harness<'static, ArchiCraftApp>;

/// Where the test text's anchor lies: in the room west of the interior wall
/// of the ground storey, clear of every line of the plan.
const ANCHOR: DVec2 = DVec2::new(1.5, 2.5);

/// Opens two_storey_shell from an IFCX file under `name` and adds the text
/// "Note" with its anchor at [`ANCHOR`] on the ground storey through the
/// model, with the text tool's defaults (style "Standard 2.5 mm",
/// bottom-left anchor position, angle 0); returns its id.
fn shell_with_text(name: &str) -> (App, ElementId) {
    let project = samples::two_storey_shell();
    let geometry = ModelGeometry::evaluate(&project);
    let options = WriteOptions {
        author: "texts test".into(),
        timestamp: "2026-11-10T00:00:00Z".into(),
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
    let text = ws.doc.project.new_text(ground, ANCHOR, "Note", 0);
    let id = text.id;
    assert!(ws.execute(Command::AddElements(vec![text])));
    assert_eq!(ws.active_storey, ground);
    harness.run_steps(3);
    (harness, id)
}

/// The text `id` as the ground storey's plan laid it out last frame.
fn laid_out(harness: &mut App, id: ElementId) -> LaidOutText {
    let ws = harness.state_mut().workspace();
    let state = &ws.plan_views[&ws.active_storey];
    let (_, text) = state.texts.iter().find(|(t, _)| *t == id).unwrap();
    text.clone()
}

/// The screen point of [`ANCHOR`] in the ground storey's plan and the
/// camera's points per metre.
fn anchor_on_screen(harness: &mut App) -> (Pos2, f64) {
    let ws = harness.state_mut().workspace();
    let state = &ws.plan_views[&ws.active_storey];
    (state.camera.to_screen(state.last_rect, ANCHOR), state.camera.scale)
}

fn centre(text: &LaidOutText) -> Pos2 {
    let [a, _, c, _] = text.quad;
    a + (c - a) / 2.0
}

fn click_at(harness: &mut App, pos: Pos2) {
    harness.hover_at(pos);
    harness.run_steps(2);
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

fn select(harness: &mut App, id: ElementId) {
    let ws = harness.state_mut().workspace();
    ws.selection.clear();
    ws.selection.insert(id);
    harness.run_steps(2);
}

/// Opens the combo box labelled `combo` and picks `option`.
fn choose(harness: &mut App, combo: &str, option: &str) {
    harness.get(By::new().role(Role::ComboBox).label(combo)).click();
    harness.run_steps(2);
    harness.get_by_label(option).click();
    harness.run_steps(3);
}

fn undo_len(harness: &mut App) -> usize {
    harness.state_mut().workspace().doc.history.undo_len()
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

/// ANN-007, VIEW-013, SEL-001, ELM-001: the plan draws a text added
/// through the model at its style's 2.5 mm paper size at 1:100 (the font
/// size is 0.25 m times the camera's points per metre) with its bottom-left
/// corner on the anchor; a click inside the text selects it and its quad is
/// highlighted; a size of 5 mm typed into the properties doubles its drawn
/// height, as one undo step; the drawing scale 1:50 halves it; Delete
/// removes the text as one undo step.
#[test]
fn a_text_is_drawn_at_paper_size_picked_resized_and_deleted() {
    let (mut harness, text) = shell_with_text("text-draw");
    harness.state_mut().workspace().selection.clear();
    harness.run_steps(2);
    let drawn = laid_out(&mut harness, text);
    let (anchor, points_per_metre) = anchor_on_screen(&mut harness);
    let expected = 0.25 * points_per_metre;
    assert!(
        (f64::from(drawn.size) - expected).abs() < 1e-3,
        "{} is not {expected}",
        drawn.size
    );
    let lines: Vec<&str> = drawn.lines.iter().map(|l| l.galley.text()).collect();
    assert_eq!(lines, ["Note"]);
    assert!(
        drawn.quad[3].distance(anchor) < 0.5,
        "the bottom left corner is on the anchor"
    );

    click_at(&mut harness, centre(&drawn));
    {
        let ws = harness.state_mut().workspace();
        assert_eq!(ws.selection.iter().copied().collect::<Vec<_>>(), [text]);
        let state = &ws.plan_views[&ws.active_storey];
        assert_eq!(
            state.selected_texts,
            [drawn.quad],
            "the selected text's quad is highlighted"
        );
    }

    let steps = undo_len(&mut harness);
    let size_field = || By::new().role(Role::TextInput).label("Size (mm)");
    harness.get(size_field()).click();
    harness.run_steps(1);
    harness.get(size_field()).type_text("5");
    harness.key_press(Key::Enter);
    harness.run_steps(3);
    assert_eq!(undo_len(&mut harness), steps + 1, "the size is one undo step");
    let element = &harness.state_mut().workspace().doc.project.elements[&text];
    assert_eq!(element.as_text().unwrap().size_mm, Some(5.0));
    let doubled = laid_out(&mut harness, text);
    assert!(
        close(doubled.size, drawn.size * 2.0),
        "{} is not twice {}",
        doubled.size,
        drawn.size
    );
    assert!(
        close(doubled.extent.y, drawn.extent.y * 2.0),
        "the drawn height doubles"
    );
    assert!(doubled.quad[3].distance(anchor) < 0.5, "the anchor stays");

    choose(&mut harness, "Drawing scale", "1:50");
    let halved = laid_out(&mut harness, text);
    assert_eq!(anchor_on_screen(&mut harness).1, points_per_metre, "the zoom stays");
    assert!(
        close(halved.size, doubled.size / 2.0),
        "{} is not half of {}",
        halved.size,
        doubled.size
    );
    assert!(close(halved.extent.y, doubled.extent.y / 2.0));

    let steps = undo_len(&mut harness);
    harness.key_press(Key::Delete);
    harness.run_steps(3);
    let ws = harness.state_mut().workspace();
    assert!(!ws.doc.project.elements.contains_key(&text), "Delete removes the text");
    assert_eq!(ws.doc.history.undo_len(), steps + 1, "deleting is one undo step");
    assert!(ws.plan_views[&ws.active_storey].texts.is_empty());
}

/// ANN-007, SEL-003: an area selection catches a text by its quad: from
/// left to right only when the quad lies fully inside, from right to left
/// already when the area touches it.
#[test]
fn an_area_selection_catches_a_text_by_its_quad() {
    let (mut harness, text) = shell_with_text("text-area");
    // Drags over the area that `area` gives for the text's current quad and
    // returns whether the text is selected. The area starts where egui
    // detects the drag, a few points after the press, so the areas leave a
    // wide margin.
    let drag = |harness: &mut App, area: &dyn Fn([Pos2; 4]) -> (Pos2, Pos2)| {
        harness.state_mut().workspace().selection.clear();
        harness.run_steps(2);
        let (from, to) = area(laid_out(harness, text).quad);
        harness.hover_at(from);
        harness.run_steps(1);
        harness.event(Event::PointerButton {
            pos: from,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        });
        harness.run_steps(1);
        for step in 1..=8 {
            harness.hover_at(from + (to - from) * (step as f32 / 8.0));
            harness.run_steps(1);
        }
        harness.event(Event::PointerButton {
            pos: to,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        });
        harness.run_steps(2);
        harness.state_mut().workspace().selection.contains(&text)
    };
    let pad = egui::vec2(20.0, 20.0);
    let half_x = |[top_left, top_right, ..]: [Pos2; 4]| (top_left.x + top_right.x) / 2.0;
    // Left to right around the whole quad: caught.
    assert!(drag(&mut harness, &|quad| (quad[0] - pad, quad[2] + pad)));
    // Left to right over the left half only: not caught.
    assert!(!drag(&mut harness, &|quad| {
        (quad[0] - pad, Pos2::new(half_x(quad), quad[3].y + pad.y))
    }));
    // Right to left from inside the left half: caught, as the area touches
    // the quad.
    assert!(drag(&mut harness, &|quad| {
        let middle_y = (quad[0].y + quad[3].y) / 2.0;
        (
            Pos2::new(half_x(quad), middle_y),
            Pos2::new(quad[0].x - pad.x, quad[3].y + pad.y),
        )
    }));
    // Right to left beside the text: not caught.
    assert!(!drag(&mut harness, &|quad| {
        (quad[0] - pad * 0.5, quad[0] - pad * 3.0)
    }));

    // Zoomed out until the text is below 1 pt and not laid out, an area
    // around its anchor still catches it.
    {
        let ws = harness.state_mut().workspace();
        let storey = ws.active_storey;
        ws.plan_views.get_mut(&storey).unwrap().camera.scale = 2.0;
        ws.selection.clear();
    }
    harness.run_steps(2);
    let (anchor, _) = anchor_on_screen(&mut harness);
    {
        let ws = harness.state_mut().workspace();
        assert!(
            ws.plan_views[&ws.active_storey].texts.is_empty(),
            "too small to lay out"
        );
    }
    let (from, to) = (anchor - pad, anchor + pad);
    harness.hover_at(from);
    harness.run_steps(1);
    harness.event(Event::PointerButton {
        pos: from,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    harness.run_steps(1);
    for step in 1..=8 {
        harness.hover_at(from + (to - from) * (step as f32 / 8.0));
        harness.run_steps(1);
    }
    harness.event(Event::PointerButton {
        pos: to,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    harness.run_steps(2);
    assert!(
        harness.state_mut().workspace().selection.contains(&text),
        "caught by its anchor"
    );
}

/// ANN-007, ELM-001, UND-001: content, style, anchor position and angle
/// changed in the properties strip are one undo step each, and the drawing
/// follows; the anchor stays fixed when the content grows by a line.
#[test]
fn properties_edit_content_style_anchor_position_and_angle() {
    let (mut harness, text) = shell_with_text("text-properties");
    select(&mut harness, text);
    let before = laid_out(&mut harness, text);
    let element = |harness: &mut App| harness.state_mut().workspace().doc.project.elements[&text].clone();
    assert!(harness.query_by_label("Material").is_none(), "a text has no material");

    // Content: a second line, committed with Ctrl/Cmd+Enter; the text grows
    // upwards from its bottom-left anchor.
    let steps = undo_len(&mut harness);
    let content = || By::new().role(Role::MultilineTextInput).value("Note");
    harness.get(content()).click();
    harness.run_steps(1);
    harness.get(content()).type_text("First line");
    harness.key_press(Key::Enter);
    harness.run_steps(1);
    harness
        .get(By::new().role(Role::MultilineTextInput).value("First line\n"))
        .type_text("Second line");
    harness.run_steps(1);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
    harness.run_steps(3);
    assert_eq!(undo_len(&mut harness), steps + 1, "the content is one undo step");
    assert_eq!(
        element(&mut harness).as_text().unwrap().content,
        "First line\nSecond line"
    );
    let grown = laid_out(&mut harness, text);
    let lines: Vec<&str> = grown.lines.iter().map(|l| l.galley.text()).collect();
    assert_eq!(lines, ["First line", "Second line"]);
    assert!(grown.extent.y > before.extent.y * 1.5, "two lines are higher than one");
    // The strip may grow with the field, which moves the view: the anchor's
    // screen point is taken anew.
    let (anchor, _) = anchor_on_screen(&mut harness);
    assert!(grown.quad[3].distance(anchor) < 0.5, "the anchor stays fixed");
    assert!(grown.quad[0].y < anchor.y - before.extent.y, "the block grows upwards");

    // Style: Heading 5.0 mm doubles the size.
    let steps = undo_len(&mut harness);
    choose(&mut harness, "Text style", "Heading 5.0 mm");
    assert_eq!(undo_len(&mut harness), steps + 1, "the style is one undo step");
    let heading = harness.state_mut().workspace().doc.project.text_styles[3].id;
    assert_eq!(element(&mut harness).as_text().unwrap().style, heading);
    assert!(close(laid_out(&mut harness, text).size, before.size * 2.0));

    // Anchor position: the top-left corner moves onto the anchor.
    let steps = undo_len(&mut harness);
    choose(&mut harness, "Anchor position", "Top left");
    assert_eq!(
        undo_len(&mut harness),
        steps + 1,
        "the anchor position is one undo step"
    );
    assert_eq!(element(&mut harness).as_text().unwrap().anchor_pos, AnchorPos::TopLeft);
    let top = laid_out(&mut harness, text);
    let (anchor, _) = anchor_on_screen(&mut harness);
    assert!(
        top.quad[0].distance(anchor) < 0.5,
        "the top left corner is on the anchor"
    );

    // Angle: 90° turns the reading direction to point up the screen.
    let before_angle = element(&mut harness);
    let steps = undo_len(&mut harness);
    let angle = || By::new().role(Role::TextInput).label("Angle");
    harness.get(angle()).click();
    harness.run_steps(1);
    harness.get(angle()).type_text("90");
    harness.key_press(Key::Enter);
    harness.run_steps(3);
    assert_eq!(undo_len(&mut harness), steps + 1, "the angle is one undo step");
    let turned = element(&mut harness).as_text().unwrap().angle;
    assert!((turned - std::f64::consts::FRAC_PI_2).abs() < 1e-9, "{turned}");
    let rotated = laid_out(&mut harness, text);
    let (anchor, _) = anchor_on_screen(&mut harness);
    let [a, b, ..] = rotated.quad;
    assert!(rotated.quad[0].distance(anchor) < 0.5, "the anchor stays");
    assert!(
        b.y < a.y - 1.0 && (b.x - a.x).abs() < 0.5,
        "the text reads upwards: {a:?} {b:?}"
    );

    // One undo reverts the angle alone, and redo brings it back.
    let edited = element(&mut harness);
    harness.state_mut().workspace().undo();
    harness.run_steps(3);
    let reverted = element(&mut harness);
    assert_eq!(reverted, before_angle, "undo restores the text before the angle edit");
    harness.state_mut().workspace().redo();
    harness.run_steps(3);
    assert_eq!(element(&mut harness), edited, "redo restores the turned text");
}

/// ANN-007, UI-002: clearing a text's Size returns it to its style's size
/// as one undo step; a Size field that shows a mixed value and is left
/// without typing changes nothing and records no step.
#[test]
fn the_size_field_clears_to_the_style_and_keeps_a_mixed_value() {
    let (mut harness, small) = shell_with_text("text-size");
    let large = {
        let ws = harness.state_mut().workspace();
        let ground = ws.doc.project.storeys.ground();
        let mut text = ws
            .doc
            .project
            .new_text(ground, ANCHOR + DVec2::new(0.0, 1.0), "Large", 1);
        text.as_text_mut().unwrap().size_mm = Some(5.0);
        let id = text.id;
        assert!(ws.execute(Command::AddElements(vec![text])));
        id
    };
    let size_field = || By::new().role(Role::TextInput).label("Size (mm)");
    let sizes = |harness: &mut App| {
        let elements = &harness.state_mut().workspace().doc.project.elements;
        [small, large].map(|id| elements[&id].as_text().unwrap().size_mm)
    };

    // Both selected: the field is mixed; focusing and leaving it keeps both.
    {
        let ws = harness.state_mut().workspace();
        ws.selection.clear();
        ws.selection.extend([small, large]);
    }
    harness.run_steps(2);
    assert_eq!(harness.get(size_field()).value().as_deref(), Some(""));
    let steps = undo_len(&mut harness);
    harness.get(size_field()).click();
    harness.run_steps(1);
    harness.key_press(Key::Tab);
    harness.run_steps(3);
    assert_eq!(
        sizes(&mut harness),
        [None, Some(5.0)],
        "a mixed field left empty keeps the values"
    );
    assert_eq!(undo_len(&mut harness), steps, "and records no step");

    // The large text alone: clearing its size returns it to the style's.
    select(&mut harness, large);
    let before = laid_out(&mut harness, large).size;
    let steps = undo_len(&mut harness);
    harness
        .get(By::new().role(Role::TextInput).label("Size (mm)").value("5"))
        .click();
    harness.run_steps(1);
    harness.key_press(Key::Backspace);
    harness.run_steps(1);
    harness.key_press(Key::Enter);
    harness.run_steps(3);
    assert_eq!(sizes(&mut harness), [None, None], "cleared to the style's size");
    assert_eq!(undo_len(&mut harness), steps + 1, "clearing is one undo step");
    assert!(
        close(laid_out(&mut harness, large).size, before / 2.0),
        "drawn at 2.5 mm again"
    );
}
