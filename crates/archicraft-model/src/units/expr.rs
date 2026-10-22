//! Arithmetic in typed lengths and angles (INP-022, INP-005).
//!
//! [`parse_length_expr`] and [`parse_angle_expr`] read `+`, `-` (or `−`),
//! `*` (or `×`), `/` (or `÷`) and parentheses with the usual precedence,
//! left to right:
//!
//! ```text
//! expr     = term (('+' | '-') term)*
//! term     = factor (('*' | '/') factor)*
//! factor   = ('-' | '+') factor | '(' expr ')' unit? | operand
//! ```
//!
//! Every operand is read by the single-value parsers of the parent module
//! ([`parse_length`](super::parse_length), [`parse_angle`](super::parse_angle)),
//! so unit suffixes, marks, mixed numbers and feet-inches work in an
//! expression as they do alone. A bare number has no unit: as a factor or
//! divisor it scales, in a sum it is a value in the default unit, and a
//! result without unit is read in the default unit, so `10/4` is 2.5 of the
//! default unit and `3*1.2m` is 3.6 m. A product of two values with units,
//! and a division by one, is not a length or angle and is rejected.
//!
//! Fractions and divisions in lengths: `a b/c` (a whole number, a space and a
//! fraction, `2 1/2`), and `b/c` followed by a unit or a mark (`1/2"`,
//! `3/4 in`), are one number; any other `/` divides, so a bare `10/4` is a
//! division. A `-` after a foot mark joins feet and inches, with or without
//! spaces (`3'-4 1/2"`, `3' - 4"`), as it does in a single value; to
//! subtract from feet, put the subtrahend in parentheses (`3' - (4")`). In
//! angles every `/` divides.

use super::{
    AngleUnit, LengthUnit, NumberFormat, ParseError, angle_operand, length_operand, normalize_number_text,
    parse_mixed_number,
};

/// Parentheses and signs nest at most this deep, so a typed or fuzzed text
/// cannot exhaust the stack.
const MAX_DEPTH: usize = 64;

/// Parses a typed length with arithmetic (INP-022): see the module
/// documentation for the grammar. Accepts every text [`super::parse_length`]
/// accepts, with the same value. Returns metres.
pub fn parse_length_expr(text: &str, default_unit: LengthUnit, nf: &NumberFormat) -> Result<f64, ParseError> {
    let kind = Kind::Length(default_unit);
    parse(text, kind, nf).map_err(|e| e.unwrap_or_else(|| ParseError::InvalidLength(text.trim().to_owned())))
}

/// Parses a typed angle with arithmetic (INP-022): see the module
/// documentation for the grammar. Accepts every text [`super::parse_angle`]
/// accepts, with the same value. Returns radians.
pub fn parse_angle_expr(text: &str, unit: AngleUnit, nf: &NumberFormat) -> Result<f64, ParseError> {
    let kind = Kind::Angle(unit);
    parse(text, kind, nf).map_err(|e| e.unwrap_or_else(|| ParseError::InvalidAngle(text.trim().to_owned())))
}

/// What the operands are, with the unit of a bare result.
#[derive(Clone, Copy)]
enum Kind {
    Length(LengthUnit),
    Angle(AngleUnit),
}

impl Kind {
    /// A bare number read in the default unit, in metres or radians, with
    /// the arithmetic of [`length_operand`] and [`angle_operand`], so a
    /// single value reads bit for bit as it does alone.
    fn in_unit(self, v: f64) -> f64 {
        match self {
            Kind::Length(unit) => v * unit.meters_per_unit(),
            Kind::Angle(AngleUnit::DecimalDegrees | AngleUnit::DegreesMinutesSeconds) => v.to_radians(),
            Kind::Angle(AngleUnit::Gradians) => v * std::f64::consts::PI / 200.0,
            Kind::Angle(AngleUnit::Radians) => v,
        }
    }

    /// An operand: a bare number, or a value with unit in metres or radians.
    fn operand(self, s: &str) -> Option<Quantity> {
        let bare = !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | ' ' | '/'));
        let value = match self {
            Kind::Length(_) if bare => parse_mixed_number(s),
            Kind::Length(unit) => length_operand(s, unit),
            Kind::Angle(_) if bare => s.parse().ok(),
            Kind::Angle(unit) => angle_operand(s, unit),
        };
        value
            .filter(|v| v.is_finite())
            .map(|value| Quantity { value, unit: !bare })
    }
}

/// A value without unit (a plain number) or with one (metres, radians).
#[derive(Clone, Copy, Debug)]
struct Quantity {
    value: f64,
    unit: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Token<'a> {
    Operand(&'a str),
    /// `+ - * / ( )`, with `×` and `÷` read as `*` and `/`.
    Op(char),
}

/// `Err(None)` is a text that is not a value of `kind`; `Err(Some(_))` a
/// more specific error.
fn parse(text: &str, kind: Kind, nf: &NumberFormat) -> Result<f64, Option<ParseError>> {
    let normalized = normalize_number_text(text, nf).ok_or(None)?;
    let s = normalized.trim();
    if s.is_empty() {
        return Err(Some(ParseError::Empty));
    }
    let tokens = tokenize(s, kind);
    let mut parser = Parser {
        tokens: &tokens,
        pos: 0,
        kind,
    };
    let q = parser.expr(0).ok_or(None)?;
    if parser.pos != tokens.len() {
        return Err(None);
    }
    let value = if q.unit { q.value } else { kind.in_unit(q.value) };
    if value.is_finite() { Ok(value) } else { Err(None) }
}

fn op(c: char) -> Option<char> {
    match c {
        '+' | '-' | '*' | '/' | '(' | ')' => Some(c),
        '×' => Some('*'),
        '÷' => Some('/'),
        _ => None,
    }
}

/// Splits normalised text into operands and operators. An operand runs to
/// the next operator, except for the `/` of a fraction and the `-` between
/// feet and inches in a length (module documentation).
fn tokenize(s: &str, kind: Kind) -> Vec<Token<'_>> {
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i].1;
        if c.is_whitespace() {
            i += 1;
        } else if let Some(o) = op(c) {
            tokens.push(Token::Op(o));
            i += 1;
        } else {
            let start = i;
            while i < chars.len() {
                let joins = match (kind, chars[i].1) {
                    (Kind::Length(_), '/') => fraction_slash(&chars, start, i),
                    (Kind::Length(_), '-') => feet_inch_dash(&chars, start, i),
                    (_, c) => op(c).is_none(),
                };
                if !joins {
                    break;
                }
                i += 1;
            }
            let end = chars.get(i).map_or(s.len(), |(b, _)| *b);
            tokens.push(Token::Operand(s[chars[start].0..end].trim_end()));
        }
    }
    tokens
}

/// Whether the `/` at `i` of an operand starting at `start` is a fraction
/// bar: between digits, and either after a whole number and a space
/// (`2 1/2`) or before a unit or a mark (`1/2"`, `3/4 in`).
fn fraction_slash(chars: &[(usize, char)], start: usize, i: usize) -> bool {
    let digit = |j: usize| chars.get(j).is_some_and(|(_, c)| c.is_ascii_digit());
    if i == start || !digit(i - 1) || !digit(i + 1) {
        return false;
    }
    let mut numerator = i - 1;
    while numerator > start && digit(numerator - 1) {
        numerator -= 1;
    }
    let mut before = numerator;
    while before > start && chars[before - 1].1 == ' ' {
        before -= 1;
    }
    let mixed = before < numerator && before > start && digit(before - 1);
    let mut after = i + 1;
    while digit(after) || chars.get(after).is_some_and(|(_, c)| *c == '.') {
        after += 1;
    }
    while chars.get(after).is_some_and(|(_, c)| *c == ' ') {
        after += 1;
    }
    let unit = chars
        .get(after)
        .is_some_and(|(_, c)| c.is_ascii_alphabetic() || matches!(c, '\'' | '"'));
    mixed || unit
}

/// Whether the `-` at `i` belongs to a feet-inch value: after a foot mark
/// and before a number, with or without spaces between (`3'-4 1/2"`,
/// `3' - 4"`), as [`parse_length`](super::parse_length) reads it. The
/// operand then must be feet and inches, so `3'-4` stays an error; before a
/// parenthesis the `-` subtracts (`3' - (4")`).
fn feet_inch_dash(chars: &[(usize, char)], start: usize, i: usize) -> bool {
    let mut before = i;
    while before > start && chars[before - 1].1 == ' ' {
        before -= 1;
    }
    let mut after = i + 1;
    while chars.get(after).is_some_and(|(_, c)| *c == ' ') {
        after += 1;
    }
    before > start
        && chars[before - 1].1 == '\''
        && chars.get(after).is_some_and(|(_, c)| c.is_ascii_digit() || *c == '.')
}

struct Parser<'t, 'a> {
    tokens: &'t [Token<'a>],
    pos: usize,
    kind: Kind,
}

impl<'a> Parser<'_, 'a> {
    fn peek(&self) -> Option<Token<'a>> {
        self.tokens.get(self.pos).copied()
    }

    fn eat(&mut self, o: char) -> bool {
        let hit = self.peek() == Some(Token::Op(o));
        if hit {
            self.pos += 1;
        }
        hit
    }

    fn expr(&mut self, depth: usize) -> Option<Quantity> {
        let mut acc = self.term(depth)?;
        loop {
            let sign = if self.eat('+') {
                1.0
            } else if self.eat('-') {
                -1.0
            } else {
                return Some(acc);
            };
            let rhs = self.term(depth)?;
            acc = self.add(acc, rhs, sign);
        }
    }

    /// A sum: a bare number next to a value with unit is in the default
    /// unit.
    fn add(&self, a: Quantity, b: Quantity, sign: f64) -> Quantity {
        let with_unit = |q: Quantity| if q.unit { q.value } else { self.kind.in_unit(q.value) };
        if a.unit || b.unit {
            Quantity {
                value: with_unit(a) + sign * with_unit(b),
                unit: true,
            }
        } else {
            Quantity {
                value: a.value + sign * b.value,
                unit: false,
            }
        }
    }

    fn term(&mut self, depth: usize) -> Option<Quantity> {
        let mut acc = self.factor(depth)?;
        loop {
            if self.eat('*') {
                let rhs = self.factor(depth)?;
                if acc.unit && rhs.unit {
                    return None;
                }
                acc = Quantity {
                    value: acc.value * rhs.value,
                    unit: acc.unit || rhs.unit,
                };
            } else if self.eat('/') {
                let rhs = self.factor(depth)?;
                if rhs.unit || rhs.value == 0.0 {
                    return None;
                }
                acc.value /= rhs.value;
            } else {
                return Some(acc);
            }
        }
    }

    fn factor(&mut self, depth: usize) -> Option<Quantity> {
        if depth >= MAX_DEPTH {
            return None;
        }
        if self.eat('-') {
            let q = self.factor(depth + 1)?;
            return Some(Quantity { value: -q.value, ..q });
        }
        if self.eat('+') {
            return self.factor(depth + 1);
        }
        if self.eat('(') {
            let q = self.expr(depth + 1)?;
            if !self.eat(')') {
                return None;
            }
            // `(1+2)m`: a unit after a bare group applies to it.
            if let Some(Token::Operand(unit)) = self.peek()
                && !q.unit
                && !unit.contains(|c: char| c.is_ascii_digit())
            {
                self.pos += 1;
                let one = self.kind.operand(&format!("1{unit}")).filter(|one| one.unit)?;
                return Some(Quantity {
                    value: q.value * one.value,
                    unit: true,
                });
            }
            return Some(q);
        }
        match self.peek()? {
            Token::Operand(s) => {
                self.pos += 1;
                self.kind.operand(s)
            }
            Token::Op(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{METERS_PER_FOOT, METERS_PER_INCH, input_alphabet, parse_angle, parse_length};

    fn nf() -> NumberFormat {
        NumberFormat::default()
    }

    fn length(text: &str) -> f64 {
        parse_length_expr(text, LengthUnit::Meter, &nf()).unwrap_or_else(|e| panic!("{text}: {e:?}"))
    }

    fn assert_close(text: &str, value: f64, expected: f64) {
        assert!(
            (value - expected).abs() < 1e-12,
            "{text} gives {value}, expected {expected}"
        );
    }

    /// INP-022: the four operations with precedence and parentheses, bare
    /// numbers in the default unit, units on operands, the typographic signs
    /// and the decimal comma.
    #[test]
    fn lengths_take_arithmetic() {
        for (text, metres) in [
            ("2.5+0.3", 2.8),
            ("3*1.2m", 3.6),
            ("(1+2)/2", 1.5),
            ("10/4", 2.5),
            ("2m-15cm", 1.85),
            ("2 + 3 * 4", 14.0),
            ("2 * 3 - 4 / 8", 5.5),
            ("8/4/2", 1.0),
            ("1-2-3", -4.0),
            ("-(1+2)*2", -6.0),
            ("2*-3", -6.0),
            ("3×1.2m", 3.6),
            ("3÷2", 1.5),
            ("2−0.5", 1.5),
            ("1.2m*3", 3.6),
            ("2m/4", 0.5),
            ("(1+2)m", 3.0),
            ("(1+2) cm", 0.03),
            ("2 + 50mm", 2.05),
            ("2/4m", 0.5),
            ("1.5/2.5m", 0.6),
            ("1.5/2.5", 0.6),
        ] {
            assert_close(text, length(text), metres);
        }
        let mm = parse_length_expr("10/4", LengthUnit::Millimeter, &nf()).unwrap();
        assert_close("10/4 in mm", mm, 0.0025);
        let mm = parse_length_expr("2m + 50", LengthUnit::Millimeter, &nf()).unwrap();
        assert_close("2m + 50 in mm", mm, 2.05);
        let comma = NumberFormat {
            decimal_separator: ',',
            grouping: Some('.'),
        };
        let v = parse_length_expr("2,5+0,3", LengthUnit::Meter, &comma).unwrap();
        assert_close("2,5+0,3", v, 2.8);
        let v = parse_length_expr("1.234,5 mm * 2", LengthUnit::Meter, &comma).unwrap();
        assert_close("1.234,5 mm * 2", v, 2.469);
    }

    /// INP-022: typed input admits the operators and parentheses.
    #[test]
    fn the_input_alphabet_admits_the_operators() {
        let alphabet = input_alphabet(&nf());
        for c in "+-−*×/÷()".chars() {
            assert!(alphabet.contains(c), "{c:?}");
        }
    }

    /// INP-005 within INP-022: feet-inches and mixed numbers stay one
    /// operand, and a bare `b/c` divides (the module's fraction rule, both
    /// ways).
    #[test]
    fn imperial_fractions_stay_operands() {
        let feet_inches = 3.0 * METERS_PER_FOOT + 4.5 * METERS_PER_INCH;
        for (text, metres) in [
            ("3'-4 1/2\"", feet_inches),
            ("3′-4 1/2″", feet_inches),
            ("3' 4 1/2\"", feet_inches),
            ("2 1/2", 2.5),
            ("2 1/2m", 2.5),
            ("1/2\"", 0.5 * METERS_PER_INCH),
            ("3/4 in", 0.75 * METERS_PER_INCH),
            ("1/2'", 0.5 * METERS_PER_FOOT),
            ("1/2", 0.5),
            ("2 1/2 + 1/2", 3.0),
            ("3' - 4\"", 40.0 * METERS_PER_INCH),
            ("3' -4\"", 40.0 * METERS_PER_INCH),
            ("3' - (4\")", 32.0 * METERS_PER_INCH),
            ("3'-4\" - 2\"", 38.0 * METERS_PER_INCH),
            ("3'+4\"", 40.0 * METERS_PER_INCH),
            ("3'-4\" * 2", 80.0 * METERS_PER_INCH),
            ("(1+2)'", 3.0 * METERS_PER_FOOT),
        ] {
            assert_close(text, length(text), metres);
        }
    }

    /// INP-022: angles take the same arithmetic; bare numbers are in the
    /// angle unit.
    #[test]
    fn angles_take_arithmetic() {
        let deg = |text: &str| {
            parse_angle_expr(text, AngleUnit::DecimalDegrees, &nf())
                .unwrap()
                .to_degrees()
        };
        for (text, degrees) in [
            ("45+15", 60.0),
            ("90/2", 45.0),
            ("30°+15'", 30.25),
            ("-30°30'", -30.5),
            ("2*(10+5)", 30.0),
            ("100gon/2", 45.0),
            ("45° - 15", 30.0),
        ] {
            assert!((deg(text) - degrees).abs() < 1e-9, "{text} gives {}", deg(text));
        }
        let gon = parse_angle_expr("45+15", AngleUnit::Gradians, &nf()).unwrap();
        assert!((gon - 60.0 * std::f64::consts::PI / 200.0).abs() < 1e-12);
    }

    /// INP-022: an incomplete or meaningless expression is an error with the
    /// rejected text as its message argument, never a panic.
    #[test]
    fn malformed_expressions_are_errors_with_a_message() {
        for text in [
            "2.5 +", "2//3", "2m*3m", "2m/3m", "1/0", "2/(1-1)", "()", "(2", "2)", "+", "*2", "2 3", "2m 3", "(2m)m",
            "inf", "nan", "1e3", "3'-4", "3' - 4",
        ] {
            let e = parse_length_expr(text, LengthUnit::Meter, &nf()).unwrap_err();
            assert_eq!(e, ParseError::InvalidLength(text.trim().to_owned()), "{text}");
            assert_eq!(e.message().key(), "model-error-parse-length");
        }
        for text in ["45°*2°", "90/2°", "45 +"] {
            let e = parse_angle_expr(text, AngleUnit::DecimalDegrees, &nf()).unwrap_err();
            assert_eq!(e, ParseError::InvalidAngle(text.to_owned()), "{text}");
        }
        assert_eq!(
            parse_length_expr("  ", LengthUnit::Meter, &nf()),
            Err(ParseError::Empty)
        );
        assert_eq!(parse_angle_expr("", AngleUnit::Radians, &nf()), Err(ParseError::Empty));
        let deep = format!("{}1{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_length_expr(&deep, LengthUnit::Meter, &nf()).is_err());
        assert!(parse_length_expr(&"-".repeat(10_000), LengthUnit::Meter, &nf()).is_err());
    }

    /// INP-005: every single value [`parse_length`] and [`parse_angle`] read
    /// before reads the same through the expression parsers, in every unit
    /// and number format.
    #[test]
    fn single_values_read_as_before() {
        let formats = [
            NumberFormat::default(),
            NumberFormat {
                decimal_separator: ',',
                grouping: Some('.'),
            },
            NumberFormat {
                decimal_separator: '.',
                grouping: Some(' '),
            },
            NumberFormat {
                decimal_separator: '.',
                grouping: Some('\''),
            },
        ];
        let lengths = [
            "2.5",
            "250",
            "-0.3",
            "+0.3",
            "- 3",
            "365mm",
            "365 mm",
            "30 cm",
            "10ft",
            "6in",
            "3'-4 1/2\"",
            "3′ 4″",
            "1/2\"",
            "12'",
            "-3'-4\"",
            "4 1/2\"",
            "3'4.5\"",
            "2,5",
            "1.234,5",
            "1\u{A0}234,5",
            "1’234’567.5",
            "1'234.5",
            "1,234.5",
            "1 234.5",
            "2,5\u{A0}m",
            "1234.5678 mm",
            "4 125/128\"",
            "5'-11 63/64\"",
            "",
            "abc",
            "3' - 4\"",
            "3' -4\"",
            "3'- 4 1/2\"",
            "3 furlongs",
            "1/0",
            "1,2,3",
            "1 234,5",
            "4 1/2",
            "1/2",
        ];
        for nf in &formats {
            for unit in LengthUnit::ALL {
                for text in lengths {
                    if let Ok(before) = parse_length(text, unit, nf) {
                        let now = parse_length_expr(text, unit, nf);
                        assert_eq!(now, Ok(before), "{text:?} in {unit:?} with {nf:?}");
                    }
                }
            }
        }
        let angles = [
            "45",
            "-45",
            "30°30'",
            "30°15'07.3\"",
            "100",
            "33.33 gon",
            "50grad",
            "0.524 rad",
            "30.00°",
            "-30°",
            "1,5",
        ];
        for nf in &formats {
            for unit in AngleUnit::ALL {
                for text in angles {
                    if let Ok(before) = parse_angle(text, unit, nf) {
                        let now = parse_angle_expr(text, unit, nf);
                        assert_eq!(now, Ok(before), "{text:?} in {unit:?} with {nf:?}");
                    }
                }
            }
        }
    }

    /// INP-022: 200 seeded random expressions of up to four decimal operands
    /// evaluate equal to the same computation in `f64`, with `*` and `/`
    /// binding before `+` and `-`, within 1e-12.
    #[test]
    fn random_expressions_match_f64() {
        let mut state = 0x2545_f491_4f6c_dd1d_u64;
        let mut next = move |n: u64| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state % n
        };
        for _ in 0..200 {
            let count = 1 + next(4) as usize;
            let operands: Vec<f64> = (0..count).map(|_| (1 + next(99_999)) as f64 / 1000.0).collect();
            let ops: Vec<char> = (1..count).map(|_| ['+', '-', '*', '/'][next(4) as usize]).collect();
            let mut text = operands[0].to_string();
            for (o, v) in ops.iter().zip(&operands[1..]) {
                let space = if next(2) == 0 { "" } else { " " };
                text.push_str(&format!("{space}{o}{space}{v}"));
            }
            // Products and quotients first, then the sum left to right.
            let mut terms = vec![operands[0]];
            let mut signs = vec![1.0];
            for (o, v) in ops.iter().zip(&operands[1..]) {
                let last = terms.last_mut().unwrap();
                match o {
                    '*' => *last *= v,
                    '/' => *last /= v,
                    '+' | '-' => {
                        terms.push(*v);
                        signs.push(if *o == '+' { 1.0 } else { -1.0 });
                    }
                    _ => unreachable!(),
                }
            }
            let expected = terms
                .iter()
                .zip(&signs)
                .skip(1)
                .fold(terms[0], |acc, (t, s)| acc + s * t);
            let value = length(&text);
            assert!(
                (value - expected).abs() <= 1e-12,
                "{text} gives {value}, expected {expected}"
            );
        }
    }
}
