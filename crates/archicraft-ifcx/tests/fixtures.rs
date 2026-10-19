//! Generic IFCX checks on every fixture of the registry (SRS Appendix D.7):
//! the file validates, every element comes back editable with its parameters,
//! and regenerated geometry equals the stored geometry (IFX-064).

#![allow(clippy::unwrap_used)]

mod common;

use archicraft_ifcx::validate;
use archicraft_model::{Project, fixtures};

use common::save;

fn built() -> Vec<(&'static str, Project)> {
    let built: Vec<_> = fixtures::all()
        .filter(|f| f.build.is_some())
        .map(|f| (f.id, fixtures::check(f).unwrap_or_else(|e| panic!("{e}"))))
        .collect();
    assert!(!built.is_empty());
    built
}

/// Every built fixture saves without warnings and the file validates
/// without problems.
#[test]
fn every_built_fixture_saves_a_valid_file() {
    for (id, project) in built() {
        let file = validate::parse(&save(&project)).unwrap();
        let problems = validate::validate(&file);
        assert!(problems.is_empty(), "{id}: {problems:#?}");
    }
}

/// IFX-064: loading gives back the saved project, every element with its
/// parameters, so it stays editable.
#[test]
fn every_built_fixture_loads_back_equal() {
    for (id, project) in built() {
        let loaded = archicraft_ifcx::load(&save(&project)).unwrap();
        assert!(loaded.problems.is_empty(), "{id}: {:#?}", loaded.problems);
        assert!(!loaded.newer_format, "{id}");
        let mut expected = project.clone();
        expected.revision = 0;
        expected.data_version = common::options().data_version;
        assert_eq!(loaded.project, expected, "{id}: every parameter survives");
    }
}

/// IFX-064: save, load, regenerate and save again gives the same bytes, so
/// the regenerated geometry equals the stored geometry (within the 1e-6 m
/// the writer rounds to, well inside 0.1 mm).
#[test]
fn every_built_fixture_saves_identically_after_loading() {
    for (id, project) in built() {
        let first = save(&project);
        let loaded = archicraft_ifcx::load(&first).unwrap().project;
        let second = save(&loaded);
        assert!(first == second, "{id}: save-load-save changed the file");
    }
}
