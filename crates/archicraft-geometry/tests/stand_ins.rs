//! The synthetic stand-ins for RM-M and RM-L evaluate like real models
//! (NFR-PERF-006, VIEW-001, SRS 7.3): no problems, and junctions on every
//! storey.

#![allow(clippy::unwrap_used)]

use archicraft_geometry::ModelGeometry;
use archicraft_geometry::evaluate::wall_inputs;
use archicraft_geometry::wall::{WallInput, footprints};
use archicraft_model::JOIN_TOLERANCE;
use archicraft_model::samples;

/// How a wall end meets the other walls of its storey.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Contact {
    /// Another wall ends at the same point.
    L,
    /// The end lies on another wall's reference line, away from its ends.
    T,
    None,
}

fn contact(walls: &[WallInput], index: usize, at_end: bool) -> Contact {
    let wall = &walls[index];
    let p = if at_end { wall.end } else { wall.start };
    let others = walls
        .iter()
        .enumerate()
        .filter(|(j, o)| *j != index && o.storey == wall.storey);
    let mut contact = Contact::None;
    for (_, other) in others {
        if other.start.distance(p) < JOIN_TOLERANCE || other.end.distance(p) < JOIN_TOLERANCE {
            return Contact::L;
        }
        let ab = other.end - other.start;
        let t = ((p - other.start).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
        if p.distance(other.start + ab * t) < JOIN_TOLERANCE {
            contact = Contact::T;
        }
    }
    contact
}

/// NFR-PERF-006, VIEW-001: `stand_in(1_000)` evaluates without problems,
/// and every storey has an L and a T junction whose ends differ from square
/// ends, so an edit there recomputes neighbours' junctions.
#[test]
fn a_stand_in_evaluates_with_l_and_t_junctions_on_every_storey() {
    let project = samples::stand_in(1_000);
    let geometry = ModelGeometry::evaluate(&project);
    assert!(geometry.problems.is_empty(), "{:?}", geometry.problems);
    assert_eq!(geometry.elements.len(), 1_000);

    let walls = wall_inputs(&project);
    let joined = footprints(&walls);
    for storey in project.storeys.as_slice() {
        let (mut l, mut t) = (0, 0);
        for (i, wall) in walls.iter().enumerate().filter(|(_, w)| w.storey == storey.id) {
            // A wall on its own has square ends.
            let square = footprints(std::slice::from_ref(wall))[&wall.id];
            let footprint = joined[&wall.id];
            // Footprint corners: start exterior, end exterior, end interior,
            // start interior.
            for (at_end, corners) in [(false, [0, 3]), (true, [1, 2])] {
                let moved = corners
                    .iter()
                    .any(|&c| footprint[c].distance(square[c]) > JOIN_TOLERANCE);
                match (contact(&walls, i, at_end), moved) {
                    (Contact::L, true) => l += 1,
                    (Contact::T, true) => t += 1,
                    _ => {}
                }
            }
        }
        assert!(l >= 1, "{}: an L junction", storey.name);
        assert!(t >= 1, "{}: a T junction", storey.name);
    }
}

/// The RM-M-sized stand-in evaluates without problems too.
#[test]
fn a_stand_in_of_5000_evaluates_without_problems() {
    let project = samples::stand_in(5_000);
    let geometry = ModelGeometry::evaluate(&project);
    assert!(geometry.problems.is_empty(), "{:?}", geometry.problems);
    assert_eq!(geometry.elements.len(), 5_000);
}
