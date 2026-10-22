//! The typed-input grammar (INP-005, INP-022) on arbitrary UTF-8: lengths in
//! every length unit and angles in every angle unit, single values and
//! arithmetic expressions, with every number format the preferences offer
//! (STR-010, NFR-L10N-004). An accepted value is finite.

#![no_main]

use archicraft_model::units::{
    AngleUnit, LengthUnit, NumberFormat, parse_angle, parse_angle_expr, parse_length, parse_length_expr,
};
use libfuzzer_sys::fuzz_target;

/// Decimal point or comma, each with or without digit grouping, as the
/// preferences build them, plus a space as grouping character.
const NUMBER_FORMATS: [NumberFormat; 6] = [
    NumberFormat {
        decimal_separator: '.',
        grouping: None,
    },
    NumberFormat {
        decimal_separator: '.',
        grouping: Some(','),
    },
    NumberFormat {
        decimal_separator: ',',
        grouping: None,
    },
    NumberFormat {
        decimal_separator: ',',
        grouping: Some('.'),
    },
    NumberFormat {
        decimal_separator: '.',
        grouping: Some(' '),
    },
    NumberFormat {
        decimal_separator: ',',
        grouping: Some(' '),
    },
];

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    for nf in &NUMBER_FORMATS {
        for unit in LengthUnit::ALL {
            for parse in [parse_length, parse_length_expr] {
                if let Ok(meters) = parse(text, unit, nf) {
                    assert!(meters.is_finite(), "{text:?} as {unit:?} gave {meters}");
                }
            }
        }
        for unit in AngleUnit::ALL {
            for parse in [parse_angle, parse_angle_expr] {
                if let Ok(radians) = parse(text, unit, nf) {
                    assert!(radians.is_finite(), "{text:?} as {unit:?} gave {radians}");
                }
            }
        }
    }
});
