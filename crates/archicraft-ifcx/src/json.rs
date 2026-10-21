//! Deterministic JSON output (IFX-007, IFX-008).
//!
//! `serde_json` writes small and large floating-point numbers in exponent
//! notation (`2e-6`). IFCX output must use plain decimal notation, so numbers
//! are formatted here: the shortest representation that reads back as the
//! same `f64`, rewritten without an exponent. Object members keep their
//! insertion order.
//!
//! Rounding is not done here: the writer rounds standard-namespace
//! coordinates with [`round_coordinate`] and leaves `archicraft::`
//! parameters at full precision (NFR-ACC-002), see [`crate::alpha::writer`].
//!
//! [`Writer`] writes a document piece by piece with the same formatting, so
//! that a large document never has to exist as one [`Value`] (NFR-SEC-003).

use serde_json::{Number, Value};

/// Writes `value` as compact JSON, or indented with two spaces.
pub fn to_string(value: &Value, pretty: bool) -> String {
    let mut writer = Writer::new(pretty);
    writer.value(value);
    writer.finish()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Container {
    Object,
    Array,
}

/// Writes one JSON document member by member and item by item, formatted
/// exactly as [`to_string`] formats the whole document (IFX-007, IFX-008).
/// The writer of IFCX files streams its data nodes through it, holding one
/// node at a time instead of a tree of the whole file (NFR-SEC-003).
///
/// In an object, [`key`](Self::key) comes before each member's value; in an
/// array, each value is an item. An array opened with
/// [`begin_array`](Self::begin_array) is written one item per line when
/// indenting, as [`to_string`] writes an array that holds anything but
/// numbers, so its items must not be numbers; a list of numbers goes in
/// whole through [`value`](Self::value).
#[derive(Debug)]
pub struct Writer {
    out: String,
    pretty: bool,
    /// The open objects and arrays, innermost last, each with the number of
    /// members or items written so far.
    open: Vec<(Container, usize)>,
}

impl Writer {
    pub fn new(pretty: bool) -> Self {
        Self {
            out: String::with_capacity(1 << 16),
            pretty,
            open: Vec::new(),
        }
    }

    /// Before a value: in an array, the separator and indentation of a new
    /// item; after a key, nothing.
    fn item(&mut self) {
        if let Some((Container::Array, items)) = self.open.last_mut() {
            if *items > 0 {
                self.out.push(',');
            }
            *items += 1;
            if self.pretty {
                indent(&mut self.out, self.open.len());
            }
        }
    }

    /// Starts the member `key` of the innermost object; its value follows.
    pub fn key(&mut self, key: &str) {
        debug_assert!(
            matches!(self.open.last(), Some((Container::Object, _))),
            "a key outside an object"
        );
        if let Some((_, members)) = self.open.last_mut() {
            if *members > 0 {
                self.out.push(',');
            }
            *members += 1;
        }
        if self.pretty {
            indent(&mut self.out, self.open.len());
        }
        write_string(key, &mut self.out);
        self.out.push(':');
        if self.pretty {
            self.out.push(' ');
        }
    }

    /// Writes a whole value: a member's value after [`key`](Self::key), an
    /// item in an array, or the document.
    pub fn value(&mut self, value: &Value) {
        debug_assert!(
            !(value.is_number() && matches!(self.open.last(), Some((Container::Array, _)))),
            "a number item in a streamed array"
        );
        self.item();
        write_value(value, self.pretty, self.open.len(), &mut self.out);
    }

    /// Opens an object, written member by member until [`end`](Self::end).
    pub fn begin_object(&mut self) {
        self.item();
        self.out.push('{');
        self.open.push((Container::Object, 0));
    }

    /// Opens an array of items that are not numbers, written item by item
    /// until [`end`](Self::end).
    pub fn begin_array(&mut self) {
        self.item();
        self.out.push('[');
        self.open.push((Container::Array, 0));
    }

    /// Closes the innermost object or array.
    pub fn end(&mut self) {
        let Some((container, count)) = self.open.pop() else {
            debug_assert!(false, "nothing to end");
            return;
        };
        if self.pretty && count > 0 {
            indent(&mut self.out, self.open.len());
        }
        self.out.push(match container {
            Container::Object => '}',
            Container::Array => ']',
        });
    }

    /// The document, with the final newline of the indented form. Objects
    /// and arrays still open are closed first.
    pub fn finish(mut self) -> String {
        while !self.open.is_empty() {
            self.end();
        }
        if self.pretty {
            self.out.push('\n');
        }
        self.out
    }
}

fn indent(out: &mut String, level: usize) {
    out.push('\n');
    for _ in 0..level {
        out.push_str("  ");
    }
}

/// True for arrays that are printed on one line even when indenting:
/// coordinate tuples and flat number lists.
fn is_flat(items: &[Value]) -> bool {
    items.iter().all(|v| matches!(v, Value::Number(_)))
}

fn write_value(value: &Value, pretty: bool, level: usize, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => write_number(n, out),
        Value::String(s) => write_string(s, out),
        Value::Array(items) => {
            out.push('[');
            let multiline = pretty && !items.is_empty() && !is_flat(items);
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                    if pretty && !multiline {
                        out.push(' ');
                    }
                }
                if multiline {
                    indent(out, level + 1);
                }
                write_value(item, pretty, level + 1, out);
            }
            if multiline {
                indent(out, level);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            for (i, (key, item)) in map.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                if pretty {
                    indent(out, level + 1);
                }
                write_string(key, out);
                out.push(':');
                if pretty {
                    out.push(' ');
                }
                write_value(item, pretty, level + 1, out);
            }
            if pretty && !map.is_empty() {
                indent(out, level);
            }
            out.push('}');
        }
    }
}

fn write_string(s: &str, out: &mut String) {
    // A plain string always serialises.
    match serde_json::to_string(s) {
        Ok(quoted) => out.push_str(&quoted),
        Err(_) => out.push_str("\"\""),
    }
}

fn write_number(n: &Number, out: &mut String) {
    if let Some(i) = n.as_i64() {
        out.push_str(&i.to_string());
    } else if let Some(u) = n.as_u64() {
        out.push_str(&u.to_string());
    } else if let Some(f) = n.as_f64() {
        out.push_str(&format_f64(f));
    } else {
        out.push('0');
    }
}

/// Formats a finite `f64` in plain decimal notation with the fewest digits
/// that read back as the same value. Integral values are written without a
/// fraction; zero is written as `0`.
pub fn format_f64(value: f64) -> String {
    if value == 0.0 || !value.is_finite() {
        return "0".to_owned();
    }
    if value.fract() == 0.0 && value.abs() < 1e15 {
        return format!("{}", value as i64);
    }
    // `{:e}` gives the shortest round-trip digits in exponent form.
    let sci = format!("{value:e}");
    plain_decimal(&sci)
}

/// Rewrites `d.ddddde±x` as a plain decimal number.
fn plain_decimal(sci: &str) -> String {
    let Some((mantissa, exponent)) = sci.split_once(['e', 'E']) else {
        return sci.to_owned();
    };
    let exponent: i64 = exponent.parse().unwrap_or(0);
    let (negative, mantissa) = match mantissa.strip_prefix('-') {
        Some(m) => (true, m),
        None => (false, mantissa),
    };
    let (int_part, frac_part) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits: String = format!("{int_part}{frac_part}");
    let point = int_part.len() as i64 + exponent;
    let mut out = String::with_capacity(digits.len() + 8);
    if negative {
        out.push('-');
    }
    if point <= 0 {
        out.push_str("0.");
        out.extend(std::iter::repeat_n('0', (-point) as usize));
        out.push_str(&digits);
    } else if point as usize >= digits.len() {
        out.push_str(&digits);
        out.extend(std::iter::repeat_n('0', point as usize - digits.len()));
    } else {
        let (a, b) = digits.split_at(point as usize);
        out.push_str(a);
        out.push('.');
        out.push_str(b);
    }
    out
}

/// Rounds a coordinate to 10⁻⁶ m (IFX-007), without negative zero.
pub fn round_coordinate(value: f64) -> f64 {
    let rounded = (value * 1e6).round() / 1e6;
    if rounded == 0.0 { 0.0 } else { rounded }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_notation_round_trips() {
        for v in [
            0.000002,
            2.8,
            -0.3,
            1e-7,
            123456.789,
            1e21,
            5.0,
            -1.0,
            0.1 + 0.2,
            1.0 / 3.0,
            6.02e23,
            -4.5e-9,
        ] {
            let text = format_f64(v);
            assert!(!text.contains('e'), "{text}");
            let back: f64 = text.parse().unwrap();
            assert_eq!(back, v, "{text}");
        }
        assert_eq!(format_f64(0.000002), "0.000002");
        assert_eq!(format_f64(5.0), "5");
        assert_eq!(format_f64(-0.0), "0");
        assert_eq!(format_f64(2.8), "2.8");
    }

    #[test]
    fn rounding_coordinates() {
        assert_eq!(round_coordinate(1.23456789), 1.234568);
        assert_eq!(round_coordinate(-0.0000001), 0.0);
        assert_eq!(format_f64(round_coordinate(4.7 + 1e-12)), "4.7");
    }

    #[test]
    fn writes_compact_and_pretty() {
        let v = serde_json::json!({"a": [1, 2.5], "b": {"c": "x\"y"}, "d": [[0.0, 1e-6]]});
        assert_eq!(
            to_string(&v, false),
            r#"{"a":[1,2.5],"b":{"c":"x\"y"},"d":[[0,0.000001]]}"#
        );
        let pretty = to_string(&v, true);
        assert!(pretty.contains("\"a\": [1, 2.5]"));
        let back: Value = serde_json::from_str(&pretty).unwrap();
        assert_eq!(back["d"][0][1], serde_json::json!(0.000001));
    }

    /// Streams `value` as deeply as the writer allows: every object member by
    /// member and every array that holds no number item by item.
    fn stream(value: &Value, writer: &mut Writer) {
        match value {
            Value::Object(map) => {
                writer.begin_object();
                for (key, item) in map {
                    writer.key(key);
                    stream(item, writer);
                }
                writer.end();
            }
            Value::Array(items) if !items.iter().any(Value::is_number) => {
                writer.begin_array();
                for item in items {
                    stream(item, writer);
                }
                writer.end();
            }
            other => writer.value(other),
        }
    }

    /// NFR-SEC-003 with IFX-007, IFX-008: a document written piece by piece
    /// has the bytes of the whole document, compact and indented, also with
    /// empty objects and arrays and with whole values among streamed ones.
    #[test]
    fn streaming_writes_the_bytes_of_the_whole_document() {
        let document = serde_json::json!({
            "header": {"id": "x", "n": 2.5},
            "empty": {},
            "none": [],
            "data": [
                {"path": "a", "attributes": {"points": [[0.0, 1e-6], [2, 3]], "name": "A"}},
                {"path": "b"},
                ["text", null, true, {"deep": [[], {}]}],
            ],
            "numbers": [1, 2, 3],
        });
        for pretty in [false, true] {
            let mut writer = Writer::new(pretty);
            stream(&document, &mut writer);
            assert_eq!(writer.finish(), to_string(&document, pretty), "indented: {pretty}");
        }
        let mut writer = Writer::new(true);
        writer.begin_object();
        writer.key("data");
        writer.begin_array();
        writer.value(&serde_json::json!({"path": "a"}));
        assert_eq!(
            writer.finish(),
            to_string(&serde_json::json!({"data": [{"path": "a"}]}), true)
        );
    }
}
