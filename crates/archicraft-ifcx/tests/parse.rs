//! The two container parsers agree (NFR-SEC-003, NFR-REL-007):
//! `validate::parse_raw`, the parser of the open path, which keeps attribute
//! values as raw JSON text, accepts exactly the documents that
//! `validate::parse` accepts, which holds them as trees, and rejects the
//! others with the same message, wherever the damage is. Validation finds
//! the same problems in both. `serde_json` checks a raw value's syntax but
//! neither its nesting nor its numbers, so the cases below damage attribute
//! values in particular.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use archicraft_ifcx::container::AttributeValue;
use archicraft_ifcx::validate;
use serde_json::value::RawValue;
use serde_json::{Value, json};

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn golden() -> Vec<u8> {
    std::fs::read(manifest_dir().join("tests/golden/walls-and-slab.ifcx")).unwrap()
}

/// Every committed IFCX file: golden files, corrupt fixtures, fuzz
/// regressions and the format files.
fn committed_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    for dir in ["golden", "corrupt", "fuzz-regressions", "formats"] {
        for entry in std::fs::read_dir(manifest_dir().join("tests").join(dir)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "ifcx") {
                files.push(path);
            }
        }
    }
    files.sort();
    assert!(files.len() > 30, "{files:?}");
    files
}

/// Both parsers accept `bytes`, or both reject them with the same problem.
#[track_caller]
fn assert_same(bytes: &[u8], what: &str) {
    let tree = validate::parse(bytes).map(drop);
    let raw = validate::parse_raw(bytes).map(drop);
    assert_eq!(raw, tree, "{what}");
}

/// A container with two data entries; `value` is the attribute value of
/// the first, between `before` and `after` in the file.
fn document(value: &str) -> Vec<u8> {
    format!(
        r#"{{"header":{{"id":"parse","ifcxVersion":"ifcx_alpha","dataVersion":"1","author":"Test","timestamp":"2026-10-22T00:00:00Z"}},"imports":[],"schemas":{{}},"data":[{{"path":"a","attributes":{{"test::value":{value}}}}},{{"path":"b","children":{{"c":"a"}}}}]}}"#
    )
    .into_bytes()
}

/// Arrays nested `levels` deep inside the attribute value of [`document`],
/// which nests it four levels deep in the container.
fn nested(levels: usize) -> String {
    format!("{}{}", "[".repeat(levels), "]".repeat(levels))
}

/// NFR-SEC-003, NFR-REL-007: attribute values that `serde_json` skips as raw
/// text but rejects as a tree (numbers out of range, nesting past the limit,
/// invalid surrogates), and plain syntax errors inside them, are rejected
/// with the message of the tree parser.
#[test]
fn damaged_attribute_values_get_the_message_of_the_tree_parser() {
    let values = [
        "1e400",
        "-1e400",
        "[1, 2e999]",
        "{\"x\": [0.5, 1e309]}",
        r#""\ud800""#,
        r#""\udc00A""#,
        r#""\x""#,
        "\"a\u{1}b\"",
        "01",
        "1.",
        "-",
        "[1,]",
        "{\"a\" 1}",
        "{\"a\": 1,}",
        "tru",
        "nul",
        "[",
        "\"open",
        // The deepest a value may nest here: 123 levels plus the four of
        // the container are the 127 of the limit.
        &nested(123),
        &nested(124),
        &nested(200),
        &format!("[{}, 1e400]", nested(124)),
        &format!("[1e400, {}]", nested(124)),
        // The keys by which serde_json reads a tree differently.
        r#"{"$serde_json::private::RawValue": 5}"#,
        r#"{"$serde_json::private::RawValue": "5"}"#,
        r#"{"$serde_json::private::RawValue": "["}"#,
        r#"{"$serde_json::private::RawValue": "5", "b": 1}"#,
        r#"{"a": 1, "$serde_json::private::RawValue": 5}"#,
        r#"{"$serde_json::private::Number": "5"}"#,
        r#"{"a": 1, "a": [2]}"#,
    ];
    for value in values {
        assert_same(&document(value), value);
    }
    let mut invalid_utf8 = document("\"ab\"");
    let quote = invalid_utf8.windows(4).position(|w| w == b"\"ab\"").unwrap();
    invalid_utf8[quote + 1] = 0xff;
    assert_same(&invalid_utf8, "invalid UTF-8 in a string");
    validate::parse_raw(&document(&nested(123))).unwrap();
    assert!(validate::parse_raw(&document("1e400")).is_err());
}

/// NFR-SEC-003, NFR-REL-007: when a document has several problems, both
/// parsers report the first in the file, whether it is in the container's
/// structure or in a raw attribute value.
#[test]
fn the_first_problem_in_the_file_is_reported() {
    let header =
        json!({"id": "parse", "ifcxVersion": "ifcx_alpha", "dataVersion": "1", "author": "Test", "timestamp": "t"});
    let node = |path: Value, value: &str| format!(r#"{{"path":{path},"attributes":{{"test::value":{value}}}}}"#);
    let file = |nodes: &[String]| {
        format!(
            r#"{{"header":{header},"imports":[],"schemas":{{}},"data":[{}]}}"#,
            nodes.join(",")
        )
        .into_bytes()
    };
    let structure = node(json!(5), "0");
    let cases = [
        (
            "structure, then nesting",
            file(&[structure.clone(), node(json!("b"), &nested(124))]),
        ),
        (
            "nesting, then structure",
            file(&[node(json!("b"), &nested(124)), structure.clone()]),
        ),
        (
            "structure, then a number",
            file(&[structure.clone(), node(json!("b"), "1e400")]),
        ),
        (
            "a number, then structure",
            file(&[node(json!("b"), "1e400"), structure.clone()]),
        ),
        (
            "a surrogate, then structure",
            file(&[node(json!("b"), r#""\ud800""#), structure]),
        ),
    ];
    for (what, bytes) in cases {
        assert_same(&bytes, what);
    }
    let mut trailing = document("0");
    trailing.extend_from_slice(b" x");
    assert_same(&trailing, "trailing characters");
    let imports: Vec<Value> = (0..33)
        .map(|i| json!({"uri": format!("https://example.org/{i}")}))
        .collect();
    let too_many = format!(
        r#"{{"header":{header},"imports":{},"schemas":{{}},"data":[{}]}}"#,
        Value::Array(imports),
        node(json!("a"), &nested(124))
    );
    assert_same(too_many.as_bytes(), "33 imports, then nesting");
    let duplicate = format!(r#"{{"header":{header},"header":{header},"imports":[],"schemas":{{}},"data":[]}}"#);
    assert_same(duplicate.as_bytes(), "a duplicate field");
}

/// NFR-REL-007: a file cut off anywhere is rejected by both parsers with the
/// same message, most cuts inside a raw attribute value.
#[test]
fn every_cut_of_a_golden_file_gets_the_same_message() {
    let bytes = golden();
    for end in (0..bytes.len()).step_by(13) {
        assert_same(&bytes[..end], &format!("cut at {end}"));
    }
}

/// NFR-REL-007: one damaged byte anywhere in a golden file gets the same
/// outcome from both parsers.
#[test]
fn a_damaged_byte_anywhere_gets_the_same_outcome() {
    let bytes = golden();
    for position in (0..bytes.len()).step_by(41) {
        for damage in [b'"', b'\\', b'[', b'{', b'}', b',', b'e', b'9', 0x01, 0xff] {
            let mut damaged = bytes.clone();
            damaged[position] = damage;
            assert_same(&damaged, &format!("byte {damage:#04x} at {position}"));
        }
    }
}

/// IFX-005, IFX-083: every committed file gets the same outcome from both
/// parsers, and validation reports the same problems for a raw file as for
/// its tree, also for attribute values that do not match their schema.
#[test]
fn committed_files_parse_and_validate_the_same() {
    let mut files: Vec<(String, Vec<u8>)> = committed_files()
        .into_iter()
        .map(|path| (path.display().to_string(), std::fs::read(path).unwrap()))
        .collect();
    let mut mismatched: Value = serde_json::from_slice(&golden()).unwrap();
    let mesh = mismatched["data"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|n| n["attributes"].get("usd::usdgeom::mesh").is_some())
        .unwrap();
    mesh["attributes"]["usd::usdgeom::mesh"]["points"][1] = json!("x");
    mesh["attributes"]["test::undeclared"] = json!(1);
    files.push(("mismatched values".into(), serde_json::to_vec(&mismatched).unwrap()));
    let mut validated = 0;
    for (name, bytes) in files {
        assert_same(&bytes, &name);
        if let Ok(tree) = validate::parse(&bytes) {
            let raw = validate::parse_raw(&bytes).unwrap();
            assert_eq!(validate::validate(&raw), validate::validate(&tree), "{name}");
            assert_eq!(raw.to_values().unwrap(), tree, "{name}");
            validated += 1;
        }
    }
    assert!(validated > 20, "{validated}");
}

/// NFR-REL-007: opening a malformed file reports the message of the tree
/// parser.
#[test]
fn opening_a_malformed_file_reports_the_parse_message() {
    for value in ["1e400", &nested(124), r#""\ud800""#, "[1,]"] {
        let bytes = document(value);
        assert_eq!(
            archicraft_ifcx::load(&bytes).unwrap_err(),
            validate::parse(&bytes).unwrap_err(),
            "{value}"
        );
    }
}

/// NFR-SEC-003: an attribute value read from raw text, whole or one field of
/// it (as the format version of a project is read), is the value read from
/// its tree, for objects with repeated fields and for the keys by which
/// serde_json reads a tree differently too.
#[test]
fn raw_attribute_values_read_as_their_trees() {
    let texts = [
        r#"{"formatVersion": "1", "materials": [{"id": 1}, [2]]}"#,
        r#" { "formatVersion" : 1 } "#,
        r#"{"formatVersion": "1", "formatVersion": "2"}"#,
        r#"{"a": 1}"#,
        "{}",
        "[1]",
        "5",
        r#""formatVersion""#,
        "null",
        r#"{"$serde_json::private::RawValue": "{\"formatVersion\": \"3\"}"}"#,
        r#"{"$serde_json::private::RawValue": "5"}"#,
        r#"{"a": 1, "$serde_json::private::RawValue": "5"}"#,
    ];
    for text in texts {
        let tree: Value = serde_json::from_str(text).unwrap();
        let raw: &RawValue = serde_json::from_str(text).unwrap();
        assert_eq!(raw.value().unwrap(), tree.value().unwrap(), "{text}");
        for key in ["formatVersion", "a", "$serde_json::private::RawValue"] {
            assert_eq!(raw.field(key).unwrap(), tree.field(key).unwrap(), "{text}: {key}");
        }
    }
}
