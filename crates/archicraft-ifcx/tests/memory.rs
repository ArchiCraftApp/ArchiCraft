//! Memory of saving (NFR-SEC-003, archicraft-4rrc): the writer holds the
//! output and one node at a time, never a JSON tree of the whole file, so a
//! valid project that saves to tens of megabytes does not need gigabytes.
//! docs/testing.md (Fuzzing) records the measurements of the full-size input
//! (`examples/memory.rs`).

mod memory_probe;

use std::sync::Mutex;

use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::WriteOptions;
use memory_probe::CountingAllocator;

#[global_allocator]
static HEAP: CountingAllocator = CountingAllocator;

/// The allocator counts every thread: tests in this file measure one at a
/// time.
static MEASURING: Mutex<()> = Mutex::new(());

/// Slabs in the input: enough for a file of several megabytes, so that the
/// fixed costs of a save do not hide how it grows.
const SLABS: usize = 30;

/// The largest peak heap of a save, as a multiple of the bytes it writes.
/// The output buffer alone takes up to three times its length while it grows
/// (a doubling reallocation holds the old and the new block); a JSON tree of
/// the whole file took more than thirty times.
const SAVE_FACTOR: usize = 4;

fn options() -> WriteOptions {
    WriteOptions {
        author: "Test".into(),
        timestamp: "2026-10-21T12:00:00Z".into(),
        data_version: 1,
    }
}

/// NFR-SEC-003: saving the archicraft-4rrc input, compact and indented, needs
/// a peak heap of at most a small multiple of the file it writes, beyond the
/// project and its geometry that the application holds anyway.
#[test]
fn saving_needs_a_small_multiple_of_the_file_size() {
    let _serial = MEASURING.lock().unwrap_or_else(|e| e.into_inner());
    let input = memory_probe::many_slabs(SLABS);
    let outcome = archicraft_ifcx::load(&input).expect("the input loads");
    assert_eq!(outcome.project.elements.len(), 5 + SLABS);
    let geometry = ModelGeometry::evaluate(&outcome.project);
    assert!(geometry.problems.is_empty(), "{:?}", geometry.problems);
    for pretty in [false, true] {
        let before = memory_probe::start();
        let (bytes, warnings) = archicraft_ifcx::save(&outcome.project, &geometry, &options(), pretty);
        let peak = memory_probe::peak() - before;
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(bytes.len() > 3_000_000, "the input saves to only {} bytes", bytes.len());
        assert!(
            peak <= SAVE_FACTOR * bytes.len(),
            "saving {} bytes (indented: {pretty}) needed a peak of {peak} bytes, {:.1} times the file",
            bytes.len(),
            peak as f64 / bytes.len() as f64
        );
    }
}
