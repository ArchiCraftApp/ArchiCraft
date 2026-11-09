//! The space generator (SPC-001): the boundary with its holes extruded
//! between the space's base and its storey-linked top, one closed body
//! without a material.

#![allow(clippy::unwrap_used)]

use archicraft_geometry::{ModelGeometry, ViewRange, plan_drawing};
use archicraft_model::glam::DVec2;
use archicraft_model::template::default_metric_project;
use archicraft_model::{Command, Element, History, TopLink};

fn points(coordinates: &[(f64, f64)]) -> Vec<DVec2> {
    coordinates.iter().map(|&(x, y)| DVec2::new(x, y)).collect()
}

/// SPC-001, IFX-032: the L-shaped 6 m x 4 m space without its 2 m x 2 m
/// notch and with a 1 m x 1 m hole has one closed, outward-oriented body
/// without a material, whose volume is the net area times the resolved
/// height: 19 m² x 3 m with the default top at the next storey, and
/// 19 m² x 2.35 m with the base 0.15 m up and a fixed height.
#[test]
fn a_space_body_is_closed_and_holds_net_area_times_height() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let g = p.storeys.ground();
    let l_shape = points(&[(0.0, 0.0), (6.0, 0.0), (6.0, 2.0), (4.0, 2.0), (4.0, 4.0), (0.0, 4.0)]);
    let hole = points(&[(1.0, 1.0), (2.0, 1.0), (2.0, 2.0), (1.0, 2.0)]);
    let linked = p.new_space(g, l_shape.clone(), vec![hole.clone()], 0);
    let mut fixed = p.new_space(g, l_shape, vec![hole], 1);
    let link = &mut fixed.as_space_mut().unwrap().link;
    link.base_offset = 0.15;
    link.top = TopLink::Fixed { height: 2.35 };
    let ids = [linked.id, fixed.id];
    h.execute(&mut p, Command::AddElements(vec![linked, fixed])).unwrap();
    let geometry = ModelGeometry::evaluate(&p);
    assert!(geometry.problems.is_empty(), "{:?}", geometry.problems);

    for id in ids {
        let element = p.element(id).unwrap();
        let space = Element::as_space(element).unwrap();
        assert!((space.net_area() - 19.0).abs() < 1e-12, "{}", space.net_area());
        let (bottom, top) = p.vertical_extent(element).unwrap();
        let body = geometry.get(id).unwrap();
        assert_eq!(body.parts.len(), 1, "{}", element.name);
        assert!(body.children.is_empty() && body.axis.is_empty());
        let part = &body.parts[0];
        assert_eq!((part.name.as_str(), part.material), ("Body", None));
        assert_eq!(part.mesh.check_closed(), Ok(()), "{}", element.name);
        assert!(part.mesh.signed_volume() > 0.0, "outward-oriented");
        let expected = space.net_area() * (top - bottom);
        let relative = (body.volume() - expected).abs() / expected;
        assert!(relative < 1e-9, "{}: {} is not {expected}", element.name, body.volume());
        let (lo, hi) = body.bounds().unwrap();
        assert!((lo.z - bottom).abs() < 1e-12 && (hi.z - top).abs() < 1e-12, "{lo} {hi}");
    }
}

/// SPC-001, INP-018: a space is no construction element, so its body adds
/// nothing to the plan: neither cut nor projected nor overhead lines, also
/// when it lies wholly below or above the cut plane.
#[test]
fn a_space_draws_no_body_lines_in_plan() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let g = p.storeys.ground();
    let square = points(&[(0.0, 0.0), (3.0, 0.0), (3.0, 3.0), (0.0, 3.0)]);
    let tall = p.new_space(g, square.clone(), Vec::new(), 0);
    let mut low = p.new_space(g, square.clone(), Vec::new(), 1);
    low.as_space_mut().unwrap().link.top = TopLink::Fixed { height: 0.5 };
    let mut high = p.new_space(g, square, Vec::new(), 2);
    high.as_space_mut().unwrap().link.base_offset = 2.0;
    let ids = [tall.id, low.id, high.id];
    h.execute(&mut p, Command::AddElements(vec![tall, low, high])).unwrap();
    let geometry = ModelGeometry::evaluate(&p);
    assert!(geometry.problems.is_empty(), "{:?}", geometry.problems);
    let plan = plan_drawing(&p, &geometry, g, ViewRange::for_storey(&p, g), |_| true);
    for id in ids {
        let element = plan.element(id).unwrap();
        assert!(element.segments().is_empty(), "{:?}", element);
        assert!(element.cut.is_empty() && element.reference_line.is_none());
    }
    assert!(plan.cut_regions.is_empty());
}

/// The L-shaped 6 m x 4 m boundary without its 2 m x 2 m notch (20 m²).
fn l_shape() -> Vec<DVec2> {
    points(&[(0.0, 0.0), (6.0, 0.0), (6.0, 2.0), (4.0, 2.0), (4.0, 4.0), (0.0, 4.0)])
}

/// Whether `p` lies strictly inside the ring `ring` (even-odd rule).
fn inside(ring: &[DVec2], p: DVec2) -> bool {
    let mut inside = false;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}

/// Adds the spaces `spaces` (boundary, long name, number) on the ground
/// storey and returns the ground storey's plan and the spaces' ids.
fn plan_with_spaces(
    p: &mut archicraft_model::Project,
    spaces: Vec<(Vec<DVec2>, &str, &str)>,
) -> (archicraft_geometry::PlanDrawing, Vec<archicraft_model::ElementId>) {
    let g = p.storeys.ground();
    let elements: Vec<Element> = spaces
        .into_iter()
        .enumerate()
        .map(|(i, (boundary, name, number))| {
            let mut e = p.new_space(g, boundary, Vec::new(), i as u32);
            let space = e.as_space_mut().unwrap();
            space.long_name = name.to_owned();
            space.number = number.to_owned();
            e
        })
        .collect();
    let ids = elements.iter().map(|e| e.id).collect();
    History::default().execute(p, Command::AddElements(elements)).unwrap();
    let geometry = ModelGeometry::evaluate(p);
    assert!(geometry.problems.is_empty(), "{:?}", geometry.problems);
    (
        plan_drawing(p, &geometry, g, ViewRange::for_storey(p, g), |_| true),
        ids,
    )
}

/// SPC-001: the plan stamp's anchor is the boundary's interior point, which
/// lies inside the L-shaped boundary and inside a C-shaped one whose
/// centroid lies in its notch; the stamp offset moves it.
#[test]
fn the_stamp_anchor_lies_inside_concave_boundaries() {
    let mut p = default_metric_project();
    let c_shape = points(&[
        (0.0, 0.0),
        (6.0, 0.0),
        (6.0, 1.0),
        (1.0, 1.0),
        (1.0, 5.0),
        (6.0, 5.0),
        (6.0, 6.0),
        (0.0, 6.0),
    ]);
    let (plan, ids) = plan_with_spaces(&mut p, vec![(l_shape(), "", ""), (c_shape.clone(), "", "")]);
    assert_eq!(plan.stamps.len(), 2);
    for (id, ring) in ids.iter().zip([l_shape(), c_shape]) {
        let stamp = plan.stamp(*id).unwrap();
        assert!(inside(&ring, stamp.anchor), "{} is outside", stamp.anchor);
        assert_eq!(stamp.boundary, vec![ring], "the boundary as validated");
        assert!(!stamp.fill.triangles.is_empty(), "the tint is triangulated");
    }

    let offset = DVec2::new(0.5, -0.25);
    let anchor = plan.stamp(ids[0]).unwrap().anchor;
    let mut p2 = default_metric_project();
    let g = p2.storeys.ground();
    let mut moved = p2.new_space(g, l_shape(), Vec::new(), 0);
    moved.as_space_mut().unwrap().stamp_offset = offset;
    let id = moved.id;
    History::default()
        .execute(&mut p2, Command::AddElements(vec![moved]))
        .unwrap();
    let geometry = ModelGeometry::evaluate(&p2);
    let plan = plan_drawing(&p2, &geometry, g, ViewRange::for_storey(&p2, g), |_| true);
    assert!(plan.stamp(id).unwrap().anchor.distance(anchor + offset) < 1e-12);
}

/// SPC-001: the stamp reads the long name, the number and the net area in
/// the project's area unit, one line each, and leaves empty lines out.
#[test]
fn the_stamp_reads_name_number_and_area_without_empty_lines() {
    use archicraft_model::units::{AreaUnit, NumberFormat};

    let mut p = default_metric_project();
    let (plan, ids) = plan_with_spaces(
        &mut p,
        vec![
            (l_shape(), "Kitchen", "0.01"),
            (l_shape(), "Kitchen", ""),
            (l_shape(), "", ""),
        ],
    );
    let units = &p.settings.units;
    let nf = NumberFormat::default();
    let lines = |i: usize| plan.stamp(ids[i]).unwrap().lines(units, &nf);
    assert!((plan.stamp(ids[0]).unwrap().area - 20.0).abs() < 1e-12);
    assert_eq!(lines(0), ["Kitchen", "0.01", "20.00 m²"]);
    assert_eq!(lines(1), ["Kitchen", "20.00 m²"]);
    assert_eq!(lines(2), ["20.00 m²"]);

    // The project's area unit and the locale's decimal separator.
    let mut units = units.clone();
    units.area = AreaUnit::SquareFoot;
    let comma = NumberFormat {
        decimal_separator: ',',
        grouping: None,
    };
    let stamp = plan.stamp(ids[1]).unwrap();
    assert_eq!(stamp.lines(&units, &comma), ["Kitchen", "215,28 ft²"]);
}

/// SPC-001, SEL-001, SEL-003: a space picks near its boundary, after the
/// construction elements there, but not inside its area, which belongs to
/// the room's contents; area selection catches it by its boundary. The
/// boundary stays out of contour detection (INP-018).
#[test]
fn a_space_picks_by_its_boundary_and_stays_out_of_contour_detection() {
    use archicraft_geometry::{ContourOptions, PlanSegmentSource};

    let mut p = default_metric_project();
    let g = p.storeys.ground();
    let wall = p.new_wall(g, DVec2::new(0.0, 0.0), DVec2::new(6.0, 0.0), 0);
    let wall_id = wall.id;
    History::default()
        .execute(&mut p, Command::AddElements(vec![wall]))
        .unwrap();
    let (plan, ids) = plan_with_spaces(&mut p, vec![(l_shape(), "Kitchen", "")]);
    let space = ids[0];

    assert_eq!(plan.pick(DVec2::new(6.005, 1.0), 0.01), [space], "near the east edge");
    assert!(plan.pick(DVec2::new(2.0, 1.0), 0.01).is_empty(), "inside the room");
    let on_wall = plan.pick(DVec2::new(3.0, 0.0), 0.01);
    assert_eq!(on_wall, [wall_id, space], "the wall first");

    let caught = |min: (f64, f64), max: (f64, f64), fully| {
        let ids = plan.in_rect(DVec2::new(min.0, min.1), DVec2::new(max.0, max.1), fully);
        ids.contains(&space)
    };
    assert!(caught((-1.0, -1.0), (7.0, 5.0), true));
    assert!(!caught((-1.0, -1.0), (5.0, 5.0), true));
    assert!(caught((5.0, 1.0), (7.0, 3.0), false));
    assert!(!caught((1.0, 1.0), (2.0, 2.0), false));

    let (lo, hi) = plan.bounds().unwrap();
    assert!(hi.y >= 4.0 && lo.x <= 0.0, "fit to window includes the space");

    let segments = plan.segments(&ContourOptions::default());
    let at_east_edge = |s: &&archicraft_geometry::contour::Segment| {
        let [a, b] = s.points;
        (a.x - 6.0).abs() < 1e-9 && (b.x - 6.0).abs() < 1e-9 && a.y.max(b.y) > 1.0
    };
    assert!(!segments.iter().any(|s| at_east_edge(&s)), "{segments:?}");
}
