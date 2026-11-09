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
