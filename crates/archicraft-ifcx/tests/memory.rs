//! Memory of saving and opening (NFR-SEC-003, archicraft-4rrc): the writer
//! holds the output and one node at a time, the reader the file's bytes and
//! one attribute at a time as a JSON tree, never a tree of the whole file, so
//! a valid project that saves to tens of megabytes does not need gigabytes.
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

/// The largest peak heap of a load, beyond the file's bytes that the caller
/// holds, as a multiple of the file's size. The container and the flattened
/// nodes take a few percent of the file. Most of the peak is the JSON tree of
/// the one attribute being checked, the largest slab mesh: about the size of
/// this 4 MB file, and no larger in the 52 MB file of 400 slabs. Trees of the
/// whole file took more than fifty times the file.
const LOAD_FACTOR: usize = 2;

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

/// The compact file of the archicraft-4rrc input, saved with its meshes.
fn saved_input() -> Vec<u8> {
    let input = memory_probe::many_slabs(SLABS);
    let outcome = archicraft_ifcx::load(&input).expect("the input loads");
    let geometry = ModelGeometry::evaluate(&outcome.project);
    let (bytes, warnings) = archicraft_ifcx::save(&outcome.project, &geometry, &options(), false);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(bytes.len() > 3_000_000, "the input saves to only {} bytes", bytes.len());
    bytes
}

/// NFR-SEC-003: opening the saved archicraft-4rrc input needs a peak heap of
/// at most a small multiple of the file, beyond the bytes read from disk:
/// the attribute values stay raw text until they are read, one at a time.
#[test]
fn loading_needs_a_small_multiple_of_the_file_size() {
    let _serial = MEASURING.lock().unwrap_or_else(|e| e.into_inner());
    let bytes = saved_input();
    let before = memory_probe::start();
    let outcome = archicraft_ifcx::load(&bytes).expect("the saved input loads");
    let peak = memory_probe::peak() - before;
    assert_eq!(outcome.project.elements.len(), 5 + SLABS);
    assert!(outcome.problems.is_empty(), "{:?}", outcome.problems);
    assert!(
        peak <= LOAD_FACTOR * bytes.len(),
        "loading {} bytes needed a peak of {peak} bytes, {:.2} times the file",
        bytes.len(),
        peak as f64 / bytes.len() as f64
    );
}

/// NFR-SEC-003: rejecting a damaged file needs no more memory than opening
/// it, wherever the damage is: the message is found without holding the
/// file's attributes as trees.
#[test]
fn rejecting_a_damaged_file_needs_a_small_multiple_of_the_file_size() {
    let _serial = MEASURING.lock().unwrap_or_else(|e| e.into_inner());
    let bytes = saved_input();
    let last_number = bytes.iter().rposition(u8::is_ascii_digit).expect("a number");
    let mut out_of_range = bytes.clone();
    out_of_range.splice(last_number..=last_number, *b"1e400");
    let mut trailing = bytes.clone();
    trailing.push(b'x');
    let damaged = [(out_of_range, "number out of range"), (trailing, "trailing characters")];
    for (damaged, message) in damaged {
        let before = memory_probe::start();
        let problem = archicraft_ifcx::load(&damaged).expect_err("the damaged file is rejected");
        let peak = memory_probe::peak() - before;
        assert!(problem.message.to_string().contains(message), "{problem}");
        assert!(
            peak <= LOAD_FACTOR * damaged.len(),
            "rejecting {} bytes ({message}) needed a peak of {peak} bytes, {:.2} times the file",
            damaged.len(),
            peak as f64 / damaged.len() as f64
        );
    }
}
