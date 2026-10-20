//! The fixture registry (SRS Appendix D.7): the D.7 table, the increment gate
//! and the generic model checks every registered fixture passes (IFX-032 and
//! IFX-064 are re-verified on them by the geometry and IFCX test crates).

#![allow(clippy::unwrap_used)]

use archicraft_model::fixtures::{self, D7, EXTRA, Fixture, REQUIRED_INCREMENT};
use archicraft_model::{LayerId, Project, samples};

/// Appendix D.7: ids F1..F9 in order with their knowledge-base sources, the
/// owning epics of docs/roadmap.md and the increment that needs them.
#[test]
fn the_d7_table_is_pinned() {
    let table: Vec<(&str, &str, &str, u8)> = D7.iter().map(|f| (f.id, f.source, f.owner, f.increment)).collect();
    assert_eq!(
        table,
        [
            ("F1", "KB 303968", "junction-engine", 1),
            ("F2", "KB 303787", "layer-structures", 1),
            ("F3", "KB 303722", "layer-structures", 1),
            ("F4", "KB 303950", "members-profiles-adv", 3),
            ("F5", "KB 303947", "members-profiles-adv", 3),
            ("F6", "KB 303922", "junction-engine", 1),
            ("F7", "KB 303922", "junction-engine", 1),
            ("F8", "KB 669891", "curved-geometry", 2),
            ("F9", "KB 364093", "terrain-and-site", 2),
        ]
    );
    for f in &D7 {
        assert!(!f.title.trim().is_empty(), "{} has a title", f.id);
    }
}

/// Registry ids are unique, and ids outside D.7 are lowercase with a hyphen
/// (or `S-shell`) so they never look like a D.7 id.
#[test]
fn registry_ids_are_unique_and_unambiguous() {
    let ids: Vec<&str> = fixtures::all().map(|f| f.id).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len(), "duplicate id in {ids:?}");
    for f in EXTRA {
        assert!(f.id.contains('-'), "{} has a hyphen", f.id);
        assert!(!D7.iter().any(|d| d.id.eq_ignore_ascii_case(f.id)), "{}", f.id);
    }
    assert_eq!(fixtures::all().count(), D7.len() + EXTRA.len());
}

/// The S-shell entry gives the generic checks something to run on today.
#[test]
fn s_shell_is_registered_at_increment_zero() {
    let shell = EXTRA.iter().find(|f| f.id == "S-shell").unwrap();
    assert_eq!(shell.increment, 0);
    // Identities are random until seeded builders exist, so compare content.
    let (built, sample) = (shell.build.unwrap()(), samples::two_storey_shell());
    assert_eq!(built.info, sample.info);
    let names = |p: &Project| p.elements.values().map(|e| e.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&built), names(&sample));
}

/// NFR-PERF-001: the provisional bench model is an extra fixture, so it
/// inherits the generic model, geometry and IFCX checks.
#[test]
fn the_tiled_shell_bench_model_is_an_extra_fixture() {
    let tiled = EXTRA.iter().find(|f| f.id == "tiled-shell").unwrap();
    assert_eq!(tiled.increment, 1);
    assert_eq!(tiled.build.unwrap()().elements.len(), 972);
}

fn entry(id: &'static str, increment: u8, build: Option<fn() -> Project>) -> Fixture {
    Fixture {
        id,
        title: "Synthetic",
        source: "test",
        owner: "regression-and-status",
        increment,
        seeded: false,
        build,
    }
}

fn shell() -> Project {
    samples::two_storey_shell()
}

/// The increment gate fails when an entry at or below the required increment
/// has no constructor and names that fixture; entries above it may wait.
#[test]
fn the_increment_gate_names_missing_constructors() {
    let table = [
        entry("x-built", 1, Some(shell)),
        entry("x-missing", 1, None),
        entry("x-later", 2, None),
    ];
    assert_eq!(fixtures::check_increment(&table, 0), Ok(()));
    let error = fixtures::check_increment(&table, 1).unwrap_err();
    assert!(error.contains("x-missing"), "{error}");
    assert!(!error.contains("x-built") && !error.contains("x-later"), "{error}");
    let error = fixtures::check_increment(&table, 2).unwrap_err();
    assert!(error.contains("x-missing") && error.contains("x-later"), "{error}");
}

/// The gate is enforced on the real registry at REQUIRED_INCREMENT, which
/// is 0 until the increment-1 exit verification raises it.
#[test]
fn the_registry_passes_the_increment_gate() {
    assert_eq!(REQUIRED_INCREMENT, 0);
    let registry: Vec<Fixture> = fixtures::all().copied().collect();
    assert_eq!(fixtures::check_increment(&registry, REQUIRED_INCREMENT), Ok(()));
    assert_eq!(fixtures::check_increment(&D7, 0), Ok(()));
}

fn dangling_layer() -> Project {
    let mut project = samples::two_storey_shell();
    let element = project.elements.values_mut().next().unwrap();
    element.layer = LayerId::new();
    project
}

/// `check` reports a broken reference with the fixture id and the problem.
#[test]
fn check_names_the_fixture_and_the_problem() {
    let error = fixtures::check(&entry("x-dangling", 0, Some(dangling_layer))).unwrap_err();
    assert!(error.contains("x-dangling"), "{error}");
    assert!(error.contains("layer is missing"), "{error}");
    let error = fixtures::check(&entry("x-unbuilt", 0, None)).unwrap_err();
    assert!(
        error.contains("x-unbuilt") && error.contains("no constructor"),
        "{error}"
    );
}

/// Every registered fixture with a constructor passes the generic model
/// checks: integrity, element validation, unique identities and, when
/// seeded, reproducible builds.
#[test]
fn every_built_fixture_passes_the_generic_checks() {
    let mut checked = 0;
    for f in fixtures::all().filter(|f| f.build.is_some()) {
        let project = fixtures::check(f).unwrap_or_else(|e| panic!("{e}"));
        assert!(!project.elements.is_empty(), "{} has elements", f.id);
        checked += 1;
    }
    assert!(checked >= 1);
}
