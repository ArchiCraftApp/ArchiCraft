//! The typed-input grammar (INP-005) on arbitrary UTF-8: lengths in every
//! length unit and angles in every angle unit, with every number format the
//! preferences offer (STR-010, NFR-L10N-004). An accepted value is finite.

#![no_main]

use archicraft_model::units::{AngleUnit, LengthUnit, NumberFormat, parse_angle, parse_length};
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
            if let Ok(meters) = parse_length(text, unit, nf) {
                assert!(meters.is_finite(), "{text:?} as {unit:?} gave {meters}");
            }
        }
        for unit in AngleUnit::ALL {
            if let Ok(radians) = parse_angle(text, unit, nf) {
                assert!(radians.is_finite(), "{text:?} as {unit:?} gave {radians}");
            }
        }
    }
});
