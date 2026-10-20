//! The fixture registry: the geometry fixtures of SRS Appendix D.7 and further
//! named cases, each with an optional constructor. Every built entry is held
//! to the same generic checks: [`check`] here, and closed bodies (IFX-032) and
//! an IFCX round trip (IFX-064) in the geometry and IFCX test crates.
//!
//! The owning epic of a fixture adds its constructor to this module (for
//! example `pub fn f1_trapezoidal_wall() -> Project`), builds the project
//! through commands like [`crate::samples`] does, and sets `build` in its
//! entry. See docs/testing.md, section "Fixture registry".

use std::collections::HashSet;

use crate::project::Project;
use crate::samples;
use crate::validate::validate_element;

/// One registry entry.
#[derive(Clone, Copy, Debug)]
pub struct Fixture {
    /// `F1`..`F9` for Appendix D.7; lowercase with a hyphen otherwise.
    pub id: &'static str,
    pub title: &'static str,
    /// Knowledge-base article the case comes from, for example `KB 303968`.
    pub source: &'static str,
    /// Key of the epic in docs/roadmap.md that owns the constructor.
    pub owner: &'static str,
    /// SRS increment whose exit needs the constructor.
    pub increment: u8,
    /// Whether the constructor gives the same identities on every call, so
    /// two builds compare equal.
    pub seeded: bool,
    pub build: Option<fn() -> Project>,
}

const fn d7(
    id: &'static str,
    title: &'static str,
    source: &'static str,
    owner: &'static str,
    increment: u8,
) -> Fixture {
    Fixture {
        id,
        title,
        source,
        owner,
        increment,
        seeded: false,
        build: None,
    }
}

/// The geometry fixtures of SRS Appendix D.7, in order.
pub const D7: [Fixture; 9] = [
    d7(
        "F1",
        "Trapezoidal wall, 300 mm at the start and 500 mm at the end",
        "KB 303968",
        "junction-engine",
        1,
    ),
    d7(
        "F2",
        "Layered wall: brick 120 / air 60 / insulation 180 / plaster 10 / concrete 300 / plaster 10 = 680 mm",
        "KB 303787",
        "layer-structures",
        1,
    ),
    d7("F3", "Floating floor", "KB 303722", "layer-structures", 1),
    d7(
        "F4",
        "Precast double-pitch beam with four segments",
        "KB 303950",
        "members-profiles-adv",
        3,
    ),
    d7("F5", "Precast corbel column", "KB 303947", "members-profiles-adv", 3),
    d7(
        "F6",
        "Three walls at one node with junction orders 8/8/8 versus 8/8/7",
        "KB 303922",
        "junction-engine",
        1,
    ),
    d7(
        "F7",
        "T-junction with concrete and insulation, then the priorities swapped",
        "KB 303922",
        "junction-engine",
        1,
    ),
    d7(
        "F8",
        "Slab traced from a curved wall's reference line and core face with zero gaps",
        "KB 669891",
        "curved-geometry",
        2,
    ),
    d7(
        "F9",
        "Sloping-site house with storeys -3..+1",
        "KB 364093",
        "terrain-and-site",
        2,
    ),
];

/// Named cases outside Appendix D.7.
pub const EXTRA: &[Fixture] = &[
    Fixture {
        id: "S-shell",
        title: "Two-storey shell with a T junction and a slab opening",
        source: "samples::two_storey_shell",
        owner: "regression-and-status",
        increment: 0,
        seeded: false,
        build: Some(samples::two_storey_shell),
    },
    // The provisional bench model of NFR-PERF-001, held to the generic
    // checks so a bench never measures a model that fails to evaluate.
    Fixture {
        id: "tiled-shell",
        title: "Two-storey shell repeated on a 9 x 9 grid (972 elements)",
        source: "samples::tiled_shell_bench",
        owner: "regression-and-status",
        increment: 1,
        seeded: false,
        build: Some(samples::tiled_shell_bench),
    },
];

/// Every entry at or below this increment must have a constructor. The
/// verification of an increment's exit criterion raises it.
pub const REQUIRED_INCREMENT: u8 = 0;

/// The D.7 fixtures followed by the extra cases.
pub fn all() -> impl Iterator<Item = &'static Fixture> {
    D7.iter().chain(EXTRA)
}

/// The increment gate: fails, naming each fixture, when an entry at or below
/// `required_increment` has no constructor.
pub fn check_increment(table: &[Fixture], required_increment: u8) -> Result<(), String> {
    let missing: Vec<&str> = table
        .iter()
        .filter(|f| f.increment <= required_increment && f.build.is_none())
        .map(|f| f.id)
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "no constructor for fixtures required by increment {required_increment}: {}",
            missing.join(", ")
        ))
    }
}

/// Builds the fixture and runs the generic model checks: no integrity
/// problems, every element valid in its project, unique identities and, when
/// seeded, two builds equal. Errors start with the fixture id.
pub fn check(fixture: &Fixture) -> Result<Project, String> {
    let fail = |problem: String| format!("{}: {problem}", fixture.id);
    let build = fixture.build.ok_or_else(|| fail("no constructor".into()))?;
    let project = build();
    if let Some(problem) = project.integrity_problems().into_iter().next() {
        return Err(fail(problem.to_string()));
    }
    for (id, element) in &project.elements {
        if *id != element.id {
            return Err(fail(format!("{} is stored under another id", element.name)));
        }
        validate_element(&project, element).map_err(|e| fail(format!("{}: {e}", element.name)))?;
    }
    let mut seen = HashSet::new();
    let ids = [project.id.uuid(), project.site.id.uuid(), project.building.id.uuid()]
        .into_iter()
        .chain(project.storeys.as_slice().iter().map(|s| s.id.uuid()))
        .chain(project.layers.iter().map(|l| l.id.uuid()))
        .chain(project.materials.iter().map(|m| m.id.uuid()))
        .chain(project.elements.keys().map(|id| id.uuid()))
        .chain(project.views.iter().map(|v| v.id.uuid()));
    for id in ids {
        if !seen.insert(id) {
            return Err(fail(format!("identity {id} is used twice")));
        }
    }
    if fixture.seeded && build() != project {
        return Err(fail("two builds differ although the fixture is seeded".into()));
    }
    Ok(project)
}
