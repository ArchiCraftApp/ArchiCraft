//! Triangulation of polygons with holes, by constrained Delaunay
//! triangulation (spade).

use archicraft_model::tolerance::near_zero;
use glam::DVec2;
use spade::handles::FixedVertexHandle;
use spade::{ConstrainedDelaunayTriangulation, HasPosition, Point2, Triangulation};

use crate::GeometryError;

#[derive(Clone, Copy, Debug)]
struct Vertex {
    position: Point2<f64>,
    index: usize,
}

impl HasPosition for Vertex {
    type Scalar = f64;

    fn position(&self) -> Point2<f64> {
        self.position
    }
}

fn point_in_ring(p: DVec2, ring: &[DVec2]) -> bool {
    let mut inside = false;
    let mut j = ring.len() - 1;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Triangulates a polygon with holes. The returned indices refer to the
/// points of `outer` followed by the points of each hole, in order. The
/// triangles are counter-clockwise. Rings must be simple, must not touch each
/// other and must not repeat their first point.
pub fn triangulate(outer: &[DVec2], holes: &[Vec<DVec2>]) -> Result<Vec<[usize; 3]>, GeometryError> {
    if outer.len() < 3 {
        return Err(GeometryError::InvalidPolygon("fewer than three points".into()));
    }
    let mut cdt: ConstrainedDelaunayTriangulation<Vertex> = ConstrainedDelaunayTriangulation::new();
    let mut handles: Vec<FixedVertexHandle> = Vec::new();
    let mut ring_ranges: Vec<std::ops::Range<usize>> = Vec::new();
    let mut index = 0;
    for ring in std::iter::once(outer).chain(holes.iter().map(Vec::as_slice)) {
        let start = index;
        for p in ring {
            if !p.is_finite() {
                return Err(GeometryError::InvalidPolygon("non-finite coordinate".into()));
            }
            let handle = cdt
                .insert(Vertex {
                    position: Point2::new(p.x, p.y),
                    index,
                })
                .map_err(|e| GeometryError::InvalidPolygon(format!("{e:?}")))?;
            if cdt.vertex(handle).data().index != index {
                return Err(GeometryError::InvalidPolygon("duplicate point".into()));
            }
            handles.push(handle);
            index += 1;
        }
        ring_ranges.push(start..index);
    }
    for range in &ring_ranges {
        let n = range.len();
        for k in 0..n {
            let from = handles[range.start + k];
            let to = handles[range.start + (k + 1) % n];
            if !cdt.can_add_constraint(from, to) {
                return Err(GeometryError::InvalidPolygon("the outline intersects itself".into()));
            }
            cdt.add_constraint(from, to);
        }
    }
    let all: Vec<DVec2> = outer.iter().chain(holes.iter().flatten()).copied().collect();
    let mut triangles = Vec::new();
    for face in cdt.inner_faces() {
        let [a, b, c] = face.vertices().map(|v| v.data().index);
        let centroid = (all[a] + all[b] + all[c]) / 3.0;
        let inside = point_in_ring(centroid, outer) && !holes.iter().any(|h| point_in_ring(centroid, h));
        if !inside {
            continue;
        }
        // Drop only triangles that are flat up to rounding. Slivers are real
        // geometry: an EDT-019 sliver (two 0.1 mm edges at 0.1°) has about
        // 8.7e-12 m², below the square of the model tolerance (NFR-ACC-003),
        // and dropping it would open the cap.
        let (u, v) = (all[b] - all[a], all[c] - all[a]);
        let area = u.perp_dot(v);
        let magnitude = all[a].abs().max(all[b].abs()).max(all[c].abs()).max_element();
        if near_zero(area, magnitude * (u.length() + v.length())) {
            continue;
        }
        triangles.push(if area > 0.0 { [a, b, c] } else { [a, c, b] });
    }
    if triangles.is_empty() {
        return Err(GeometryError::InvalidPolygon("the polygon has no area".into()));
    }
    Ok(triangles)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_with_hole() {
        let outer = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(4.0, 0.0),
            DVec2::new(4.0, 4.0),
            DVec2::new(0.0, 4.0),
        ];
        let hole = vec![
            DVec2::new(1.0, 1.0),
            DVec2::new(1.0, 3.0),
            DVec2::new(3.0, 3.0),
            DVec2::new(3.0, 1.0),
        ];
        let tris = triangulate(&outer, std::slice::from_ref(&hole)).unwrap();
        let all: Vec<DVec2> = outer.iter().chain(hole.iter()).copied().collect();
        let area: f64 = tris
            .iter()
            .map(|t| (all[t[1]] - all[t[0]]).perp_dot(all[t[2]] - all[t[0]]) / 2.0)
            .sum();
        assert!((area - 12.0).abs() < 1e-12);
        assert!(
            tris.iter()
                .all(|t| (all[t[1]] - all[t[0]]).perp_dot(all[t[2]] - all[t[0]]) > 0.0)
        );
    }

    #[test]
    fn collinear_points_produce_no_degenerate_triangles() {
        let outer = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(1.0, 0.0),
            DVec2::new(2.0, 0.0),
            DVec2::new(3.0, 0.0),
            DVec2::new(3.0, 1.0),
            DVec2::new(0.0, 1.0),
        ];
        let tris = triangulate(&outer, &[]).unwrap();
        assert_eq!(tris.len(), 4);
    }

    #[test]
    fn concave_l_shape() {
        let outer = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(2.0, 0.0),
            DVec2::new(2.0, 1.0),
            DVec2::new(1.0, 1.0),
            DVec2::new(1.0, 2.0),
            DVec2::new(0.0, 2.0),
        ];
        let tris = triangulate(&outer, &[]).unwrap();
        let area: f64 = tris
            .iter()
            .map(|t| (outer[t[1]] - outer[t[0]]).perp_dot(outer[t[2]] - outer[t[0]]) / 2.0)
            .sum();
        assert!((area - 3.0).abs() < 1e-12);
    }
}
