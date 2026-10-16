//! Units, number formatting and length parsing (STR-009, STR-010, INP-005,
//! NFR-ACC-002, NFR-ACC-005, NFR-L10N-003, NFR-L10N-004).
//!
//! Internally every length is a metre value and every angle a radian value in
//! `f64`. Units only affect display and input; changing a unit never changes
//! stored geometry.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const METERS_PER_INCH: f64 = 0.0254;
pub const METERS_PER_FOOT: f64 = 0.3048;

/// Display and input unit for lengths.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum LengthUnit {
    #[default]
    Meter,
    Centimeter,
    Millimeter,
    /// Feet and inches with fractions, for example `3'-4 1/2"`.
    FeetInches,
    /// Decimal feet.
    Feet,
    /// Decimal inches.
    Inches,
}

impl LengthUnit {
    pub const ALL: [LengthUnit; 6] = [
        LengthUnit::Meter,
        LengthUnit::Centimeter,
        LengthUnit::Millimeter,
        LengthUnit::FeetInches,
        LengthUnit::Feet,
        LengthUnit::Inches,
    ];

    /// Short code, as written to the ArchiCraft namespace in IFCX.
    pub fn code(self) -> &'static str {
        match self {
            LengthUnit::Meter => "m",
            LengthUnit::Centimeter => "cm",
            LengthUnit::Millimeter => "mm",
            LengthUnit::FeetInches => "ft-in",
            LengthUnit::Feet => "ft",
            LengthUnit::Inches => "in",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|u| u.code() == code)
    }

    /// Human-readable name for menus.
    pub fn label(self) -> &'static str {
        match self {
            LengthUnit::Meter => "Metre",
            LengthUnit::Centimeter => "Centimetre",
            LengthUnit::Millimeter => "Millimetre",
            LengthUnit::FeetInches => "Feet and inches",
            LengthUnit::Feet => "Decimal feet",
            LengthUnit::Inches => "Decimal inches",
        }
    }

    /// Length of one unit in metres. Feet-inches input without a unit mark is
    /// read as feet.
    pub fn meters_per_unit(self) -> f64 {
        match self {
            LengthUnit::Meter => 1.0,
            LengthUnit::Centimeter => 0.01,
            LengthUnit::Millimeter => 0.001,
            LengthUnit::FeetInches | LengthUnit::Feet => METERS_PER_FOOT,
            LengthUnit::Inches => METERS_PER_INCH,
        }
    }

    pub fn is_imperial(self) -> bool {
        matches!(self, LengthUnit::FeetInches | LengthUnit::Feet | LengthUnit::Inches)
    }
}

/// Display and input unit for angles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum AngleUnit {
    #[default]
    DecimalDegrees,
    DegreesMinutesSeconds,
    Gradians,
    Radians,
}

impl AngleUnit {
    pub const ALL: [AngleUnit; 4] = [
        AngleUnit::DecimalDegrees,
        AngleUnit::DegreesMinutesSeconds,
        AngleUnit::Gradians,
        AngleUnit::Radians,
    ];

    pub fn code(self) -> &'static str {
        match self {
            AngleUnit::DecimalDegrees => "deg",
            AngleUnit::DegreesMinutesSeconds => "dms",
            AngleUnit::Gradians => "grad",
            AngleUnit::Radians => "rad",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|u| u.code() == code)
    }

    pub fn label(self) -> &'static str {
        match self {
            AngleUnit::DecimalDegrees => "Decimal degrees",
            AngleUnit::DegreesMinutesSeconds => "Degrees, minutes, seconds",
            AngleUnit::Gradians => "Gradians",
            AngleUnit::Radians => "Radians",
        }
    }
}

/// How slopes (for example roof pitches) are shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum SlopeFormat {
    #[default]
    Degrees,
    Percent,
    Ratio,
}

impl SlopeFormat {
    pub const ALL: [SlopeFormat; 3] = [SlopeFormat::Degrees, SlopeFormat::Percent, SlopeFormat::Ratio];

    pub fn code(self) -> &'static str {
        match self {
            SlopeFormat::Degrees => "deg",
            SlopeFormat::Percent => "percent",
            SlopeFormat::Ratio => "ratio",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|u| u.code() == code)
    }
}

/// Display unit for areas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum AreaUnit {
    #[default]
    SquareMeter,
    SquareFoot,
}

impl AreaUnit {
    pub const ALL: [AreaUnit; 2] = [AreaUnit::SquareMeter, AreaUnit::SquareFoot];

    pub fn code(self) -> &'static str {
        match self {
            AreaUnit::SquareMeter => "m2",
            AreaUnit::SquareFoot => "ft2",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|u| u.code() == code)
    }
}

/// Display unit for volumes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum VolumeUnit {
    #[default]
    CubicMeter,
    CubicFoot,
}

impl VolumeUnit {
    pub const ALL: [VolumeUnit; 2] = [VolumeUnit::CubicMeter, VolumeUnit::CubicFoot];

    pub fn code(self) -> &'static str {
        match self {
            VolumeUnit::CubicMeter => "m3",
            VolumeUnit::CubicFoot => "ft3",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|u| u.code() == code)
    }
}

/// Project unit settings (STR-009). Stored in the project file (PRJ-007).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UnitSettings {
    pub length: LengthUnit,
    /// Decimals for metric and decimal units; for feet-inches the fraction
    /// denominator is `2^length_decimals` (4 = 1/16").
    pub length_decimals: u8,
    pub angle: AngleUnit,
    pub angle_decimals: u8,
    pub slope: SlopeFormat,
    pub area: AreaUnit,
    pub volume: VolumeUnit,
}

impl Default for UnitSettings {
    /// The default metric template (Appendix D.1).
    fn default() -> Self {
        Self {
            length: LengthUnit::Meter,
            length_decimals: 3,
            angle: AngleUnit::DecimalDegrees,
            angle_decimals: 2,
            slope: SlopeFormat::Degrees,
            area: AreaUnit::SquareMeter,
            volume: VolumeUnit::CubicMeter,
        }
    }
}

/// Number appearance (STR-010, NFR-L10N-004). A personal setting, not stored in
/// projects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NumberFormat {
    pub decimal_separator: char,
    /// Digit grouping character for the integer part, if any.
    pub grouping: Option<char>,
}

impl Default for NumberFormat {
    fn default() -> Self {
        Self {
            decimal_separator: '.',
            grouping: None,
        }
    }
}

impl NumberFormat {
    /// Formats `value` with `decimals` decimals, using the separator and
    /// grouping of this format. Negative zero is shown as zero.
    pub fn format_decimal(&self, value: f64, decimals: usize) -> String {
        let mut text = format!("{value:.decimals$}");
        if text.starts_with('-') && text[1..].chars().all(|c| c == '0' || c == '.') {
            text.remove(0);
        }
        let (sign, rest) = match text.strip_prefix('-') {
            Some(rest) => ("-", rest.to_owned()),
            None => ("", text),
        };
        let (int_part, frac_part) = match rest.split_once('.') {
            Some((i, f)) => (i.to_owned(), Some(f.to_owned())),
            None => (rest, None),
        };
        let int_part = match self.grouping {
            Some(group) => group_digits(&int_part, group),
            None => int_part,
        };
        match frac_part {
            Some(frac) => format!("{sign}{int_part}{}{frac}", self.decimal_separator),
            None => format!("{sign}{int_part}"),
        }
    }
}

fn group_digits(digits: &str, separator: char) -> String {
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(separator);
        }
        out.push(c);
    }
    out
}

/// Formats a length given in metres in the project's length unit. Values are
/// rounded for display only (NFR-ACC-005).
pub fn format_length(meters: f64, units: &UnitSettings, nf: &NumberFormat, with_unit: bool) -> String {
    let decimals = usize::from(units.length_decimals);
    match units.length {
        LengthUnit::FeetInches => format_feet_inches(meters, units.length_decimals),
        unit => {
            let value = meters / unit.meters_per_unit();
            let text = nf.format_decimal(value, decimals);
            if with_unit {
                format!("{text} {}", unit_symbol(unit))
            } else {
                text
            }
        }
    }
}

fn unit_symbol(unit: LengthUnit) -> &'static str {
    match unit {
        LengthUnit::Meter => "m",
        LengthUnit::Centimeter => "cm",
        LengthUnit::Millimeter => "mm",
        LengthUnit::FeetInches => "",
        LengthUnit::Feet => "ft",
        LengthUnit::Inches => "in",
    }
}

/// Formats a length as feet and inches with a fraction of `1 / 2^precision`
/// inches, for example `3'-4 1/2"`.
pub fn format_feet_inches(meters: f64, precision: u8) -> String {
    let denominator = 1_i64 << precision.min(6);
    let total = (meters / METERS_PER_INCH * denominator as f64).round() as i64;
    let sign = if total < 0 { "-" } else { "" };
    let total = total.abs();
    let whole_inches = total / denominator;
    let mut numerator = total % denominator;
    let feet = whole_inches / 12;
    let inches = whole_inches % 12;
    let mut den = denominator;
    while numerator != 0 && numerator % 2 == 0 {
        numerator /= 2;
        den /= 2;
    }
    if numerator == 0 {
        format!("{sign}{feet}'-{inches}\"")
    } else {
        format!("{sign}{feet}'-{inches} {numerator}/{den}\"")
    }
}

/// Formats an angle given in radians in the project's angle unit.
pub fn format_angle(radians: f64, units: &UnitSettings, nf: &NumberFormat, with_unit: bool) -> String {
    let decimals = usize::from(units.angle_decimals);
    match units.angle {
        AngleUnit::DecimalDegrees => {
            let text = nf.format_decimal(radians.to_degrees(), decimals);
            if with_unit { format!("{text}°") } else { text }
        }
        AngleUnit::Gradians => {
            let text = nf.format_decimal(radians * 200.0 / std::f64::consts::PI, decimals);
            if with_unit { format!("{text} gon") } else { text }
        }
        AngleUnit::Radians => {
            let text = nf.format_decimal(radians, decimals.max(3));
            if with_unit { format!("{text} rad") } else { text }
        }
        AngleUnit::DegreesMinutesSeconds => {
            let total_seconds = (radians.to_degrees() * 3600.0).round() as i64;
            let sign = if total_seconds < 0 { "-" } else { "" };
            let s = total_seconds.abs();
            format!("{sign}{}°{:02}'{:02}\"", s / 3600, (s / 60) % 60, s % 60)
        }
    }
}

/// Formats an area given in square metres.
pub fn format_area(square_meters: f64, units: &UnitSettings, nf: &NumberFormat) -> String {
    match units.area {
        AreaUnit::SquareMeter => format!("{} m²", nf.format_decimal(square_meters, 2)),
        AreaUnit::SquareFoot => format!(
            "{} ft²",
            nf.format_decimal(square_meters / (METERS_PER_FOOT * METERS_PER_FOOT), 2)
        ),
    }
}

/// Formats a volume given in cubic metres.
pub fn format_volume(cubic_meters: f64, units: &UnitSettings, nf: &NumberFormat) -> String {
    match units.volume {
        VolumeUnit::CubicMeter => format!("{} m³", nf.format_decimal(cubic_meters, 3)),
        VolumeUnit::CubicFoot => format!("{} ft³", nf.format_decimal(cubic_meters / METERS_PER_FOOT.powi(3), 2)),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ParseError {
    #[error("enter a number")]
    Empty,
    #[error("“{0}” is not a valid length")]
    InvalidLength(String),
    #[error("“{0}” is not a valid angle")]
    InvalidAngle(String),
}

/// Parses a typed length (INP-005). Accepts plain numbers in the default unit
/// and numbers with a unit suffix (`mm`, `cm`, `m`, `ft`, `in`, `'`, `"`, `′`,
/// `″`) in any unit setting, including imperial fractions such as
/// `3'-4 1/2"`. Returns metres at full precision.
pub fn parse_length(text: &str, default_unit: LengthUnit, nf: &NumberFormat) -> Result<f64, ParseError> {
    let normalized = normalize_number_text(text, nf);
    let s = normalized.trim();
    if s.is_empty() {
        return Err(ParseError::Empty);
    }
    let invalid = || ParseError::InvalidLength(text.trim().to_owned());

    let (negative, body) = match s.strip_prefix('-') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, s.strip_prefix('+').unwrap_or(s).trim_start()),
    };

    let meters = if body.contains('\'') || body.contains('"') {
        parse_feet_inches(body).ok_or_else(invalid)?
    } else {
        let (number, unit) = split_unit_suffix(body);
        let value = parse_mixed_number(number.trim()).ok_or_else(invalid)?;
        let unit_factor = match unit.trim() {
            "" => default_unit.meters_per_unit(),
            "m" => 1.0,
            "cm" => 0.01,
            "mm" => 0.001,
            "ft" => METERS_PER_FOOT,
            "in" => METERS_PER_INCH,
            _ => return Err(invalid()),
        };
        value * unit_factor
    };
    if !meters.is_finite() {
        return Err(invalid());
    }
    Ok(if negative { -meters } else { meters })
}

/// Parses a typed angle in the project's angle unit and returns radians.
/// A trailing `°` is accepted. Degrees-minutes-seconds accept `d°m's"`.
pub fn parse_angle(text: &str, unit: AngleUnit, nf: &NumberFormat) -> Result<f64, ParseError> {
    let normalized = normalize_number_text(text, nf);
    let s = normalized.trim();
    if s.is_empty() {
        return Err(ParseError::Empty);
    }
    let invalid = || ParseError::InvalidAngle(text.trim().to_owned());
    let (negative, body) = match s.strip_prefix('-') {
        Some(rest) => (true, rest.trim()),
        None => (false, s),
    };
    let value = if body.contains('\'') || body.contains('"') || unit == AngleUnit::DegreesMinutesSeconds {
        let mut degrees = 0.0;
        let mut rest = body;
        for (mark, factor) in [('°', 1.0), ('\'', 1.0 / 60.0), ('"', 1.0 / 3600.0)] {
            if let Some((number, tail)) = rest.split_once(mark) {
                let n: f64 = number.trim().parse().map_err(|_| invalid())?;
                degrees += n * factor;
                rest = tail;
            }
        }
        if !rest.trim().is_empty() {
            let n: f64 = rest.trim().parse().map_err(|_| invalid())?;
            degrees += n;
        }
        degrees.to_radians()
    } else {
        let number = body.trim_end_matches('°').trim();
        let n: f64 = number.parse().map_err(|_| invalid())?;
        match unit {
            AngleUnit::DecimalDegrees | AngleUnit::DegreesMinutesSeconds => n.to_radians(),
            AngleUnit::Gradians => n * std::f64::consts::PI / 200.0,
            AngleUnit::Radians => n,
        }
    };
    if !value.is_finite() {
        return Err(invalid());
    }
    Ok(if negative { -value } else { value })
}

/// Replaces typographic marks and the locale decimal separator with their
/// ASCII forms, and removes digit grouping.
fn normalize_number_text(text: &str, nf: &NumberFormat) -> String {
    text.chars()
        .filter(|&c| Some(c) != nf.grouping || nf.grouping == Some(nf.decimal_separator))
        .map(|c| match c {
            '′' | '’' => '\'',
            '″' | '”' => '"',
            '−' => '-',
            c if c == nf.decimal_separator => '.',
            c => c,
        })
        .collect()
}

fn split_unit_suffix(s: &str) -> (&str, &str) {
    let idx = s
        .char_indices()
        .find(|(_, c)| c.is_ascii_alphabetic())
        .map_or(s.len(), |(i, _)| i);
    (&s[..idx], &s[idx..])
}

/// Parses `12`, `12.5`, `1/2` or `4 1/2`.
fn parse_mixed_number(s: &str) -> Option<f64> {
    if s.is_empty() {
        return None;
    }
    let parts: Vec<&str> = s.split_whitespace().collect();
    match parts.as_slice() {
        [single] => parse_simple_or_fraction(single),
        [whole, fraction] if fraction.contains('/') => {
            let whole: f64 = whole.parse().ok()?;
            let fraction = parse_simple_or_fraction(fraction)?;
            Some(whole + fraction)
        }
        _ => None,
    }
}

fn parse_simple_or_fraction(s: &str) -> Option<f64> {
    if let Some((num, den)) = s.split_once('/') {
        let num: f64 = num.trim().parse().ok()?;
        let den: f64 = den.trim().parse().ok()?;
        if den == 0.0 {
            return None;
        }
        Some(num / den)
    } else {
        s.parse().ok()
    }
}

/// Parses `3'`, `3'-4"`, `3' 4 1/2"`, `3'4.5"`, `4 1/2"` (without sign).
fn parse_feet_inches(s: &str) -> Option<f64> {
    let (feet_text, inch_text) = match s.split_once('\'') {
        Some((feet, rest)) => (Some(feet.trim()), rest.trim()),
        None => (None, s.trim()),
    };
    let feet = match feet_text {
        Some(t) => parse_mixed_number(t)?,
        None => 0.0,
    };
    let inch_text = inch_text.trim_start_matches('-').trim();
    let inches = if inch_text.is_empty() {
        0.0
    } else {
        let without_mark = inch_text.strip_suffix('"')?.trim();
        if without_mark.is_empty() {
            0.0
        } else {
            parse_mixed_number(without_mark)?
        }
    };
    if feet_text.is_none() && !s.trim_end().ends_with('"') {
        return None;
    }
    Some(feet * METERS_PER_FOOT + inches * METERS_PER_INCH)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nf() -> NumberFormat {
        NumberFormat::default()
    }

    #[test]
    fn parses_plain_numbers_in_default_unit() {
        assert_eq!(parse_length("2.5", LengthUnit::Meter, &nf()), Ok(2.5));
        assert!((parse_length("250", LengthUnit::Centimeter, &nf()).unwrap() - 2.5).abs() < 1e-12);
        assert!((parse_length("2500", LengthUnit::Millimeter, &nf()).unwrap() - 2.5).abs() < 1e-12);
        assert_eq!(parse_length("-0.3", LengthUnit::Meter, &nf()), Ok(-0.3));
    }

    #[test]
    fn parses_unit_suffixes_in_any_setting() {
        let mm = parse_length("365mm", LengthUnit::Meter, &nf()).unwrap();
        assert!((mm - 0.365).abs() < 1e-12);
        let cm = parse_length("30 cm", LengthUnit::FeetInches, &nf()).unwrap();
        assert!((cm - 0.3).abs() < 1e-12);
        let ft = parse_length("10ft", LengthUnit::Meter, &nf()).unwrap();
        assert!((ft - 3.048).abs() < 1e-12);
        let inch = parse_length("6in", LengthUnit::Meter, &nf()).unwrap();
        assert!((inch - 0.1524).abs() < 1e-12);
    }

    #[test]
    fn parses_imperial_fractions() {
        let v = parse_length("3'-4 1/2\"", LengthUnit::Meter, &nf()).unwrap();
        assert!((v - (3.0 * 0.3048 + 4.5 * 0.0254)).abs() < 1e-12);
        let v = parse_length("3′ 4″", LengthUnit::Meter, &nf()).unwrap();
        assert!((v - (3.0 * 0.3048 + 4.0 * 0.0254)).abs() < 1e-12);
        let v = parse_length("1/2\"", LengthUnit::Meter, &nf()).unwrap();
        assert!((v - 0.0127).abs() < 1e-12);
        let v = parse_length("12'", LengthUnit::Meter, &nf()).unwrap();
        assert!((v - 12.0 * 0.3048).abs() < 1e-12);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_length("", LengthUnit::Meter, &nf()).is_err());
        assert!(parse_length("abc", LengthUnit::Meter, &nf()).is_err());
        assert!(parse_length("3 furlongs", LengthUnit::Meter, &nf()).is_err());
        assert!(parse_length("1/0", LengthUnit::Meter, &nf()).is_err());
    }

    #[test]
    fn respects_decimal_comma() {
        let comma = NumberFormat {
            decimal_separator: ',',
            grouping: Some('.'),
        };
        assert_eq!(parse_length("2,5", LengthUnit::Meter, &comma), Ok(2.5));
        assert_eq!(parse_length("1.234,5", LengthUnit::Millimeter, &comma), Ok(1.2345));
        let units = UnitSettings::default();
        assert_eq!(format_length(1234.5, &units, &comma, false), "1.234,500");
    }

    #[test]
    fn formats_lengths() {
        let mut units = UnitSettings::default();
        assert_eq!(format_length(2.5, &units, &nf(), true), "2.500 m");
        assert_eq!(format_length(-0.0001, &units, &nf(), false), "0.000");
        units.length = LengthUnit::Centimeter;
        units.length_decimals = 1;
        assert_eq!(format_length(0.365, &units, &nf(), true), "36.5 cm");
        units.length = LengthUnit::FeetInches;
        units.length_decimals = 4;
        assert_eq!(
            format_length(3.0 * 0.3048 + 4.5 * 0.0254, &units, &nf(), true),
            "3'-4 1/2\""
        );
    }

    #[test]
    fn display_rounding_does_not_lose_typed_precision() {
        // NFR-ACC-002: 0.1 mm input resolution, stored at full precision.
        let v = parse_length("1234.5678 mm", LengthUnit::Meter, &nf()).unwrap();
        assert_eq!(v, 1.2345678);
    }

    #[test]
    fn parses_and_formats_angles() {
        let units = UnitSettings::default();
        let a = parse_angle("45", AngleUnit::DecimalDegrees, &nf()).unwrap();
        assert!((a - std::f64::consts::FRAC_PI_4).abs() < 1e-12);
        assert_eq!(format_angle(a, &units, &nf(), true), "45.00°");
        let dms = parse_angle("30°30'", AngleUnit::DegreesMinutesSeconds, &nf()).unwrap();
        assert!((dms.to_degrees() - 30.5).abs() < 1e-9);
        let g = parse_angle("100", AngleUnit::Gradians, &nf()).unwrap();
        assert!((g - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    }
}
