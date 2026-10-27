//! Triangulation of polygons with holes, by constrained Delaunay
//! triangulation (spade).

use std::ops::Range;

use archicraft_model::tolerance::cross_is_zero;
use glam::DVec2;
use spade::handles::{FixedDirectedEdgeHandle, FixedFaceHandle, FixedVertexHandle, InnerTag};
use spade::{
    ConstrainedDelaunayTriangulation, HasPosition, Intersection, LineIntersectionIterator, Point2, Triangulation,
};

use crate::GeometryError;
use crate::message::Message;

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

type Cdt = ConstrainedDelaunayTriangulation<Vertex>;

/// The constrained Delaunay triangulation of a polygon's rings: their
/// points, in order, are its vertices, and their edges its constraint edges.
struct RingTriangulation {
    cdt: Cdt,
    /// The points of the outer ring followed by those of each hole.
    points: Vec<DVec2>,
    /// The vertex of each point.
    vertices: Vec<FixedVertexHandle>,
    /// The range of each ring in `points`, the outer ring first.
    rings: Vec<Range<usize>>,
}

impl RingTriangulation {
    fn new(outer: &[DVec2], holes: &[Vec<DVec2>]) -> Result<Self, GeometryError> {
        let mut cdt = Cdt::new();
        let count = outer.len() + holes.iter().map(Vec::len).sum::<usize>();
        let mut points = Vec::with_capacity(count);
        let mut vertices = Vec::with_capacity(count);
        let mut rings = Vec::new();
        for ring in std::iter::once(outer).chain(holes.iter().map(Vec::as_slice)) {
            let start = points.len();
            for &p in ring {
                if !p.is_finite() {
                    return Err(invalid("geometry-polygon-not-finite"));
                }
                let index = points.len();
                let before = cdt.num_vertices();
                let handle = cdt
                    .insert(Vertex {
                        position: Point2::new(p.x, p.y),
                        index,
                    })
                    .map_err(|_| invalid("geometry-polygon-coordinate-range"))?;
                // spade keeps one vertex per position and gives a repeated
                // position the new data, so a repeat adds no vertex. Rings
                // may share a vertex (the plan's cut unions give holes that
                // touch the outline at a corner); a ring may not repeat one.
                if cdt.num_vertices() == before && vertices[start..].contains(&handle) {
                    return Err(invalid("geometry-polygon-duplicate-point"));
                }
                points.push(p);
                vertices.push(handle);
            }
            rings.push(start..points.len());
        }
        for range in &rings {
            let n = range.len();
            for k in 0..n {
                let from = vertices[range.start + k];
                let to = vertices[range.start + (k + 1) % n];
                if !cdt.can_add_constraint(from, to) {
                    return Err(invalid("geometry-polygon-self-intersection"));
                }
                cdt.add_constraint(from, to);
            }
        }
        Ok(Self {
            cdt,
            points,
            vertices,
            rings,
        })
    }

    /// Whether each face, by index, lies inside the outer ring and inside no
    /// hole (SLB-003, NFR-PERF-006). A walk over the faces from the outer
    /// face, which lies outside every ring, counts the rings it enters and
    /// leaves at their edges, so it visits each face once, where a
    /// point-in-ring test of every face against every ring took faces times
    /// points. Rings that do not cross are classified exactly, whatever their
    /// orientation or nesting.
    fn inside_faces(&self) -> Vec<bool> {
        let cdt = &self.cdt;
        // A simple ring whose points all lie on the convex hull is the hull,
        // so every face lies inside it: most wall footprints and slabs.
        if self.rings.len() == 1 && cdt.convex_hull_size() == self.points.len() {
            return vec![true; cdt.num_all_faces()];
        }
        // The ring edges along each undirected edge, as lists threaded
        // through `Boundary::next`: one on a ring's edge, more only where
        // rings touch or a degenerate ring runs along an edge twice.
        let mut first = vec![NONE; cdt.num_undirected_edges()];
        let mut boundaries: Vec<Boundary> = Vec::with_capacity(self.points.len());
        for (ring, range) in self.rings.iter().enumerate() {
            let counter_clockwise = twice_signed_area(&self.points[range.clone()]) > 0.0;
            let mut add = |along: FixedDirectedEdgeHandle| {
                let edge = along.as_undirected().index();
                boundaries.push(Boundary {
                    inward: if counter_clockwise { along } else { along.rev() },
                    ring,
                    next: first[edge],
                });
                first[edge] = boundaries.len() - 1;
            };
            let n = range.len();
            for k in 0..n {
                let from = self.vertices[range.start + k];
                let to = self.vertices[range.start + (k + 1) % n];
                match cdt.get_edge_from_neighbors(from, to) {
                    Some(edge) => add(edge.fix()),
                    // Points on this edge split the constraint.
                    None => {
                        for intersection in LineIntersectionIterator::new_from_handles(cdt, from, to) {
                            if let Intersection::EdgeOverlap(edge) = intersection {
                                add(edge.fix());
                            }
                        }
                    }
                }
            }
        }

        let mut cover: Vec<Option<Cover>> = vec![None; cdt.num_all_faces()];
        let outside = cdt.outer_face().fix();
        cover[outside.index()] = Some(Cover::default());
        let mut pending = vec![(outside, Cover::default())];
        while let Some((face, here)) = pending.pop() {
            let Some(start) = cdt.face(face).adjacent_edge() else {
                continue;
            };
            let mut edge = start;
            loop {
                let across = edge.rev().face().fix();
                if cover[across.index()].is_none() {
                    let mut there = here;
                    let mut boundary = first[edge.fix().as_undirected().index()];
                    while let Some(b) = boundaries.get(boundary) {
                        // The face across lies left of the reversed edge.
                        there.cross(b.ring, edge.fix().rev() == b.inward);
                        boundary = b.next;
                    }
                    cover[across.index()] = Some(there);
                    pending.push((across, there));
                }
                edge = edge.next();
                if edge == start {
                    break;
                }
            }
        }
        cover.into_iter().map(|c| c == Some(Cover::INSIDE)).collect()
    }

    /// The faces that `keep` accepts, as counter-clockwise triangles of
    /// point indices, in the order of the triangulation's faces.
    fn triangles(&self, keep: impl Fn(FixedFaceHandle<InnerTag>) -> bool) -> Result<Vec<[usize; 3]>, GeometryError> {
        let all = &self.points;
        let mut triangles = Vec::new();
        for face in self.cdt.inner_faces() {
            if !keep(face.fix()) {
                continue;
            }
            let [a, b, c] = face.vertices().map(|v| v.data().index);
            // Drop only triangles that are flat up to rounding. Slivers are real
            // geometry: an EDT-019 sliver (two 0.1 mm edges at 0.1°) has about
            // 8.7e-12 m², below the square of the model tolerance (NFR-ACC-003),
            // and dropping it would open the cap.
            if cross_is_zero(all[a], all[b], all[c]) {
                continue;
            }
            let area = (all[b] - all[a]).perp_dot(all[c] - all[a]);
            triangles.push(if area > 0.0 { [a, b, c] } else { [a, c, b] });
        }
        if triangles.is_empty() {
            return Err(invalid("geometry-polygon-no-area"));
        }
        Ok(triangles)
    }
}

/// No [`Boundary`].
const NONE: usize = usize::MAX;

/// An edge of a ring on an undirected edge of the triangulation.
struct Boundary {
    /// The edge in the direction that has the ring's inside on its left.
    inward: FixedDirectedEdgeHandle,
    /// The ring, 0 for the outer ring.
    ring: usize,
    /// The next ring edge on the same undirected edge, or [`NONE`].
    next: usize,
}

/// How many times the outer ring and the holes contain a face: entering a
/// ring counts up and leaving it counts down, so where a degenerate ring runs
/// along an edge in both directions, the two cancel.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Cover {
    outer: i32,
    holes: i32,
}

impl Cover {
    /// Inside the outer ring and inside no hole.
    const INSIDE: Cover = Cover { outer: 1, holes: 0 };

    /// Crossing an edge of `ring` (0 is the outer ring) into it or out of it.
    fn cross(&mut self, ring: usize, entering: bool) {
        let step = if entering { 1 } else { -1 };
        if ring == 0 {
            self.outer += step;
        } else {
            self.holes += step;
        }
    }
}

/// Twice the signed area of a ring, positive when it runs
/// counter-clockwise. Summed relative to its first point, which keeps the
/// rounding small far from the origin.
fn twice_signed_area(ring: &[DVec2]) -> f64 {
    let Some(&origin) = ring.first() else {
        return 0.0;
    };
    ring.windows(2).map(|w| (w[0] - origin).perp_dot(w[1] - origin)).sum()
}

/// Triangulation refuses the polygon for the reason `key`, a
/// `geometry-polygon-` message (NFR-L10N-001).
fn invalid(key: &'static str) -> GeometryError {
    GeometryError::InvalidPolygon(Message::new(key))
}

/// Triangulates a polygon with holes. The returned indices refer to the
/// points of `outer` followed by the points of each hole, in order. The
/// triangles are counter-clockwise and cover the part of the triangulation
/// inside `outer` and outside every hole. Rings must be simple and must not
/// cross each other, though they may share points; a ring must not repeat a
/// point, its first included.
pub fn triangulate(outer: &[DVec2], holes: &[Vec<DVec2>]) -> Result<Vec<[usize; 3]>, GeometryError> {
    if outer.len() < 3 {
        return Err(invalid("geometry-polygon-too-few-points"));
    }
    let triangulation = RingTriangulation::new(outer, holes)?;
    let inside = triangulation.inside_faces();
    triangulation.triangles(|face| inside[face.index()])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The classification `triangulate` used before archicraft-e2t1: a
    /// point-in-ring test of each face's centroid against every ring.
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

    fn by_point_in_ring(outer: &[DVec2], holes: &[Vec<DVec2>]) -> Result<Vec<[usize; 3]>, GeometryError> {
        let t = RingTriangulation::new(outer, holes)?;
        t.triangles(|face| {
            let centroid = t
                .cdt
                .face(face)
                .vertices()
                .map(|v| t.points[v.data().index])
                .iter()
                .sum::<DVec2>()
                / 3.0;
            point_in_ring(centroid, outer) && !holes.iter().any(|h| point_in_ring(centroid, h))
        })
    }

    fn square(corner: DVec2, size: f64) -> Vec<DVec2> {
        [DVec2::ZERO, DVec2::X, DVec2::ONE, DVec2::Y]
            .map(|p| corner + p * size)
            .to_vec()
    }

    fn reversed(mut ring: Vec<DVec2>) -> Vec<DVec2> {
        ring.reverse();
        ring
    }

    /// A star-shaped ring of `n` points around `centre`, radii between 1
    /// and 1.5 in a fixed irregular pattern.
    fn star(centre: DVec2, n: usize) -> Vec<DVec2> {
        (0..n)
            .map(|k| {
                let radius = 1.0 + 0.5 * ((k * 7919) % 13) as f64 / 13.0;
                centre + DVec2::from_angle(std::f64::consts::TAU * k as f64 / n as f64) * radius
            })
            .collect()
    }

    /// A comb of `teeth` unit teeth on a 1 m deep back (clockwise).
    fn comb(teeth: usize) -> Vec<DVec2> {
        let mut ring = Vec::new();
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
        ring
    }

    /// SLB-003, NFR-PERF-006: classifying the faces by walking the
    /// triangulation, or by the convex hull for a convex outline, keeps every
    /// triangle, and the order, that a point-in-ring test of each face
    /// against every ring kept: for either orientation of the rings, convex
    /// outlines with points along their edges, holes in holes, holes outside
    /// the outline, a hole point on an outline edge, which splits that
    /// constraint edge, and degenerate holes, which contain nothing.
    #[test]
    fn the_classification_keeps_the_triangles_of_a_point_in_ring_test() {
        let outline = square(DVec2::ZERO, 10.0);
        let grid: Vec<Vec<DVec2>> = (0..9)
            .flat_map(|i| (0..9).map(move |j| reversed(square(DVec2::new(i as f64, j as f64) + 0.6, 0.5))))
            .collect();
        let cases = vec![
            ("square", outline.clone(), Vec::new()),
            ("clockwise square", reversed(outline.clone()), Vec::new()),
            (
                "square with points along its edges",
                (0..40)
                    .map(|k| match k / 10 {
                        0 => DVec2::new(k as f64, 0.0),
                        1 => DVec2::new(10.0, (k - 10) as f64),
                        2 => DVec2::new((30 - k) as f64, 10.0),
                        _ => DVec2::new(0.0, (40 - k) as f64),
                    })
                    .collect(),
                Vec::new(),
            ),
            (
                "regular 64-gon",
                (0..64)
                    .map(|k| DVec2::from_angle(std::f64::consts::TAU * k as f64 / 64.0) * 5.0)
                    .collect(),
                Vec::new(),
            ),
            ("comb", comb(25), Vec::new()),
            ("counter-clockwise comb", reversed(comb(25)), Vec::new()),
            ("star", star(DVec2::new(3.0, -2.0), 97), Vec::new()),
            (
                "star with holes",
                star(DVec2::ZERO, 61),
                vec![
                    reversed(square(DVec2::splat(-0.6), 0.5)),
                    square(DVec2::splat(0.1), 0.5),
                ],
            ),
            ("grid of holes", outline.clone(), grid),
            (
                "hole in a hole",
                outline.clone(),
                vec![
                    reversed(square(DVec2::splat(2.0), 6.0)),
                    reversed(square(DVec2::splat(4.0), 2.0)),
                ],
            ),
            (
                "island in a hole, wrongly as a hole",
                outline.clone(),
                vec![reversed(square(DVec2::splat(2.0), 6.0)), square(DVec2::splat(4.0), 2.0)],
            ),
            (
                "hole outside the outline",
                outline.clone(),
                vec![reversed(square(DVec2::splat(12.0), 2.0))],
            ),
            (
                "hole point on an outline edge",
                outline.clone(),
                vec![vec![DVec2::new(5.0, 0.0), DVec2::new(4.0, 1.0), DVec2::new(6.0, 1.0)]],
            ),
            (
                "outline inside a hole",
                outline.clone(),
                vec![reversed(square(DVec2::splat(-1.0), 12.0))],
            ),
            ("one-point hole", outline.clone(), vec![vec![DVec2::splat(2.0)]]),
            (
                "two-point hole",
                outline.clone(),
                vec![vec![DVec2::splat(2.0), DVec2::splat(3.0)]],
            ),
            (
                "collinear hole",
                outline.clone(),
                vec![vec![DVec2::splat(2.0), DVec2::splat(3.0), DVec2::splat(5.0)]],
            ),
        ];
        for (name, outer, holes) in &cases {
            let expected = by_point_in_ring(outer, holes);
            assert_eq!(triangulate(outer, holes), expected, "{name}");
        }
        // The point-in-ring test failed on an empty ring in test builds.
        assert_eq!(triangulate(&outline, &[Vec::new()]), triangulate(&outline, &[]));
    }

    /// NFR-L10N-001: each reason triangulation refuses a polygon for gives
    /// its own `geometry-polygon-` key.
    #[test]
    fn each_refusal_gives_its_message_key() {
        let outline = square(DVec2::ZERO, 10.0);
        let cases = vec![
            (
                "two points",
                "geometry-polygon-too-few-points",
                vec![DVec2::ZERO, DVec2::X],
                Vec::new(),
            ),
            (
                "NaN coordinate",
                "geometry-polygon-not-finite",
                vec![DVec2::ZERO, DVec2::X, DVec2::new(f64::NAN, 1.0)],
                Vec::new(),
            ),
            (
                "coordinate out of range",
                "geometry-polygon-coordinate-range",
                vec![DVec2::ZERO, DVec2::X, DVec2::new(1e200, 1.0)],
                Vec::new(),
            ),
            (
                "outline repeats a point",
                "geometry-polygon-duplicate-point",
                vec![DVec2::ZERO, DVec2::X, DVec2::ONE, DVec2::X, DVec2::Y],
                Vec::new(),
            ),
            (
                "outline repeats its first point",
                "geometry-polygon-duplicate-point",
                vec![DVec2::ZERO, DVec2::X, DVec2::ONE, DVec2::Y, DVec2::ZERO],
                Vec::new(),
            ),
            (
                "hole repeats a point",
                "geometry-polygon-duplicate-point",
                outline.clone(),
                vec![vec![
                    DVec2::ZERO,
                    DVec2::new(1.0, 2.0),
                    DVec2::new(2.0, 2.0),
                    DVec2::new(1.0, 2.0),
                    DVec2::new(2.0, 1.0),
                ]],
            ),
            (
                "bow tie",
                "geometry-polygon-self-intersection",
                vec![DVec2::ZERO, DVec2::ONE, DVec2::X, DVec2::Y],
                Vec::new(),
            ),
            (
                "hole covers the outline",
                "geometry-polygon-no-area",
                outline.clone(),
                vec![reversed(square(DVec2::splat(-1.0), 12.0))],
            ),
        ];
        for (name, key, outer, holes) in &cases {
            assert_eq!(
                triangulate(outer, holes),
                Err(GeometryError::InvalidPolygon(Message::new(key))),
                "{name}: {key}"
            );
        }
    }

    /// WAL-027, DSP-011: a hole that shares a corner with the outline, as the
    /// plan's cut-fill unions (`plan::fill_mesh`) produce, triangulates to the area
    /// between them; only a point repeated within one ring is refused.
    #[test]
    fn a_hole_touching_the_outline_at_a_corner_keeps_the_area_between() {
        let outer = square(DVec2::ZERO, 10.0);
        let hole = vec![DVec2::ZERO, DVec2::new(1.0, 2.0), DVec2::new(2.0, 1.0)];
        let tris = triangulate(&outer, std::slice::from_ref(&hole)).unwrap();
        let all: Vec<DVec2> = outer.iter().chain(hole.iter()).copied().collect();
        let area: f64 = tris
            .iter()
            .map(|t| (all[t[1]] - all[t[0]]).perp_dot(all[t[2]] - all[t[0]]) / 2.0)
            .sum();
        assert!((area - 98.5).abs() < 1e-12, "{area}");
    }

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
