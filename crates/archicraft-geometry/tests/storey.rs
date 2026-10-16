//! A storey with four joined walls and a slab: closed bodies, clean plan.

#![allow(clippy::unwrap_used)]

use archicraft_geometry::{ModelGeometry, ViewRange, plan_drawing};
use archicraft_model::glam::DVec2;
use archicraft_model::template::default_metric_project;
use archicraft_model::{Command, History, Project};
use geo::Area;

fn house() -> Project {
    let mut p = default_metric_project();
    let mut h = History::default();
    let g = p.storeys.ground();
    let corners = [
        DVec2::new(0.0, 0.0),
        DVec2::new(8.0, 0.0),
        DVec2::new(8.0, 6.0),
        DVec2::new(0.0, 6.0),
    ];
    let walls = (0..4)
        .map(|i| p.new_wall(g, corners[i], corners[(i + 1) % 4], i as u32))
        .collect();
    h.execute(&mut p, Command::AddElements(walls)).unwrap();
    let slab = p.new_slab(g, corners.to_vec(), Vec::new(), 0);
    h.execute(&mut p, Command::AddElements(vec![slab])).unwrap();
    p
}

#[test]
fn bodies_are_closed_and_volumes_add_up() {
    let p = house();
    let geometry = ModelGeometry::evaluate(&p);
    assert!(geometry.problems.is_empty(), "{:?}", geometry.problems);
    let mut wall_volume = 0.0;
    for e in p.elements.values() {
        let g = geometry.get(e.id).unwrap();
        for part in &g.parts {
            assert_eq!(part.mesh.check_closed(), Ok(()), "{}", e.name);
        }
        if e.as_wall().is_some() {
            wall_volume += g.volume();
        }
    }
    // Exterior faces on the reference line: the outer outline is 8 × 6 and
    // the inner one 7.5 × 5.5, with mitred corners and no overlap.
    let expected = (8.0 * 6.0 - 7.5 * 5.5) * 3.0;
    assert!((wall_volume - expected).abs() < 1e-9, "{wall_volume} vs {expected}");
}

#[test]
fn plan_cuts_walls_and_projects_slab() {
    let p = house();
    let geometry = ModelGeometry::evaluate(&p);
    let g = p.storeys.ground();
    let plan = plan_drawing(&p, &geometry, g, ViewRange::for_storey(&p, g), |_| true);
    assert_eq!(plan.elements.len(), 5);
    // The four walls share one material and merge into a single ring.
    assert_eq!(plan.cut_regions.len(), 1);
    let region = &plan.cut_regions[0];
    assert_eq!(region.polygons.0.len(), 1);
    assert_eq!(region.polygons.0[0].interiors().len(), 1);
    assert!((region.polygons.unsigned_area() - (48.0 - 41.25)).abs() < 1e-6);
    assert!(!region.fill.triangles.is_empty());
    // The slab is below the cut plane: projected outline only.
    let slab = plan.elements.iter().find(|e| e.reference_line.is_none()).unwrap();
    assert!(slab.cut.is_empty());
    assert_eq!(slab.projected.len(), 4);
    // Picking inside a wall finds the wall first.
    let hits = plan.pick(DVec2::new(4.0, 0.1), 0.01);
    assert_eq!(p.element(hits[0]).unwrap().name, "W-001");
    // The upper storey is empty.
    let upper = p.storeys.as_slice()[1].id;
    let plan = plan_drawing(&p, &geometry, upper, ViewRange::for_storey(&p, upper), |_| true);
    assert!(plan.elements.is_empty());
}
