//! The mock successor adapter behind the cargo feature `successor-stub`
//! (IFX-091, NFR-MNT-004): a second IFCX version plugs into the adapter table
//! without any change to the domain model or geometry crates. Run with
//! `cargo test -p archicraft-ifcx --features successor-stub --test successor`;
//! without the feature only the check that the stub stays out of the default
//! build runs. The runbook for a real successor is docs/ifcx-evolution.md.

#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};

use serde_json::Value;

/// The `ifcxVersion` of the mock successor.
const MOCK: &str = "ifcx_successor_mock";

fn golden_files() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "ifcx"))
        .collect();
    files.sort();
    assert!(!files.is_empty());
    files
}

fn with_version(bytes: &[u8], version: &str) -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(bytes).unwrap();
    value["header"]["ifcxVersion"] = version.into();
    serde_json::to_vec(&value).unwrap()
}

/// IFX-090: the default build reads only the versions an ArchiCraft release
/// has written; the mock successor is not one of them.
#[cfg(not(feature = "successor-stub"))]
#[test]
fn the_default_build_has_no_successor_adapter() {
    assert!(archicraft_ifcx::ADAPTERS.iter().all(|a| a.version != MOCK));
    let golden = std::fs::read(&golden_files()[0]).unwrap();
    let error = archicraft_ifcx::load(&with_version(&golden, MOCK)).unwrap_err();
    assert!(
        error.message.contains(&format!("“{MOCK}” is not supported")),
        "{error:?}"
    );
}

/// IFX-091, NFR-MNT-004: a golden file relabelled as the successor version
/// is read by the successor adapter (its `source_version`) into the same
/// project, with the same problems, as the original read by `ifcx_alpha`.
#[cfg(feature = "successor-stub")]
#[test]
fn a_successor_file_routes_to_the_stub_and_loads_to_the_same_project() {
    for path in golden_files() {
        let original = std::fs::read(&path).unwrap();
        let alpha = archicraft_ifcx::load(&original).unwrap();
        assert_eq!(alpha.source_version, "ifcx_alpha");

        let successor = archicraft_ifcx::load(&with_version(&original, MOCK)).unwrap();
        assert_eq!(successor.source_version, MOCK, "{}", path.display());
        assert_eq!(successor.project, alpha.project, "{}", path.display());
        assert_eq!(successor.problems, alpha.problems, "{}", path.display());
        assert!(successor.problems.is_empty(), "{:#?}", successor.problems);
        assert!(!successor.newer_format);
    }
}

/// IFX-090: with the successor registered, an unknown version still fails
/// and the message names every supported version, both of them.
#[cfg(feature = "successor-stub")]
#[test]
fn an_unknown_version_names_alpha_and_the_successor() {
    let versions: Vec<&str> = archicraft_ifcx::ADAPTERS.iter().map(|a| a.version).collect();
    assert_eq!(versions, ["ifcx_alpha", MOCK]);
    let golden = std::fs::read(&golden_files()[0]).unwrap();
    let error = archicraft_ifcx::load(&with_version(&golden, "ifcx_omega")).unwrap_err();
    assert!(error.message.contains("“ifcx_omega” is not supported"), "{error:?}");
    for version in versions {
        assert!(error.message.contains(version), "{error:?}");
    }
}

/// IFX-090: a project read from the successor version is saved as
/// `ifcx_alpha`, the one version this release writes, byte for byte as the
/// project read from the original golden file.
#[cfg(feature = "successor-stub")]
#[test]
fn a_successor_file_is_saved_as_ifcx_alpha() {
    assert_eq!(archicraft_ifcx::WRITTEN_VERSION, "ifcx_alpha");
    for path in golden_files() {
        let original = std::fs::read(&path).unwrap();
        let successor = archicraft_ifcx::load(&with_version(&original, MOCK)).unwrap();
        let alpha = archicraft_ifcx::load(&original).unwrap();
        let saved = common::save(&successor.project);
        let header = archicraft_ifcx::validate::parse(&saved).unwrap().header;
        assert_eq!(header.ifcx_version, "ifcx_alpha", "{}", path.display());
        assert_eq!(saved, common::save(&alpha.project), "{}", path.display());
    }
}
