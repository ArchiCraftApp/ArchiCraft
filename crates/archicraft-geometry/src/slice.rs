//! Horizontal slicing of closed meshes, for plan cuts (VIEW-010, WAL-027).
//!
//! Section points are chained by their cell of the model-tolerance grid
//! (NFR-ACC-003). Neighbouring triangles compute their shared section points
//! bit-identically (see [`crossing`]), so an exact cell lookup suffices;
//! segments within one cell are shorter than the tolerance and dropped.

use std::collections::HashMap;

use archicraft_model::outline::Ring;
use archicraft_model::tolerance::{Cell2, cross_is_zero, quantise};
use geo::Polygon;
use glam::{DVec2, DVec3};

use crate::mesh::TriMesh;

type Key = Cell2;

fn key(p: DVec2) -> Key {
    quantise(p)
}

fn lexicographic_order(a: DVec3, b: DVec3) -> (DVec3, DVec3) {
    if (a.x, a.y, a.z) <= (b.x, b.y, b.z) {
        (a, b)
    } else {
        (b, a)
    }
}

/// Where the edge `a`–`b` crosses the plane, computed from the endpoints in
/// a fixed order so that neighbouring triangles get bit-identical points.
fn crossing(a: DVec3, b: DVec3, z: f64) -> DVec2 {
    let (u, v) = lexicographic_order(a, b);
    let t = (z - u.z) / (v.z - u.z);
    (u + (v - u) * t).truncate()
}

/// Cuts a closed, outward-oriented mesh with the plane at height `z` and
/// returns the closed section loops. Outer boundaries are counter-clockwise
/// and holes clockwise (the material is on the left of every loop).
pub fn slice_loops(mesh: &TriMesh, z: f64) -> Vec<Vec<DVec2>> {
    let mut segments: Vec<(DVec2, DVec2)> = Vec::new();
    for tri in mesh.triangles() {
        let above = tri.map(|p| p.z >= z);
        if above.iter().all(|a| *a) || above.iter().all(|a| !*a) {
            continue;
        }
        let mut points = Vec::with_capacity(2);
        for i in 0..3 {
            let (p, q) = (tri[i], tri[(i + 1) % 3]);
            if above[i] != above[(i + 1) % 3] {
                points.push(crossing(p, q, z));
            }
        }
        let [p1, p2] = [points[0], points[1]];
        if key(p1) == key(p2) {
            continue;
        }
        let normal = (tri[1] - tri[0]).cross(tri[2] - tri[0]);
        // The material lies on the left when walking along up × normal.
        let along = DVec2::new(-normal.y, normal.x);
        if (p2 - p1).dot(along) >= 0.0 {
            segments.push((p1, p2));
        } else {
            segments.push((p2, p1));
        }
    }
    chain(segments)
}

fn chain(segments: Vec<(DVec2, DVec2)>) -> Vec<Vec<DVec2>> {
    let mut by_start: HashMap<Key, Vec<usize>> = HashMap::new();
    for (i, (a, _)) in segments.iter().enumerate() {
        by_start.entry(key(*a)).or_default().push(i);
    }
    let mut used = vec![false; segments.len()];
    let mut loops = Vec::new();
    for first in 0..segments.len() {
        if used[first] {
            continue;
        }
        used[first] = true;
        let start_key = key(segments[first].0);
        let mut ring = vec![segments[first].0];
        let mut current = segments[first].1;
        let mut closed = false;
        for _ in 0..segments.len() {
            if key(current) == start_key {
                closed = true;
                break;
            }
            ring.push(current);
            let next = by_start
                .get(&key(current))
                .and_then(|candidates| candidates.iter().copied().find(|&i| !used[i]));
            match next {
                Some(i) => {
                    used[i] = true;
                    current = segments[i].1;
                }
                None => break,
            }
        }
        if closed && ring.len() >= 3 {
            loops.push(simplify_collinear(ring));
        }
    }
    loops
}

/// Removes points that lie on the straight line between their neighbours,
/// which slicing produces at every triangle edge of a planar face.
fn simplify_collinear(ring: Vec<DVec2>) -> Vec<DVec2> {
    let n = ring.len();
    let mut out: Vec<DVec2> = Vec::with_capacity(n);
    for i in 0..n {
        let prev = ring[(i + n - 1) % n];
        let p = ring[i];
        let next = ring[(i + 1) % n];
        // A point where the ring turns back (a spike) is kept.
        if !cross_is_zero(prev, p, next) || (p - prev).dot(next - p) < 0.0 {
            out.push(p);
        }
    }
    if out.len() >= 3 { out } else { ring }
}

/// Turns section loops into polygons: every hole is assigned to the smallest
/// outer loop that contains it. Each loop's area is computed once.
pub fn loops_to_polygons(loops: Vec<Vec<DVec2>>) -> Vec<Polygon<f64>> {
    let (outers, holes): (Vec<_>, Vec<_>) = loops
        .into_iter()
        .map(|points| {
            let ring = Ring::<()>::from_points(points);
            let area = ring.signed_area();
            (ring, area)
        })
        .partition(|(_, area)| *area > 0.0);
    let mut assigned: Vec<Vec<Ring>> = vec![Vec::new(); outers.len()];
    for (hole, _) in holes {
        let candidate = outers
            .iter()
            .enumerate()
            .filter(|(_, (outer, _))| outer.contains(hole.node(0)))
            .min_by(|(_, (_, a)), (_, (_, b))| a.total_cmp(b))
            .map(|(i, _)| i);
        if let Some(i) = candidate {
            assigned[i].push(hole);
        }
    }
    outers
        .iter()
        .zip(assigned)
        .map(|((outer, _), holes)| Polygon::new(outer.to_geo(), holes.iter().map(Ring::to_geo).collect()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extrude::extrude;
    use archicraft_model::MODEL_TOLERANCE;
    use geo::Area;

    #[test]
    fn slicing_a_slab_with_hole() {
        let outer = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(6.0, 0.0),
            DVec2::new(6.0, 4.0),
            DVec2::new(0.0, 4.0),
        ];
        let hole = vec![
            DVec2::new(1.0, 1.0),
            DVec2::new(1.0, 2.0),
            DVec2::new(2.0, 2.0),
            DVec2::new(2.0, 1.0),
        ];
        let mesh = extrude(&outer, &[hole], 0.0, 3.0).unwrap();
        let loops = slice_loops(&mesh, 1.1);
        assert_eq!(loops.len(), 2);
        let polygons = loops_to_polygons(loops);
        assert_eq!(polygons.len(), 1);
        assert_eq!(polygons[0].interiors().len(), 1);
        assert!((polygons[0].unsigned_area() - 23.0).abs() < 1e-9);
        assert_eq!(polygons[0].exterior().0.len(), 5, "collinear points removed");
    }

    /// Rounding grows with the coordinates and the collinearity guard grows
    /// with them, so a slab 2 km from the origin still gets clean rings.
    #[test]
    fn collinear_points_are_removed_far_from_the_origin() {
        let offset = DVec2::new(1500.0, -2000.0);
        let outer: Vec<DVec2> = [(0.0, 0.0), (0.3, 0.0), (0.3, 0.2), (0.1, 0.2), (0.0, 0.2)]
            .iter()
            .map(|&(x, y)| DVec2::new(x, y) + offset)
            .collect();
        let mesh = extrude(&outer, &[], 0.0, 3.0).unwrap();
        let loops = slice_loops(&mesh, 1.1);
        assert_eq!(loops.len(), 1);
        assert_eq!(loops[0].len(), 4, "{:?}", loops[0]);
    }

    /// NFR-ACC-003: section points are chained on the model-tolerance grid.
    /// An outline edge of 4.5 µm whose ends lie in one cell leaves no
    /// section segment, and the loop still closes, with one of its ends as
    /// the corner there instead of both.
    #[test]
    fn section_segments_shorter_than_the_model_tolerance_are_dropped() {
        let d = DVec2::new(0.000_003, 1.000_003);
        let e = DVec2::new(0.000_001, 1.000_007);
        assert!(d.distance(e) < MODEL_TOLERANCE && quantise(d) == quantise(e));
        let outer = vec![DVec2::new(0.0, 0.0), DVec2::new(2.0, 0.0), DVec2::new(2.0, 1.0), d, e];
        let mesh = extrude(&outer, &[], 0.0, 3.0).unwrap();
        let loops = slice_loops(&mesh, 1.1);
        assert_eq!(loops.len(), 1);
        assert!(loops[0].contains(&d) != loops[0].contains(&e), "{:?}", loops[0]);
    }

    #[test]
    fn plane_outside_mesh_gives_nothing() {
        let outer = vec![DVec2::new(0.0, 0.0), DVec2::new(1.0, 0.0), DVec2::new(0.0, 1.0)];
        let mesh = extrude(&outer, &[], 0.0, 1.0).unwrap();
        assert!(slice_loops(&mesh, 2.0).is_empty());
        assert!(slice_loops(&mesh, -1.0).is_empty());
    }
}
