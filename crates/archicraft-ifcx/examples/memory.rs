//! Measures the memory of saving and opening a large project (NFR-SEC-003,
//! archicraft-4rrc), for the record in docs/testing.md (Fuzzing).
//!
//! The input is that of the fuzz finding: valid slabs of 999 points (the
//! point limit was 1,000 then), whose meshes make the saved file about 15
//! times larger than the input.
//! Each command prints every phase's peak heap, counted by a global
//! allocator, next to the heap held before the phase. Run each command in a
//! release build under `/usr/bin/time -l` (macOS) or `/usr/bin/time -v`
//! (Linux) for the process's peak resident memory as well:
//!
//! ```sh
//! cargo build --release -p archicraft-ifcx --example memory
//! target/release/examples/memory repro repro.ifcx 400   # the 3.3 MB input
//! target/release/examples/memory save repro.ifcx saved.ifcx   # open, evaluate, save 52 MB
//! target/release/examples/memory load saved.ifcx   # open and evaluate the 52 MB file
//! ```
//!
//! `save … --indented` saves the indented form (IFX-008) instead.

#[path = "../tests/memory_probe/mod.rs"]
mod memory_probe;

use std::path::Path;
use std::time::Instant;

use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::{ReadOutcome, WriteOptions};
use memory_probe::CountingAllocator;

#[global_allocator]
static HEAP: CountingAllocator = CountingAllocator;

const USAGE: &str =
    "usage: memory repro <out> [slabs, default 400] | memory save <in> <out> [--indented] | memory load <in>";

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Megabytes as docs/testing.md counts them: 10⁶ bytes.
const MB: f64 = 1e6;

/// Runs `step` and prints its peak heap, the heap held before it and its
/// duration.
fn phase<T>(name: &str, step: impl FnOnce() -> T) -> T {
    let before = memory_probe::start();
    let started = Instant::now();
    let result = step();
    let seconds = started.elapsed().as_secs_f64();
    let peak = memory_probe::peak();
    println!(
        "{name:<34} peak heap {:>8.1} MB (held before {:>7.1} MB), {seconds:.2} s",
        peak as f64 / MB,
        before as f64 / MB,
    );
    result
}

/// Opens a project file as the application does: load, then evaluate.
fn open(path: &Path) -> Result<(ReadOutcome, ModelGeometry)> {
    let size = std::fs::metadata(path)?.len() as f64 / MB;
    let outcome = phase(&format!("load ({size:.1} MB)"), || archicraft_ifcx::load_path(path))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    println!(
        "{:<34} {} elements, {} problems",
        "",
        outcome.project.elements.len(),
        outcome.problems.len()
    );
    let geometry = phase("evaluate", || ModelGeometry::evaluate(&outcome.project));
    Ok((outcome, geometry))
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["repro", out, rest @ ..] => {
            let slabs = match rest {
                [] => 400,
                [n] => n.parse()?,
                _ => return Err(USAGE.into()),
            };
            let bytes = memory_probe::many_slabs(slabs);
            std::fs::write(out, &bytes)?;
            println!("{out}: {slabs} slabs, {:.1} MB", bytes.len() as f64 / MB);
        }
        ["save", input, out, rest @ ..] => {
            let pretty = match rest {
                [] => false,
                ["--indented"] => true,
                _ => return Err(USAGE.into()),
            };
            let (outcome, geometry) = open(Path::new(input))?;
            let options = WriteOptions {
                author: "ArchiCraft memory example".into(),
                timestamp: "2026-10-21T00:00:00Z".into(),
                data_version: 1,
            };
            let (bytes, _warnings) = phase(if pretty { "save (indented)" } else { "save" }, || {
                archicraft_ifcx::save(&outcome.project, &geometry, &options, pretty)
            });
            std::fs::write(out, &bytes)?;
            println!("{:<34} {:.1} MB written to {out}", "", bytes.len() as f64 / MB);
        }
        ["load", input] => {
            open(Path::new(input))?;
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}
