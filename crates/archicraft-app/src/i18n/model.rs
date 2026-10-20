//! Rendering of the messages archicraft-model returns (NFR-L10N-001).
//!
//! The model gives command labels as a [`Message`]: a `model-` key from
//! `locales/en/model.ftl` and typed arguments. [`render`] turns each argument
//! into a Fluent argument of the same name:
//!
//! | Argument | Fluent arguments |
//! |---|---|
//! | [`Arg::Count`] | `name` (the number, for plural selection) and `name_text` (formatted with the user's number format) |
//! | [`Arg::Text`] | `name`, the text as is |
//! | [`Arg::Length`], [`Arg::Angle`] | `name`, formatted in the project unit with its symbol |
//! | [`Arg::Kind`] | `name`, the kind's noun (`model-kind-<code>-noun` with `count` 1) |
//! | [`Arg::Nested`] | `name`, the nested message rendered |

use archicraft_model::units::{NumberFormat, UnitSettings, format_angle, format_length};
use archicraft_model::{Arg, Message};

use super::{FluentArgs, current};

/// The Fluent argument names a message argument of `arg`'s type called
/// `name` provides (see the module documentation).
pub fn fluent_names(name: &str, arg: &Arg) -> Vec<String> {
    match arg {
        Arg::Count(_) => vec![name.to_owned(), format!("{name}_text")],
        _ => vec![name.to_owned()],
    }
}

/// Renders `message` with the current localizer, lengths and angles in
/// `units` and numbers in `nf`.
pub fn render(message: &Message, units: &UnitSettings, nf: &NumberFormat) -> String {
    let localizer = current();
    let mut args = FluentArgs::new();
    for (name, arg) in message.args() {
        let name = *name;
        match arg {
            Arg::Count(n) => {
                args.set(name, *n);
                args.set(format!("{name}_text"), nf.format_decimal(*n as f64, 0));
            }
            Arg::Text(text) => args.set(name, text.clone()),
            Arg::Length(meters) => args.set(name, format_length(*meters, units, nf, true)),
            Arg::Angle(radians) => args.set(name, format_angle(*radians, units, nf, true)),
            Arg::Kind(kind) => {
                let mut noun = FluentArgs::new();
                noun.set("count", 1);
                args.set(name, localizer.format(kind.noun_key(), Some(&noun)));
            }
            Arg::Nested(nested) => args.set(name, render(nested, units, nf)),
        }
    }
    localizer.format(message.key(), Some(&args))
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use archicraft_model::KindTag;
    use archicraft_model::units::{AngleUnit, LengthUnit};

    use super::*;
    use crate::i18n::catalogue::{self, CatalogueFile};
    use crate::i18n::{Localizer, with};

    /// Messages with every argument type, only for these tests.
    const TEST_FILE: CatalogueFile = CatalogueFile {
        name: "test.ftl",
        source: "test-sizes = { $name }: { $width } at { $angle }, { $count_text } { $kind } ({ $inner })\n",
        prefixes: &["test-"],
    };

    /// NFR-L10N-001: lengths and angles show in the project units, counts in
    /// the user's number format, kinds as nouns and nested messages rendered.
    #[test]
    fn arguments_render_in_project_units_and_the_number_format() {
        let mut files = catalogue::ENGLISH.to_vec();
        files.push(TEST_FILE);
        let localizer = Rc::new(Localizer::from_catalogue(catalogue::english(), &files));
        let message = Message::new("test-sizes")
            .text("name", "W-001")
            .length("width", 0.365)
            .angle("angle", std::f64::consts::FRAC_PI_2)
            .count("count", 2500)
            .kind("kind", KindTag::Slab)
            .nested(
                "inner",
                Message::new("model-label-create-kind").kind("kind", KindTag::Wall),
            );
        let nf = NumberFormat {
            decimal_separator: ',',
            grouping: Some(' '),
        };
        let units = UnitSettings {
            length: LengthUnit::Centimeter,
            angle: AngleUnit::DecimalDegrees,
            ..UnitSettings::default()
        };
        let text = with(Rc::clone(&localizer), || render(&message, &units, &nf));
        assert_eq!(
            text,
            format!(
                "W-001: {} at {}, 2 500 slab (Create wall)",
                format_length(0.365, &units, &nf, true),
                format_angle(std::f64::consts::FRAC_PI_2, &units, &nf, true)
            )
        );
        assert!(text.contains("36,500 cm") && text.contains("90,00"), "{text}");
        assert!(localizer.reported_keys().is_empty(), "{:?}", localizer.reported_keys());
    }

    #[test]
    fn counts_provide_the_number_and_its_text() {
        assert_eq!(fluent_names("count", &Arg::Count(1)), ["count", "count_text"]);
        assert_eq!(fluent_names("width", &Arg::Length(1.0)), ["width"]);
    }
}
