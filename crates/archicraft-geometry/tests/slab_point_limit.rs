//! Slabs at the point limit evaluate quickly (SLB-003, NFR-PERF-006): a slab
//! has up to 10,000 points in its outline and holes together (SRS Appendix
//! D.9), and its body is evaluated on open and on every edit of the slab.

#![allow(clippy::unwrap_used)]

use std::time::{Duration, Instant};

use archicraft_geometry::evaluate::evaluate_element;
use archicraft_model::glam::DVec2;
use archicraft_model::template::default_metric_project;
use archicraft_model::{Element, Project};

/// The slab point limit of SRS Appendix D.9 (SLB-003).
const LIMIT: usize = 10_000;

/// A comb of `teeth` unit teeth on a 1 m deep back, counter-clockwise:
/// `4 * teeth + 3` points (the comb of archicraft-model's validation
/// tests, reversed).
fn comb(teeth: usize) -> Vec<DVec2> {
    let mut ring = Vec::with_capacity(4 * teeth + 3);
    for i in 0..teeth {
        let x = 2.0 * i as f64;
        ring.extend([
            DVec2::new(x, 0.0),
            DVec2::new(x, 1.0),
            DVec2::new(x + 1.0, 1.0),
            DVec2::new(x + 1.0, 0.0),
        ]);
    }
    let end = 2.0 * teeth as f64;
    ring.extend([DVec2::new(end, 0.0), DVec2::new(end, -1.0), DVec2::new(0.0, -1.0)]);
    ring.reverse();
    ring
}

/// A rectangle with `count` square holes of 0.5 m on a 1 m grid, holes
/// clockwise: `4 + 4 * count` points.
fn rectangle_with_holes(count: usize) -> (Vec<DVec2>, Vec<Vec<DVec2>>) {
    let columns = (count as f64).sqrt().ceil() as usize;
    let rows = count.div_ceil(columns);
    let size = DVec2::new(columns as f64, rows as f64);
    let outline = vec![DVec2::ZERO, DVec2::new(size.x, 0.0), size, DVec2::new(0.0, size.y)];
    let holes = (0..count)
        .map(|k| {
            let corner = DVec2::new((k % columns) as f64, (k / columns) as f64) + DVec2::splat(0.25);
            [
                DVec2::ZERO,
                DVec2::new(0.0, 0.5),
                DVec2::splat(0.5),
                DVec2::new(0.5, 0.0),
            ]
            .map(|p| corner + p)
            .to_vec()
        })
        .collect();
    (outline, holes)
}

fn area(ring: &[DVec2]) -> f64 {
    let n = ring.len();
    (0..n).map(|i| ring[i].perp_dot(ring[(i + 1) % n])).sum::<f64>() / 2.0
}

fn slabs_at_the_limit(project: &Project) -> Vec<(&'static str, Element, f64)> {
    let storey = project.storeys.ground();
    let outline = comb((LIMIT - 4) / 4);
    assert_eq!(outline.len(), LIMIT - 1);
    let comb_area = area(&outline);
    let (rectangle, holes) = rectangle_with_holes((LIMIT - 4) / 4);
    assert_eq!(holes.len(), 2_499);
    assert_eq!(rectangle.len() + holes.iter().map(Vec::len).sum::<usize>(), LIMIT);
    let holed_area = area(&rectangle) + holes.iter().map(|h| area(h)).sum::<f64>();
    vec![
        (
            "comb outline",
            project.new_slab(storey, outline, Vec::new(), 0),
            comb_area,
        ),
        (
            "rectangle with holes",
            project.new_slab(storey, rectangle, holes, 1),
            holed_area,
        ),
    ]
}

/// SLB-003, NFR-PERF-006: a slab at the point limit (a comb outline of
/// 9,999 points, a rectangle with 2,499 square holes) evaluates in under
/// 20 ms in a release build (about 4 and 8 ms, 2026-10-22, Apple M-series;
/// `cargo test --release -p archicraft-geometry --test slab_point_limit`).
/// The gate's and CI's test builds hold it to 60 ms (about 4 and 9 ms
/// there), which a point-in-ring test of every triangle against every ring,
/// as before archicraft-e2t1, exceeds (about 120 and 210 ms). The fastest
/// of three runs counts, so a busy machine does not fail the test. The body
/// is checked too: a slab must not evaluate quickly by evaluating wrongly.
#[test]
fn slabs_at_the_point_limit_evaluate_quickly() {
    let budget = Duration::from_millis(if cfg!(debug_assertions) { 60 } else { 20 });
    let project = default_metric_project();
    for (name, slab, area) in slabs_at_the_limit(&project) {
        let mut fastest = Duration::MAX;
        for _ in 0..3 {
            let start = Instant::now();
            let geometry = evaluate_element(&project, &slab, None).unwrap();
            fastest = fastest.min(start.elapsed());

            let mesh = &geometry.parts[0].mesh;
            assert_eq!(mesh.check_closed(), Ok(()), "{name}");
            let thickness = slab.as_slab().unwrap().thickness;
            let volume = geometry.volume();
            assert!((volume - area * thickness).abs() < 1e-6, "{name}: {volume}");
        }
        assert!(fastest < budget, "{name} took {fastest:?}, more than {budget:?}");
    }
}
