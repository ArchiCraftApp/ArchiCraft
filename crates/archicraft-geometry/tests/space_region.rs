//! Space boundaries detected from walls and columns (SPC-001, INP-018): the
//! region around a seed in the contour graph of a storey's wall and column
//! footprints.

#![allow(clippy::unwrap_used)]

use std::f64::consts::FRAC_PI_6;

use archicraft_geometry::space::{RegionError, SpaceRegions, space_region};
use archicraft_geometry::{ContourGraph, ContourOptions, ModelGeometry, ViewRange, plan_drawing};
use archicraft_model::glam::DVec2;
use archicraft_model::kinds::{Anchor, Column, CrossSection};
use archicraft_model::outline::Outline;
use archicraft_model::samples::two_storey_shell_with_shaft;
use archicraft_model::template::default_metric_project;
use archicraft_model::{
    Command, Element, ElementId, History, MODEL_TOLERANCE, Profile, ProfileRegion, Project, ReferenceLine, StoreyId,
};

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

/// SPC-001, INP-018: walls really drawn in another order get other
/// identities, and junction resolution intersects the faces of two walls in
/// the order of their identities (`ordered_intersect`), so their corners may
/// differ in the last bits. The shell's walls, deleted and added again in
/// reverse through `AddElements`, with new identities in the reverse order
/// of the old ones so that every junction intersects its faces the other
/// way round, give the shell's boundaries within the model tolerance.
#[test]
fn walls_drawn_again_in_reverse_give_the_same_boundaries() {
    let project = two_storey_shell_with_shaft();
    let walls: Vec<Element> = project
        .elements
        .values()
        .filter(|e| e.as_wall().is_some())
        .cloned()
        .collect();
    assert_eq!(walls.len(), 14, "five walls on each storey and the shaft's four");
    let mut old: Vec<ElementId> = walls.iter().map(|w| w.id).collect();
    old.sort();
    let mut new: Vec<ElementId> = walls.iter().map(|_| ElementId::new()).collect();
    new.sort();
    new.reverse();
    let renamed = |id: ElementId| new[old.binary_search(&id).unwrap()];
    for a in &walls {
        for b in &walls {
            assert_eq!(a.id < b.id, renamed(b.id) < renamed(a.id), "{} and {}", a.name, b.name);
        }
    }

    let mut redrawn = project.clone();
    let mut history = History::default();
    history
        .execute(&mut redrawn, Command::DeleteElements(old.clone()))
        .unwrap();
    let again = walls.iter().rev().map(|wall| Element {
        id: renamed(wall.id),
        ..wall.clone()
    });
    history
        .execute(&mut redrawn, Command::AddElements(again.collect()))
        .unwrap();

    let seeds = [v(2.0, 3.0), v(2.0, 7.25), v(9.0, 7.0), v(1.0, 7.25), v(-1.0, 3.0)];
    let mut rooms = 0;
    for index in [0, 1] {
        let on = storey(&project, index);
        for seed in seeds {
            let (drawn, again) = (space_region(&project, on, seed), space_region(&redrawn, on, seed));
            let (Ok(drawn), Ok(again)) = (&drawn, &again) else {
                assert_eq!(drawn, again, "storey {index}, {seed}");
                continue;
            };
            let ((outer, holes), (outer_again, holes_again)) = (drawn.to_points(), again.to_points());
            assert_ring(&outer_again, &outer);
            assert_eq!(holes_again.len(), holes.len(), "storey {index}, {seed}");
            for (hole_again, hole) in holes_again.iter().zip(&holes) {
                assert_ring(hole_again, hole);
            }
            rooms += 1;
        }
    }
    assert_eq!(
        rooms, 7,
        "four seeds in rooms on the ground storey, two and the shaft above"
    );
}

/// The 20 cm wall referenced at its centre from `start` to `end`, the
/// `nth` new one.
fn centred_wall(project: &Project, storey: StoreyId, start: DVec2, end: DVec2, nth: u32) -> Element {
    let mut element = project.new_wall(storey, start, end, nth);
    let wall = element.as_wall_mut().unwrap();
    wall.thickness = 0.2;
    wall.reference_line = ReferenceLine::Center;
    element
}

/// [`walled_room`] with a U of three walls like its own standing free in
/// it: from (1, 2) east to (3, 2), 15 cm north and back west to (1, 2.15).
/// The middle wall is shorter than it is thick, so its L junctions fold it:
/// its joined footprint crosses itself, its body has square ends instead
/// (WAL-027), and the U's outline has a notch at each of its outer corners.
fn room_with_a_folded_u() -> (Project, StoreyId) {
    let (mut project, ground) = walled_room(false, &[]);
    let u = [
        (v(1.0, 2.0), v(3.0, 2.0)),
        (v(3.0, 2.0), v(3.0, 2.15)),
        (v(3.0, 2.15), v(1.0, 2.15)),
    ];
    let walls = u
        .iter()
        .zip(0..)
        .map(|(&(start, end), nth)| centred_wall(&project, ground, start, end, nth))
        .collect();
    History::default()
        .execute(&mut project, Command::AddElements(walls))
        .unwrap();
    (project, ground)
}

/// SPC-001, WAL-027: a detected boundary follows the walls' bodies as the
/// plan and the 3D views show them, also where a junction folds a short
/// wall and its body falls back to square ends. The U in
/// [`room_with_a_folded_u`] is an island of the room with a notch at each
/// outer corner, as in the plan's contour region around the seed, and a seed
/// in a notch lies in the room, not in a wall's footprint.
#[test]
fn a_wall_folded_by_its_junctions_bounds_the_room_with_its_square_ends() {
    let (project, ground) = room_with_a_folded_u();
    let seed = v(0.5, 0.5);
    let boundary = space_region(&project, ground, seed).unwrap();
    let (outer, holes) = boundary.to_points();
    assert_ring(&outer, &rectangle(v(0.1, 0.1), v(4.9, 3.9)));
    assert_eq!(holes.len(), 1, "{holes:?}");
    // The mitres of the outer corners end on the middle wall's square ends,
    // 10 cm in from its east face.
    let mut u = vec![
        v(1.0, 1.9),
        v(3.1, 1.9),
        v(3.0, 2.0),
        v(3.1, 2.0),
        v(3.1, 2.15),
        v(3.0, 2.15),
        v(3.1, 2.25),
        v(1.0, 2.25),
    ];
    u.reverse();
    assert_ring(&holes[0], &u);

    let geometry = ModelGeometry::evaluate(&project);
    assert!(geometry.problems.is_empty(), "{:?}", geometry.problems);
    let plan = plan_drawing(
        &project,
        &geometry,
        ground,
        ViewRange::for_storey(&project, ground),
        |_| true,
    );
    let in_plan = ContourGraph::build(&plan, &ContourOptions::default())
        .region_at(seed)
        .unwrap();
    assert_ring(&in_plan.outer, &outer);
    assert_eq!(in_plan.islands.len(), 1, "{:?}", in_plan.islands);
    assert_ring(&in_plan.islands[0], &u);

    for notch in [v(3.08, 1.99), v(3.08, 2.16)] {
        assert_eq!(space_region(&project, ground, notch).as_ref(), Ok(&boundary), "{notch}");
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

/// A column for [`walled_room`]: its position, its rotation and the change
/// of the column tool's defaults that gives its section.
type ColumnAt<'a> = (DVec2, f64, &'a dyn Fn(&mut Column));

/// Four walls 20 cm thick referenced at their centres around the 5 m x 4 m
/// rectangle from the origin, 4.8 m x 3.8 m inside; with `gap` the west wall
/// is two walls with their square ends 30 cm apart, from y 1.85 to 2.15.
/// `columns` are added on the ground storey.
fn walled_room(gap: bool, columns: &[ColumnAt<'_>]) -> (Project, StoreyId) {
    let mut project = default_metric_project();
    let ground = project.storeys.ground();
    let mut lines = vec![
        (v(0.0, 0.0), v(5.0, 0.0)),
        (v(5.0, 0.0), v(5.0, 4.0)),
        (v(5.0, 4.0), v(0.0, 4.0)),
    ];
    if gap {
        lines.extend([(v(0.0, 4.0), v(0.0, 2.15)), (v(0.0, 1.85), v(0.0, 0.0))]);
    } else {
        lines.push((v(0.0, 4.0), v(0.0, 0.0)));
    }
    let mut elements: Vec<Element> = lines
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
    for (&(position, rotation, change), nth) in columns.iter().zip(0..) {
        let mut element = project.new_column(ground, position, rotation, nth);
        change(element.as_column_mut().unwrap());
        elements.push(element);
    }
    History::default()
        .execute(&mut project, Command::AddElements(elements))
        .unwrap();
    (project, ground)
}

/// A rectangular section `width` x `height`.
fn rectangular(width: f64, height: f64) -> impl Fn(&mut Column) {
    move |column: &mut Column| column.section = CrossSection::Rectangular { width, height }
}

/// A circular section of `diameter`.
fn circular(diameter: f64) -> impl Fn(&mut Column) {
    move |column: &mut Column| column.section = CrossSection::Circular { diameter }
}

/// The plan corners of a `width` x `height` rectangle centred on `centre`
/// and turned by `rotation`, counter-clockwise.
fn turned_rectangle(centre: DVec2, width: f64, height: f64, rotation: f64) -> Vec<DVec2> {
    let turn = DVec2::from_angle(rotation);
    let (x, y) = (width / 2.0, height / 2.0);
    [v(-x, -y), v(x, -y), v(x, y), v(-x, y)]
        .into_iter()
        .map(|p| centre + turn.rotate(p))
        .collect()
}

/// The inner area of [`walled_room`] without columns or gap.
const WALLED_ROOM_AREA: f64 = 4.8 * 3.8;

/// SPC-001: columns standing free in a room bounded by four walls become
/// holes of the detected space, clockwise like a slab's holes, each the
/// column's outline: a 0.30 m square, a 0.40 m x 0.20 m rectangle turned by
/// 30 degrees and a 0.30 m circle as the column's body tessellates it. The
/// net area is the room's less the columns'; a seed in a column is refused.
#[test]
fn columns_standing_free_in_a_room_become_its_holes() {
    let square = v(2.5, 2.0);
    let turned = v(1.0, 1.0);
    let round = v(4.0, 3.0);
    let (project, ground) = walled_room(
        false,
        &[
            (square, 0.0, &rectangular(0.3, 0.3)),
            (turned, FRAC_PI_6, &rectangular(0.4, 0.2)),
            (round, 0.0, &circular(0.3)),
        ],
    );
    let boundary = space_region(&project, ground, v(3.0, 1.0)).unwrap();
    let (outer, holes) = boundary.to_points();
    assert_ring(&outer, &rectangle(v(0.1, 0.1), v(4.9, 3.9)));
    assert_eq!(holes.len(), 3, "{holes:?}");
    assert!(boundary.holes().iter().all(|hole| !hole.is_counter_clockwise()));

    let hole_through = |p: DVec2| {
        holes
            .iter()
            .find(|hole| hole.iter().any(|q| q.distance(p) < 0.25))
            .unwrap_or_else(|| panic!("no hole near {p}: {holes:?}"))
    };
    for (centre, width, height, rotation) in [(square, 0.3, 0.3, 0.0), (turned, 0.4, 0.2, FRAC_PI_6)] {
        let mut expected = turned_rectangle(centre, width, height, rotation);
        expected.reverse();
        assert_ring(hole_through(centre), &expected);
    }
    let circle = hole_through(round);
    assert!(circle.len() >= 16, "{circle:?}");
    for p in circle {
        assert!((p.distance(round) - 0.15).abs() < MODEL_TOLERANCE, "{p}");
    }
    assert!(
        circle
            .iter()
            .any(|p| p.distance(round + v(0.15, 0.0)) < MODEL_TOLERANCE)
    );
    // The shoelace area of the clockwise ring, negated.
    let circle_area = -(0..circle.len())
        .map(|i| circle[i].perp_dot(circle[(i + 1) % circle.len()]))
        .sum::<f64>()
        / 2.0;
    assert!(
        circle_area < std::f64::consts::PI * 0.15 * 0.15 && circle_area > 0.06,
        "{circle_area}"
    );
    let area = WALLED_ROOM_AREA - 0.09 - 0.08 - circle_area;
    assert!((boundary.area() - area).abs() < 1e-6, "{}", boundary.area());

    for seed in [
        square,
        square + v(0.14, -0.14),
        turned,
        turned + DVec2::from_angle(FRAC_PI_6) * 0.19,
        round,
        round + v(0.0, 0.14),
    ] {
        assert_eq!(
            space_region(&project, ground, seed),
            Err(RegionError::SeedInFootprint),
            "{seed}"
        );
    }
    // Just outside the turned column's corner along its unturned box.
    assert_eq!(space_region(&project, ground, turned + v(0.0, 0.19)).unwrap(), boundary);
}

/// SPC-001: the west wall of the room has a 30 cm gap through which the
/// room is open, so a seed in it is unbounded. A 0.30 m square column whose
/// faces meet the walls' square ends closes the room; it stands 5 cm into
/// the room beyond the walls' inner face. A square turned by 45 degrees
/// and a circle, each overlapping the wall ends, close it alike.
#[test]
fn a_column_filling_the_gap_in_a_wall_line_closes_the_room() {
    let seed = v(2.5, 2.0);
    let (open, ground) = walled_room(true, &[]);
    assert_eq!(space_region(&open, ground, seed), Err(RegionError::Unbounded));

    let gap = v(0.0, 2.0);
    let (closed, ground) = walled_room(true, &[(gap, 0.0, &rectangular(0.3, 0.3))]);
    let boundary = space_region(&closed, ground, seed).unwrap();
    assert!(boundary.holes().is_empty());
    assert_ring(
        &boundary.to_points().0,
        &[
            v(0.1, 0.1),
            v(4.9, 0.1),
            v(4.9, 3.9),
            v(0.1, 3.9),
            v(0.1, 2.15),
            v(0.15, 2.15),
            v(0.15, 1.85),
            v(0.1, 1.85),
        ],
    );
    let area = WALLED_ROOM_AREA - 0.05 * 0.3;
    assert!((boundary.area() - area).abs() < 1e-6, "{}", boundary.area());
    assert_eq!(space_region(&closed, ground, gap), Err(RegionError::SeedInFootprint));

    for change in [
        (
            std::f64::consts::FRAC_PI_4,
            &rectangular(0.4, 0.4) as &dyn Fn(&mut Column),
        ),
        (0.0, &circular(0.4)),
    ] {
        let (closed, ground) = walled_room(true, &[(gap, change.0, change.1)]);
        let boundary = space_region(&closed, ground, seed).unwrap();
        assert!(boundary.holes().is_empty());
        // The column takes a little of the room, less than its own area.
        let taken = WALLED_ROOM_AREA - boundary.area();
        assert!(taken > 0.0 && taken < 0.16, "{taken}");
        assert_eq!(space_region(&closed, ground, gap), Err(RegionError::SeedInFootprint));
    }
}

/// SPC-001, PRF-004: a column whose section is a custom profile of two
/// touching plates forming an L is one hole of the room, the outline of the
/// L, anchored at its bottom left corner; a column on another storey is
/// none.
#[test]
fn a_profile_column_is_a_hole_with_the_profile_outline() {
    let (mut project, ground) = walled_room(false, &[]);
    let upper = storey(&project, 1);
    let material = project.materials[0].id;
    let plate = |min: DVec2, max: DVec2| ProfileRegion {
        outer: rectangle(min, max),
        holes: Vec::new(),
        material,
    };
    let profile = Profile::custom(
        "L",
        vec![plate(v(0.0, 0.0), v(0.3, 0.1)), plate(v(0.0, 0.1), v(0.1, 0.3))],
    );
    let mut history = History::default();
    history
        .execute(&mut project, Command::AddProfile(profile.clone()))
        .unwrap();
    let mut elements = Vec::new();
    for (on, at, nth) in [(ground, v(3.5, 1.0), 0), (upper, v(2.0, 2.0), 1)] {
        let mut element = project.new_column(on, at, 0.0, nth);
        let column = element.as_column_mut().unwrap();
        column.section = CrossSection::Profile(profile.id);
        column.anchor = Anchor::BottomLeft;
        elements.push(element);
    }
    history.execute(&mut project, Command::AddElements(elements)).unwrap();

    let boundary = space_region(&project, ground, v(1.0, 1.0)).unwrap();
    let (_, holes) = boundary.to_points();
    assert_eq!(holes.len(), 1, "{holes:?}");
    let mut l = [
        v(0.0, 0.0),
        v(0.3, 0.0),
        v(0.3, 0.1),
        v(0.1, 0.1),
        v(0.1, 0.3),
        v(0.0, 0.3),
    ]
    .map(|p| p + v(3.5, 1.0));
    l.reverse();
    assert_ring(&holes[0], &l);
    let area = WALLED_ROOM_AREA - 0.05;
    assert!((boundary.area() - area).abs() < 1e-6, "{}", boundary.area());
    assert_eq!(
        space_region(&project, ground, v(3.55, 1.2)),
        Err(RegionError::SeedInFootprint)
    );
    assert_eq!(space_region(&project, ground, v(3.7, 1.2)).unwrap(), boundary);
}
