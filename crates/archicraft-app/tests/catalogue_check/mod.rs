//! Checks shared by the catalogue tests of the crates that return message
//! keys (archicraft-model, archicraft-ifcx, archicraft-geometry): the Fluent
//! variables of each catalogue message against the arguments the crate
//! passes (NFR-L10N-001).

// Each test crate that includes this module uses only some of the helpers.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use archicraft_app::i18n::{catalogue, model::fluent_names};
use archicraft_model::{Arg, Message};
use fluent_syntax::ast::{Entry, Expression, InlineExpression, Pattern, PatternElement};
use fluent_syntax::parser;

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

/// The Fluent variables a pattern uses, also in selectors and variants.
pub fn variables_of_pattern(pattern: &Pattern<&str>, out: &mut BTreeSet<String>) {
    for element in &pattern.elements {
        if let PatternElement::Placeable { expression } = element {
            variables_of_expression(expression, out);
        }
    }
}

/// The Fluent variables of each message in the English catalogue file
/// `name`, such as `en/model.ftl`.
pub fn ftl_variables(name: &str) -> BTreeMap<String, BTreeSet<String>> {
    let file = catalogue::ENGLISH.iter().find(|f| f.name == name).unwrap();
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

/// Checks that the catalogue file `name` defines exactly `all_keys`, that
/// `messages` has one representative per key, and that each message uses
/// exactly the arguments its representative passes (a count also as
/// `<name>_text`), besides the `implicit` variables of its key.
pub fn assert_catalogue_matches(
    name: &str,
    all_keys: &[&str],
    messages: &[Message],
    implicit: impl Fn(&str) -> BTreeSet<String>,
) {
    let defined = ftl_variables(name);
    let represented: BTreeSet<&str> = messages.iter().map(Message::key).collect();
    let all: BTreeSet<&str> = all_keys.iter().copied().collect();
    assert_eq!(represented, all, "the table has one message per key in ALL_KEYS");
    let defined_keys: BTreeSet<&str> = defined.keys().map(String::as_str).collect();
    assert_eq!(defined_keys, all, "{name} defines exactly the keys of ALL_KEYS");

    for message in messages {
        let mut expected = implicit(message.key());
        for (name, arg) in message.args() {
            expected.extend(fluent_names(name, arg));
        }
        let used = &defined[message.key()];
        // A count may be used as the number, the text or both.
        let counts: BTreeSet<String> = message
            .args()
            .iter()
            .filter(|(_, a)| matches!(a, Arg::Count(_)))
            .flat_map(|(n, a)| fluent_names(n, a))
            .collect();
        assert!(
            used.is_subset(&expected),
            "{}: uses {used:?}, the crate passes {expected:?}",
            message.key()
        );
        let missing: Vec<&String> = expected.difference(used).filter(|v| !counts.contains(*v)).collect();
        assert!(missing.is_empty(), "{}: does not use {missing:?}", message.key());
        for (name, arg) in message.args() {
            if matches!(arg, Arg::Count(_)) {
                assert!(
                    fluent_names(name, arg).iter().any(|v| used.contains(v)),
                    "{}: does not use the count {name}",
                    message.key()
                );
            }
        }
    }
}
