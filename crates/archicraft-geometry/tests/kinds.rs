//! Kinds without a geometry generator (NFR-REL-006): until kinds register
//! their own generators, an element of such a kind is listed as a problem
//! and does not stop the others. Uses the test kind, which the
//! dev-dependency on archicraft-model with `test-kinds` provides.

#![allow(clippy::unwrap_used)]

use archicraft_geometry::{GeometryError, ModelGeometry, ViewRange, plan_drawing};
use archicraft_model::glam::DVec2;
use archicraft_model::kinds::Dummy;
use archicraft_model::template::default_metric_project;
use archicraft_model::{Command, History};

#[test]
fn a_kind_without_a_generator_is_listed_as_a_problem() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let g = p.storeys.ground();
    let wall = p.new_wall(g, DVec2::ZERO, DVec2::new(4.0, 0.0), 0);
    let dummy = Dummy::new(DVec2::new(1.0, 1.0), DVec2::ONE, p.materials[0].id).into_element(&p, g, 0);
    let (wall_id, dummy_id) = (wall.id, dummy.id);
    h.execute(&mut p, Command::AddElements(vec![wall, dummy])).unwrap();

    let geometry = ModelGeometry::evaluate(&p);
    assert!(geometry.get(wall_id).is_some(), "the wall still evaluates");
    assert!(geometry.get(dummy_id).is_none());
    assert_eq!(
        geometry.problems.get(&dummy_id),
        Some(&GeometryError::UnsupportedKind("dummy").to_string())
    );
    assert_eq!(geometry.problems.len(), 1, "{:?}", geometry.problems);

    // The plan shows the wall and leaves the element without geometry out.
    let plan = plan_drawing(&p, &geometry, g, ViewRange::for_storey(&p, g), |_| true);
    let drawn: Vec<_> = plan.elements.iter().map(|e| e.id).collect();
    assert_eq!(drawn, [wall_id]);
}
