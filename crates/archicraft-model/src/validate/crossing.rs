//! Deep crossings between the edges of two profile regions, found with a
//! sweep line (PRF-004, NFR-SEC-003).
//!
//! Two edges cross *deeply* when each has an end point more than
//! [`MODEL_TOLERANCE`] on either side of the other's line: they cross at a
//! point inside both, and neither merely reaches the other within the
//! tolerance. Regions whose edges cross deeply overlap, whatever the area
//! they share. Finding that out with a boolean intersection takes time and
//! memory in the number of crossings, which two crossed combs of 5,000
//! points each push to millions; the sweep stops at the first deep crossing
//! instead.
//!
//! The sweep (Shamos and Hoey) moves a vertical line across the end points
//! from left to right, keeps the edges crossing it in order of height, and
//! tests two edges of different regions whenever they become neighbours on
//! it. Edges that only touch, sharing a point or lying along each other as
//! touching regions do, keep their order. An edge that starts on another
//! one goes above or below it by its direction. If no edges crossed left of
//! the leftmost deep crossing, its two edges are neighbours just before it
//! and so have been tested. Edges crossing within the tolerance swap places
//! on the plane but not on the line; that can hide a deep crossing, and the
//! boolean intersection, which the caller runs when the sweep finds none,
//! then decides. Sorting the end points and finding places on the line take
//! O(n log n); inserting or removing an edge moves up to n indices.

use std::cmp::Ordering;

use glam::DVec2;

use crate::tolerance::MODEL_TOLERANCE;

/// An edge with its end points in the order the sweep line meets them, and
/// whether it belongs to the second region.
#[derive(Clone, Copy, Debug)]
struct Edge {
    from: DVec2,
    to: DVec2,
    second: bool,
}

/// The order in which the sweep line meets points: by x, then by y.
fn order(p: DVec2, q: DVec2) -> Ordering {
    p.x.total_cmp(&q.x).then(p.y.total_cmp(&q.y))
}

impl Edge {
    fn new((a, b): (DVec2, DVec2), second: bool) -> Self {
        // Adding 0.0 turns -0.0 into 0.0, so that the order agrees with
        // comparing the numbers.
        let (a, b) = (a + 0.0, b + 0.0);
        let (from, to) = if order(a, b) == Ordering::Greater {
            (b, a)
        } else {
            (a, b)
        };
        Edge { from, to, second }
    }

    /// The height of the edge on the sweep line through `p`, exact at its
    /// end points; for a vertical edge, its height nearest to `p`.
    fn height(&self, p: DVec2) -> f64 {
        let (a, b) = (self.from, self.to);
        if a.x == b.x {
            p.y.max(a.y).min(b.y)
        } else if p.x <= a.x {
            a.y
        } else if p.x >= b.x {
            b.y
        } else {
            a.y + (p.x - a.x) * (b.y - a.y) / (b.x - a.x)
        }
    }

    /// Whether the edge lies below `other`, which starts at a point on it or
    /// off it, just right of that point: by height, then by direction.
    fn below(&self, other: &Edge) -> bool {
        let p = other.from;
        let h = self.height(p);
        if h != p.y {
            return h < p.y;
        }
        (self.to - self.from).perp_dot(other.to - other.from) > 0.0
    }

    /// The signed distance of `p` from the edge's line, positive on its left.
    fn side(&self, p: DVec2) -> f64 {
        let d = self.to - self.from;
        d.perp_dot(p - self.from) / d.length()
    }

    /// Whether the edges belong to different regions and cross deeply.
    fn crosses_deeply(&self, other: &Edge) -> bool {
        let apart = |s: f64, t: f64| {
            (s > MODEL_TOLERANCE && t < -MODEL_TOLERANCE) || (s < -MODEL_TOLERANCE && t > MODEL_TOLERANCE)
        };
        self.second != other.second
            && apart(other.side(self.from), other.side(self.to))
            && apart(self.side(other.from), self.side(other.to))
    }
}

/// Whether an edge of `first` crosses an edge of `second` deeply (see the
/// module documentation). Edges are pairs of end points of nonzero length.
pub(super) fn cross_deeply(
    first: impl IntoIterator<Item = (DVec2, DVec2)>,
    second: impl IntoIterator<Item = (DVec2, DVec2)>,
) -> bool {
    let edges: Vec<Edge> = first
        .into_iter()
        .map(|e| Edge::new(e, false))
        .chain(second.into_iter().map(|e| Edge::new(e, true)))
        .collect();
    // Each edge starts and ends once; at the same point, edges end first.
    let mut events: Vec<(usize, bool)> = (0..edges.len()).flat_map(|e| [(e, true), (e, false)]).collect();
    let at = |&(e, start): &(usize, bool)| if start { edges[e].from } else { edges[e].to };
    events.sort_unstable_by(|a, b| order(at(a), at(b)).then(a.1.cmp(&b.1)).then(a.0.cmp(&b.0)));
    let mut line: Vec<usize> = Vec::new();
    let deep = |line: &[usize], i: usize, j: usize| edges[line[i]].crosses_deeply(&edges[line[j]]);
    for (e, start) in events {
        if start {
            let place = line.partition_point(|&f| edges[f].below(&edges[e]));
            line.insert(place, e);
            if (place > 0 && deep(&line, place - 1, place)) || (place + 1 < line.len() && deep(&line, place, place + 1))
            {
                return true;
            }
        } else {
            let p = edges[e].to;
            let from = line.partition_point(|&f| edges[f].height(p) < p.y);
            let found = line[from.min(line.len())..]
                .iter()
                .take_while(|&&f| edges[f].height(p) == p.y)
                .position(|&f| f == e)
                .map(|i| from + i)
                // Edges crossing within the tolerance can put it elsewhere.
                .or_else(|| line.iter().position(|&f| f == e));
            if let Some(i) = found {
                line.remove(i);
                if i > 0 && i < line.len() && deep(&line, i - 1, i) {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edges(points: &[DVec2]) -> Vec<(DVec2, DVec2)> {
        (0..points.len())
            .map(|i| (points[i], points[(i + 1) % points.len()]))
            .collect()
    }

    fn square(min: DVec2, size: f64) -> Vec<DVec2> {
        vec![
            min,
            min + DVec2::new(size, 0.0),
            min + DVec2::splat(size),
            min + DVec2::new(0.0, size),
        ]
    }

    /// PRF-004: squares overlapping at a corner cross deeply; squares that
    /// share an edge, part of one or a corner, one in the other, and two
    /// whose edges cross less than MODEL_TOLERANCE from an end do not.
    #[test]
    fn deep_crossings_are_told_from_touching_edges() {
        let a = square(DVec2::ZERO, 1.0);
        let cross = |b: Vec<DVec2>| cross_deeply(edges(&a), edges(&b));
        assert!(cross(square(DVec2::splat(0.5), 1.0)));
        // A thin plate across the square, poking out on both sides.
        let plate = vec![
            DVec2::new(-0.1, 0.4),
            DVec2::new(1.1, 0.4),
            DVec2::new(1.1, 0.6),
            DVec2::new(-0.1, 0.6),
        ];
        assert!(cross(plate));
        assert!(!cross(square(DVec2::new(1.0, 0.0), 1.0)));
        assert!(!cross(square(DVec2::new(1.0, 0.5), 1.0)));
        assert!(!cross(square(DVec2::splat(1.0), 1.0)));
        assert!(!cross(square(DVec2::splat(0.25), 0.5)));
        assert!(!cross(a.clone()));
        let shallow = 0.5 * MODEL_TOLERANCE;
        assert!(!cross(square(DVec2::new(1.0 - shallow, 0.5), 1.0)));
        assert!(cross(square(DVec2::new(1.0 - 3.0 * MODEL_TOLERANCE, 0.5), 1.0)));
    }

    /// PRF-004: edges of the same region never count, however they lie.
    #[test]
    fn only_edges_of_different_regions_count() {
        let a = square(DVec2::ZERO, 1.0);
        let b = square(DVec2::splat(0.5), 1.0);
        let mut both = edges(&a);
        both.extend(edges(&b));
        assert!(!cross_deeply(both.clone(), Vec::new()));
        assert!(!cross_deeply(Vec::new(), both));
    }
}
