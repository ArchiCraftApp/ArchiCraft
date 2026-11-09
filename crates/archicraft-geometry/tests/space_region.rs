//! Space boundaries detected from walls (SPC-001, INP-018): the region
//! around a seed in the contour graph of a storey's wall footprints.

#![allow(clippy::unwrap_used)]

use archicraft_geometry::space::{RegionError, SpaceRegions, space_region};
use archicraft_model::glam::DVec2;
use archicraft_model::outline::Outline;
use archicraft_model::samples::two_storey_shell_with_shaft;
use archicraft_model::template::default_metric_project;
use archicraft_model::{Command, History, MODEL_TOLERANCE, Project, ReferenceLine, StoreyId};

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn rectangle(min: DVec2, max: DVec2) -> Vec<DVec2> {
    vec![min, v(max.x, min.y), max, v(min.x, max.y)]
}

fn storey(project: &Project, index: usize) -> StoreyId {
    project.storeys.as_slice()[index].id
}

/// Asserts that `found` is the ring `expected`, in the same direction and
/// within the model tolerance, starting anywhere.
fn assert_ring(found: &[DVec2], expected: &[DVec2]) {
    assert_eq!(found.len(), expected.len(), "{found:?} is not {expected:?}");
    let start = found
        .iter()
        .position(|p| p.distance(expected[0]) < MODEL_TOLERANCE)
        .unwrap_or_else(|| panic!("{found:?} does not pass {}", expected[0]));
    for (k, e) in expected.iter().enumerate() {
        let f = found[(start + k) % found.len()];
        assert!(
            f.distance(*e) < MODEL_TOLERANCE,
            "point {k}: {f} is not {e} in {found:?}"
        );
    }
}

/// The interior faces of the shell's exterior walls, 25 cm thick with the
/// reference line outside, with the notch of the 11.5 cm interior wall
/// centred on x = 5, which joins the south wall in a T and ends free at
/// y = 6: the same on both storeys.
fn room() -> Vec<DVec2> {
    vec![
        v(0.25, 0.25),
        v(4.9425, 0.25),
        v(4.9425, 6.0),
        v(5.0575, 6.0),
        v(5.0575, 0.25),
        v(11.75, 0.25),
        v(11.75, 8.75),
        v(0.25, 8.75),
    ]
}

/// The room's area: 11.5 m x 8.5 m inside the exterior walls less the
/// interior wall's 0.115 m x 5.75 m.
const ROOM_AREA: f64 = 11.5 * 8.5 - 0.115 * 5.75;

/// The outer faces of the shaft walls on the upper storey, 11.5 cm thick
/// around the centre lines x 1.0-3.0, y 6.5-8.0.
fn shaft_outside() -> Vec<DVec2> {
    rectangle(v(0.9425, 6.4425), v(3.0575, 8.0575))
}

/// SPC-001, INP-018: on the ground storey of the shell with a shaft, a seed
/// in the room gives the inner faces of the walls around it, with the notch
/// of the T-joined interior wall, counter-clockwise and without holes; its
/// area is the analytic inner area. A seed in the shaft on the upper storey
/// gives the shaft's inner faces, 1.885 m x 1.385 m; a seed outside the
/// building is unbounded.
#[test]
fn a_seed_in_a_room_gives_the_inner_wall_faces() {
    let project = two_storey_shell_with_shaft();
    let (ground, upper) = (storey(&project, 0), storey(&project, 1));

    let boundary = space_region(&project, ground, v(2.0, 3.0)).unwrap();
    let (outer, holes) = boundary.to_points();
    assert_ring(&outer, &room());
    assert!(boundary.outer().is_counter_clockwise());
    assert!(holes.is_empty(), "{holes:?}");
    assert!((boundary.area() - ROOM_AREA).abs() < 1e-6, "{}", boundary.area());
    assert!((boundary.area() - 97.08875).abs() < 1e-6);

    let shaft = space_region(&project, upper, v(2.0, 7.25)).unwrap();
    let (outer, holes) = shaft.to_points();
    assert_ring(&outer, &rectangle(v(1.0575, 6.5575), v(2.9425, 7.9425)));
    assert!(holes.is_empty(), "{holes:?}");
    assert!((shaft.area() - 1.885 * 1.385).abs() < 1e-6, "{}", shaft.area());
    assert!((shaft.area() - 2.610725).abs() < 1e-6);

    for outside in [v(-1.0, 3.0), v(13.0, 3.0), v(6.0, 20.0)] {
        for on in [ground, upper] {
            assert_eq!(
                space_region(&project, on, outside),
                Err(RegionError::Unbounded),
                "{outside}"
            );
        }
    }
}

/// SPC-001: a seed inside a wall, an exterior wall, the interior wall or a
/// shaft wall, is no seed of a room, even though the wall's footprint
/// bounds a face of the graph.
#[test]
fn a_seed_inside_a_wall_footprint_is_refused() {
    let project = two_storey_shell_with_shaft();
    let (ground, upper) = (storey(&project, 0), storey(&project, 1));
    for (on, seed) in [
        (ground, v(0.1, 3.0)),
        (ground, v(5.0, 3.0)),
        // Off the mitre diagonal of the north-east corner.
        (ground, v(11.9, 8.8)),
        (upper, v(1.0, 7.25)),
        (upper, v(2.0, 8.0)),
    ] {
        assert_eq!(
            space_region(&project, on, seed),
            Err(RegionError::SeedInFootprint),
            "{seed}"
        );
    }
    // Where the shaft stands on the upper storey, the ground storey has
    // none: the seed lies in the ground-floor room.
    assert_ring(
        &space_region(&project, ground, v(1.0, 7.25)).unwrap().to_points().0,
        &room(),
    );
}

/// SPC-001, SLB-003: on the upper storey the room around the shaft has the
/// shaft walls' outer faces as its only hole, clockwise like a slab's hole;
/// the floor slab's stair hole is no wall and no island. Its net area is the
/// room's less the shaft's 2.115 m x 1.615 m.
#[test]
fn free_standing_walls_in_a_room_become_its_holes() {
    let project = two_storey_shell_with_shaft();
    let upper = storey(&project, 1);
    let boundary = space_region(&project, upper, v(2.0, 3.0)).unwrap();
    let (outer, holes) = boundary.to_points();
    assert_ring(&outer, &room());
    assert_eq!(holes.len(), 1, "{holes:?}");
    let mut hole = shaft_outside();
    hole.reverse();
    assert_ring(&holes[0], &hole);
    assert!(!boundary.holes()[0].is_counter_clockwise());
    let area = ROOM_AREA - 2.115 * 1.615;
    assert!((boundary.area() - area).abs() < 1e-6, "{}", boundary.area());

    // The stair hole of the floor slab (x 6-9, y 1-2.2) lies in the room.
    let in_stair_hole = space_region(&project, upper, v(7.5, 1.6)).unwrap();
    assert_eq!(in_stair_hole, boundary);
}

/// SPC-001, INP-018: one SpaceRegions of a storey answers seeds in every
/// room as space_region does, regions with their islands clockwise, and the
/// result does not depend on the order in which the walls were drawn.
#[test]
fn one_graph_answers_every_seed_whatever_the_drawing_order() {
    let project = two_storey_shell_with_shaft();
    let mut reversed = project.clone();
    reversed.elements.reverse();
    let seeds = [v(2.0, 3.0), v(2.0, 7.25), v(9.0, 7.0), v(1.0, 7.25), v(-1.0, 3.0)];

    for index in [0, 1] {
        let on = storey(&project, index);
        let regions = SpaceRegions::build(&project, on);
        assert_eq!(regions.storey(), on);
        let other = SpaceRegions::build(&reversed, on);
        for seed in seeds {
            let region = regions.region_at(seed);
            assert_eq!(region, other.region_at(seed), "storey {index}, {seed}");
            let expected = space_region(&project, on, seed);
            let outline = region.map(|r| Outline::from_point_rings(std::iter::once(r.outer).chain(r.islands)));
            assert_eq!(outline, expected, "storey {index}, {seed}");
        }
    }
}

/// Three walls joined at (5, 0) and (5, 4), 20 cm thick and referenced at
/// their centres, around a room open to the west, where a free wall closes
/// it with `gap` between its east face and the square ends of the others.
fn room_closed_with_a_gap(gap: f64) -> (Project, StoreyId) {
    let mut project = default_metric_project();
    let ground = project.storeys.ground();
    let x = -0.1 - gap;
    let lines = [
        (v(0.0, 0.0), v(5.0, 0.0)),
        (v(5.0, 0.0), v(5.0, 4.0)),
        (v(5.0, 4.0), v(0.0, 4.0)),
        (v(x, 4.1), v(x, -0.1)),
    ];
    let walls = lines
        .iter()
        .zip(0..)
        .map(|(&(start, end), nth)| {
            let mut element = project.new_wall(ground, start, end, nth);
            let wall = element.as_wall_mut().unwrap();
            wall.thickness = 0.2;
            wall.reference_line = ReferenceLine::Center;
            element
        })
        .collect();
    History::default()
        .execute(&mut project, Command::AddElements(walls))
        .unwrap();
    (project, ground)
}

/// SPC-001, NFR-ACC-003: wall bodies 5 µm apart touch and close the room,
/// 4.9 m x 3.8 m inside; 20 µm apart they leave a slit through which the
/// room is open, so no region surrounds a seed in it.
#[test]
fn a_gap_below_the_tolerance_closes_a_room_and_a_wider_one_leaves_it_open() {
    let seed = v(2.5, 2.0);
    let (project, ground) = room_closed_with_a_gap(5e-6);
    let boundary = space_region(&project, ground, seed).unwrap();
    assert!((boundary.area() - 4.9 * 3.8).abs() < 1e-4, "{}", boundary.area());
    assert!(boundary.holes().is_empty());

    let (project, ground) = room_closed_with_a_gap(20e-6);
    assert_eq!(space_region(&project, ground, seed), Err(RegionError::Unbounded));
}
