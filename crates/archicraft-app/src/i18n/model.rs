//! Rendering of the messages archicraft-model, archicraft-ifcx and
//! archicraft-geometry return (NFR-L10N-001).
//!
//! The model gives command labels as a [`Message`]: a `model-` key from
//! `locales/en/model.ftl` and typed arguments. archicraft-ifcx reports the
//! problems of reading and saving files the same way, with `ifcx-` keys
//! from `locales/en/ifcx.ftl`, and archicraft-geometry the reasons an
//! element has no geometry, with `geometry-` keys from
//! `locales/en/geometry.ftl`. [`render`] turns each argument into a Fluent
//! argument of the same name:
//!
//! | Argument | Fluent arguments |
//! |---|---|
//! | [`Arg::Count`] | `name` (the number, for plural selection) and `name_text` (formatted with the user's number format) |
//! | [`Arg::Text`] | `name`, the text as is |
//! | [`Arg::Length`], [`Arg::Angle`] | `name`, formatted in the project unit with its symbol, with more decimals when the project's would round a non-zero value to zero |
//! | [`Arg::Kind`] | `name`, the kind's noun (`model-kind-<code>-noun` with `count` 1) |
//! | [`Arg::Nested`] | `name`, the nested message rendered |

use archicraft_model::units::{LengthUnit, NumberFormat, UnitSettings, format_angle, format_length};
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
            Arg::Length(meters) => args.set(name, length_text(*meters, units, nf)),
            Arg::Angle(radians) => args.set(name, angle_text(*radians, units, nf)),
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

/// The most decimals a length or angle argument gets to read non-zero.
const WIDEST_DECIMALS: u8 = 12;

/// `true` when `text` shows a digit other than zero.
fn shows_non_zero(text: &str) -> bool {
    text.chars().any(|c| matches!(c, '1'..='9'))
}

/// `meters` in the project length unit with its symbol, with as many more
/// decimals as it takes to read non-zero (NFR-L10N-001): a limit such as
/// `MIN_EDGE_LENGTH` (0.1 mm) shows as `0.0001 m`, not `0.000 m`. Feet and
/// inches finer than 1/64" show in decimal inches.
fn length_text(meters: f64, units: &UnitSettings, nf: &NumberFormat) -> String {
    let mut units = units.clone();
    loop {
        let text = format_length(meters, &units, nf, true);
        if meters == 0.0 || shows_non_zero(&text) {
            return text;
        }
        if units.length == LengthUnit::FeetInches && units.length_decimals >= 6 {
            units.length = LengthUnit::Inches;
            units.length_decimals = 0;
        } else if units.length_decimals >= WIDEST_DECIMALS {
            return text;
        } else {
            units.length_decimals += 1;
        }
    }
}

/// `radians` in the project angle unit with its symbol, with as many more
/// decimals as it takes to read non-zero (NFR-L10N-001).
fn angle_text(radians: f64, units: &UnitSettings, nf: &NumberFormat) -> String {
    let mut units = units.clone();
    loop {
        let text = format_angle(radians, &units, nf, true);
        if radians == 0.0 || shows_non_zero(&text) || units.angle_decimals >= WIDEST_DECIMALS {
            return text;
        }
        units.angle_decimals += 1;
    }
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
        source: "test-sizes = { $name }: { $width } at { $angle }, { $count_text } { $kind } ({ $inner })\n\
                 test-angle = at least { $angle }\n",
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

    /// Renders `message` with the English catalogue and the test messages,
    /// numbers in the default format.
    fn render_english(message: &Message, units: &UnitSettings) -> String {
        let mut files = catalogue::ENGLISH.to_vec();
        files.push(TEST_FILE);
        let localizer = Rc::new(Localizer::from_catalogue(catalogue::english(), &files));
        let text = with(Rc::clone(&localizer), || {
            render(message, units, &NumberFormat::default())
        });
        assert!(localizer.reported_keys().is_empty(), "{:?}", localizer.reported_keys());
        text
    }

    fn lengths_in(length: LengthUnit, length_decimals: u8) -> UnitSettings {
        UnitSettings {
            length,
            length_decimals,
            ..UnitSettings::default()
        }
    }

    /// NFR-L10N-001: a length limit finer than the display precision gets
    /// the decimals it needs to read non-zero, so `MIN_EDGE_LENGTH` never
    /// shows as `0.000 m`.
    #[test]
    fn a_fine_length_limit_never_renders_as_zero() {
        let message = Message::new("model-error-wall-length").length("min", 0.0001);
        let cases = [
            (LengthUnit::Meter, 3, "0.0001 m"),
            (LengthUnit::Centimeter, 1, "0.01 cm"),
            (LengthUnit::Millimeter, 0, "0.1 mm"),
            (LengthUnit::Feet, 2, "0.0003 ft"),
            // Finer than 1/64": decimal inches.
            (LengthUnit::FeetInches, 4, "0.004 in"),
        ];
        for (unit, decimals, shown) in cases {
            let text = render_english(&message, &lengths_in(unit, decimals));
            assert_eq!(text, format!("A wall must be at least {shown} long."), "{unit:?}");
        }
    }

    /// NFR-L10N-001: an angle limit finer than the display precision gets
    /// the decimals it needs to read non-zero.
    #[test]
    fn a_fine_angle_limit_never_renders_as_zero() {
        let message = Message::new("test-angle").angle("angle", 0.1_f64.to_radians());
        let units = UnitSettings {
            angle_decimals: 0,
            ..UnitSettings::default()
        };
        assert_eq!(render_english(&message, &units), "at least 0.1°");
        let units = UnitSettings {
            angle: AngleUnit::Gradians,
            angle_decimals: 0,
            ..UnitSettings::default()
        };
        assert_eq!(render_english(&message, &units), "at least 0.1 gon");
    }

    /// NFR-L10N-001: limits the display precision already shows non-zero,
    /// and zero itself, render at the project precision as before.
    #[test]
    fn limits_the_precision_shows_render_unchanged() {
        let message = Message::new("model-error-wall-length").length("min", 3.0);
        assert_eq!(
            render_english(&message, &lengths_in(LengthUnit::Meter, 3)),
            "A wall must be at least 3.000 m long."
        );
        assert_eq!(
            render_english(&message, &lengths_in(LengthUnit::Millimeter, 0)),
            "A wall must be at least 3000 mm long."
        );
        let zero = Message::new("model-error-wall-length").length("min", 0.0);
        assert_eq!(
            render_english(&zero, &lengths_in(LengthUnit::Meter, 3)),
            "A wall must be at least 0.000 m long."
        );
        let units = UnitSettings {
            angle_decimals: 0,
            ..UnitSettings::default()
        };
        let right = Message::new("test-angle").angle("angle", std::f64::consts::FRAC_PI_2);
        assert_eq!(render_english(&right, &units), "at least 90°");
    }

    #[test]
    fn counts_provide_the_number_and_its_text() {
        assert_eq!(fluent_names("count", &Arg::Count(1)), ["count", "count_text"]);
        assert_eq!(fluent_names("width", &Arg::Length(1.0)), ["width"]);
    }
}
