//! Text styles and the text tool's defaults (ANN-007): the template's
//! presets, the rules a style must keep and the defaults slot of the text
//! tool, which binds a new text to the plan view of its storey (ANN-002).

#![allow(clippy::unwrap_used)]

use std::collections::BTreeSet;

use archicraft_model::template::{default_metric_project, default_metric_project_with};
use archicraft_model::text::{TextStyle, check_text_styles, text_style_problems};
use archicraft_model::{
    Arg, Command, CommandError, DraftingView, ElementKind, History, KindTag, Message, TextAlign, TextStyleId,
};
use uuid::Uuid;

/// The explanation nested in a rejected tool-defaults change.
fn slot_reason(result: Result<(), CommandError>) -> Message {
    let Err(CommandError::Invalid(message)) = result else {
        panic!("expected CommandError::Invalid, got {result:?}");
    };
    assert_eq!(message.key(), "model-error-default-invalid");
    assert_eq!(message.arg("kind"), Some(&Arg::Kind(KindTag::Text)));
    match message.arg("reason") {
        Some(Arg::Nested(reason)) => (**reason).clone(),
        other => panic!("expected a nested reason, got {other:?}"),
    }
}

/// ANN-007: the template holds the four presets 'Standard 2.5 mm',
/// 'Dimension 2.5 mm', 'Small 1.8 mm' and 'Heading 5.0 mm', each
/// left-aligned with line spacing 1.2, and they are valid together.
#[test]
fn the_template_holds_the_four_text_style_presets() {
    let p = default_metric_project();
    let presets: Vec<(&str, f64, TextAlign, f64)> = p
        .text_styles
        .iter()
        .map(|s| (s.name.as_str(), s.size_mm, s.align, s.line_spacing))
        .collect();
    assert_eq!(
        presets,
        [
            ("Standard 2.5 mm", 2.5, TextAlign::Left, 1.2),
            ("Dimension 2.5 mm", 2.5, TextAlign::Left, 1.2),
            ("Small 1.8 mm", 1.8, TextAlign::Left, 1.2),
            ("Heading 5.0 mm", 5.0, TextAlign::Left, 1.2),
        ]
    );
    assert_eq!(check_text_styles(&p.text_styles), Ok(()));
    for style in &p.text_styles {
        assert_eq!(p.text_style(style.id), Some(style));
    }
    assert_eq!(p.text_style(TextStyleId::new()), None);
}

/// ANN-007, IFX-011: the presets are no IFCX nodes, so they keep fixed
/// identities: every project from the template, seeded or not, has the same
/// presets with the same identities, and references to them resolve in any
/// project made from the template.
#[test]
fn the_presets_have_the_same_identities_in_every_project() {
    let (a, b) = (default_metric_project(), default_metric_project());
    assert_ne!(a.id, b.id);
    assert_eq!(a.text_styles, b.text_styles);
    let seeded = default_metric_project_with(&mut |_| Uuid::new_v4());
    assert_eq!(seeded.text_styles, a.text_styles);
    let ids: BTreeSet<TextStyleId> = a.text_styles.iter().map(|s| s.id).collect();
    assert_eq!(ids.len(), 4, "distinct identities");
    assert!(ids.iter().all(|id| id.uuid().get_version_num() == 5));
}

/// ANN-007: a style needs a size and a line spacing that are positive
/// finite numbers and a name that is not blank; a list of styles needs
/// unique names and identities. Each broken rule has its own message.
#[test]
fn a_text_style_needs_a_positive_size_and_line_spacing_and_a_unique_name() {
    let p = default_metric_project();
    let standard = p.text_styles[0].clone();
    assert_eq!(standard.check(), Ok(()));
    let with = |change: &dyn Fn(&mut TextStyle)| {
        let mut style = standard.clone();
        change(&mut style);
        style
    };
    for (style, key) in [
        (with(&|s| s.size_mm = 0.0), "model-error-text-style-size"),
        (with(&|s| s.size_mm = -2.5), "model-error-text-style-size"),
        (with(&|s| s.size_mm = f64::NAN), "model-error-text-style-size"),
        (with(&|s| s.size_mm = f64::INFINITY), "model-error-text-style-size"),
        (with(&|s| s.line_spacing = 0.0), "model-error-text-style-line-spacing"),
        (
            with(&|s| s.line_spacing = f64::NAN),
            "model-error-text-style-line-spacing",
        ),
        (with(&|s| s.name.clear()), "model-error-text-style-name"),
        (with(&|s| s.name = "  ".into()), "model-error-text-style-name"),
    ] {
        let error = style.check().unwrap_err();
        assert_eq!(error.key(), key, "{style:?}");
        let mut styles = p.text_styles.clone();
        styles[0] = style;
        assert_eq!(check_text_styles(&styles).unwrap_err().key(), key);
    }

    let mut same_name = p.text_styles.clone();
    same_name[1].name = standard.name.clone();
    let error = check_text_styles(&same_name).unwrap_err();
    assert_eq!(error.key(), "model-error-text-style-duplicate-name");
    assert_eq!(error.arg("name"), Some(&Arg::Text("Standard 2.5 mm".into())));

    let mut same_id = p.text_styles.clone();
    same_id[1].id = standard.id;
    assert_eq!(
        check_text_styles(&same_id).unwrap_err().key(),
        "model-error-duplicate-text-style-id"
    );
}

/// ELM-001, ANN-002, ANN-007: the text tool's slot passes the tool-defaults
/// check although its view is a placeholder: a text made from it on any
/// storey belongs to that storey's plan view. A slot whose style the
/// project does not have, or whose size override is not positive, is
/// rejected with the reason.
#[test]
fn the_text_tools_defaults_pass_the_tool_defaults_check() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let slot = p.tool_defaults.get(KindTag::Text).unwrap();
    let style = slot.kind.text_style().unwrap();
    assert_eq!(p.text_style(style).unwrap().name, "Standard 2.5 mm");
    assert_eq!(p.layer(slot.layer).unwrap().name, "2D construction");
    let upper = p.storeys.as_slice()[1].id;
    let prototype = p.new_element(KindTag::Text, upper, 0).unwrap();
    assert_eq!(prototype.as_text().unwrap().view, DraftingView::plan(upper));

    let unchanged = p.tool_defaults.clone();
    assert_eq!(
        h.execute(&mut p, Command::SetToolDefaults(unchanged)),
        Err(CommandError::NoChange)
    );

    let mut larger = p.tool_defaults.clone();
    if let ElementKind::Text(text) = &mut larger.get_mut(KindTag::Text).unwrap().kind {
        text.size_mm = Some(3.5);
    }
    h.execute(&mut p, Command::SetToolDefaults(larger)).unwrap();
    let made = p.new_text(p.storeys.ground(), Default::default(), "Hall", 0);
    assert_eq!(made.as_text().unwrap().size_mm, Some(3.5));

    let mut missing = p.tool_defaults.clone();
    if let ElementKind::Text(text) = &mut missing.get_mut(KindTag::Text).unwrap().kind {
        text.style = TextStyleId::new();
    }
    let reason = slot_reason(h.execute(&mut p, Command::SetToolDefaults(missing)));
    assert_eq!(reason.key(), "model-error-text-style-missing");

    let mut flat = p.tool_defaults.clone();
    if let ElementKind::Text(text) = &mut flat.get_mut(KindTag::Text).unwrap().kind {
        text.size_mm = Some(0.0);
    }
    let reason = slot_reason(h.execute(&mut p, Command::SetToolDefaults(flat)));
    assert_eq!(reason.key(), "model-error-text-size");
}

/// ANN-007, PRJ-008: every style of a list that breaks a rule is named by
/// its place with the reason that `check_text_styles` gives: one that fails
/// `TextStyle::check`, and one that repeats the name or the identity of an
/// earlier style that is fine. A broken style claims no name, so a later
/// style of its name is fine. Without the named styles the list passes the
/// check, and a list without a broken style names none.
#[test]
fn every_style_that_breaks_a_rule_is_named_with_its_reason() {
    let p = default_metric_project();
    assert_eq!(text_style_problems(&p.text_styles), []);
    let standard = p.text_styles[0].clone();
    let mut flat = standard.clone();
    flat.size_mm = 0.0;
    let mut same_name = p.text_styles[1].clone();
    same_name.name = standard.name.clone();
    let mut same_id = p.text_styles[2].clone();
    same_id.id = standard.id;
    let mut unnamed = p.text_styles[3].clone();
    unnamed.name = " ".into();
    let styles = vec![
        flat.clone(),
        standard.clone(),
        same_name,
        same_id,
        unnamed,
        p.text_styles[3].clone(),
    ];
    let problems: Vec<(usize, &str)> = text_style_problems(&styles)
        .iter()
        .map(|(index, reason)| (*index, reason.key()))
        .collect();
    assert_eq!(
        problems,
        [
            (0, "model-error-text-style-size"),
            (2, "model-error-text-style-duplicate-name"),
            (3, "model-error-duplicate-text-style-id"),
            (4, "model-error-text-style-name"),
        ]
    );
    assert_eq!(
        check_text_styles(&styles).unwrap_err(),
        text_style_problems(&styles)[0].1,
        "the check fails with the first problem"
    );
    let kept: Vec<TextStyle> = styles
        .iter()
        .enumerate()
        .filter(|(index, _)| ![0, 2, 3, 4].contains(index))
        .map(|(_, style)| style.clone())
        .collect();
    assert_eq!(check_text_styles(&kept), Ok(()));
    assert_eq!(kept, [standard, p.text_styles[3].clone()]);
}

/// NFR-REL-004, UI-012, ANN-007: a text whose style the project does not
/// have is an integrity problem that names the text; a text whose style
/// exists is none.
#[test]
fn a_text_whose_style_is_missing_is_an_integrity_problem() {
    let mut p = default_metric_project();
    let text = p.new_text(p.storeys.ground(), Default::default(), "Hall", 0);
    let id = text.id;
    History::default()
        .execute(&mut p, Command::AddElements(vec![text]))
        .unwrap();
    assert_eq!(p.integrity_problems(), []);
    p.elements[&id].kind.set_text_style(TextStyleId::new());
    let problems = p.integrity_problems();
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(problems[0].element, Some(id));
    assert_eq!(problems[0].relation, None);
    assert_eq!(problems[0].message.key(), "model-problem-text-style-missing");
    assert_eq!(problems[0].message.arg("name"), Some(&Arg::Text("TX-001".into())));
}

/// ANN-007: the text style of an element can be replaced, as its material
/// can; a kind without text styles ignores it.
#[test]
fn a_texts_style_is_replaced_and_other_kinds_ignore_it() {
    let p = default_metric_project();
    let mut text = p.new_text(p.storeys.ground(), Default::default(), "Hall", 0);
    let heading = p.text_styles[3].id;
    text.kind.set_text_style(heading);
    assert_eq!(text.kind.text_style(), Some(heading));
    let mut wall = p.new_wall(
        p.storeys.ground(),
        Default::default(),
        archicraft_model::glam::DVec2::X,
        0,
    );
    let before = wall.clone();
    wall.kind.set_text_style(heading);
    assert_eq!(wall, before);
    assert_eq!(wall.kind.text_style(), None);
}
