//! The English catalogue covers every message key archicraft-model emits,
//! with the arguments the model passes (NFR-L10N-001).

#![allow(clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};

use archicraft_app::i18n::{self, Localizer, catalogue, model::fluent_names};
use archicraft_model::message::ALL_KEYS;
use archicraft_model::units::{NumberFormat, UnitSettings};
use archicraft_model::{KindTag, Message};
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
