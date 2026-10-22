//! Units, number formatting and length parsing (STR-009, STR-010, INP-005,
//! INP-022, NFR-ACC-002, NFR-ACC-005, NFR-L10N-003, NFR-L10N-004).
//!
//! Internally every length is a metre value and every angle a radian value in
//! `f64`. Units only affect display and input; changing a unit never changes
//! stored geometry.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::message::Message;
use crate::tolerance::ANGLE_TOLERANCE;

mod expr;

pub use expr::{parse_angle_expr, parse_length_expr};

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

    /// Message key of the name shown in menus (NFR-L10N-001).
    pub fn label_key(self) -> &'static str {
        match self {
            LengthUnit::Meter => "model-unit-length-m",
            LengthUnit::Centimeter => "model-unit-length-cm",
            LengthUnit::Millimeter => "model-unit-length-mm",
            LengthUnit::FeetInches => "model-unit-length-ft-in",
            LengthUnit::Feet => "model-unit-length-ft",
            LengthUnit::Inches => "model-unit-length-in",
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

    /// Message key of the name shown in menus (NFR-L10N-001).
    pub fn label_key(self) -> &'static str {
        match self {
            AngleUnit::DecimalDegrees => "model-unit-angle-deg",
            AngleUnit::DegreesMinutesSeconds => "model-unit-angle-dms",
            AngleUnit::Gradians => "model-unit-angle-grad",
            AngleUnit::Radians => "model-unit-angle-rad",
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

    /// Message key of the name shown in menus (NFR-L10N-001).
    pub fn label_key(self) -> &'static str {
        match self {
            SlopeFormat::Degrees => "model-unit-slope-deg",
            SlopeFormat::Percent => "model-unit-slope-percent",
            SlopeFormat::Ratio => "model-unit-slope-ratio",
        }
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

    /// Message key of the name shown in menus (NFR-L10N-001).
    pub fn label_key(self) -> &'static str {
        match self {
            AreaUnit::SquareMeter => "model-unit-area-m2",
            AreaUnit::SquareFoot => "model-unit-area-ft2",
        }
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

    /// Message key of the name shown in menus (NFR-L10N-001).
    pub fn label_key(self) -> &'static str {
        match self {
            VolumeUnit::CubicMeter => "model-unit-volume-m3",
            VolumeUnit::CubicFoot => "model-unit-volume-ft3",
        }
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

    /// Parses a plain number typed with this format's decimal separator and
    /// any digit grouping that [`parse_length`] accepts (NFR-L10N-004).
    /// Returns `None` for text with a unit, a fraction or a point or comma
    /// that is neither the separator nor a digit group.
    pub fn parse_decimal(&self, text: &str) -> Option<f64> {
        let normalized = normalize_number_text(text, self)?;
        let s = normalized.trim();
        let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
        if !digits.starts_with(|c: char| c.is_ascii_digit() || c == '.')
            || !digits.chars().all(|c| c.is_ascii_digit() || c == '.')
        {
            return None;
        }
        s.parse().ok().filter(|v: &f64| v.is_finite())
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
        AngleUnit::DegreesMinutesSeconds => format_dms(radians, decimals.min(6), nf),
    }
}

/// Degrees, minutes and seconds with `decimals` decimals of a second
/// (STR-009), for example `30°15'07.3"`. The marks are always shown, because
/// they are the notation.
fn format_dms(radians: f64, decimals: usize, nf: &NumberFormat) -> String {
    let per_second = 10_u64.pow(decimals as u32);
    // `as i64` saturates for angles beyond its range, and `unsigned_abs`
    // keeps `i64::MIN` from overflowing, so every finite angle formats.
    let total = (radians.to_degrees() * 3600.0 * per_second as f64).round() as i64;
    let sign = if total < 0 { "-" } else { "" };
    let total = total.unsigned_abs();
    let fraction = total % per_second;
    let seconds = total / per_second;
    let (degrees, minutes, seconds) = (seconds / 3600, (seconds / 60) % 60, seconds % 60);
    if decimals == 0 {
        format!("{sign}{degrees}°{minutes:02}'{seconds:02}\"")
    } else {
        let separator = nf.decimal_separator;
        format!("{sign}{degrees}°{minutes:02}'{seconds:02}{separator}{fraction:0decimals$}\"")
    }
}

/// Formats a slope, given as a pitch angle in radians, in the project's slope
/// format (STR-009, NFR-L10N-003): degrees (`30.00°`) and percent (`57.74 %`,
/// `100 · tan(pitch)`) with the angle decimals, and a ratio of rise to run
/// (`1 : 1.732`) with the run in the length decimals; zero is `0 : 1`.
/// `with_unit` only drops the `°` and `%` of the first two, so the text
/// still reads back in its format: a ratio keeps its colon, and a slope that
/// has no percent or ratio (vertical, or a run that rounds to zero) is shown
/// in degrees with the degree sign. A pitch within [`ANGLE_TOLERANCE`] of a
/// right angle is vertical.
pub fn format_slope(pitch: f64, units: &UnitSettings, nf: &NumberFormat, with_unit: bool) -> String {
    let decimals = usize::from(units.angle_decimals);
    let degrees = |with_unit: bool| {
        let text = nf.format_decimal(pitch.to_degrees(), decimals);
        if with_unit { format!("{text}°") } else { text }
    };
    let vertical = pitch.abs() >= std::f64::consts::FRAC_PI_2 - ANGLE_TOLERANCE;
    match units.slope {
        SlopeFormat::Degrees => degrees(with_unit),
        _ if vertical => degrees(true),
        SlopeFormat::Percent => {
            let text = nf.format_decimal(100.0 * pitch.tan(), decimals);
            if with_unit { format!("{text} %") } else { text }
        }
        SlopeFormat::Ratio => {
            if pitch == 0.0 {
                return "0 : 1".to_owned();
            }
            let run = 1.0 / pitch.tan().abs();
            let run_text = nf.format_decimal(run, usize::from(units.length_decimals));
            if run_text.chars().all(|c| c == '0' || c == nf.decimal_separator) {
                return degrees(true);
            }
            let sign = if pitch < 0.0 { "-" } else { "" };
            format!("{sign}1 : {run_text}")
        }
    }
}

/// Parses a typed slope and returns the pitch angle in radians (STR-009,
/// NFR-L10N-003). An explicit notation wins over the display format: a ratio
/// of rise to run with `:`, `∶` or `/` (`6:12`, `1 : 1.732`), a percentage
/// with `%`, an angle with the degree sign, a degrees-minutes-seconds mark or
/// a `gon`, `grad` or `rad` suffix ([`parse_angle`]). A bare number is read
/// in `format`; in the ratio format it is the run of `1 : n`. Vertical slopes,
/// including a percent or ratio within [`ANGLE_TOLERANCE`] of a right angle,
/// can only be typed as angles, and pitches beyond a right angle are
/// rejected.
pub fn parse_slope(text: &str, format: SlopeFormat, nf: &NumberFormat) -> Result<f64, ParseError> {
    let trimmed = text.trim();
    let invalid = || ParseError::InvalidSlope(trimmed.to_owned());
    if trimmed.is_empty() {
        return Err(ParseError::Empty);
    }
    let number = |s: &str| nf.parse_decimal(s).ok_or_else(invalid);
    // A percent or ratio pitch within `ANGLE_TOLERANCE` of a right angle is
    // vertical, as in `format_slope`, and has no percent or ratio.
    let not_vertical = |pitch: f64| {
        if pitch.abs() >= std::f64::consts::FRAC_PI_2 - ANGLE_TOLERANCE {
            Err(invalid())
        } else {
            Ok(pitch)
        }
    };
    let from_ratio = |rise: f64, run: f64| {
        if run == 0.0 {
            Err(invalid())
        } else {
            not_vertical((rise / run).atan())
        }
    };
    let from_percent = |percent: f64| not_vertical((percent / 100.0).atan());
    if let Some((rise, run)) = trimmed.split_once([':', '∶', '/']) {
        return from_ratio(number(rise)?, number(run)?);
    }
    if let Some(percent) = trimmed.strip_suffix('%') {
        return from_percent(number(percent)?);
    }
    let bare = nf.parse_decimal(trimmed);
    let pitch = match (format, bare) {
        (SlopeFormat::Percent, Some(percent)) => return from_percent(percent),
        (SlopeFormat::Ratio, Some(run)) => return from_ratio(run.signum(), run.abs()),
        _ => parse_angle(trimmed, AngleUnit::DecimalDegrees, nf).map_err(|_| invalid())?,
    };
    if pitch.abs() > std::f64::consts::FRAC_PI_2 + ANGLE_TOLERANCE {
        return Err(invalid());
    }
    Ok(pitch.clamp(-std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2))
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

/// Why a typed length, angle or slope was not accepted. `Display` is the key form,
/// for logs only; the user interface renders [`ParseError::message`]
/// (NFR-L10N-001).
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ParseError {
    #[error("{}", self.message())]
    Empty,
    #[error("{}", self.message())]
    InvalidLength(String),
    #[error("{}", self.message())]
    InvalidAngle(String),
    #[error("{}", self.message())]
    InvalidSlope(String),
}

impl ParseError {
    /// The message to show the user, with the rejected text as the `text`
    /// argument.
    pub fn message(&self) -> Message {
        match self {
            ParseError::Empty => Message::new("model-error-parse-empty"),
            ParseError::InvalidLength(text) => Message::new("model-error-parse-length").text("text", text.as_str()),
            ParseError::InvalidAngle(text) => Message::new("model-error-parse-angle").text("text", text.as_str()),
            ParseError::InvalidSlope(text) => Message::new("model-error-parse-slope").text("text", text.as_str()),
        }
    }
}

/// Parses a typed length (INP-005). Accepts plain numbers in the default unit
/// and numbers with a unit suffix (`mm`, `cm`, `m`, `ft`, `in`, `'`, `"`, `′`,
/// `″`) in any unit setting, including imperial fractions such as
/// `3'-4 1/2"`. Returns metres at full precision.
pub fn parse_length(text: &str, default_unit: LengthUnit, nf: &NumberFormat) -> Result<f64, ParseError> {
    let invalid = || ParseError::InvalidLength(text.trim().to_owned());
    let normalized = normalize_number_text(text, nf).ok_or_else(invalid)?;
    let s = normalized.trim();
    if s.is_empty() {
        return Err(ParseError::Empty);
    }
    length_operand(s, default_unit).ok_or_else(invalid)
}

/// Reads one signed length from text that [`normalize_number_text`] has
/// already normalised: the body of [`parse_length`], and an operand of
/// [`parse_length_expr`].
fn length_operand(s: &str, default_unit: LengthUnit) -> Option<f64> {
    let (negative, body) = match s.strip_prefix('-') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, s.strip_prefix('+').unwrap_or(s).trim_start()),
    };

    let meters = if body.contains('\'') || body.contains('"') {
        parse_feet_inches(body)?
    } else {
        let (number, unit) = split_unit_suffix(body);
        let value = parse_mixed_number(number.trim())?;
        let unit_factor = match unit.trim() {
            "" => default_unit.meters_per_unit(),
            "m" => 1.0,
            "cm" => 0.01,
            "mm" => 0.001,
            "ft" => METERS_PER_FOOT,
            "in" => METERS_PER_INCH,
            _ => return None,
        };
        value * unit_factor
    };
    if !meters.is_finite() {
        return None;
    }
    Some(if negative { -meters } else { meters })
}

/// Parses a typed angle and returns radians (INP-005, NFR-L10N-003). Accepts
/// the notation of every angle unit whatever the display unit, as
/// [`parse_length`] accepts every length suffix: a trailing `°` (degrees),
/// `gon` or `grad` (gradians), `rad` (radians) and degrees-minutes-seconds
/// `d°m's"`. A bare number is read in `unit`.
pub fn parse_angle(text: &str, unit: AngleUnit, nf: &NumberFormat) -> Result<f64, ParseError> {
    let invalid = || ParseError::InvalidAngle(text.trim().to_owned());
    let normalized = normalize_number_text(text, nf).ok_or_else(invalid)?;
    let s = normalized.trim();
    if s.is_empty() {
        return Err(ParseError::Empty);
    }
    angle_operand(s, unit).ok_or_else(invalid)
}

/// Reads one signed angle from text that [`normalize_number_text`] has
/// already normalised: the body of [`parse_angle`], and an operand of
/// [`parse_angle_expr`].
fn angle_operand(s: &str, unit: AngleUnit) -> Option<f64> {
    let (negative, body) = match s.strip_prefix('-') {
        Some(rest) => (true, rest.trim()),
        None => (false, s),
    };
    let (value_text, suffix) = split_unit_suffix(body);
    let number = |s: &str| s.trim().parse::<f64>().ok();
    let value = match suffix.trim() {
        "gon" | "grad" => number(value_text)? * std::f64::consts::PI / 200.0,
        "rad" => number(value_text)?,
        "" if body.contains(['°', '\'', '"']) || unit == AngleUnit::DegreesMinutesSeconds => {
            let mut degrees = 0.0;
            let mut rest = body;
            for (mark, factor) in [('°', 1.0), ('\'', 1.0 / 60.0), ('"', 1.0 / 3600.0)] {
                if let Some((n, tail)) = rest.split_once(mark) {
                    degrees += number(n)? * factor;
                    rest = tail;
                }
            }
            if !rest.trim().is_empty() {
                degrees += number(rest)?;
            }
            degrees.to_radians()
        }
        "" => {
            let n = number(body)?;
            match unit {
                AngleUnit::DecimalDegrees | AngleUnit::DegreesMinutesSeconds => n.to_radians(),
                AngleUnit::Gradians => n * std::f64::consts::PI / 200.0,
                AngleUnit::Radians => n,
            }
        }
        _ => return None,
    };
    if !value.is_finite() {
        return None;
    }
    Some(if negative { -value } else { value })
}

/// Grouping characters accepted between digits besides the active one:
/// no-break space, narrow no-break space and the apostrophes of Swiss-style
/// grouping (NFR-L10N-004).
const LOCALE_GROUPINGS: [char; 4] = ['\u{A0}', '\u{202F}', '\'', '’'];

/// Replaces typographic marks and the locale decimal separator with their
/// ASCII forms, and removes digit grouping. A grouping character counts only
/// between digits of the integer part and before a group of exactly three
/// digits. When the active grouping is a space, a typed plain space groups
/// digits too.
///
/// Apostrophes are feet, inch or minute marks once the text uses that
/// syntax, that is once it contains a `"`, `″`, `”` or `°`, even when an
/// apostrophe is the active grouping. Any `°` counts, not only
/// degrees-minutes-seconds input, so `1’234°` under Swiss grouping is an
/// error rather than 1234 degrees.
///
/// Returns `None` when a point or comma is neither the decimal separator nor
/// a digit group, so `2.5` under a decimal comma is an error rather than a
/// different number.
fn normalize_number_text(text: &str, nf: &NumberFormat) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let marks_syntax = chars.iter().any(|c| matches!(c, '"' | '″' | '”' | '°'));
    let space_grouping = matches!(nf.grouping, Some(' ' | '\u{A0}' | '\u{202F}'));
    let is_grouping = |c: char| match c {
        '\'' | '’' => !marks_syntax,
        '\u{A0}' | '\u{202F}' => true,
        ' ' => space_grouping,
        c => nf.grouping == Some(c) && c != nf.decimal_separator,
    };
    let mut out = String::with_capacity(text.len());
    let mut in_fraction = false;
    for (i, &c) in chars.iter().enumerate() {
        if !in_fraction && is_grouping(c) && is_group_boundary(&chars, i) {
            continue;
        }
        if c == nf.decimal_separator {
            in_fraction = true;
            out.push('.');
            continue;
        }
        if !c.is_ascii_digit() {
            in_fraction = false;
        }
        out.push(match c {
            '.' | ',' => return None,
            '\u{A0}' | '\u{202F}' => ' ',
            '′' | '’' => '\'',
            '″' | '”' => '"',
            '−' => '-',
            c => c,
        });
    }
    Some(out)
}

/// Whether the character at `i` separates a digit from a group of exactly
/// three digits that is not the numerator of a fraction (`4 125/128`).
fn is_group_boundary(chars: &[char], i: usize) -> bool {
    let digit = |j: usize| chars.get(j).is_some_and(char::is_ascii_digit);
    i > 0
        && digit(i - 1)
        && digit(i + 1)
        && digit(i + 2)
        && digit(i + 3)
        && !digit(i + 4)
        && chars.get(i + 4) != Some(&'/')
}

/// The characters a typed length or angle may contain (INP-005,
/// NFR-L10N-004), for filtering keyboard input before it is parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputAlphabet {
    chars: Vec<char>,
}

impl InputAlphabet {
    /// Whether `c` may be typed into a length or angle field: an ASCII digit
    /// or one of the characters [`input_alphabet`] collected.
    pub fn contains(&self, c: char) -> bool {
        c.is_ascii_digit() || self.chars.contains(&c)
    }
}

/// Returns the characters [`parse_length_expr`] and [`parse_angle_expr`]
/// accept under `nf`: digits, the operators `+ - − * × / ÷` and parentheses
/// (INP-022), the decimal separator, the grouping characters, mixed-number
/// spaces and fraction slashes, the length and angle unit suffixes (`gon`,
/// `grad`, `rad`) and the feet, inch, degree, minute and second marks. The
/// unit settings do not narrow it, because the parsers accept every suffix
/// and mark in any unit.
pub fn input_alphabet(nf: &NumberFormat) -> InputAlphabet {
    let mut chars: Vec<char> = vec!['+', '-', '−', '*', '×', '÷', '(', ')', ' ', '/', nf.decimal_separator];
    chars.extend(nf.grouping);
    chars.extend(LOCALE_GROUPINGS);
    chars.extend("mcfitngorad".chars());
    chars.extend(['\'', '"', '′', '″', '’', '”', '°']);
    chars.sort_unstable();
    chars.dedup();
    InputAlphabet { chars }
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

    /// NFR-L10N-001: parse errors are message keys with the rejected text
    /// as the `text` argument; `Display` is the key form.
    #[test]
    fn parse_errors_are_message_keys_with_the_rejected_text() {
        let empty = parse_length("  ", LengthUnit::Meter, &nf()).unwrap_err();
        assert_eq!(empty.message().key(), "model-error-parse-empty");
        let length = parse_length(" 1/0 ", LengthUnit::Meter, &nf()).unwrap_err();
        assert_eq!(length.message().key(), "model-error-parse-length");
        assert_eq!(
            length.message().arg("text"),
            Some(&crate::message::Arg::Text("1/0".into()))
        );
        assert_eq!(length.to_string(), length.message().to_string());
        let angle = parse_angle("north", AngleUnit::DecimalDegrees, &nf()).unwrap_err();
        assert_eq!(angle.message().key(), "model-error-parse-angle");
        assert_eq!(
            angle.message().arg("text"),
            Some(&crate::message::Arg::Text("north".into()))
        );
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

    /// STR-010, NFR-L10N-004: the grouping character comes from the locale,
    /// for example the apostrophe of de-CH.
    #[test]
    fn formats_with_any_grouping_character() {
        let swiss = NumberFormat {
            decimal_separator: '.',
            grouping: Some('’'),
        };
        assert_eq!(swiss.format_decimal(1234567.891, 2), "1’234’567.89");
        let french = NumberFormat {
            decimal_separator: ',',
            grouping: Some('\u{202F}'),
        };
        assert_eq!(french.format_decimal(-1234.5, 1), "-1\u{202F}234,5");
    }

    /// NFR-L10N-004: no-break, narrow no-break and apostrophe groupings are
    /// accepted between digits besides the active grouping character.
    #[test]
    fn parses_locale_digit_groupings() {
        let comma = NumberFormat {
            decimal_separator: ',',
            grouping: Some('.'),
        };
        assert_eq!(parse_length("1\u{A0}234,5", LengthUnit::Meter, &comma), Ok(1234.5));
        assert_eq!(parse_length("1\u{202F}234,5", LengthUnit::Meter, &comma), Ok(1234.5));
        let point = NumberFormat {
            decimal_separator: '.',
            grouping: Some(','),
        };
        assert_eq!(parse_length("1’234’567.5", LengthUnit::Meter, &point), Ok(1234567.5));
        assert_eq!(parse_length("1'234.5", LengthUnit::Meter, &point), Ok(1234.5));
        assert_eq!(parse_length("1,234.5", LengthUnit::Meter, &point), Ok(1234.5));
        assert_eq!(parse_length("2,5\u{A0}m", LengthUnit::Millimeter, &comma), Ok(2.5));
    }

    /// NFR-L10N-004: a separator that forms no decimal and no group of three
    /// digits is an error, not silently another number.
    #[test]
    fn rejects_misplaced_separators() {
        let comma = NumberFormat {
            decimal_separator: ',',
            grouping: Some('.'),
        };
        assert!(parse_length("1,2,3", LengthUnit::Meter, &comma).is_err());
        assert!(parse_length("2.5", LengthUnit::Meter, &comma).is_err());
        let point = NumberFormat {
            decimal_separator: '.',
            grouping: Some(','),
        };
        assert!(parse_length("1,2,3", LengthUnit::Meter, &point).is_err());
        assert!(parse_length("2,5", LengthUnit::Meter, &point).is_err());
        let comma_ungrouped = NumberFormat {
            decimal_separator: ',',
            grouping: None,
        };
        assert!(parse_length("2.5", LengthUnit::Meter, &comma_ungrouped).is_err());
        assert_eq!(parse_length("2,5", LengthUnit::Meter, &comma_ungrouped), Ok(2.5));
    }

    /// NFR-L10N-004: a `°` makes apostrophes minute marks, so they no longer
    /// group digits even under Swiss grouping.
    #[test]
    fn degree_sign_stops_apostrophe_grouping() {
        let swiss = NumberFormat {
            decimal_separator: '.',
            grouping: Some('’'),
        };
        for text in ["1’234°", "1'234°"] {
            assert!(parse_angle(text, AngleUnit::DecimalDegrees, &swiss).is_err(), "{text}");
        }
        let v = parse_angle("1’234", AngleUnit::DecimalDegrees, &swiss).unwrap();
        assert!((v.to_degrees() - 1234.0).abs() < 1e-9);
    }

    /// INP-005, NFR-L10N-004: feet-inches syntax keeps its marks with grouping on.
    #[test]
    fn feet_inches_parse_with_grouping_on() {
        let expected = 3.0 * 0.3048 + 4.5 * 0.0254;
        for grouping in [',', '’', '\'', '\u{A0}'] {
            let nf = NumberFormat {
                decimal_separator: '.',
                grouping: Some(grouping),
            };
            for text in ["3'-4 1/2\"", "3’-4 1/2”", "3' 4 1/2\""] {
                let v = parse_length(text, LengthUnit::FeetInches, &nf).unwrap();
                assert!((v - expected).abs() < 1e-12, "{text} with {grouping:?}");
            }
        }
        let swiss = NumberFormat {
            decimal_separator: '.',
            grouping: Some('’'),
        };
        let dms = parse_angle("30°30'", AngleUnit::DegreesMinutesSeconds, &swiss).unwrap();
        assert!((dms.to_degrees() - 30.5).abs() < 1e-9);
        for text in ["1’000\"", "1'000\""] {
            let v = parse_length(text, LengthUnit::FeetInches, &swiss).unwrap();
            assert!((v - 0.3048).abs() < 1e-12, "{text} is one foot");
        }
    }

    /// NFR-L10N-004: under a space grouping (fr-FR) a typed plain space groups
    /// digits, while mixed numbers keep their space.
    #[test]
    fn plain_space_groups_under_space_grouping() {
        let french = NumberFormat {
            decimal_separator: ',',
            grouping: Some('\u{202F}'),
        };
        assert_eq!(parse_length("1 234,5", LengthUnit::Meter, &french), Ok(1234.5));
        let v = parse_length("4 125/128\"", LengthUnit::Meter, &french).unwrap();
        assert!((v - (4.0 + 125.0 / 128.0) * 0.0254).abs() < 1e-12);
        let comma = NumberFormat {
            decimal_separator: ',',
            grouping: Some('.'),
        };
        assert!(parse_length("1 234,5", LengthUnit::Meter, &comma).is_err());
    }

    /// INP-005, NFR-L10N-004: typed input accepts exactly the characters the
    /// parsers understand under the active settings.
    #[test]
    fn input_alphabet_follows_number_format() {
        let comma = NumberFormat {
            decimal_separator: ',',
            grouping: Some('.'),
        };
        let alphabet = input_alphabet(&comma);
        for c in "0123456789,.-+/ \u{A0}\u{202F}'’\"°mcfitngorad".chars() {
            assert!(alphabet.contains(c), "{c:?} should be accepted");
        }
        for c in "xyzqbe%;".chars() {
            assert!(!alphabet.contains(c), "{c:?} should be rejected");
        }
        let plain = NumberFormat {
            decimal_separator: ',',
            grouping: None,
        };
        assert!(!input_alphabet(&plain).contains('.'));
        let point = NumberFormat::default();
        assert!(!input_alphabet(&point).contains(','));
        assert!(input_alphabet(&point).contains('.'));
    }
    /// NFR-L10N-004: plain numbers in dialog fields parse with the active
    /// separator and grouping, and reject units, fractions and a foreign
    /// separator.
    #[test]
    fn parse_decimal_follows_number_format() {
        let comma = NumberFormat {
            decimal_separator: ',',
            grouping: Some('.'),
        };
        assert_eq!(comma.parse_decimal("2,5"), Some(2.5));
        assert_eq!(comma.parse_decimal(" 10.000 "), Some(10_000.0));
        assert_eq!(comma.parse_decimal("−0,25"), Some(-0.25));
        assert_eq!(comma.parse_decimal("2.5"), None);
        let point = NumberFormat::default();
        assert_eq!(point.parse_decimal("2.5"), Some(2.5));
        assert_eq!(point.parse_decimal("+7"), Some(7.0));
        assert_eq!(point.parse_decimal("2,5"), None);
        for text in ["", "-", "2 m", "1/2", "4 1/2", "1e3", "inf", "NaN", "2°"] {
            assert_eq!(point.parse_decimal(text), None, "{text:?}");
        }
        let french = NumberFormat {
            decimal_separator: ',',
            grouping: Some('\u{A0}'),
        };
        assert_eq!(french.parse_decimal("1\u{A0}234,5"), Some(1234.5));
        assert_eq!(french.parse_decimal("1 234,5"), Some(1234.5));
    }

    fn with_slope(slope: SlopeFormat) -> UnitSettings {
        UnitSettings {
            slope,
            ..UnitSettings::default()
        }
    }

    /// STR-009, NFR-L10N-003: a slope shown in any format and typed back
    /// gives the same display, and the pitch differs from the original by
    /// less than the display precision allows.
    #[test]
    fn slopes_round_trip_in_every_format_within_display_precision() {
        for format in SlopeFormat::ALL {
            let units = with_slope(format);
            for degrees in [0.0, 15.0, 30.0, 45.0, 60.0, 89.99] {
                let pitch = f64::to_radians(degrees);
                let shown = format_slope(pitch, &units, &nf(), true);
                let parsed = parse_slope(&shown, format, &nf()).unwrap_or_else(|e| panic!("{shown}: {e:?}"));
                assert_eq!(
                    format_slope(parsed, &units, &nf(), true),
                    shown,
                    "{format:?} {degrees}°"
                );
                assert!(
                    (parsed - pitch).abs().to_degrees() < 0.05,
                    "{format:?} {degrees}° shown as {shown} reads back as {}°",
                    parsed.to_degrees()
                );
                let bare = format_slope(pitch, &units, &nf(), false);
                let parsed_bare = parse_slope(&bare, format, &nf()).unwrap_or_else(|e| panic!("{bare}: {e:?}"));
                assert_eq!(format_slope(parsed_bare, &units, &nf(), true), shown, "{bare}");
            }
        }
    }

    /// NFR-L10N-003: 45° in the three formats, and zero as a ratio.
    #[test]
    fn formats_slopes_in_degrees_percent_and_ratio() {
        let pitch = std::f64::consts::FRAC_PI_4;
        assert_eq!(
            format_slope(pitch, &with_slope(SlopeFormat::Degrees), &nf(), true),
            "45.00°"
        );
        assert_eq!(
            format_slope(pitch, &with_slope(SlopeFormat::Percent), &nf(), true),
            "100.00 %"
        );
        assert_eq!(
            format_slope(pitch, &with_slope(SlopeFormat::Ratio), &nf(), true),
            "1 : 1.000"
        );
        let thirty = 30f64.to_radians();
        assert_eq!(
            format_slope(thirty, &with_slope(SlopeFormat::Percent), &nf(), true),
            "57.74 %"
        );
        assert_eq!(
            format_slope(thirty, &with_slope(SlopeFormat::Ratio), &nf(), true),
            "1 : 1.732"
        );
        assert_eq!(format_slope(0.0, &with_slope(SlopeFormat::Ratio), &nf(), true), "0 : 1");
        let comma = NumberFormat {
            decimal_separator: ',',
            grouping: Some('.'),
        };
        assert_eq!(
            format_slope(thirty, &with_slope(SlopeFormat::Percent), &comma, true),
            "57,74 %"
        );
    }

    /// NFR-L10N-003: ratios are rise to run, typed with a colon, the ratio
    /// sign or a slash; an explicit suffix wins over the display format.
    #[test]
    fn parses_slopes_by_suffix_whatever_the_display_format() {
        let half = 0.5f64.atan();
        for format in SlopeFormat::ALL {
            for text in ["6:12", "6 : 12", "6∶12", "6/12", "1 : 2"] {
                let v = parse_slope(text, format, &nf()).unwrap_or_else(|e| panic!("{text}: {e:?}"));
                assert!((v - half).abs() < 1e-12, "{text} in {format:?}");
            }
            let v = parse_slope("100 %", format, &nf()).unwrap();
            assert!((v - std::f64::consts::FRAC_PI_4).abs() < 1e-12);
            let v = parse_slope("30°", format, &nf()).unwrap();
            assert!((v - 30f64.to_radians()).abs() < 1e-12);
        }
        let v = parse_slope("30", SlopeFormat::Degrees, &nf()).unwrap();
        assert!((v - 30f64.to_radians()).abs() < 1e-12);
        let v = parse_slope("100", SlopeFormat::Percent, &nf()).unwrap();
        assert!((v - std::f64::consts::FRAC_PI_4).abs() < 1e-12);
        // A bare number in the ratio format is the run of `1 : n`.
        let v = parse_slope("2", SlopeFormat::Ratio, &nf()).unwrap();
        assert!((v - half).abs() < 1e-12);
        let comma = NumberFormat {
            decimal_separator: ',',
            grouping: Some('.'),
        };
        let v = parse_slope("57,74 %", SlopeFormat::Degrees, &comma).unwrap();
        assert!((v - 0.5774f64.atan()).abs() < 1e-12);
    }

    /// NFR-L10N-003: negative slopes keep their sign in every format.
    #[test]
    fn negative_slopes_keep_their_sign() {
        let pitch = -30f64.to_radians();
        for format in SlopeFormat::ALL {
            let shown = format_slope(pitch, &with_slope(format), &nf(), true);
            assert!(shown.starts_with('-'), "{shown}");
            let v = parse_slope(&shown, format, &nf()).unwrap();
            assert!(v < 0.0 && (v - pitch).abs().to_degrees() < 0.05, "{shown}");
        }
        let v = parse_slope("-6:12", SlopeFormat::Degrees, &nf()).unwrap();
        assert!((v + 0.5f64.atan()).abs() < 1e-12);
        let v = parse_slope("-50 %", SlopeFormat::Degrees, &nf()).unwrap();
        assert!((v + 0.5f64.atan()).abs() < 1e-12);
    }

    /// NFR-L10N-003: a vertical slope has no percent or ratio, so it cannot
    /// be typed that way and is shown in degrees in those formats.
    #[test]
    fn vertical_slopes_are_degrees_only() {
        let vertical = std::f64::consts::FRAC_PI_2;
        for format in [SlopeFormat::Percent, SlopeFormat::Ratio] {
            let units = with_slope(format);
            assert_eq!(format_slope(vertical, &units, &nf(), true), "90.00°");
            assert_eq!(format_slope(-vertical, &units, &nf(), true), "-90.00°");
            let v = parse_slope("90.00°", format, &nf()).unwrap();
            assert!((v - vertical).abs() < 1e-12);
        }
        for text in ["1:0", "1 : 0.000", "0", "0/0"] {
            let e = parse_slope(text, SlopeFormat::Ratio, &nf()).unwrap_err();
            assert_eq!(e, ParseError::InvalidSlope(text.to_owned()), "{text}");
        }
        for text in ["inf %", "1e99 %", "abc %", "%"] {
            assert!(parse_slope(text, SlopeFormat::Percent, &nf()).is_err(), "{text}");
        }
        assert!(parse_slope("91°", SlopeFormat::Degrees, &nf()).is_err());
        assert_eq!(parse_slope(" ", SlopeFormat::Degrees, &nf()), Err(ParseError::Empty));
        let e = parse_slope("steep", SlopeFormat::Degrees, &nf()).unwrap_err();
        assert_eq!(e.message().key(), "model-error-parse-slope");
    }

    /// NFR-L10N-003: a percent or ratio whose pitch is within
    /// `ANGLE_TOLERANCE` of a right angle is vertical, as `format_slope`
    /// treats it, so it cannot be typed that way either.
    #[test]
    fn nearly_vertical_percent_and_ratio_slopes_are_rejected() {
        let huge = format!("1{}", "0".repeat(40));
        let tiny = format!("0.{}1", "0".repeat(40));
        for (text, format) in [
            (format!("{huge} %"), SlopeFormat::Percent),
            (huge.clone(), SlopeFormat::Percent),
            (format!("-{huge}%"), SlopeFormat::Ratio),
            (format!("{huge} : 1"), SlopeFormat::Degrees),
            (format!("1 : {tiny}"), SlopeFormat::Percent),
            (tiny.clone(), SlopeFormat::Ratio),
            (format!("-{tiny}"), SlopeFormat::Ratio),
        ] {
            let e = parse_slope(&text, format, &nf()).unwrap_err();
            assert_eq!(e, ParseError::InvalidSlope(text.clone()), "{text} in {format:?}");
        }
        // Steep but not vertical: 1 : 0.001 is about 89.94°.
        let v = parse_slope("1 : 0.001", SlopeFormat::Ratio, &nf()).unwrap();
        assert!((v - 1000f64.atan()).abs() < 1e-12);
        assert!(parse_slope("100000 %", SlopeFormat::Percent, &nf()).is_ok());
    }

    /// STR-009: degrees-minutes-seconds format every finite angle without
    /// panicking, even one too large for the integer count of seconds.
    #[test]
    fn dms_formats_huge_angles_without_panicking() {
        for decimals in [0, 1, 6] {
            let units = UnitSettings {
                angle: AngleUnit::DegreesMinutesSeconds,
                angle_decimals: decimals,
                ..UnitSettings::default()
            };
            for angle in [1e300, -1e300, f64::MAX, f64::MIN] {
                let text = format_angle(angle, &units, &nf(), true);
                assert_eq!(text.starts_with('-'), angle < 0.0, "{text}");
                assert!(text.ends_with('"'), "{text}");
            }
        }
    }

    /// NFR-L10N-003: typed angles accept the suffix of every angle unit,
    /// whatever the display unit, as lengths accept every length suffix.
    #[test]
    fn parse_angle_accepts_the_suffix_of_every_unit() {
        use std::f64::consts::FRAC_PI_2;
        for unit in AngleUnit::ALL {
            for text in [
                "100gon",
                "100 gon",
                "100grad",
                "90°",
                "1.5707963267948966 rad",
                "90°0'0\"",
            ] {
                let v = parse_angle(text, unit, &nf()).unwrap_or_else(|e| panic!("{text} in {unit:?}: {e:?}"));
                assert!((v - FRAC_PI_2).abs() < 1e-12, "{text} in {unit:?}");
            }
        }
        // 1.5708 as an exact quotient, which clippy does not take for π/2.
        assert_eq!(
            parse_angle("1.5708rad", AngleUnit::DecimalDegrees, &nf()),
            Ok(15_708.0 / 10_000.0)
        );
        let v = parse_angle("100gon", AngleUnit::DecimalDegrees, &nf()).unwrap();
        assert!((v - FRAC_PI_2).abs() < 1e-12);
        let v = parse_angle("-50 gon", AngleUnit::Radians, &nf()).unwrap();
        assert!((v + FRAC_PI_2 / 2.0).abs() < 1e-12);
        let comma = NumberFormat {
            decimal_separator: ',',
            grouping: Some('.'),
        };
        assert_eq!(parse_angle("1,5 rad", AngleUnit::DecimalDegrees, &comma), Ok(1.5));
        for text in ["100 gons", "rad", "1 rad 2", "5 m"] {
            assert!(parse_angle(text, AngleUnit::DecimalDegrees, &nf()).is_err(), "{text}");
        }
    }

    /// STR-009: degrees-minutes-seconds show the seconds with the angle
    /// decimals.
    #[test]
    fn dms_shows_seconds_with_the_angle_decimals() {
        let mut units = UnitSettings {
            angle: AngleUnit::DegreesMinutesSeconds,
            angle_decimals: 1,
            ..UnitSettings::default()
        };
        let angle = (30.0 + 15.0 / 60.0 + 7.26 / 3600.0f64).to_radians();
        assert_eq!(format_angle(angle, &units, &nf(), true), "30°15'07.3\"");
        let comma = NumberFormat {
            decimal_separator: ',',
            grouping: Some('.'),
        };
        assert_eq!(format_angle(angle, &units, &comma, true), "30°15'07,3\"");
        let v = parse_angle("30°15'07,3\"", AngleUnit::DegreesMinutesSeconds, &comma).unwrap();
        assert!((v.to_degrees() - (30.0 + 15.0 / 60.0 + 7.3 / 3600.0)).abs() < 1e-9);
        units.angle_decimals = 0;
        assert_eq!(format_angle(angle, &units, &nf(), true), "30°15'07\"");
        assert_eq!(format_angle(-angle, &units, &nf(), true), "-30°15'07\"");
        units.angle_decimals = 2;
        let almost = (59.0 + 59.0 / 60.0 + 59.999 / 3600.0f64).to_radians();
        assert_eq!(format_angle(almost, &units, &nf(), true), "60°00'00.00\"");
    }

    /// NFR-L10N-003: a value in each of the six length units and the four
    /// angle units is shown and typed back unchanged.
    #[test]
    fn every_length_and_angle_unit_formats_and_parses() {
        let expected = [
            (LengthUnit::Meter, "2.540 m"),
            (LengthUnit::Centimeter, "254.000 cm"),
            (LengthUnit::Millimeter, "2540.000 mm"),
            (LengthUnit::FeetInches, "8'-4\""),
            (LengthUnit::Feet, "8.333 ft"),
            (LengthUnit::Inches, "100.000 in"),
        ];
        for (unit, text) in expected {
            let units = UnitSettings {
                length: unit,
                ..UnitSettings::default()
            };
            assert_eq!(format_length(2.54, &units, &nf(), true), text, "{unit:?}");
            let v = parse_length(text, unit, &nf()).unwrap();
            assert!((v - 2.54).abs() < 0.001, "{text} reads back as {v}");
            let bare = format_length(2.54, &units, &nf(), false);
            let v = parse_length(&bare, unit, &nf()).unwrap();
            assert!((v - 2.54).abs() < 0.001, "{bare} in {unit:?} reads back as {v}");
        }
        let angle = 30f64.to_radians();
        let expected = [
            (AngleUnit::DecimalDegrees, "30.00°"),
            (AngleUnit::DegreesMinutesSeconds, "30°00'00.00\""),
            (AngleUnit::Gradians, "33.33 gon"),
            (AngleUnit::Radians, "0.524 rad"),
        ];
        for (unit, text) in expected {
            let units = UnitSettings {
                angle: unit,
                ..UnitSettings::default()
            };
            assert_eq!(format_angle(angle, &units, &nf(), true), text, "{unit:?}");
            for shown in [text.to_owned(), format_angle(angle, &units, &nf(), false)] {
                let v = parse_angle(&shown, unit, &nf()).unwrap_or_else(|e| panic!("{shown}: {e:?}"));
                assert!((v - angle).abs() < 0.001, "{shown} reads back as {v}");
            }
        }
    }
}
