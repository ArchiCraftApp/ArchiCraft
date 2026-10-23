//! The parser limits of SRS Appendix D.9 (NFR-SEC-003): file size, JSON
//! nesting, IFCX composition depth and imports. Every imported file is
//! untrusted input, so each limit is tested at its exact boundary.

#![allow(clippy::unwrap_used)]

use std::io::{self, Read};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use archicraft_ifcx::validate::{self, MAX_COMPOSITION_DEPTH, MAX_FILE_SIZE, MAX_IMPORTS, read_bounded};
use archicraft_ifcx::{Problem, ReadOutcome};
use serde_json::{Value, json};

fn golden(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    std::fs::read(path).unwrap()
}

/// A minimal IFCX document with the given imports and one attribute value,
/// not declared by any schema (parsing does not check declarations).
fn document(imports: &[String], value: &str) -> String {
    let imports: Vec<Value> = imports.iter().map(|uri| json!({ "uri": uri })).collect();
    format!(
        r#"{{"header":{{"id":"limits","ifcxVersion":"ifcx_alpha","dataVersion":"1","author":"Test","timestamp":"2026-10-19T00:00:00Z"}},"imports":{},"schemas":{{}},"data":[{{"path":"node","attributes":{{"test::deep":{value}}}}}]}}"#,
        Value::Array(imports)
    )
}

/// The deepest nesting of arrays and objects in a JSON text (no strings in
/// the texts tested here contain brackets).
fn nesting(text: &str) -> usize {
    let (mut level, mut deepest) = (0usize, 0usize);
    for c in text.chars() {
        match c {
            '[' | '{' => {
                level += 1;
                deepest = deepest.max(level);
            }
            ']' | '}' => level -= 1,
            _ => {}
        }
    }
    deepest
}

/// A document nested `levels` deep in total, the deepest part inside an
/// attribute value.
fn nested_document(levels: usize) -> String {
    let wrapper = nesting(&document(&[], "0"));
    let inner = levels - wrapper;
    let text = document(&[], &format!("{}{}", "[".repeat(inner), "]".repeat(inner)));
    assert_eq!(nesting(&text), levels);
    text
}

fn unknown_imports(count: usize) -> Vec<String> {
    (0..count)
        .map(|i| format!("https://example.org/unknown-{i}.ifcx"))
        .collect()
}

/// The D.9 values themselves (NFR-SEC-003).
#[test]
fn limits_are_those_of_appendix_d9() {
    assert_eq!(MAX_FILE_SIZE, 2 * 1024 * 1024 * 1024);
    assert_eq!(MAX_IMPORTS, 32);
    assert_eq!(MAX_COMPOSITION_DEPTH, 64);
}

/// 32 imports pass the limit; imports that are not available offline are
/// reported as unresolved, not as the limit error (D.9, NFR-SEC-003).
#[test]
fn thirty_two_imports_pass_the_limit() {
    let text = document(&unknown_imports(MAX_IMPORTS), "0");
    let file = validate::parse(text.as_bytes()).unwrap();
    let problems = validate::validate(&file);
    let unresolved = problems
        .iter()
        .filter(|p| p.message.to_string().contains("ifcx-import-offline"))
        .count();
    assert_eq!(unresolved, MAX_IMPORTS, "{problems:#?}");
    assert!(
        problems
            .iter()
            .all(|p| !p.message.to_string().contains("ifcx-file-too-many-imports")),
        "{problems:#?}"
    );
}

/// 33 imports are rejected (D.9, NFR-SEC-003).
#[test]
fn thirty_three_imports_are_rejected() {
    let text = document(&unknown_imports(MAX_IMPORTS + 1), "0");
    let problem = validate::parse(text.as_bytes()).unwrap_err();
    assert!(
        problem
            .message
            .to_string()
            .contains("ifcx-file-too-many-imports(max=32)"),
        "{problem}"
    );
}

/// The JSON nesting limit is serde_json's recursion limit: a document nested
/// 127 levels parses, 128 do not (D.9: fewer than 128 levels). The limit
/// counts every level of the document, so an attribute value has the levels
/// left after the container's own nesting. Asserted exactly because a
/// serde_json upgrade could move it.
#[test]
fn json_nested_127_levels_parses() {
    let text = nested_document(127);
    validate::parse(text.as_bytes()).unwrap();
}

/// 128 or more levels are rejected with a message instead of overflowing
/// the stack (D.9, NFR-SEC-003, NFR-REL-007).
#[test]
fn json_nested_128_levels_or_more_is_rejected() {
    for levels in [128, 129, 1_000, 100_000] {
        let text = nested_document(levels);
        let problem = validate::parse(text.as_bytes()).unwrap_err();
        assert!(
            problem
                .message
                .to_string()
                .contains("ifcx-file-nested-too-deep(max=127)"),
            "{levels}: {problem}"
        );
    }
}

/// The open path's parser, which keeps attribute values as raw text,
/// enforces the same nesting and import limits with the same messages
/// (D.9, NFR-SEC-003): `serde_json` does not check the nesting of a raw
/// value by itself, and the deepest part of these documents is inside an
/// attribute value.
#[test]
fn the_raw_parser_enforces_the_same_limits() {
    validate::parse_raw(nested_document(127).as_bytes()).unwrap();
    for levels in [128, 129, 1_000, 100_000] {
        let text = nested_document(levels);
        let problem = validate::parse_raw(text.as_bytes()).unwrap_err();
        assert_eq!(problem, validate::parse(text.as_bytes()).unwrap_err(), "{levels}");
        assert!(
            problem
                .message
                .to_string()
                .contains("ifcx-file-nested-too-deep(max=127)"),
            "{levels}: {problem}"
        );
    }
    validate::parse_raw(document(&unknown_imports(MAX_IMPORTS), "0").as_bytes()).unwrap();
    let text = document(&unknown_imports(MAX_IMPORTS + 1), "0");
    let problem = validate::parse_raw(text.as_bytes()).unwrap_err();
    assert!(
        problem
            .message
            .to_string()
            .contains("ifcx-file-too-many-imports(max=32)"),
        "{problem}"
    );
}

/// The walls-and-slab reference model with extra data entries appended.
fn model_with(entries: impl IntoIterator<Item = Value>) -> Vec<u8> {
    let mut file: Value = serde_json::from_slice(&golden("walls-and-slab.ifcx")).unwrap();
    file["data"].as_array_mut().unwrap().extend(entries);
    serde_json::to_vec(&file).unwrap()
}

/// A chain `chain-0 → chain-1 → … → chain-<edges>` through `children` or
/// `inherits`, separate from the project tree, in file order or reversed.
fn chain(relation: &str, edges: usize, reversed: bool) -> Vec<Value> {
    let mut entries: Vec<Value> = (0..edges)
        .map(|i| json!({ "path": format!("chain-{i}"), relation: { "next": format!("chain-{}", i + 1) } }))
        .collect();
    entries.push(json!({ "path": format!("chain-{edges}") }));
    if reversed {
        entries.reverse();
    }
    entries
}

fn too_deep(outcome: &ReadOutcome) -> Vec<&Problem> {
    outcome
        .problems
        .iter()
        .filter(|p| p.message.to_string().contains("ifcx-composition-too-deep"))
        .collect()
}

/// A `children` chain of depth 64 loads; depth 65 is reported with the node
/// path (D.9, NFR-SEC-003).
#[test]
fn children_chain_is_limited_to_depth_64() {
    let outcome = archicraft_ifcx::load(&model_with(chain("children", MAX_COMPOSITION_DEPTH, false))).unwrap();
    assert!(too_deep(&outcome).is_empty(), "{:#?}", outcome.problems);

    let outcome = archicraft_ifcx::load(&model_with(chain("children", MAX_COMPOSITION_DEPTH + 1, false))).unwrap();
    let problems = too_deep(&outcome);
    assert_eq!(problems.len(), 1, "{:#?}", outcome.problems);
    assert!(
        problems[0].path.as_deref().is_some_and(|p| p.starts_with("chain-")),
        "{:?}",
        problems[0]
    );
}

/// An `inherits` chain behaves the same (D.9, NFR-SEC-003).
#[test]
fn inherits_chain_is_limited_to_depth_64() {
    let outcome = archicraft_ifcx::load(&model_with(chain("inherits", MAX_COMPOSITION_DEPTH, false))).unwrap();
    assert!(too_deep(&outcome).is_empty(), "{:#?}", outcome.problems);

    let outcome = archicraft_ifcx::load(&model_with(chain("inherits", MAX_COMPOSITION_DEPTH + 1, false))).unwrap();
    let problems = too_deep(&outcome);
    assert_eq!(problems.len(), 1, "{:#?}", outcome.problems);
    assert!(
        problems[0].path.as_deref().is_some_and(|p| p.starts_with("chain-")),
        "{:?}",
        problems[0]
    );
}

/// A chain of 100,000 nodes, in either file order, is rejected in under a
/// second without overflowing the stack: the reader never walks the
/// composition recursively past the limit (D.9, NFR-SEC-003).
#[test]
fn chain_of_100000_nodes_completes_quickly() {
    for relation in ["children", "inherits"] {
        for reversed in [false, true] {
            let bytes = model_with(chain(relation, 100_000, reversed));
            let start = Instant::now();
            let outcome = archicraft_ifcx::load(&bytes).unwrap();
            let elapsed = start.elapsed();
            assert_eq!(too_deep(&outcome).len(), 1, "{relation}, reversed {reversed}");
            assert!(
                elapsed < Duration::from_secs(1),
                "{relation}, reversed {reversed}: {elapsed:?}"
            );
        }
    }
}

/// A reader that counts the bytes it hands out, from a source of `len`
/// bytes.
struct CountingReader {
    len: usize,
    read: usize,
}

impl CountingReader {
    fn new(len: usize) -> Self {
        Self { len, read: 0 }
    }
}

impl Read for CountingReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = buf.len().min(self.len - self.read);
        buf[..n].fill(b' ');
        self.read += n;
        Ok(n)
    }
}

const LIMIT: u64 = 1024;

/// A declared length above the limit is rejected before a single byte is
/// read (D.9 file size, NFR-SEC-003).
#[test]
fn declared_length_above_the_limit_is_rejected_unread() {
    let mut reader = CountingReader::new(2048);
    let problem = read_bounded(&mut reader, Some(LIMIT + 1), LIMIT).unwrap_err();
    assert_eq!(reader.read, 0);
    assert!(
        problem
            .message
            .to_string()
            .contains("ifcx-file-too-large(limit=ifcx-size-kb(size=1))"),
        "{problem}"
    );
}

/// A stream that grows past the limit while it is read (or whose length is
/// unknown) is rejected after reading at most one byte past the limit.
#[test]
fn stream_growing_past_the_limit_is_rejected() {
    for declared in [Some(10), None] {
        let mut reader = CountingReader::new(10 * LIMIT as usize);
        let problem = read_bounded(&mut reader, declared, LIMIT).unwrap_err();
        assert!(reader.read as u64 <= LIMIT + 1, "{declared:?}: read {}", reader.read);
        assert!(
            problem
                .message
                .to_string()
                .contains("ifcx-file-too-large(limit=ifcx-size-kb(size=1))"),
            "{problem}"
        );
    }
}

/// A stream of exactly the limit is accepted.
#[test]
fn stream_at_exactly_the_limit_is_accepted() {
    for declared in [Some(LIMIT), None] {
        let mut reader = CountingReader::new(LIMIT as usize);
        let bytes = read_bounded(&mut reader, declared, LIMIT).unwrap();
        assert_eq!(bytes.len() as u64, LIMIT);
    }
}

/// The size message names the D.9 limit in its largest whole unit, for the
/// application to render (NFR-L10N-001).
#[test]
fn size_message_names_the_limit() {
    let mut reader = CountingReader::new(0);
    let problem = read_bounded(&mut reader, Some(MAX_FILE_SIZE as u64 + 1), MAX_FILE_SIZE as u64).unwrap_err();
    assert_eq!(
        problem.message.to_string(),
        "ifcx-file-too-large(limit=ifcx-size-gb(size=2))"
    );
    assert_eq!(reader.read, 0);
}

/// `load_path` reads a project file from disk and reports a missing file
/// as a problem.
#[test]
fn load_path_reads_files_and_reports_missing_ones() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/walls-and-slab.ifcx");
    let outcome = archicraft_ifcx::load_path(&path).unwrap();
    assert!(!outcome.project.elements.is_empty());

    let problem = archicraft_ifcx::load_path(&path.with_file_name("missing.ifcx")).unwrap_err();
    assert!(
        problem.message.to_string().contains("ifcx-file-unreadable"),
        "{problem}"
    );

    let problem = archicraft_ifcx::load_path_with_limit(&path, LIMIT).unwrap_err();
    assert!(
        problem
            .message
            .to_string()
            .contains("ifcx-file-too-large(limit=ifcx-size-kb(size=1))"),
        "{problem}"
    );
}
