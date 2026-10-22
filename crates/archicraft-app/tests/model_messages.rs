//! The English catalogue covers every message key archicraft-model emits,
//! with the arguments the model passes (NFR-L10N-001).

#![allow(clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};

use archicraft_app::i18n::{self, Localizer, catalogue, model::fluent_names};
use archicraft_model::message::ALL_KEYS;
use archicraft_model::units::{
    AngleUnit, AreaUnit, LengthUnit, NumberFormat, ParseError, SlopeFormat, UnitSettings, VolumeUnit, format_length,
};
use archicraft_model::{IfcClass, KindTag, Message, ReferenceLine};
use fluent_syntax::ast::{Entry, Expression, InlineExpression, Pattern, PatternElement};
use fluent_syntax::parser;

/// A representative message for every key in `ALL_KEYS`, with the arguments
/// the model passes.
fn representatives() -> Vec<Message> {
    let count = |key| Message::new(key).count("count", 2);
    let mut messages = vec![
        Message::new("model-label-create-kind").kind("kind", KindTag::Wall),
        count("model-label-create-elements"),
        count("model-label-delete-elements"),
        count("model-label-edit-elements"),
        count("model-label-move-elements"),
    ];
    messages.extend(
        [
            "model-label-insert-storey",
            "model-label-delete-storey",
            "model-label-rename-storey",
            "model-label-storey-elevation",
            "model-label-storey-height",
            "model-label-cut-plane",
            "model-label-new-layer",
            "model-label-change-layer",
            "model-label-delete-layer",
            "model-label-new-material",
            "model-label-change-material",
            "model-label-delete-material",
            "model-label-tool-settings",
            "model-label-project-settings",
            "model-label-project-info",
            "model-label-spatial-names",
            "model-label-location",
        ]
        .map(Message::new),
    );
    for &kind in KindTag::ALL {
        messages.push(Message::new(kind.label_key()));
        // The noun takes a plain count for its plural, rendered by the
        // sentence around it; see i18n::model.
        messages.push(Message::new(kind.noun_key()));
    }
    messages.extend(error_representatives());
    messages.extend(label_representatives());
    messages
}

/// The names of units, IFC classes and wall reference lines, and the parse
/// errors of typed lengths and angles (units.rs, kinds/wall.rs).
fn label_representatives() -> Vec<Message> {
    let mut keys: Vec<&'static str> = Vec::new();
    keys.extend(LengthUnit::ALL.map(LengthUnit::label_key));
    keys.extend(AngleUnit::ALL.map(AngleUnit::label_key));
    keys.extend(SlopeFormat::ALL.map(SlopeFormat::label_key));
    keys.extend(AreaUnit::ALL.map(AreaUnit::label_key));
    keys.extend(VolumeUnit::ALL.map(VolumeUnit::label_key));
    keys.extend(
        [
            IfcClass::Wall,
            IfcClass::Slab,
            IfcClass::Footing,
            IfcClass::Covering,
            IfcClass::BuildingElementProxy,
        ]
        .map(IfcClass::label_key),
    );
    keys.extend(
        [
            ReferenceLine::Outside,
            ReferenceLine::Center,
            ReferenceLine::Inside,
            ReferenceLine::Custom(0.1),
        ]
        .map(ReferenceLine::label_key),
    );
    let mut messages: Vec<Message> = keys.into_iter().map(Message::new).collect();
    messages.extend(
        [
            ParseError::Empty,
            ParseError::InvalidLength("3 furlongs".into()),
            ParseError::InvalidAngle("north".into()),
            ParseError::InvalidSlope("steep".into()),
        ]
        .iter()
        .map(ParseError::message),
    );
    messages
}

/// A representative message for every error and problem key, with the
/// arguments the model passes (CommandError, validation, storeys,
/// materials, plan grid, locks and integrity problems).
fn error_representatives() -> Vec<Message> {
    let mut messages: Vec<Message> = [
        "model-error-no-change",
        "model-error-invalid-distance",
        "model-error-element-not-found",
        "model-error-duplicate-element-id",
        "model-error-kind-change",
        "model-error-duplicate-storey-id",
        "model-error-move-target-same",
        "model-error-target-storey-not-found",
        "model-error-duplicate-layer-id",
        "model-error-layer-not-found",
        "model-error-layer-name",
        "model-error-reassign-layer",
        "model-error-duplicate-material-id",
        "model-error-material-not-found",
        "model-error-replacement-material",
        "model-error-tessellation-range",
        "model-error-project-name",
        "model-error-spatial-names",
        "model-error-location-invalid",
        "model-error-polygon-points",
        "model-error-polygon-area",
        "model-error-polygon-self-intersection",
        "model-error-hole-outside",
        "model-error-holes-overlap",
        "model-error-wall-top-storey-missing",
        "model-error-element-name",
        "model-error-home-storey-missing",
        "model-error-element-layer-missing",
        "model-error-element-material-missing",
        "model-error-vertical-extent",
        "model-error-top-below-base",
        "model-error-no-storeys",
        "model-error-ground-not-in-list",
        "model-error-storey-name",
        "model-error-storey-unknown",
        "model-error-last-storey",
        "model-error-no-ground-left",
        "model-error-invalid-elevation",
        "model-error-storey-below",
        "model-error-storey-above",
        "model-error-storey-collision",
        "model-error-material-name",
        "model-error-opacity-range",
        "model-error-colour-range",
        "model-error-auxiliary-spacing",
        "model-error-grid-rotation",
    ]
    .map(Message::new)
    .to_vec();
    let max = |key, meters| Message::new(key).length("max", meters);
    let range = |key, min, max| Message::new(key).length("min", min).length("max", max);
    let named = |key| Message::new(key).text("name", "W-001");
    messages.extend([
        max("model-error-cut-plane-range", 100.0),
        max("model-error-coordinates-range", 100_000.0),
        max("model-error-base-offset-range", 1000.0),
        max("model-error-top-offset-range", 1000.0),
        max("model-error-slab-offset-range", 1000.0),
        max("model-error-reference-line-range", 10.0),
        max("model-error-snap-grid-range", 1000.0),
        max("model-error-construction-grid-range", 1000.0),
        max("model-error-grid-origin", 100_000.0),
        range("model-error-thickness-range", 0.001, 10.0),
        range("model-error-wall-height-range", 0.001, 1000.0),
        Message::new("model-error-wall-length").length("min", 0.0001),
        Message::new("model-error-storey-height-min").length("min", 0.01),
        Message::new("model-error-polygon-sliver").angle("min", 0.1_f64.to_radians()),
        Message::new("model-error-display-precision").count("max", 8),
        Message::new("model-error-linked-storey-range").count("max", 200),
        Message::new("model-error-slab-points").count("max", 1000),
        Message::new("model-error-priority-range")
            .count("min", 1)
            .count("max", 999),
        named("model-error-element-invalid").nested("reason", Message::new("model-error-element-name")),
        Message::new("model-error-default-missing").kind("kind", KindTag::Slab),
        Message::new("model-error-default-invalid")
            .kind("kind", KindTag::Wall)
            .nested("reason", Message::new("model-error-element-layer-missing")),
        Message::new("model-error-hole").nested("reason", Message::new("model-error-polygon-area")),
        named("model-error-element-not-movable"),
        named("model-error-linked-element-too-low")
            .count("count", 1)
            .length("min", 0.001),
        named("model-error-transform-unsupported"),
        named("model-error-element-locked"),
        named("model-error-layer-locked").text("layer", "Walls"),
        named("model-problem-home-storey-missing"),
        named("model-problem-layer-missing"),
        named("model-problem-material-missing"),
        Message::new("model-error-ifc-class-not-allowed")
            .text("class", "IfcCovering")
            .kind("kind", KindTag::Wall),
        Message::new("model-error-storeys-too-close")
            .text("lower", "00")
            .text("upper", "01")
            .length("min", 0.01),
        Message::new("model-error-storey-invalid-values").text("name", "00"),
        Message::new("model-error-storey-cut-plane")
            .text("name", "00")
            .length("max", 100.0),
    ]);
    messages
}

/// Fluent variables that a message may use without a model argument.
fn implicit_variables(key: &str) -> BTreeSet<String> {
    if KindTag::ALL.iter().any(|k| k.noun_key() == key) {
        BTreeSet::from(["count".to_owned()])
    } else {
        BTreeSet::new()
    }
}

fn variables_of_inline(expression: &InlineExpression<&str>, out: &mut BTreeSet<String>) {
    match expression {
        InlineExpression::VariableReference { id } => {
            out.insert(id.name.to_owned());
        }
        InlineExpression::Placeable { expression } => variables_of_expression(expression, out),
        InlineExpression::FunctionReference { arguments, .. } => {
            for arg in &arguments.positional {
                variables_of_inline(arg, out);
            }
            for arg in &arguments.named {
                variables_of_inline(&arg.value, out);
            }
        }
        _ => {}
    }
}

fn variables_of_expression(expression: &Expression<&str>, out: &mut BTreeSet<String>) {
    match expression {
        Expression::Inline(inline) => variables_of_inline(inline, out),
        Expression::Select { selector, variants } => {
            variables_of_inline(selector, out);
            for variant in variants {
                variables_of_pattern(&variant.value, out);
            }
        }
    }
}

fn variables_of_pattern(pattern: &Pattern<&str>, out: &mut BTreeSet<String>) {
    for element in &pattern.elements {
        if let PatternElement::Placeable { expression } = element {
            variables_of_expression(expression, out);
        }
    }
}

/// The Fluent variables of each message in model.ftl.
fn model_ftl_variables() -> BTreeMap<String, BTreeSet<String>> {
    let file = catalogue::ENGLISH.iter().find(|f| f.name == "en/model.ftl").unwrap();
    let resource = parser::parse(file.source).unwrap();
    resource
        .body
        .iter()
        .filter_map(|entry| match entry {
            Entry::Message(m) => {
                let mut vars = BTreeSet::new();
                if let Some(value) = &m.value {
                    variables_of_pattern(value, &mut vars);
                }
                Some((m.id.name.to_owned(), vars))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn the_variable_scan_follows_selectors_and_variants() {
    let resource = parser::parse("k = { $a ->\n    [one] { $b }\n   *[other] { NUMBER($c) }\n}\n").unwrap();
    let Entry::Message(m) = &resource.body[0] else { panic!() };
    let mut vars = BTreeSet::new();
    variables_of_pattern(m.value.as_ref().unwrap(), &mut vars);
    assert_eq!(vars, BTreeSet::from(["a".into(), "b".into(), "c".into()]));
}

/// NFR-L10N-001: every key the model emits is in model.ftl, model.ftl has no
/// other keys, and each message uses exactly the arguments the model passes
/// (a count also as `<name>_text`).
#[test]
fn model_ftl_matches_the_model_keys_and_arguments() {
    let defined = model_ftl_variables();
    let messages = representatives();
    let represented: BTreeSet<&str> = messages.iter().map(Message::key).collect();
    let all: BTreeSet<&str> = ALL_KEYS.iter().copied().collect();
    assert_eq!(represented, all, "the table has one message per key in ALL_KEYS");
    let defined_keys: BTreeSet<&str> = defined.keys().map(String::as_str).collect();
    assert_eq!(defined_keys, all, "model.ftl defines exactly the keys of ALL_KEYS");

    for message in &messages {
        let mut expected = implicit_variables(message.key());
        for (name, arg) in message.args() {
            expected.extend(fluent_names(name, arg));
        }
        let used = &defined[message.key()];
        // A count may be used as the number, the text or both.
        let counts: BTreeSet<String> = message
            .args()
            .iter()
            .filter(|(_, a)| matches!(a, archicraft_model::Arg::Count(_)))
            .flat_map(|(n, a)| fluent_names(n, a))
            .collect();
        assert!(
            used.is_subset(&expected),
            "{}: uses {used:?}, the model passes {expected:?}",
            message.key()
        );
        let missing: Vec<&String> = expected.difference(used).filter(|v| !counts.contains(*v)).collect();
        assert!(missing.is_empty(), "{}: does not use {missing:?}", message.key());
        for (name, arg) in message.args() {
            if matches!(arg, archicraft_model::Arg::Count(_)) {
                assert!(
                    fluent_names(name, arg).iter().any(|v| used.contains(v)),
                    "{}: does not use the count {name}",
                    message.key()
                );
            }
        }
    }
}

/// NFR-L10N-001: labels render as English sentences, the kind as a noun and
/// counts with the user's number format.
#[test]
fn model_messages_render_in_english() {
    let units = UnitSettings::default();
    let nf = NumberFormat {
        decimal_separator: ',',
        grouping: Some('.'),
    };
    let render = |m: &Message| i18n::with(std::rc::Rc::new(Localizer::english()), || i18n::render(m, &units, &nf));
    assert_eq!(
        render(&Message::new("model-label-create-kind").kind("kind", KindTag::Wall)),
        "Create wall"
    );
    assert_eq!(
        render(&Message::new("model-label-create-kind").kind("kind", KindTag::Slab)),
        "Create slab"
    );
    assert_eq!(
        render(&Message::new("model-label-delete-elements").count("count", 1)),
        "Delete 1 element"
    );
    assert_eq!(
        render(&Message::new("model-label-move-elements").count("count", 1500)),
        "Move 1.500 elements"
    );
    assert_eq!(render(&Message::new("model-label-new-layer")), "New layer");
    assert_eq!(render(&Message::new(KindTag::Wall.label_key())), "Wall");
}

/// NFR-L10N-001: error messages render as English sentences with lengths in
/// the project unit, kinds as nouns and nested reasons in place.
#[test]
fn model_errors_render_in_english_with_project_units() {
    let units = UnitSettings {
        length: LengthUnit::Millimeter,
        ..UnitSettings::default()
    };
    let nf = NumberFormat {
        decimal_separator: '.',
        grouping: None,
    };
    let render = |m: &Message| i18n::with(std::rc::Rc::new(Localizer::english()), || i18n::render(m, &units, &nf));
    let thickness = Message::new("model-error-thickness-range")
        .length("min", 0.001)
        .length("max", 10.0);
    assert_eq!(
        render(&thickness),
        format!(
            "Thickness must be between {} and {}.",
            format_length(0.001, &units, &nf, true),
            format_length(10.0, &units, &nf, true)
        )
    );
    assert!(render(&thickness).contains("10000"), "{}", render(&thickness));
    let element = Message::new("model-error-element-invalid")
        .text("name", "W-001")
        .nested("reason", Message::new("model-error-polygon-area"));
    assert_eq!(render(&element), "W-001: The polygon has no area.");
    let class = Message::new("model-error-ifc-class-not-allowed")
        .text("class", "IfcCovering")
        .kind("kind", KindTag::Wall);
    assert_eq!(render(&class), "IfcCovering is not an allowed IFC class for a wall.");
    assert_eq!(render(&Message::new("model-error-no-change")), "Nothing to change.");
    // NFR-REL-005: a refused storey edit names the first element too low
    // and counts the others.
    let min = format_length(0.001, &units, &nf, true);
    let too_low = |others| {
        render(
            &Message::new("model-error-linked-element-too-low")
                .text("name", "W-001")
                .count("count", others)
                .length("min", 0.001),
        )
    };
    assert_eq!(too_low(0), format!("W-001 would be less than {min} high."));
    assert_eq!(
        too_low(1),
        format!("W-001 and 1 other element would be less than {min} high.")
    );
    assert_eq!(
        too_low(2),
        format!("W-001 and 2 other elements would be less than {min} high.")
    );
}

/// NFR-L10N-001: unit, IFC class and reference-line names and the parse
/// errors of typed values render as English text.
#[test]
fn unit_names_and_parse_errors_render_in_english() {
    let units = UnitSettings::default();
    let nf = NumberFormat {
        decimal_separator: '.',
        grouping: None,
    };
    let render = |m: &Message| i18n::with(std::rc::Rc::new(Localizer::english()), || i18n::render(m, &units, &nf));
    let label = |key| render(&Message::new(key));
    assert_eq!(label(LengthUnit::Millimeter.label_key()), "Millimetre");
    assert_eq!(label(LengthUnit::FeetInches.label_key()), "Feet and inches");
    assert_eq!(
        label(AngleUnit::DegreesMinutesSeconds.label_key()),
        "Degrees, minutes, seconds"
    );
    assert_eq!(label(SlopeFormat::Percent.label_key()), "Percent");
    assert_eq!(label(AreaUnit::SquareFoot.label_key()), "Square feet");
    assert_eq!(label(VolumeUnit::CubicMeter.label_key()), "Cubic metres");
    assert_eq!(label(IfcClass::Covering.label_key()), "IfcCovering");
    assert_eq!(
        label(IfcClass::BuildingElementProxy.label_key()),
        "IfcBuildingElementProxy"
    );
    assert_eq!(label(ReferenceLine::Custom(0.1).label_key()), "Custom offset");
    assert_eq!(render(&ParseError::Empty.message()), "Enter a number.");
    assert_eq!(
        render(&ParseError::InvalidLength("3 furlongs".into()).message()),
        "“3 furlongs” is not a valid length."
    );
    assert_eq!(
        render(&ParseError::InvalidAngle("north".into()).message()),
        "“north” is not a valid angle."
    );
    assert_eq!(
        render(&ParseError::InvalidSlope("1:0".into()).message()),
        "“1:0” is not a valid slope."
    );
}
