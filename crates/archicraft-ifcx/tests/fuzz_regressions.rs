//! Fuzz regressions (NFR-REL-007, NFR-SEC-003): every file in
//! `tests/fuzz-regressions/` goes through the open path of the `ifcx_load`
//! fuzz target (`fuzz/fuzz_targets/ifcx_load.rs`) on stable, in the normal
//! test job: load, and when that succeeds evaluate the geometry, save and
//! load again. None of it may panic, and a rejected file carries a message.
//!
//! A crash found by `cargo fuzz` is minimised with `cargo fuzz tmin` and
//! committed here under a descriptive name (docs/testing.md, Fuzzing).

#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};

use archicraft_geometry::ModelGeometry;

fn regression_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fuzz-regressions")
}

fn regression_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(regression_dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .collect();
    files.sort();
    files
}

/// The open path of the `ifcx_load` fuzz target.
fn open(bytes: &[u8]) {
    match archicraft_ifcx::load(bytes) {
        Ok(outcome) => {
            let geometry = ModelGeometry::evaluate(&outcome.project);
            let (saved, _warnings) = archicraft_ifcx::save(&outcome.project, &geometry, &common::options(), false);
            let _ = archicraft_ifcx::load(&saved);
        }
        Err(problem) => assert!(!problem.message.is_empty(), "rejected without a message"),
    }
}

/// NFR-REL-007: no file of the regression corpus crashes the open path.
#[test]
fn every_fuzz_regression_opens_without_panicking() {
    let files = regression_files();
    assert!(!files.is_empty(), "no files in {}", regression_dir().display());
    for path in files {
        let bytes = std::fs::read(&path).unwrap();
        let outcome = std::panic::catch_unwind(|| open(&bytes));
        assert!(outcome.is_ok(), "{} panicked", path.display());
    }
}

/// NFR-REL-007: the corpus starts with the inputs of
/// `malformed_input_is_rejected_without_panicking` (tests/roundtrip.rs), and
/// each of them is still rejected.
#[test]
fn the_regression_corpus_holds_the_malformed_inputs() {
    let deep = format!("{}{}", "[".repeat(200), "]".repeat(200));
    let seeds: [(&str, &[u8]); 8] = [
        ("empty", b""),
        ("not-json", b"not json"),
        ("empty-object", b"{}"),
        ("empty-array", b"[]"),
        ("header-not-object", b"{\"header\": 1}"),
        ("nesting-200", deep.as_bytes()),
        (
            "alpha-without-project",
            br#"{"header":{"id":"x","ifcxVersion":"ifcx_alpha","dataVersion":"1","author":"a","timestamp":"t"},"imports":[],"schemas":{},"data":[]}"#,
        ),
        (
            "unknown-version",
            br#"{"header":{"id":"x","ifcxVersion":"ifcx_beta","dataVersion":"1","author":"a","timestamp":"t"},"imports":[],"schemas":{},"data":[]}"#,
        ),
    ];
    for (name, input) in seeds {
        let path = regression_dir().join(format!("{name}.ifcx"));
        assert_eq!(std::fs::read(&path).unwrap(), input, "{}", path.display());
        assert!(archicraft_ifcx::load(input).is_err(), "{name} was accepted");
    }
}
