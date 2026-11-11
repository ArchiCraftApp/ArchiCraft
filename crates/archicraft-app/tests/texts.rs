//! Texts in the application (ANN-007): a text block drawn in the plan view
//! at its paper size, picked by a click inside it, deleted, and edited in
//! the properties strip; the Text tool and its inline editor, which creates
//! a text and edits one on a double-click; a text saved and reopened.

#![allow(clippy::unwrap_used)]

mod common;

use archicraft_app::tools::ToolKind;
use archicraft_app::views::annotation_draw::LaidOutText;
use archicraft_app::{ArchiCraftApp, tr};
use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::WriteOptions;
use archicraft_model::glam::DVec2;
use archicraft_model::{AnchorPos, Command, ElementId, samples};
use common::{assert_present, data_dir, harness};
use egui::accesskit::Role;
use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, Queryable};

type App = Harness<'static, ArchiCraftApp>;

/// Where the test text's anchor lies: in the room west of the interior wall
/// of the ground storey, clear of every line of the plan.
const ANCHOR: DVec2 = DVec2::new(1.5, 2.5);

/// Opens two_storey_shell from an IFCX file under `name`, showing the plan
/// of its ground storey.
fn shell(name: &str) -> App {
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
    assert_eq!(ws.active_storey, ws.doc.project.storeys.ground());
    harness
}

/// Opens two_storey_shell from an IFCX file under `name` and adds the text
/// "Note" with its anchor at [`ANCHOR`] on the ground storey through the
/// model, with the text tool's defaults (style "Standard 2.5 mm",
/// bottom-left anchor position, angle 0); returns its id.
fn shell_with_text(name: &str) -> (App, ElementId) {
    let mut harness = shell(name);
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

/// The screen point of the plan point `p` in the ground storey's plan.
fn on_screen(harness: &mut App, p: DVec2) -> Pos2 {
    let ws = harness.state_mut().workspace();
    let state = &ws.plan_views[&ws.active_storey];
    state.camera.to_screen(state.last_rect, p)
}

/// Types `text` key by key as a keyboard does: the key's press, the typed
/// character and the release, one frame each character.
fn type_keys(harness: &mut App, text: &str) {
    for c in text.chars() {
        let key = match c {
            ' ' => Some(Key::Space),
            c => Key::from_name(&c.to_ascii_uppercase().to_string()),
        };
        let key_event = |pressed| Event::Key {
            key: key.unwrap(),
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        if key.is_some() {
            harness.event(key_event(true));
        }
        harness.event(Event::Text(c.to_string()));
        if key.is_some() {
            harness.event(key_event(false));
        }
        harness.run_steps(1);
    }
    harness.run_steps(1);
}

/// Double-clicks at `pos`: each click presses and releases in one frame, so
/// the second one, a frame (0.25 s) later, is the second click of a
/// double-click.
fn double_click_at(harness: &mut App, pos: Pos2) {
    harness.hover_at(pos);
    harness.run_steps(2);
    for _ in 0..2 {
        let click = [true, false].map(|pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        harness.input_mut().events.extend(click);
        harness.run_steps(1);
    }
    assert!(
        harness
            .ctx
            .input(|i| i.pointer.button_double_clicked(PointerButton::Primary)),
        "egui saw a double-click"
    );
    harness.run_steps(2);
}

fn tool(harness: &mut App) -> ToolKind {
    harness.state_mut().workspace().tool.kind
}

/// The content of the open text editor, `None` while none is open.
fn editor(harness: &mut App) -> Option<String> {
    let ws = harness.state_mut().workspace();
    ws.tool.editor.as_ref().map(|e| e.content.clone())
}

/// The texts of the project.
fn texts(harness: &mut App) -> Vec<(ElementId, archicraft_model::Text)> {
    let project = &harness.state_mut().workspace().doc.project;
    project
        .elements
        .values()
        .filter_map(|e| Some((e.id, e.as_text()?.clone())))
        .collect()
}

/// ANN-007, UI-008, UND-001, INP-001 (demo 2, step 3): T selects the Text
/// tool; a click in the plan opens the inline editor at the clicked point,
/// at the text's drawn size; while typing, W, S and Space type letters and
/// switch no tool; Enter starts a second line and Ctrl/Cmd+Enter commits
/// the two lines as one undo step, a text with its anchor where clicked,
/// drawn at 2.5 mm paper size at 1:100 (the font size is 0.25 m times the
/// camera's points per metre). Esc in a fresh editor discards the typed
/// text: it creates nothing and records no undo step; a click outside the
/// editor commits it and opens no other.
#[test]
fn the_text_tool_places_a_typed_text_as_one_undo_step() {
    let mut harness = shell("text-tool");
    harness.key_press(Key::T);
    harness.run_steps(2);
    assert_eq!(tool(&mut harness), ToolKind::Text, "T selects the Text tool");
    assert!(texts(&mut harness).is_empty());

    let (anchor, points_per_metre) = anchor_on_screen(&mut harness);
    click_at(&mut harness, anchor);
    assert_eq!(editor(&mut harness).as_deref(), Some(""), "the click opens the editor");
    let (rect, size) = {
        let ws = harness.state_mut().workspace();
        ws.plan_views[&ws.active_storey].editor.unwrap()
    };
    assert!(
        (f64::from(size) - 0.25 * points_per_metre).abs() < 1e-3,
        "the editor types at the drawn size: {size}"
    );
    assert!(
        (rect.left() - anchor.x).abs() < 1.0 && (rect.bottom() - anchor.y).abs() < 1.0,
        "the editor's bottom left corner is on the anchor: {rect:?} {anchor:?}"
    );
    harness.run_steps(1);
    assert!(harness.ctx.text_edit_focused(), "the editor has the keyboard");

    let steps = undo_len(&mut harness);
    type_keys(&mut harness, "West wall");
    harness.key_press(Key::Enter);
    harness.run_steps(1);
    type_keys(&mut harness, "Shaft");
    assert_eq!(tool(&mut harness), ToolKind::Text, "W and S switch no tool");
    assert_eq!(editor(&mut harness).as_deref(), Some("West wall\nShaft"));
    assert!(texts(&mut harness).is_empty(), "nothing is created while typing");
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
    harness.run_steps(3);

    assert_eq!(editor(&mut harness), None, "Ctrl/Cmd+Enter closes the editor");
    assert_eq!(undo_len(&mut harness), steps + 1, "one undo step");
    let created = texts(&mut harness);
    let [(id, text)] = created.as_slice() else {
        panic!("one text: {created:?}")
    };
    assert_eq!(text.content, "West wall\nShaft");
    assert!(
        text.anchor.distance(ANCHOR) < 0.05,
        "anchored where clicked: {}",
        text.anchor
    );
    let drawn = laid_out(&mut harness, *id);
    assert!(
        (f64::from(drawn.size) - 0.25 * points_per_metre).abs() < 1e-3,
        "drawn at 2.5 mm: {}",
        drawn.size
    );
    let lines: Vec<&str> = drawn.lines.iter().map(|l| l.galley.text()).collect();
    assert_eq!(lines, ["West wall", "Shaft"]);
    harness.state_mut().workspace().undo();
    harness.run_steps(2);
    assert!(texts(&mut harness).is_empty(), "one undo removes the text");
    harness.state_mut().workspace().redo();
    harness.run_steps(2);
    assert_eq!(texts(&mut harness), created, "redo brings the text back");
    harness.state_mut().workspace().undo();
    harness.run_steps(2);
    assert_eq!(tool(&mut harness), ToolKind::Text, "the tool stays");

    // Esc in a fresh editor discards what was typed: it creates nothing
    // and records no step.
    let steps = undo_len(&mut harness);
    let elsewhere = on_screen(&mut harness, ANCHOR + DVec2::new(0.5, -1.0));
    click_at(&mut harness, elsewhere);
    assert_eq!(editor(&mut harness).as_deref(), Some(""));
    harness.run_steps(1);
    type_keys(&mut harness, "Draft");
    assert_eq!(editor(&mut harness).as_deref(), Some("Draft"));
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert_eq!(editor(&mut harness), None, "Esc closes the editor");
    assert!(texts(&mut harness).is_empty(), "Esc creates nothing");
    assert_eq!(undo_len(&mut harness), steps);
    assert_eq!(tool(&mut harness), ToolKind::Text, "Esc in the editor keeps the tool");

    // A click outside the editor commits it and opens no other editor.
    click_at(&mut harness, anchor);
    harness.run_steps(1);
    type_keys(&mut harness, "Stair");
    click_at(&mut harness, elsewhere);
    assert_eq!(editor(&mut harness), None, "the click outside opens no editor");
    assert_eq!(undo_len(&mut harness), steps + 1, "one undo step");
    let created = texts(&mut harness);
    let [(_, text)] = created.as_slice() else {
        panic!("one text: {created:?}")
    };
    assert_eq!(text.content, "Stair");
    assert!(text.anchor.distance(ANCHOR) < 0.05, "{}", text.anchor);
}

/// ANN-007, UND-001: a double-click on an unselected text with the
/// selection tool opens the inline editor with its content in place of the
/// drawn text, although the first click's selection grows the properties
/// strip; a second line typed and committed with Ctrl/Cmd+Enter is one undo
/// step that changes the content alone, which undo and redo take back and
/// restore. With the Text tool, a click on the text opens its editor too,
/// rather than a new text's; Esc after typing there leaves the text as it
/// was and records no undo step.
#[test]
fn a_double_click_on_a_text_edits_its_content_as_one_undo_step() {
    let (mut harness, text) = shell_with_text("text-double-click");
    harness.state_mut().workspace().selection.clear();
    harness.run_steps(2);
    assert_eq!(tool(&mut harness), ToolKind::Select);
    let before = harness.state_mut().workspace().doc.project.elements[&text].clone();
    let pos = centre(&laid_out(&mut harness, text));
    double_click_at(&mut harness, pos);
    assert_eq!(
        editor(&mut harness).as_deref(),
        Some("Note"),
        "the editor shows the content"
    );
    {
        let ws = harness.state_mut().workspace();
        let state = &ws.plan_views[&ws.active_storey];
        assert!(state.editor.is_some());
        assert!(
            state.texts.iter().all(|(id, _)| *id != text),
            "the editor stands in for the drawn text"
        );
    }
    assert!(harness.ctx.text_edit_focused(), "the editor has the keyboard");

    let steps = undo_len(&mut harness);
    harness.key_press(Key::End);
    harness.key_press(Key::Enter);
    harness.run_steps(1);
    type_keys(&mut harness, "more");
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
    harness.run_steps(3);
    assert_eq!(editor(&mut harness), None);
    assert_eq!(undo_len(&mut harness), steps + 1, "one undo step");
    let mut expected = before.clone();
    expected.as_text_mut().unwrap().content = "Note\nmore".into();
    let element = harness.state_mut().workspace().doc.project.elements[&text].clone();
    assert_eq!(element, expected, "only the content changes");
    let lines: Vec<String> = laid_out(&mut harness, text)
        .lines
        .iter()
        .map(|l| l.galley.text().to_owned())
        .collect();
    assert_eq!(lines, ["Note", "more"], "drawn again with both lines");
    harness.state_mut().workspace().undo();
    harness.run_steps(2);
    let element = harness.state_mut().workspace().doc.project.elements[&text].clone();
    assert_eq!(element, before, "undo restores the content");
    harness.state_mut().workspace().redo();
    harness.run_steps(2);
    let element = harness.state_mut().workspace().doc.project.elements[&text].clone();
    assert_eq!(element, expected, "redo restores the edit");

    // The Text tool: a click on the text edits it; Esc leaves it as it is.
    harness.state_mut().workspace().select_tool(ToolKind::Text);
    harness.run_steps(2);
    let pos = centre(&laid_out(&mut harness, text));
    click_at(&mut harness, pos);
    assert_eq!(
        editor(&mut harness).as_deref(),
        Some("Note\nmore"),
        "the click edits the text"
    );
    harness.run_steps(1);
    let steps = undo_len(&mut harness);
    harness.key_press(Key::End);
    type_keys(&mut harness, " draft");
    assert_eq!(editor(&mut harness).as_deref(), Some("Note\nmore draft"));
    harness.key_press(Key::Escape);
    harness.run_steps(2);
    assert_eq!(editor(&mut harness), None);
    assert_eq!(texts(&mut harness).len(), 1, "no new text");
    let element = harness.state_mut().workspace().doc.project.elements[&text].clone();
    assert_eq!(element, expected, "Esc discards the typed change");
    assert_eq!(undo_len(&mut harness), steps, "Esc records no step");
}

/// ANN-007, UND-001: with the editor open on the text "Note", a
/// double-click on the text "Other" commits the typed edit of "Note" as one
/// undo step and opens the editor on "Other", not on "Note" again: the click
/// that commits the editor selects nothing, so the double-click acts on what
/// its second click hits.
#[test]
fn a_double_click_on_another_text_commits_the_editor_and_edits_that_text() {
    let (mut harness, note) = shell_with_text("text-double-click-other");
    let other = {
        let ws = harness.state_mut().workspace();
        let ground = ws.doc.project.storeys.ground();
        let text = ws.doc.project.new_text(ground, DVec2::new(8.0, 4.5), "Other", 1);
        let id = text.id;
        assert!(ws.execute(Command::AddElements(vec![text])));
        ws.selection.clear();
        id
    };
    harness.run_steps(2);
    let (at_note, at_other) = (
        centre(&laid_out(&mut harness, note)),
        centre(&laid_out(&mut harness, other)),
    );
    double_click_at(&mut harness, at_note);
    assert_eq!(editor(&mut harness).as_deref(), Some("Note"));
    harness.key_press(Key::End);
    type_keys(&mut harness, "s");
    assert_eq!(editor(&mut harness).as_deref(), Some("Notes"));
    let steps = undo_len(&mut harness);

    double_click_at(&mut harness, at_other);
    assert_eq!(editor(&mut harness).as_deref(), Some("Other"), "the editor is on Other");
    let ws = harness.state_mut().workspace();
    assert_eq!(ws.tool.editor.as_ref().map(|e| e.element()), Some(other));
    let content = |id: ElementId| ws.doc.project.elements[&id].as_text().unwrap().content.clone();
    assert_eq!(content(note), "Notes", "the edit of Note is committed");
    assert_eq!(content(other), "Other");
    assert_eq!(ws.doc.history.undo_len(), steps + 1, "one undo step");
    assert_eq!(ws.selection.iter().copied().collect::<Vec<_>>(), [other]);
    assert!(harness.ctx.text_edit_focused(), "the editor has the keyboard");
}

/// The status line while a text editor is open: its prompt with its Enter,
/// Ctrl/Cmd+Enter and Esc keys.
const EDITOR_STATUS: &str = "Text: type the text, a click outside places it · Enter starts a new line \
     · Ctrl/Cmd+Enter places the text · Esc cancels the text";

/// ANN-007, UI-008, NFR-USE-003, SEL-003: with the selection tool, the editor a
/// double-click opens on the text "Note" shows its prompt and its Enter,
/// Ctrl/Cmd+Enter and Esc keys in the status line, as with the Text tool,
/// and S typed there switches no tool (UI-008); a press-and-drag in the plan around the text "Other" commits the typed
/// edit as one undo step and does nothing else: it starts no area
/// selection, so "Other" stays unselected. Once the editor is closed, the
/// status line is the selection tool's again.
#[test]
fn with_the_selection_tool_the_editor_shows_its_keys_and_a_committing_drag_selects_nothing() {
    let (mut harness, note) = shell_with_text("text-select-drag");
    let other = {
        let ws = harness.state_mut().workspace();
        let ground = ws.doc.project.storeys.ground();
        let text = ws.doc.project.new_text(ground, DVec2::new(8.0, 4.5), "Other", 1);
        let id = text.id;
        assert!(ws.execute(Command::AddElements(vec![text])));
        ws.selection.clear();
        id
    };
    harness.run_steps(2);
    assert_eq!(tool(&mut harness), ToolKind::Select);
    let at_note = centre(&laid_out(&mut harness, note));
    double_click_at(&mut harness, at_note);
    assert_eq!(editor(&mut harness).as_deref(), Some("Note"));
    assert_present(&harness, EDITOR_STATUS);
    harness.key_press(Key::End);
    type_keys(&mut harness, "s");
    assert_eq!(editor(&mut harness).as_deref(), Some("Notes"));
    assert_eq!(
        tool(&mut harness),
        ToolKind::Select,
        "S types a letter and switches no tool"
    );
    let steps = undo_len(&mut harness);

    // A drag that would catch "Other" left to right, as in
    // an_area_selection_catches_a_text_by_its_quad.
    let (from, to) = {
        let quad = laid_out(&mut harness, other).quad;
        let pad = egui::vec2(20.0, 20.0);
        (quad[0] - pad, quad[2] + pad)
    };
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

    assert_eq!(editor(&mut harness), None, "the press commits the editor");
    let ws = harness.state_mut().workspace();
    assert_eq!(ws.doc.history.undo_len(), steps + 1, "one undo step");
    assert_eq!(ws.doc.project.elements[&note].as_text().unwrap().content, "Notes");
    assert!(
        !ws.selection.contains(&other),
        "the committing drag starts no area selection"
    );
    assert!(harness.query_all_by_label(EDITOR_STATUS).next().is_none());
    assert_present(&harness, &tr!("status-prompt-select"));
}

/// ANN-007, UND-003: switching tools with the toolbox while a text is typed
/// commits it as one undo step and switches the tool.
#[test]
fn switching_tools_commits_the_text_editor() {
    let mut harness = shell("text-switch-tool");
    harness.state_mut().workspace().select_tool(ToolKind::Text);
    harness.run_steps(2);
    let (anchor, _) = anchor_on_screen(&mut harness);
    click_at(&mut harness, anchor);
    harness.run_steps(1);
    type_keys(&mut harness, "Hall");
    assert_eq!(editor(&mut harness).as_deref(), Some("Hall"));
    let steps = undo_len(&mut harness);

    harness.get(By::new().role(Role::Button).label("Wall")).click();
    harness.run_steps(2);
    assert_eq!(tool(&mut harness), ToolKind::Wall, "the toolbox switches the tool");
    assert_eq!(editor(&mut harness), None, "the editor is closed");
    assert_eq!(undo_len(&mut harness), steps + 1, "one undo step");
    let contents: Vec<String> = texts(&mut harness).into_iter().map(|(_, t)| t.content).collect();
    assert_eq!(contents, ["Hall"], "the typed text is created");
}

/// Whether the last frame asked the root viewport to cancel its close.
fn close_cancelled(harness: &App) -> bool {
    harness
        .output()
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .is_some_and(|v| v.commands.contains(&egui::ViewportCommand::CancelClose))
}

/// Asks the window to close, as its close button does, for one frame.
fn request_close(harness: &mut App) {
    let root = harness.input_mut().viewports.entry(egui::ViewportId::ROOT).or_default();
    root.events.push(egui::ViewportEvent::Close);
    harness.run_steps(1);
}

/// ANN-007, PRJ-003: closing the window while a text is typed commits the
/// text first, which then counts as unsaved: the close waits for the
/// Save changes? prompt. While the project rejects the text (here a size
/// of 0 mm), the window stays open with the editor and its content and no
/// prompt, and the status line names the error.
#[test]
fn closing_the_window_commits_the_text_editor_or_stays_open() {
    let mut harness = shell("text-close");
    harness.state_mut().workspace().select_tool(ToolKind::Text);
    harness.run_steps(2);
    let (anchor, _) = anchor_on_screen(&mut harness);
    click_at(&mut harness, anchor);
    harness.run_steps(1);
    type_keys(&mut harness, "Hall");
    assert!(!harness.state_mut().workspace().doc.is_dirty());
    let editing = |harness: &mut App| harness.state_mut().workspace().tool.editor.clone().unwrap();
    let mut rejected = editing(&mut harness);
    rejected.text.as_text_mut().unwrap().size_mm = Some(0.0);
    harness.state_mut().workspace().tool.editor = Some(rejected);

    request_close(&mut harness);
    assert!(close_cancelled(&harness), "the window stays open");
    assert_eq!(editor(&mut harness).as_deref(), Some("Hall"), "with the editor");
    assert!(texts(&mut harness).is_empty());
    let status = harness.state_mut().workspace().status.clone().unwrap();
    assert!(status.error, "{status:?}");
    harness.run_steps(2);
    assert!(harness.query_by_label("Save changes?").is_none(), "no prompt");

    let mut valid = editing(&mut harness);
    valid.text.as_text_mut().unwrap().size_mm = None;
    harness.state_mut().workspace().tool.editor = Some(valid);
    request_close(&mut harness);
    assert!(close_cancelled(&harness), "the close waits for the prompt");
    assert_eq!(editor(&mut harness), None, "the editor is committed");
    let contents: Vec<String> = texts(&mut harness).into_iter().map(|(_, t)| t.content).collect();
    assert_eq!(contents, ["Hall"]);
    harness.run_steps(2);
    assert!(harness.query_by_label("Save changes?").is_some(), "the prompt asks");
}

/// ANN-007, PRJ-003 (demo 2, step 4): a project with a text of 5 mm saves
/// and reopens with the text at the same anchor with the same content,
/// style and size, drawn at the same size.
#[test]
fn a_text_saves_and_reopens_with_its_anchor_content_and_size() {
    let (mut harness, text) = shell_with_text("text-save");
    {
        let ws = harness.state_mut().workspace();
        let mut element = ws.doc.project.elements[&text].clone();
        let t = element.as_text_mut().unwrap();
        t.content = "Living\nroom".into();
        t.size_mm = Some(5.0);
        assert!(ws.execute(Command::UpdateElements(vec![element])));
    }
    harness.run_steps(2);
    let drawn = laid_out(&mut harness, text);
    let saved = harness.state_mut().workspace().doc.project.elements[&text].clone();
    let dir = data_dir("text-save-file");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("with-text.ifcx");
    assert!(harness.state_mut().save_to(&path));

    let mut reopened = common::harness("text-save-reopened");
    reopened.state_mut().open_path(&path);
    reopened.run_steps(3);
    let ws = reopened.state_mut().workspace();
    let element = ws.doc.project.elements.get(&text).expect("the text is back").clone();
    let (t, s) = (element.as_text().unwrap(), saved.as_text().unwrap());
    assert_eq!(t.anchor, s.anchor, "the same anchor");
    assert_eq!(t.content, "Living\nroom", "the same content");
    assert_eq!(t.size_mm, Some(5.0), "the same size");
    assert_eq!(t.style, s.style);
    assert_eq!(element.storey, saved.storey);
    let again = laid_out(&mut reopened, text);
    assert!(close(again.size, drawn.size), "{} is not {}", again.size, drawn.size);

    // Ctrl/Cmd+S while a text is typed saves it: the editor commits first.
    reopened.state_mut().workspace().select_tool(ToolKind::Text);
    reopened.run_steps(2);
    let elsewhere = on_screen(&mut reopened, ANCHOR + DVec2::new(0.5, -1.0));
    click_at(&mut reopened, elsewhere);
    reopened.run_steps(1);
    type_keys(&mut reopened, "Hall");
    reopened.key_press_modifiers(Modifiers::COMMAND, Key::S);
    reopened.run_steps(3);
    assert_eq!(editor(&mut reopened), None, "saving commits the editor");
    let contents: Vec<String> = texts(&mut reopened).into_iter().map(|(_, t)| t.content).collect();
    assert!(contents.contains(&"Hall".to_owned()), "{contents:?}");
    assert!(!reopened.state_mut().workspace().doc.is_dirty(), "the text is saved");
}

/// ANN-007, UI-002: with the Text tool and nothing selected, the properties
/// strip edits the tool's defaults, the style among them, without a
/// Content row: each text gets its content in the inline editor.
#[test]
fn the_text_tools_defaults_have_no_content_row() {
    let mut harness = shell("text-defaults");
    harness.state_mut().workspace().select_tool(ToolKind::Text);
    harness.run_steps(3);
    assert!(
        harness
            .query(By::new().role(Role::ComboBox).label("Text style"))
            .is_some(),
        "the defaults' style is shown"
    );
    assert!(harness.query_by_label("Content").is_none(), "no Content row");
    assert!(
        harness.query(By::new().role(Role::MultilineTextInput)).is_none(),
        "no content field"
    );
}
