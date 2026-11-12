//! The lock file of the cargo-fuzz workspace stays current (NFR-REL-007,
//! NFR-SEC-003). `fuzz/` is its own Cargo workspace, so the stable build
//! never resolves it: a dependency added to or removed from the model,
//! geometry or IFCX crate leaves `fuzz/Cargo.lock` stale, and the nightly
//! Fuzz job (`.github/workflows/fuzz.yml`), which fails when
//! `cargo fuzz build` changes that file, stops before any target runs. This
//! test resolves the fuzz workspace with `--locked` in the normal test job,
//! so the stale lock fails the pull request that causes it instead of every
//! night after it merged (docs/testing.md, Fuzzing).

use std::path::Path;
use std::process::Command;

/// `cargo metadata --locked` resolves the dependency graph from the lock
/// file and refuses to change it, the same resolution `cargo fuzz build`
/// does without refusing. Without `--no-deps`, which would skip the
/// resolution.
#[test]
fn fuzz_lock_file_matches_the_crates_it_builds() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fuzz/Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--locked", "--format-version", "1", "--manifest-path"])
        .arg(&manifest)
        .output()
        .expect("cargo runs");
    assert!(
        output.status.success(),
        "fuzz/Cargo.lock is out of date with the crates fuzz/Cargo.toml builds; \
         update it with `cargo update --workspace --manifest-path fuzz/Cargo.toml` \
         and commit it.\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
