//! Frozen format fixtures: files written by earlier ArchiCraft format
//! versions, which every later release must keep reading (IFX-090, IFX-091,
//! IFX-062).
//!
//! The files under tests/formats/ are never regenerated: unlike the golden
//! files under tests/golden/, `ARCHICRAFT_BLESS` does not touch them, and a
//! hash test fails when their bytes change. `format-1.ifcx` is a copy of the
//! two-storey-shell golden as written at FORMAT_VERSION 1 in `ifcx_alpha`.

#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};

use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::{WRITTEN_VERSION, validate};

use common::options;

fn formats_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/formats")
}

fn format_1() -> Vec<u8> {
    std::fs::read(formats_dir().join("format-1.ifcx")).unwrap()
}

/// 64-bit FNV-1a, enough to notice any change of a fixture's bytes.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The frozen fixtures keep their bytes forever, also when the golden files
/// are blessed: a migration test is only worth something against the bytes
/// an earlier release really wrote.
#[test]
fn frozen_format_fixtures_never_change() {
    let frozen: [(&str, usize, u64); 1] = [("format-1.ifcx", 36_786, 0x4b1c_2083_91de_01b8)];
    let mut found: Vec<String> = std::fs::read_dir(formats_dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    let mut expected: Vec<String> = frozen.iter().map(|(name, ..)| (*name).to_owned()).collect();
    expected.sort();
    assert_eq!(found, expected, "every frozen format fixture is listed here");
    for (name, len, hash) in frozen {
        let bytes = std::fs::read(formats_dir().join(name)).unwrap();
        assert!(
            bytes.len() == len && fnv1a(&bytes) == hash,
            "frozen format fixture changed: {name} ({} bytes, hash {:#x}); restore it from git, never regenerate it",
            bytes.len(),
            fnv1a(&bytes)
        );
    }
}

/// IFX-091, IFX-062: a file written at format 1 loads with no problem at all
/// into the project it holds, and reports the IFCX version it was read as;
/// format 1 is the current format, so nothing was migrated.
#[test]
fn format_1_loads_without_problems_into_the_expected_project() {
    let loaded = archicraft_ifcx::load(&format_1()).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert!(!loaded.newer_format);
    assert_eq!(loaded.source_version, "ifcx_alpha");
    assert_eq!(loaded.migrated_from, None);

    let project = &loaded.project;
    assert_eq!(project.info.name, "Two-storey shell");
    let storeys: Vec<(&str, f64)> = project
        .storeys
        .as_slice()
        .iter()
        .map(|s| (s.name.as_str(), s.elevation))
        .collect();
    assert_eq!(storeys, [("00 Ground floor", 0.0), ("01 First floor", 3.0)]);
    for (storey, names) in project.storeys.as_slice().iter().zip([
        ["SL-001", "W-001", "W-002", "W-003", "W-004", "W-005"],
        ["SL-002", "W-006", "W-007", "W-008", "W-009", "W-010"],
    ]) {
        let mut found: Vec<&str> = project.elements_on(storey.id).map(|e| e.name.as_str()).collect();
        found.sort_unstable();
        assert_eq!(found, names, "elements on {}", storey.name);
    }
    assert!(project.integrity_problems().is_empty());
}

/// IFX-090: a file read from an earlier format is saved in the one version
/// this release writes, `ifcx_alpha`, and reads back to the same project.
#[test]
fn format_1_is_saved_in_the_written_version() {
    let loaded = archicraft_ifcx::load(&format_1()).unwrap();
    let geometry = ModelGeometry::evaluate(&loaded.project);
    let (bytes, warnings) = archicraft_ifcx::save(&loaded.project, &geometry, &options(), false);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(validate::parse(&bytes).unwrap().header.ifcx_version, WRITTEN_VERSION);
    assert_eq!(WRITTEN_VERSION, "ifcx_alpha");
    let reloaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(reloaded.problems.is_empty(), "{:#?}", reloaded.problems);
    assert_eq!(reloaded.project.elements.len(), loaded.project.elements.len());
}

/// IFX-090: `load` names every version this release reads when it refuses an
/// unknown one.
#[test]
fn an_unknown_version_names_the_supported_versions() {
    let mut value: serde_json::Value = serde_json::from_slice(&format_1()).unwrap();
    value["header"]["ifcxVersion"] = "ifcx_omega".into();
    let error = archicraft_ifcx::load(&serde_json::to_vec(&value).unwrap()).unwrap_err();
    assert!(error.message.contains("“ifcx_omega” is not supported"), "{error:?}");
    for adapter in archicraft_ifcx::ADAPTERS {
        assert!(error.message.contains(adapter.version), "{error:?}");
    }
}
