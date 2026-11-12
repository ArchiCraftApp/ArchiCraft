//! Contour detection (INP-018), geometry half: the planar graph of a plan's
//! lines, the smallest closed region around a point, and the clicked edge or
//! chain of connected edges.

#![allow(clippy::unwrap_used)]

use archicraft_geometry::contour::{Chain, ContourGraph, ContourOptions, PlanSegmentSource, Region, Segment};
use archicraft_geometry::{ModelGeometry, PlanDrawing, ViewRange, plan_drawing};
use archicraft_model::glam::DVec2;
use archicraft_model::samples::{FixtureBuilder, two_storey_shell, two_storey_shell_with_shaft};
use archicraft_model::{ANGLE_TOLERANCE, MODEL_TOLERANCE, Position, Project, ReferenceLine};

const MICROMETRE: f64 = 1e-6;

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

/// The plan of the storey at `index` with every element shown.
fn plan_of(project: &Project, index: usize) -> PlanDrawing {
    let geometry = ModelGeometry::evaluate(project);
    assert!(geometry.problems.is_empty(), "{:?}", geometry.problems);
    let storey = project.storeys.as_slice()[index].id;
    plan_drawing(
        project,
        &geometry,
        storey,
        ViewRange::for_storey(project, storey),
        |_| true,
    )
}

fn graph_of(segments: &[[DVec2; 2]]) -> ContourGraph {
    let segments: Vec<Segment> = segments.iter().map(|[a, b]| Segment::new(*a, *b)).collect();
    ContourGraph::build(segments.as_slice(), &ContourOptions::default())
}

/// The closed polyline through `points`, as segments.
fn ring_segments(points: &[DVec2]) -> Vec<[DVec2; 2]> {
    (0..points.len())
        .map(|i| [points[i], points[(i + 1) % points.len()]])
        .collect()
}

fn rectangle(min: DVec2, max: DVec2) -> Vec<DVec2> {
    vec![min, v(max.x, min.y), max, v(min.x, max.y)]
}

/// Signed area of a ring, positive for counter-clockwise.
fn signed_area(ring: &[DVec2]) -> f64 {
    (0..ring.len())
        .map(|i| ring[i].perp_dot(ring[(i + 1) % ring.len()]))
        .sum::<f64>()
        / 2.0
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

/// A pick radius around a clicked point.
const PICK_RADIUS: f64 = 0.15;

/// Asserts that `chain` runs through `expected` in this order, within the
/// model tolerance, and is closed or open as `closed` says.
fn assert_chain(chain: &Chain, expected: &[DVec2], closed: bool) {
    assert_eq!(chain.closed, closed, "{chain:?}");
    assert_eq!(chain.points.len(), expected.len(), "{chain:?} is not {expected:?}");
    for (found, e) in chain.points.iter().zip(expected) {
        assert!(found.distance(*e) < MODEL_TOLERANCE, "{found} is not {e} in {chain:?}");
    }
}

/// The interior faces of the two-storey shell's exterior walls on the ground
/// storey, with the notch of the interior T wall (0.115 m, centred on x = 5,
/// from the south wall to y = 6).
fn ground_room() -> Vec<DVec2> {
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

/// INP-018, SPC-001: contour detection ignores spaces. Spaces drawn
/// inside the ground room of the two-storey shell, one up to the next
/// storey and one lower than the cut plane, neither split the room's region
/// nor add one: the room is the same region, and the plan has as many
/// regions as without them.
#[test]
fn spaces_inside_a_room_do_not_split_its_region() {
    let mut p = two_storey_shell();
    let before = ContourGraph::build(&plan_of(&p, 0), &ContourOptions::default())
        .regions()
        .len();
    let ground = p.storeys.ground();
    let tall = p.new_space(ground, rectangle(v(1.0, 1.0), v(4.0, 5.0)), Vec::new(), 0);
    let mut low = p.new_space(ground, rectangle(v(6.0, 1.0), v(10.0, 7.0)), Vec::new(), 1);
    low.as_space_mut().unwrap().link.top = archicraft_model::TopLink::Fixed { height: 0.5 };
    archicraft_model::History::default()
        .execute(&mut p, archicraft_model::Command::AddElements(vec![tall, low]))
        .unwrap();

    let graph = ContourGraph::build(&plan_of(&p, 0), &ContourOptions::default());
    for inside in [v(2.0, 3.0), v(8.0, 3.0), v(11.0, 8.0)] {
        let room = graph.region_at(inside).unwrap();
        assert_ring(&room.outer, &ground_room());
        assert!(room.islands.is_empty(), "{:?}", room.islands);
    }
    assert_eq!(graph.regions().len(), before);
}

/// INP-018: on the ground storey of the two-storey shell, the smallest
/// region around a point in the room is bounded by the interior faces of the
/// exterior walls and the notch of the T wall, without islands; a point in
/// the walls gets the wall body, whose island is the room's boundary.
#[test]
fn the_smallest_region_around_a_point_in_the_ground_room() {
    let plan = plan_of(&two_storey_shell(), 0);
    let graph = ContourGraph::build(&plan, &ContourOptions::default());

    let room = graph.region_at(v(2.0, 3.0)).unwrap();
    assert_ring(&room.outer, &ground_room());
    let area = 11.5 * 8.5 - 0.115 * 5.75;
    assert!((signed_area(&room.outer) - 97.08875).abs() < 1e-6, "{room:?}");
    assert!((room.area() - area).abs() < 1e-6);
    assert!(room.islands.is_empty(), "{:?}", room.islands);

    let walls = graph.region_at(v(0.1, 3.0)).unwrap();
    assert_ring(&walls.outer, &rectangle(v(0.0, 0.0), v(12.0, 9.0)));
    assert_eq!(walls.islands.len(), 1);
    let mut island = ground_room();
    island.reverse();
    assert_ring(&walls.islands[0], &island);
    assert!((walls.area() - (12.0 * 9.0 - area)).abs() < 1e-6);

    assert_eq!(graph.region_at(v(-1.0, 3.0)), None, "outside the building");
    assert_eq!(graph.region_at(v(13.0, 3.0)), None);
}

/// INP-018: on the upper storey of the shell with a shaft, a point in the
/// shaft gets the rectangle of the shaft walls' inner faces; a point in the
/// room gets the room with two islands, the stair hole of the floor slab and
/// the outer faces of the shaft walls; a point outside gets nothing.
#[test]
fn the_shaft_walls_on_the_upper_storey_are_an_island_of_the_room() {
    let plan = plan_of(&two_storey_shell_with_shaft(), 1);
    let graph = ContourGraph::build(&plan, &ContourOptions::default());

    let inner = rectangle(v(1.0575, 6.5575), v(2.9425, 7.9425));
    let shaft = graph.region_at(v(2.0, 7.25)).unwrap();
    assert_ring(&shaft.outer, &inner);
    assert!(shaft.islands.is_empty(), "{:?}", shaft.islands);
    assert!((shaft.area() - 2.610725).abs() < 1e-6, "{shaft:?}");

    let room = graph.region_at(v(2.0, 3.0)).unwrap();
    assert_ring(&room.outer, &ground_room());
    assert_eq!(room.islands.len(), 2, "{:?}", room.islands);
    let mut stair_hole = rectangle(v(6.0, 1.0), v(9.0, 2.2));
    stair_hole.reverse();
    let mut shaft_outside = rectangle(v(0.9425, 6.4425), v(3.0575, 8.0575));
    shaft_outside.reverse();
    let (hole, outside) = if room.islands[0][0].y < 4.0 { (0, 1) } else { (1, 0) };
    assert_ring(&room.islands[hole], &stair_hole);
    assert_ring(&room.islands[outside], &shaft_outside);
    let area = 11.5 * 8.5 - 0.115 * 5.75 - 3.0 * 1.2 - 2.115 * 1.615;
    assert!((room.area() - area).abs() < 1e-6, "{room:?}");

    // The shaft walls' body: between their outer and inner faces.
    let walls = graph.region_at(v(1.0, 7.25)).unwrap();
    assert_ring(&walls.outer, &rectangle(v(0.9425, 6.4425), v(3.0575, 8.0575)));
    assert_eq!(walls.islands.len(), 1, "{:?}", walls.islands);

    assert_eq!(graph.region_at(v(-1.0, 3.0)), None, "outside the building");
}

/// INP-018, NFR-ACC-003: endpoints 5 µm apart are one vertex, so the square
/// closes; 20 µm apart they are two, and the square stays open.
#[test]
fn a_gap_below_the_tolerance_closes_the_region_and_a_wider_one_does_not() {
    let square = |gap: DVec2| {
        vec![
            [v(0.0, 0.0), v(4.0, 0.0)],
            [v(4.0, 0.0) + gap, v(4.0, 4.0)],
            [v(4.0, 4.0), v(0.0, 4.0)],
            [v(0.0, 4.0), v(0.0, 0.0)],
        ]
    };
    let closed = graph_of(&square(v(3.0, 4.0) * MICROMETRE));
    let region = closed.region_at(v(2.0, 2.0)).expect("5 µm apart: the region closes");
    assert!((region.area() - 16.0).abs() < 1e-4, "{region:?}");
    assert_eq!(region.outer.len(), 4);

    let open = graph_of(&square(v(0.0, 20.0 * MICROMETRE)));
    assert_eq!(open.region_at(v(2.0, 2.0)), None, "20 µm apart: open");

    // A gap within one straight side behaves the same.
    let mut split_side = square(DVec2::ZERO);
    split_side[0] = [v(0.0, 0.0), v(2.0, 0.0)];
    split_side.push([v(2.0 + 5.0 * MICROMETRE, 0.0), v(4.0, 0.0)]);
    let region = graph_of(&split_side).region_at(v(2.0, 2.0)).unwrap();
    assert_eq!(region.outer.len(), 4, "the side's two halves merge: {region:?}");
    split_side[4][0] = v(2.0 + 20.0 * MICROMETRE, 0.0);
    assert_eq!(graph_of(&split_side).region_at(v(2.0, 2.0)), None);
}

/// INP-018: overlapping collinear segments become one set of edges: no
/// duplicate edge and no zero-area face between them.
#[test]
fn collinear_overlapping_segments_are_deduplicated() {
    let graph = graph_of(&[
        [v(0.0, 0.0), v(10.0, 0.0)],
        [v(5.0, 0.0), v(15.0, 0.0)],
        [v(10.0, 0.0), v(0.0, 0.0)],
        [v(2.0, 0.0), v(3.0, 0.0)],
    ]);
    let mut edges: Vec<(f64, f64)> = graph
        .edges()
        .map(|e| {
            let [a, b] = e.points;
            assert!(a.y.abs() < MODEL_TOLERANCE && b.y.abs() < MODEL_TOLERANCE);
            (a.x.min(b.x), a.x.max(b.x))
        })
        .collect();
    edges.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(edges, [(0.0, 2.0), (2.0, 3.0), (3.0, 5.0), (5.0, 10.0), (10.0, 15.0)]);
    assert!(graph.regions().is_empty(), "a line encloses nothing");

    // A square whose sides are each drawn twice, in overlapping halves.
    let corners = rectangle(v(0.0, 0.0), v(4.0, 4.0));
    let mut segments = ring_segments(&corners);
    for [a, b] in ring_segments(&corners) {
        segments.push([a, a.lerp(b, 0.75)]);
        segments.push([b, a.lerp(b, 0.25)]);
    }
    let graph = graph_of(&segments);
    let regions = graph.regions();
    assert_eq!(regions.len(), 1, "{regions:?}");
    assert_ring(&regions[0].outer, &corners);
    assert_eq!(graph.edges().count(), 12, "every side in three pieces");
}

/// INP-018, NFR-ACC-003: two 100 m segments crossing in their middles at
/// 0.0003°, parallel within `ANGLE_TOLERANCE`, are split at the crossing,
/// although neither has an end on the other (their ends are 0.26 mm apart);
/// with their ends joined, the two thin triangles they bound are the regions.
#[test]
fn segments_crossing_at_a_very_shallow_angle_are_split_at_the_crossing() {
    let angle = 0.0003_f64.to_radians();
    assert!(angle.sin() < ANGLE_TOLERANCE, "parallel within the angle tolerance");
    let (centre, heading) = (v(20.0, 30.0), 0.7);
    let half = |turn: f64| DVec2::from_angle(heading + turn) * 50.0;
    let (a, b) = (half(-angle / 2.0), half(angle / 2.0));
    let [a0, a1, b0, b1] = [centre - a, centre + a, centre - b, centre + b];
    assert!(a1.distance(b1) > 20.0 * MODEL_TOLERANCE, "no end on the other segment");

    let crossing = graph_of(&[[a0, a1], [b0, b1]]);
    let edges: Vec<Segment> = crossing.edges().collect();
    assert_eq!(edges.len(), 4, "each segment in two halves: {edges:?}");
    for edge in &edges {
        let [p, q] = edge.points;
        assert!(
            p.distance(centre) < MODEL_TOLERANCE || q.distance(centre) < MODEL_TOLERANCE,
            "{edge:?} does not end at the crossing {centre}"
        );
    }

    let graph = graph_of(&[[a0, a1], [b0, b1], [a0, b0], [a1, b1]]);
    assert_eq!(graph.regions().len(), 2, "{:?}", graph.regions());
    let along = DVec2::from_angle(heading) * 25.0;
    let area = 0.5 * 50.0 * 50.0 * angle.sin();
    for (probe, triangle) in [(centre + along, [centre, a1, b1]), (centre - along, [centre, a0, b0])] {
        let region = graph.region_at(probe).unwrap_or_else(|| panic!("no region at {probe}"));
        assert_ring(&region.outer, &triangle);
        assert!(region.islands.is_empty(), "{region:?}");
        assert!((region.area() - area).abs() < 1e-9, "{} is not {area}", region.area());
    }
    assert_eq!(
        graph.region_at(centre + along.perp() * 1e-4),
        None,
        "beside the triangles"
    );
}

/// The distance from `p` to the nearest point of the closed ring `ring`.
fn distance_to_ring(ring: &[DVec2], p: DVec2) -> f64 {
    (0..ring.len())
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            let t = ((p - a).dot(b - a) / a.distance_squared(b)).clamp(0.0, 1.0);
            p.distance(a.lerp(b, t))
        })
        .fold(f64::INFINITY, f64::min)
}

/// The piece angle at which a tessellation of a circle of radius `radius`
/// deviates by `deviation` from the circle between its vertices.
fn piece_angle(radius: f64, deviation: f64) -> f64 {
    2.0 * (1.0 - deviation / radius).acos()
}

/// INP-018, NFR-ACC-003: a sector of a 5 m circle, a 64-gon of its centre
/// and 63 vertices of a tessellated arc whose pieces deviate from the circle
/// by less than the tolerance, keeps every vertex it leaves out within
/// `MODEL_TOLERANCE` of its boundary, and still leaves vertices out where the
/// tessellation is finer than the tolerance needs.
#[test]
fn a_finely_tessellated_arc_simplifies_within_the_tolerance() {
    let (centre, radius) = (v(1.0, 2.0), 5.0);
    for fraction in [1.0 / 256.0, 1.0 / 64.0, 1.0 / 16.0, 1.0 / 4.0, 0.99] {
        let step = piece_angle(radius, fraction * MODEL_TOLERANCE);
        let mut sector: Vec<DVec2> = (0..63)
            .map(|k| centre + DVec2::from_angle(1.2 + k as f64 * step) * radius)
            .collect();
        sector.push(centre);
        let inside = centre + DVec2::from_angle(1.2 + 31.0 * step) * 4.0;
        let region = graph_of(&ring_segments(&sector))
            .region_at(inside)
            .unwrap_or_else(|| panic!("no region at {inside} for {fraction}"));
        for (k, p) in sector.iter().enumerate() {
            let distance = distance_to_ring(&region.outer, *p);
            assert!(
                distance < MODEL_TOLERANCE,
                "deviation {fraction}: vertex {k} is {distance} m off {:?}",
                region.outer
            );
        }
        if fraction <= 1.0 / 16.0 {
            assert!(region.outer.len() < sector.len(), "{fraction}: nothing left out");
        }
    }
}

/// INP-018, NFR-ACC-003: a 5 m circle tessellated so finely that it deviates
/// from the circle by a sixteenth of the tolerance, a ring without corners,
/// simplifies within the tolerance, and the island it makes of a square
/// around it simplifies to the same points, in reverse.
#[test]
fn a_finely_tessellated_circle_and_its_island_simplify_alike() {
    let (centre, radius) = (v(-3.0, 4.0), 5.0);
    let n = (std::f64::consts::TAU / piece_angle(radius, MODEL_TOLERANCE / 16.0)).ceil() as usize;
    let circle: Vec<DVec2> = (0..n)
        .map(|k| centre + DVec2::from_angle(0.3 + k as f64 * std::f64::consts::TAU / n as f64) * radius)
        .collect();
    let mut segments = ring_segments(&circle);
    segments.extend(ring_segments(&rectangle(centre - v(6.0, 6.0), centre + v(6.0, 6.0))));
    let graph = graph_of(&segments);

    let disc = graph.region_at(centre).unwrap();
    assert!(disc.outer.len() < n, "nothing left out of {n}");
    for (k, p) in circle.iter().enumerate() {
        let distance = distance_to_ring(&disc.outer, *p);
        assert!(distance < MODEL_TOLERANCE, "vertex {k} of {n} is {distance} m off");
    }

    let room = graph.region_at(centre + v(5.5, 5.5)).unwrap();
    assert_eq!(room.islands.len(), 1, "{room:?}");
    let mut island = room.islands[0].clone();
    island.reverse();
    assert_ring(&island, &disc.outer);
}

/// INP-018: lines that end inside a region (antennas) are not part of its
/// boundary, and a free-standing line encloses nothing, so it is no island;
/// a closed loop inside is one, with its own antenna removed.
#[test]
fn antennas_are_removed_and_closed_components_become_islands() {
    let corners = rectangle(v(0.0, 0.0), v(6.0, 4.0));
    let island = rectangle(v(3.0, 1.0), v(4.0, 2.0));
    let mut segments = ring_segments(&corners);
    segments.extend(ring_segments(&island));
    segments.extend([
        // From the south side into the room, with a branch.
        [v(1.0, 0.0), v(1.0, 2.0)],
        [v(1.0, 2.0), v(1.5, 2.5)],
        // Free-standing.
        [v(0.5, 3.0), v(2.0, 3.5)],
        // From the island into the room.
        [v(4.0, 2.0), v(5.0, 3.0)],
    ]);
    let graph = graph_of(&segments);
    let region = graph.region_at(v(2.0, 1.0)).unwrap();
    assert_ring(&region.outer, &corners);
    assert_eq!(region.islands.len(), 1, "{region:?}");
    let mut hole = island.clone();
    hole.reverse();
    assert_ring(&region.islands[0], &hole);
    assert!((region.area() - 23.0).abs() < 1e-9);
    assert_ring(&graph.region_at(v(3.5, 1.5)).unwrap().outer, &island);
}

/// Asserts that no ring of `region` runs along an edge twice, in either
/// direction: no ring is joined to itself or another by a bridge.
fn assert_no_edge_twice(region: &Region) {
    let mut seen: Vec<[[u64; 2]; 2]> = Vec::new();
    for ring in std::iter::once(&region.outer).chain(&region.islands) {
        for i in 0..ring.len() {
            let [a, b] = [ring[i], ring[(i + 1) % ring.len()]].map(|p| [p.x.to_bits(), p.y.to_bits()]);
            let key = if a < b { [a, b] } else { [b, a] };
            assert!(!seen.contains(&key), "{key:?} twice in {region:?}");
            seen.push(key);
        }
    }
}

/// INP-018: a line that joins the outer boundary of a region to a closed
/// loop inside it (a bridge) is not part of the region's boundary: the
/// region keeps its outer boundary, and the loop is an island, as if the
/// line were not there. The loop's own region stays as it is.
#[test]
fn a_bridge_to_a_loop_inside_a_region_leaves_the_loop_an_island() {
    let corners = rectangle(v(0.0, 0.0), v(6.0, 4.0));
    let inner = rectangle(v(2.0, 1.0), v(4.0, 3.0));
    let mut hole = inner.clone();
    hole.reverse();
    // A bridge of one edge, of two collinear ones, of two with a corner and
    // of one with an antenna branching off it.
    let bridges: [&[[DVec2; 2]]; 4] = [
        &[[v(0.0, 2.0), v(2.0, 2.0)]],
        &[[v(0.0, 2.0), v(1.0, 2.0)], [v(1.0, 2.0), v(2.0, 2.0)]],
        &[[v(3.0, 0.0), v(3.0, 0.5)], [v(3.0, 0.5), v(2.0, 1.0)]],
        &[[v(6.0, 2.0), v(4.0, 2.0)], [v(5.0, 2.0), v(5.0, 3.0)]],
    ];
    for bridge in bridges {
        let mut segments = ring_segments(&corners);
        segments.extend(ring_segments(&inner));
        segments.extend_from_slice(bridge);
        let graph = graph_of(&segments);
        let region = graph.region_at(v(0.5, 0.5)).unwrap();
        assert_ring(&region.outer, &corners);
        assert_eq!(region.islands.len(), 1, "{bridge:?}: {region:?}");
        assert_ring(&region.islands[0], &hole);
        assert!((region.area() - 20.0).abs() < 1e-9, "{bridge:?}: {region:?}");
        let loop_region = graph.region_at(v(2.5, 2.5)).unwrap();
        assert_ring(&loop_region.outer, &inner);
        assert!(loop_region.islands.is_empty());
        assert_eq!(graph.regions().len(), 2, "{bridge:?}");
        graph.regions().iter().for_each(assert_no_edge_twice);
    }
}

/// INP-018: two loops joined by a line inside a region, and a loop inside a
/// loop joined to it and to a third loop, are islands of the region each,
/// and every loop keeps its own region.
#[test]
fn loops_joined_by_bridges_are_separate_islands() {
    let room = rectangle(v(0.0, 0.0), v(10.0, 6.0));
    let west = rectangle(v(1.0, 1.0), v(3.0, 3.0));
    let east = rectangle(v(5.0, 1.0), v(7.0, 3.0));
    let mut segments = ring_segments(&room);
    segments.extend(ring_segments(&west));
    segments.extend(ring_segments(&east));
    segments.push([v(3.0, 2.0), v(5.0, 2.0)]);
    let graph = graph_of(&segments);
    let region = graph.region_at(v(0.5, 0.5)).unwrap();
    assert_ring(&region.outer, &room);
    assert_eq!(region.islands.len(), 2, "{region:?}");
    for (island, expected) in region.islands.iter().zip(if region.islands[0][0].x < 4.0 {
        [&west, &east]
    } else {
        [&east, &west]
    }) {
        let mut hole = expected.clone();
        hole.reverse();
        assert_ring(island, &hole);
    }
    assert!((region.area() - 52.0).abs() < 1e-9, "{region:?}");
    assert_ring(&graph.region_at(v(2.0, 2.5)).unwrap().outer, &west);
    assert_ring(&graph.region_at(v(6.0, 2.5)).unwrap().outer, &east);
    graph.regions().iter().for_each(assert_no_edge_twice);

    // A loop inside the west loop, bridged to it, and the west loop bridged
    // to the room's boundary.
    let core = rectangle(v(1.5, 1.5), v(2.5, 2.5));
    segments.extend(ring_segments(&core));
    segments.extend([[v(1.0, 2.0), v(1.5, 2.0)], [v(0.0, 2.0), v(1.0, 2.0)]]);
    let graph = graph_of(&segments);
    let region = graph.region_at(v(0.5, 0.5)).unwrap();
    assert_ring(&region.outer, &room);
    assert_eq!(region.islands.len(), 2, "{region:?}");
    let ring = graph.region_at(v(1.2, 1.2)).unwrap();
    assert_ring(&ring.outer, &west);
    assert_eq!(ring.islands.len(), 1, "{ring:?}");
    let mut hole = core.clone();
    hole.reverse();
    assert_ring(&ring.islands[0], &hole);
    assert!((ring.area() - 3.0).abs() < 1e-9, "{ring:?}");
    assert_ring(&graph.region_at(v(2.0, 2.2)).unwrap().outer, &core);
    graph.regions().iter().for_each(assert_no_edge_twice);
}

/// INP-018: with reference lines on, the T wall's centred reference line
/// (5, 0)-(5, 6) of the two-storey shell's ground storey joins the outer
/// faces of the exterior walls to the room's boundary. The wall body is
/// still the region between them, with the room's boundary as an island,
/// and no region runs along an edge twice.
#[test]
fn a_reference_line_across_the_wall_body_keeps_the_room_an_island() {
    let plan = plan_of(&two_storey_shell(), 0);
    let graph = ContourGraph::build(&plan, &ContourOptions { reference_lines: true });

    let walls = graph.region_at(v(0.1, 3.0)).unwrap();
    assert_ring(&walls.outer, &rectangle(v(0.0, 0.0), v(12.0, 9.0)));
    assert_eq!(walls.islands.len(), 1, "{walls:?}");
    let mut island = ground_room();
    island.reverse();
    assert_ring(&walls.islands[0], &island);
    let room = 11.5 * 8.5 - 0.115 * 5.75;
    assert!((walls.area() - (12.0 * 9.0 - room)).abs() < 1e-6, "{walls:?}");

    assert_ring(&graph.region_at(v(2.0, 3.0)).unwrap().outer, &ground_room());
    graph.regions().iter().for_each(assert_no_edge_twice);
}

/// A storey with a 10 x 8 m ring of exterior walls and two 0.115 m interior
/// walls, centred on x = 5 and y = 4, crossing each other.
fn crossing_walls() -> Project {
    let mut b = FixtureBuilder::new("crossing-walls").titled("Crossing walls");
    let ground = b.storey(0);
    let corners = rectangle(v(0.0, 0.0), v(10.0, 8.0));
    for [start, end] in ring_segments(&corners) {
        b.wall(ground, start, end);
    }
    b.commit();
    for [start, end] in [[v(5.0, 0.0), v(5.0, 8.0)], [v(0.0, 4.0), v(10.0, 4.0)]] {
        let wall = b.wall(ground, start, end);
        b.edit(wall, |e| {
            e.position = Position::Interior;
            if let Some(wall) = e.as_wall_mut() {
                wall.thickness = 0.115;
                wall.reference_line = ReferenceLine::Center;
            }
        });
    }
    b.finish()
}

/// INP-018: two crossing walls divide the room inside a ring of walls into
/// four regions, one per quadrant.
#[test]
fn two_crossing_walls_give_four_regions() {
    let plan = plan_of(&crossing_walls(), 0);
    let graph = ContourGraph::build(&plan, &ContourOptions::default());
    let (west, east) = ((0.25, 4.9425), (5.0575, 9.75));
    let (south, north) = ((0.25, 3.9425), (4.0575, 7.75));
    for ((x0, x1), (y0, y1)) in [(west, south), (east, south), (west, north), (east, north)] {
        let centre = v(x0 + x1, y0 + y1) / 2.0;
        let region = graph.region_at(centre).unwrap();
        assert_ring(&region.outer, &rectangle(v(x0, y0), v(x1, y1)));
        assert!(region.islands.is_empty());
    }
    // The quadrants and the wall body around them, nothing else.
    let quadrant = (4.9425 - 0.25) * (3.9425 - 0.25);
    let areas: Vec<f64> = graph.regions().iter().map(Region::area).collect();
    assert_eq!(areas.len(), 5, "{areas:?}");
    let rooms = areas.iter().filter(|a| (*a - quadrant).abs() < 1e-6).count();
    assert_eq!(rooms, 4, "{areas:?}");

    // The same with single lines, which cross each other and the square's
    // sides and overshoot them.
    let mut segments = ring_segments(&rectangle(v(0.0, 0.0), v(4.0, 4.0)));
    segments.extend([[v(-1.0, 1.5), v(5.0, 1.5)], [v(2.5, 5.0), v(2.5, -1.0)]]);
    let graph = graph_of(&segments);
    let (west, east) = ((0.0, 2.5), (2.5, 4.0));
    let (south, north) = ((0.0, 1.5), (1.5, 4.0));
    for ((x0, x1), (y0, y1)) in [(west, south), (east, south), (west, north), (east, north)] {
        let region = graph.region_at(v(x0 + x1, y0 + y1) / 2.0).unwrap();
        assert_ring(&region.outer, &rectangle(v(x0, y0), v(x1, y1)));
    }
    assert_eq!(graph.regions().len(), 4);
}

/// INP-018: a slab outline on the outer faces of the walls (the base slab of
/// the two-storey shell) or on their inner faces adds no face of its own:
/// the coinciding lines are one edge.
#[test]
fn slab_outlines_on_wall_faces_create_no_zero_area_faces() {
    let plan = plan_of(&two_storey_shell(), 0);
    let graph = ContourGraph::build(&plan, &ContourOptions::default());
    let mut areas: Vec<f64> = graph.regions().iter().map(Region::area).collect();
    areas.sort_by(f64::total_cmp);
    let room = 11.5 * 8.5 - 0.115 * 5.75;
    assert_eq!(areas.len(), 2, "the wall body and the room: {areas:?}");
    assert!((areas[0] - (108.0 - room)).abs() < 1e-6, "{areas:?}");
    assert!((areas[1] - room).abs() < 1e-6, "{areas:?}");

    let mut b = FixtureBuilder::new("inner-slab").titled("Slab on the inner faces");
    let ground = b.storey(0);
    for [start, end] in ring_segments(&rectangle(v(0.0, 0.0), v(8.0, 6.0))) {
        b.wall(ground, start, end);
    }
    b.commit();
    b.slab(ground, rectangle(v(0.25, 0.25), v(7.75, 5.75)), Vec::new());
    let plan = plan_of(&b.finish(), 0);
    let graph = ContourGraph::build(&plan, &ContourOptions::default());
    let mut areas: Vec<f64> = graph.regions().iter().map(Region::area).collect();
    areas.sort_by(f64::total_cmp);
    assert_eq!(areas.len(), 2, "{areas:?}");
    assert!((areas[0] - (48.0 - 41.25)).abs() < 1e-6, "{areas:?}");
    assert!((areas[1] - 41.25).abs() < 1e-6, "{areas:?}");
}

/// INP-018: on the ground storey of the two-storey shell, the clicked edge
/// on the interior face of the south wall runs from the room's corner to the
/// face of the interior T wall; the chain through it is the room's closed
/// boundary of eight corners, the same from whichever of its edges it is
/// picked.
#[test]
fn the_clicked_edge_and_its_chain_on_the_ground_storey() {
    let plan = plan_of(&two_storey_shell(), 0);
    let graph = ContourGraph::build(&plan, &ContourOptions::default());

    let edge = graph.edge_at(v(2.0, 0.25), PICK_RADIUS).unwrap();
    assert_chain(&edge, &[v(0.25, 0.25), v(4.9425, 0.25)], false);

    let chain = graph.chain_at(v(2.0, 0.25), PICK_RADIUS).unwrap();
    assert_chain(&chain, &ground_room(), true);

    // The west face of the T wall, picked from a point beside it.
    let edge = graph.edge_at(v(4.93, 3.0), PICK_RADIUS).unwrap();
    assert_chain(&edge, &[v(4.9425, 0.25), v(4.9425, 6.0)], false);
    assert_eq!(graph.chain_at(v(4.93, 3.0), PICK_RADIUS), Some(chain));
}

/// INP-018: on the upper storey, the clicked edge on the projected stair-hole
/// loop is one side of it and the chain through it the closed loop of four
/// corners; on the free-standing shaft walls, the chains are their inner and
/// outer faces.
#[test]
fn the_clicked_edge_and_its_chain_on_the_upper_storey() {
    let plan = plan_of(&two_storey_shell(), 1);
    let graph = ContourGraph::build(&plan, &ContourOptions::default());
    let stair_hole = rectangle(v(6.0, 1.0), v(9.0, 2.2));
    let edge = graph.edge_at(v(7.5, 1.02), PICK_RADIUS).unwrap();
    assert_chain(&edge, &[v(6.0, 1.0), v(9.0, 1.0)], false);
    let edge = graph.edge_at(v(9.0, 1.6), PICK_RADIUS).unwrap();
    assert_chain(&edge, &[v(9.0, 1.0), v(9.0, 2.2)], false);
    let chain = graph.chain_at(v(7.5, 1.02), PICK_RADIUS).unwrap();
    assert_chain(&chain, &stair_hole, true);

    let plan = plan_of(&two_storey_shell_with_shaft(), 1);
    let graph = ContourGraph::build(&plan, &ContourOptions::default());
    let inner = rectangle(v(1.0575, 6.5575), v(2.9425, 7.9425));
    let outer = rectangle(v(0.9425, 6.4425), v(3.0575, 8.0575));
    assert_chain(&graph.chain_at(v(1.07, 7.0), PICK_RADIUS).unwrap(), &inner, true);
    assert_chain(&graph.chain_at(v(0.93, 7.0), PICK_RADIUS).unwrap(), &outer, true);
    let edge = graph.edge_at(v(2.0, 7.95), PICK_RADIUS).unwrap();
    assert_chain(&edge, &[v(1.0575, 7.9425), v(2.9425, 7.9425)], false);
    assert_chain(&graph.chain_at(v(7.5, 1.02), PICK_RADIUS).unwrap(), &stair_hole, true);
}

/// INP-018: a chain runs on through every vertex where two edges meet,
/// merging collinear edges, and ends at a free end or where more than two
/// edges meet; the clicked edge ends at the first corner as well. An open
/// chain runs from its lexically first end.
#[test]
fn chains_run_through_vertices_of_degree_two_and_edges_end_at_corners() {
    let graph = graph_of(&[
        // Drawn in pieces and in both directions.
        [v(0.0, 0.0), v(1.0, 0.0)],
        [v(2.0, 0.0), v(1.0, 0.0)],
        [v(2.0, 0.0), v(2.0, 1.0)],
        [v(2.0, 2.0), v(2.0, 1.0)],
        [v(3.0, 3.0), v(2.0, 2.0)],
        // A junction at (3, 3).
        [v(3.0, 3.0), v(5.0, 3.0)],
        [v(3.0, 3.0), v(3.0, 5.0)],
    ]);
    let path = [v(0.0, 0.0), v(2.0, 0.0), v(2.0, 2.0), v(3.0, 3.0)];
    for p in [v(0.5, 0.0), v(1.5, 0.1), v(2.0, 1.5), v(2.6, 2.5)] {
        assert_chain(&graph.chain_at(p, PICK_RADIUS).unwrap(), &path, false);
    }
    assert_chain(&graph.edge_at(v(0.5, 0.0), PICK_RADIUS).unwrap(), &path[..2], false);
    assert_chain(&graph.edge_at(v(1.5, 0.1), PICK_RADIUS).unwrap(), &path[..2], false);
    assert_chain(&graph.edge_at(v(2.0, 0.5), PICK_RADIUS).unwrap(), &path[1..3], false);
    assert_chain(&graph.edge_at(v(2.6, 2.5), PICK_RADIUS).unwrap(), &path[2..], false);

    let beyond = graph.chain_at(v(4.0, 3.1), PICK_RADIUS).unwrap();
    assert_chain(&beyond, &[v(3.0, 3.0), v(5.0, 3.0)], false);
    assert_eq!(graph.edge_at(v(4.0, 3.1), PICK_RADIUS), Some(beyond));
}

/// INP-018: a closed chain runs counter-clockwise from its lexically first
/// corner, however its segments were drawn and whichever of its edges is
/// picked, also when it returns to its start at a junction; a sliver keeps
/// its corners.
#[test]
fn closed_chains_run_counter_clockwise_from_their_first_corner() {
    // Clockwise, with a straight vertex on the east side and an antenna at
    // the north-east corner.
    let clockwise = [v(0.0, 0.0), v(0.0, 4.0), v(4.0, 4.0), v(4.0, 2.0), v(4.0, 0.0)];
    let mut segments = ring_segments(&clockwise);
    segments.push([v(4.0, 4.0), v(6.0, 6.0)]);
    let mut reversed = segments.clone();
    reversed.reverse();
    let square = rectangle(v(0.0, 0.0), v(4.0, 4.0));
    for graph in [graph_of(&segments), graph_of(&reversed)] {
        for p in [v(2.0, 0.0), v(4.1, 1.0), v(4.0, 3.0), v(2.0, 3.9), v(0.0, 2.0)] {
            assert_chain(&graph.chain_at(p, PICK_RADIUS).unwrap(), &square, true);
        }
        let east = graph.edge_at(v(4.1, 1.0), PICK_RADIUS).unwrap();
        assert_chain(&east, &[v(4.0, 0.0), v(4.0, 4.0)], false);
        let antenna = graph.chain_at(v(5.0, 5.0), PICK_RADIUS).unwrap();
        assert_chain(&antenna, &[v(4.0, 4.0), v(6.0, 6.0)], false);
    }

    // 10 m long and 20 µm high: its apex turns by less than the angle
    // tolerance, but lies off the line between its neighbours.
    let sliver = [v(0.0, 0.0), v(10.0, 0.0), v(5.0, 20.0 * MICROMETRE)];
    let chain = graph_of(&ring_segments(&sliver)).chain_at(v(5.0, -0.01), PICK_RADIUS);
    assert_chain(&chain.unwrap(), &sliver, true);
}

/// INP-018: a pick takes the edge nearest to the clicked point within the
/// radius, and nothing beyond it.
#[test]
fn the_nearest_edge_within_the_radius_is_picked() {
    let south = [v(0.0, 0.0), v(4.0, 0.0)];
    let north = [v(0.0, 1.0), v(4.0, 1.0)];
    let graph = graph_of(&[south, north]);
    assert_chain(&graph.edge_at(v(2.0, 0.3), 0.5).unwrap(), &south, false);
    assert_chain(&graph.chain_at(v(2.0, 0.6), 0.5).unwrap(), &north, false);
    assert_eq!(graph.edge_at(v(2.0, 0.3), 0.2), None, "beyond the radius");
    assert_eq!(graph.chain_at(v(2.0, 0.3), 0.2), None);
    // Past the end of an edge, the distance is to its end.
    assert_eq!(graph.edge_at(v(4.4, 0.0), 0.3), None);
    assert!(graph.edge_at(v(4.2, 0.0), 0.3).is_some());
    assert!(graph.edge_at(v(2.0, 0.0), 0.0).is_some(), "on the edge");
    assert_eq!(graph.edge_at(v(2.0, 0.0), -1.0), None, "a negative radius");
    assert_eq!(graph_of(&[]).edge_at(v(0.0, 0.0), 1.0), None, "no edges");
}

/// INP-018: the reference lines of walls are graph input only when the
/// options ask for them.
#[test]
fn wall_reference_lines_are_included_on_request() {
    let plan = plan_of(&two_storey_shell(), 0);
    let is_t_wall_line = |s: &Segment| s.points == [v(5.0, 0.0), v(5.0, 6.0)];
    let without = plan.segments(&ContourOptions::default());
    assert!(!without.iter().any(is_t_wall_line));
    let with = plan.segments(&ContourOptions { reference_lines: true });
    assert!(with.iter().any(is_t_wall_line));
    assert_eq!(with.len(), without.len() + 5, "one per wall");
}

/// INP-018: the graph does not depend on the order or the direction in which
/// the segments arrive.
#[test]
fn segments_in_reversed_order_give_identical_regions_and_chains() {
    let plan = plan_of(&crossing_walls(), 0);
    let options = ContourOptions { reference_lines: true };
    let segments = plan.segments(&options);
    let mut reversed = segments.clone();
    reversed.reverse();
    let mut flipped = segments.clone();
    for s in &mut flipped {
        s.points.reverse();
    }
    let graphs = [&segments, &reversed, &flipped].map(|s| ContourGraph::build(s.as_slice(), &options));
    let probes = [v(2.0, 2.0), v(7.0, 6.0), v(0.1, 3.0), v(5.01, 4.02), v(-1.0, 0.0)];
    for graph in &graphs[1..] {
        assert_eq!(graph.regions(), graphs[0].regions());
        for p in probes {
            assert_eq!(graph.region_at(p), graphs[0].region_at(p), "at {p}");
            assert_eq!(graph.edge_at(p, 1.0), graphs[0].edge_at(p, 1.0), "at {p}");
            assert_eq!(graph.chain_at(p, 1.0), graphs[0].chain_at(p, 1.0), "at {p}");
        }
    }
    assert!(graphs[0].region_at(v(2.0, 2.0)).is_some());
    assert!(graphs[0].chain_at(v(5.01, 4.02), 1.0).is_some());
}

/// A small linear congruential generator, so the random segments are the
/// same on every run.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// INP-018: 10,000 random segments, many of them crossing, touching or
/// nearly coinciding, build into a graph without panicking, and its queries
/// answer.
#[test]
fn ten_thousand_random_segments_build_without_panic() {
    let mut rng = Lcg(0x5eed);
    let mut segments: Vec<Segment> = Vec::new();
    while segments.len() < 10_000 {
        let n = segments.len();
        let random = v(rng.next() * 100.0, rng.next() * 100.0);
        let length = 2.0 + 2.0 * rng.next().round();
        let [a, b] = match n % 4 {
            // Axis-parallel, as walls mostly are, starting on a 2 m grid, so
            // that many overlap or meet end to end.
            0 => {
                let a = v((random.x / 2.0).round() * 2.0, random.y);
                [a, a + v(length, 0.0)]
            }
            1 => {
                let a = v(random.x, (random.y / 2.0).round() * 2.0);
                [a, a + v(0.0, length)]
            }
            // Starting within a few micrometres of the previous end.
            2 => {
                let [_, end] = segments[n - 1].points;
                let a = end + v(rng.next(), rng.next()) * 3.0 * MICROMETRE;
                [a, a + DVec2::from_angle(rng.next() * std::f64::consts::TAU)]
            }
            // Zero-length ones among them.
            _ if n % 100 == 3 => [random, random],
            _ => [
                random,
                random + DVec2::from_angle(rng.next() * std::f64::consts::TAU) * length,
            ],
        };
        segments.push(Segment::new(a, b));
    }
    let graph = ContourGraph::build(segments.as_slice(), &ContourOptions::default());
    assert!(graph.edges().count() > 0);
    let regions = graph.regions();
    assert!(regions.iter().all(|r| r.area() > 0.0));
    regions.iter().for_each(assert_no_edge_twice);
    for k in 0..100 {
        let p = v(k as f64, (k * 37 % 100) as f64 + 0.5);
        if let Some(region) = graph.region_at(p) {
            assert!(region.area() > 0.0);
        }
        for chain in [graph.edge_at(p, 1.0), graph.chain_at(p, 1.0)].into_iter().flatten() {
            assert!(chain.points.len() >= if chain.closed { 3 } else { 2 }, "{chain:?}");
        }
    }
}
