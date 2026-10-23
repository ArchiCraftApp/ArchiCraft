//! Fuzz regressions (NFR-REL-007, NFR-SEC-003): every file in
//! `tests/fuzz-regressions/` goes through the open path of the `ifcx_load`
//! fuzz target (`fuzz/fuzz_targets/ifcx_load.rs`) on stable, in the normal
//! test job: load, and when that succeeds evaluate the geometry, save and
//! load again. None of it may panic, a rejected file carries a message, and
//! no file takes longer than the fuzz runs' time-out (`-timeout=10` in
//! `scripts/fuzz-run.sh`): a file that stalls the reader is a denial of
//! service (NFR-SEC-003).
//!
//! A crash found by `cargo fuzz` is minimised with `cargo fuzz tmin` and
//! committed here under a descriptive name (docs/testing.md, Fuzzing).

#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use archicraft_geometry::ModelGeometry;

/// The time-out of the fuzz runs (`scripts/fuzz-run.sh`): an input that
/// takes longer is a finding.
const TIME_LIMIT: Duration = Duration::from_secs(10);

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
        Err(problem) => assert!(!problem.message.key().is_empty(), "rejected without a message"),
    }
}

/// NFR-REL-007, NFR-SEC-003: no file of the regression corpus crashes the
/// open path or stalls it beyond the fuzz runs' time-out.
#[test]
fn every_fuzz_regression_opens_without_panicking() {
    let files = regression_files();
    assert!(!files.is_empty(), "no files in {}", regression_dir().display());
    for path in files {
        let bytes = std::fs::read(&path).unwrap();
        let start = Instant::now();
        let outcome = std::panic::catch_unwind(|| open(&bytes));
        let elapsed = start.elapsed();
        assert!(outcome.is_ok(), "{} panicked", path.display());
        assert!(elapsed < TIME_LIMIT, "{} took {elapsed:?}", path.display());
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

/// NFR-SEC-003, SLB-003: a slab outline of 32,003 points stalled the open
/// path in the first fuzz campaign (docs/testing.md, Fuzzing), because the
/// self-intersection checks then grew with the square of the points. A slab
/// of more than `MAX_SLAB_POINTS` (10,000) points is skipped with a message
/// naming the limit, before those checks run, and the rest of the project
/// opens; here the outline has one tooth more than the comb at the limit.
#[test]
fn a_slab_with_too_many_points_is_skipped_with_a_message() {
    let bytes = std::fs::read(regression_dir().join("slab-outline-10003-points.ifcx")).unwrap();
    let outcome = archicraft_ifcx::load(&bytes).unwrap();
    assert_eq!(outcome.project.elements.len(), 4, "only the four walls are left");
    assert!(
        outcome.problems.iter().any(|p| p.message.to_string().contains("SL-001")
            && p.message.to_string().contains("model-error-slab-points(max=10000)")),
        "{:?}",
        outcome.problems
    );
}

/// NFR-SEC-003: a storey listed many times among the building's children was
/// read once per listing in the first fuzz campaign; 5,000 listings in a
/// 260 KB file made 5,000 storeys and saving them took more than 2 GB. A
/// storey is now read once and each further listing is reported.
#[test]
fn a_storey_listed_three_times_is_read_once() {
    let bytes = std::fs::read(regression_dir().join("storey-listed-three-times.ifcx")).unwrap();
    let outcome = archicraft_ifcx::load(&bytes).unwrap();
    assert_eq!(outcome.project.storeys.len(), 2);
    assert_eq!(outcome.project.elements.len(), 5);
    let repeats = outcome
        .problems
        .iter()
        .filter(|p| {
            p.message.to_string().contains(
                r#"ifcx-skipped(record=ifcx-record-storey(name="00 Ground floor"), reason=ifcx-storey-listed-twice)"#,
            )
        })
        .count();
    assert_eq!(repeats, 2, "{:?}", outcome.problems);
}

/// NFR-SEC-003: an element listed many times among a storey's children was
/// read and validated once per listing, found when reviewing the first fuzz
/// campaign: a slab of 999 points listed 20,000 times (1 MB) took longer than
/// the 10 s time-out. An element is now read once and each further listing
/// is reported; here the slab (10,003 points) is above the point limit, so
/// reading it again would report the limit again.
#[test]
fn an_element_listed_three_times_is_read_once() {
    let bytes = std::fs::read(regression_dir().join("slab-listed-three-times.ifcx")).unwrap();
    let outcome = archicraft_ifcx::load(&bytes).unwrap();
    let count = |text: &str| {
        outcome
            .problems
            .iter()
            .filter(|p| p.message.to_string().contains(text))
            .count()
    };
    assert_eq!(count("model-error-slab-points"), 1, "{:?}", outcome.problems);
    assert_eq!(count(r#"ifcx-duplicate(name="SL-001")"#), 2, "{:?}", outcome.problems);
}
