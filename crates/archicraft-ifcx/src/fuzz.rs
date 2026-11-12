//! The open path of the `ifcx_load` fuzz target (NFR-REL-007, NFR-SEC-003;
//! docs/testing.md, Fuzzing): the application's open path on arbitrary
//! bytes, then a save and a second load. `fuzz/fuzz_targets/ifcx_load.rs`
//! runs [`open_path`] under libFuzzer, and the stable replays of its corpus
//! (`tests/fuzz_regressions.rs`, the tests of `examples/fuzz_seeds.rs`) run
//! the same function, so that they test what the fuzz runs test.
//!
//! The saved file grows with the body meshes, not with the input: the 400
//! slabs of 999 points of the first campaign's finding 4 are a 3.4 MB input
//! with 2.4 million mesh points, saved as 52.6 MB. Under AddressSanitizer and
//! libFuzzer's coverage instrumentation, saving and reading those 52.6 MB took
//! 11.7 s of the 13.8 s one run of the input needed, beyond the fuzz runs'
//! 10 s time-out, while a release build needs about a second for them
//! (archicraft-tuy0). So a project whose bodies have more than
//! [`ROUND_TRIP_MESH_POINTS`] points is saved without them
//! ([`save_for_round_trip`]): the round trip still writes and reads every
//! element with its axis, every storey and the project's materials, and
//! leaves out the body nodes and the material nodes that only bodies refer
//! to, which every smaller input reaches.

use std::sync::Arc;

use archicraft_geometry::{ElementGeometry, ModelGeometry};
use archicraft_model::Project;

use crate::WriteOptions;

/// The most body mesh points with which [`open_path`] saves a project with
/// its bodies (NFR-SEC-003). A body of a closed mesh has about twice as many
/// triangles as points, so this bounds the meshes of the saved file at about
/// 4.4 MB, the size of the fuzz runs' longest input (`-max_len`, 4 MB):
/// about 33 slabs of 999 points. The largest seed of `examples/fuzz_seeds.rs`
/// has 12,120 points.
pub const ROUND_TRIP_MESH_POINTS: usize = 200_000;

/// What [`open_path`] did with an input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opened {
    /// The reader rejected the input, with a message.
    Rejected,
    /// The reader loaded the input, and the project was evaluated, saved and
    /// loaded again; `bodies` tells whether the saved file held the body
    /// meshes ([`save_for_round_trip`]).
    RoundTrip { bodies: bool },
}

/// The points of every body mesh of `geometry`.
pub fn mesh_points(geometry: &ModelGeometry) -> usize {
    geometry
        .elements
        .values()
        .flat_map(|element| element.bodies())
        .map(|body| body.mesh.points.len())
        .sum()
}

/// Whether [`open_path`] saves a project of this geometry with its bodies:
/// when they have at most [`ROUND_TRIP_MESH_POINTS`] points.
pub fn round_trips_with_bodies(geometry: &ModelGeometry) -> bool {
    mesh_points(geometry) <= ROUND_TRIP_MESH_POINTS
}

/// The save of [`open_path`]: the project with its evaluated geometry, and
/// whether the file holds the body meshes. Beyond [`ROUND_TRIP_MESH_POINTS`]
/// each element keeps only its axis, so the file holds no body nodes and no
/// material nodes, which only bodies refer to (NFR-SEC-003).
pub fn save_for_round_trip(project: &Project, geometry: &ModelGeometry) -> (Vec<u8>, bool) {
    let options = WriteOptions {
        author: "Fuzz".into(),
        timestamp: "2026-10-20T00:00:00Z".into(),
        data_version: 1,
    };
    if round_trips_with_bodies(geometry) {
        let (saved, _warnings) = crate::save(project, geometry, &options, false);
        return (saved, true);
    }
    let mut axes = ModelGeometry::default();
    axes.elements = geometry
        .elements
        .iter()
        .map(|(id, element)| {
            let axis = ElementGeometry {
                axis: element.axis.clone(),
                ..ElementGeometry::default()
            };
            (*id, Arc::new(axis))
        })
        .collect();
    let (saved, _warnings) = crate::save(project, &axes, &options, false);
    (saved, false)
}

/// The open path of the `ifcx_load` fuzz target (NFR-REL-007, NFR-SEC-003):
/// [`crate::load`], as `ArchiCraftApp::open_path` loads a file, and when that
/// succeeds [`ModelGeometry::evaluate`], as `Document::opened` evaluates it,
/// then [`save_for_round_trip`] and [`crate::load`] again. The save leaves
/// the body meshes out beyond [`ROUND_TRIP_MESH_POINTS`]. None of it may
/// panic, and a rejected input carries a message.
///
/// # Panics
///
/// When the reader rejects the input with a message without a key: a
/// finding of the fuzz runs.
pub fn open_path(bytes: &[u8]) -> Opened {
    match crate::load(bytes) {
        Ok(outcome) => {
            let geometry = ModelGeometry::evaluate(&outcome.project);
            let (saved, bodies) = save_for_round_trip(&outcome.project, &geometry);
            let _ = crate::load(&saved);
            Opened::RoundTrip { bodies }
        }
        Err(problem) => {
            assert!(!problem.message.key().is_empty(), "rejected without a message");
            Opened::Rejected
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use archicraft_geometry::{BodyPart, ElementGeometry, ModelGeometry, PartGeometry, TriMesh};
    use archicraft_model::ElementId;
    use archicraft_model::glam::DVec3;

    use super::{ROUND_TRIP_MESH_POINTS, mesh_points, round_trips_with_bodies};

    fn body(points: usize) -> BodyPart {
        BodyPart {
            name: "Body".into(),
            material: None,
            mesh: TriMesh {
                points: vec![DVec3::ZERO; points],
                indices: Vec::new(),
            },
        }
    }

    /// A geometry of two elements, one with a body of `own` points and one
    /// with a child part of `child` points.
    fn geometry(own: usize, child: usize) -> ModelGeometry {
        let mut geometry = ModelGeometry::default();
        let first = ElementGeometry {
            parts: vec![body(own)],
            ..ElementGeometry::default()
        };
        let second = ElementGeometry {
            children: vec![PartGeometry {
                parts: vec![body(child)],
                ..PartGeometry::default()
            }],
            ..ElementGeometry::default()
        };
        geometry.elements.insert(ElementId::new(), Arc::new(first));
        geometry.elements.insert(ElementId::new(), Arc::new(second));
        geometry
    }

    /// NFR-SEC-003: the points of every body count, a child part's too.
    #[test]
    fn mesh_points_count_every_body_of_every_element() {
        assert_eq!(mesh_points(&ModelGeometry::default()), 0);
        assert_eq!(mesh_points(&geometry(10, 32)), 42);
    }

    /// NFR-REL-007, NFR-SEC-003: a project is saved with its bodies up to
    /// the bound, and without them beyond it.
    #[test]
    fn the_round_trip_keeps_the_bodies_up_to_the_bound() {
        let half = ROUND_TRIP_MESH_POINTS / 2;
        assert!(round_trips_with_bodies(&ModelGeometry::default()));
        assert!(round_trips_with_bodies(&geometry(half, ROUND_TRIP_MESH_POINTS - half)));
        assert!(!round_trips_with_bodies(&geometry(
            half,
            ROUND_TRIP_MESH_POINTS - half + 1
        )));
    }
}
