//! Touching edges of polygon rings, found with a sweep line (EDT-018,
//! SLB-003, NFR-SEC-003).
//!
//! Two edges touch when [`segments_touch`] says so: they cross, or an end
//! point of one lies within [`MODEL_TOLERANCE`] of the other. Edges that
//! follow each other in a ring share a point and never count. Testing every
//! pair of edges grows with the square of the points (a slab outline of
//! 32,003 points took 16 s to open), so beyond [`FEW_POINTS`] points
//! [`nesting`] finds a touching pair, if there is one, in three steps that
//! test a few pairs per point ([`by_sweep`]):
//!
//! 1. *Close points:* every two points less than [`NEAR`] apart, found
//!    through a grid of `NEAR` cells, have their edges tested.
//! 2. *Sweep in x* (Shamos and Hoey): the edges crossing a vertical line
//!    that sweeps the points from left to right are kept in order of height,
//!    and two edges are tested whenever they become neighbours on it, which
//!    finds a crossing if there is one. At each point, the edges less than
//!    `NEAR` above or below it are tested against the point's edges.
//! 3. *Sweep in y:* the same on the plane turned by 90°, which tests the
//!    edges less than `NEAR` to the left or right of each point.
//!
//! The steps find every touching pair that does not cross: if an end point
//! `p` lies `d` < `MODEL_TOLERANCE` from an edge `t`, either the point of
//! `t` closest to `p` is an end point of `t` (step 1), or the line of `t`
//! crosses the vertical or the horizontal through `p` less than `√2·d` from
//! `p`; `t` reaches that crossing (step 2 or 3) or ends before it, less than
//! `√2·d` from `p` (step 1). They stop at the first touching pair, and until
//! then points lie at least `MODEL_TOLERANCE` apart and so do edges that do
//! not follow each other: a grid cell holds a few points, and a few edges
//! lie less than `NEAR` from a point. Sorting the points and finding places
//! in the ordered edges take O(n log n); the ordered edges are a ring
//! buffer, so adding or removing one moves up to n / 2 indices.

use std::cmp::Ordering;
use std::collections::{HashMap, VecDeque};

use glam::DVec2;

use super::{point_in_ring, segments_touch};
use crate::tolerance::MODEL_TOLERANCE;

/// Up to this many points in all, [`nesting`] tests every pair of edges
/// ([`every_pair`]): fewer steps than sweeping, which pays off from about 50
/// points.
const FEW_POINTS: usize = 48;

/// How far the steps look around each point: more than √2 times the model
/// tolerance (see the module documentation).
const NEAR: f64 = 1.5 * MODEL_TOLERANCE;

/// Closed rings of points. Edge `e` runs from point `e` to point `next[e]`.
#[derive(Debug)]
pub(super) struct Rings {
    points: Vec<DVec2>,
    next: Vec<usize>,
    prev: Vec<usize>,
    ring: Vec<usize>,
    /// Where each ring's points start, and the number of points at the end.
    starts: Vec<usize>,
    /// How many pairs of edges [`Rings::touch`] has tested.
    #[cfg(test)]
    pub(super) tested: std::cell::Cell<usize>,
}

impl Rings {
    /// The rings as [`super::validate_ring`] returns them: at least three
    /// points each, counter-clockwise, consecutive points at least
    /// [`super::MIN_EDGE_LENGTH`] apart and no corner sharper than
    /// [`super::MIN_CORNER_ANGLE_DEG`].
    pub(super) fn new(rings: &[&[DVec2]]) -> Self {
        let total = rings.iter().map(|r| r.len()).sum();
        let mut out = Rings {
            points: Vec::with_capacity(total),
            next: Vec::with_capacity(total),
            prev: Vec::with_capacity(total),
            ring: Vec::with_capacity(total),
            starts: Vec::with_capacity(rings.len() + 1),
            #[cfg(test)]
            tested: std::cell::Cell::new(0),
        };
        for (index, ring) in rings.iter().enumerate() {
            let (start, n) = (out.points.len(), ring.len());
            out.starts.push(start);
            for (i, &p) in ring.iter().enumerate() {
                out.points.push(p);
                out.next.push(start + (i + 1) % n);
                out.prev.push(start + (i + n - 1) % n);
                out.ring.push(index);
            }
        }
        out.starts.push(total);
        out
    }

    /// The number of rings.
    fn count(&self) -> usize {
        self.starts.len() - 1
    }

    /// The points of ring `r`.
    fn points_of(&self, r: usize) -> &[DVec2] {
        &self.points[self.starts[r]..self.starts[r + 1]]
    }

    /// The ring that edge `e` belongs to.
    pub(super) fn ring(&self, e: usize) -> usize {
        self.ring[e]
    }

    /// The two edges that end at point `v`.
    fn edges_at(&self, v: usize) -> [usize; 2] {
        [self.prev[v], v]
    }

    /// Whether edges `e` and `f` touch: they do not follow each other in a
    /// ring, and [`segments_touch`].
    fn touch(&self, e: usize, f: usize) -> bool {
        let adjacent = e == f || self.next[e] == f || self.next[f] == e;
        #[cfg(test)]
        self.tested.set(self.tested.get() + usize::from(!adjacent));
        !adjacent
            && segments_touch(
                self.points[e],
                self.points[self.next[e]],
                self.points[f],
                self.points[self.next[f]],
            )
    }

    /// `Err((e, f))` if edges `e` and `f` touch.
    fn test(&self, e: usize, f: usize) -> Result<(), (usize, usize)> {
        if self.touch(e, f) { Err((e, f)) } else { Ok(()) }
    }
}

/// How the rings lie in each other: for each ring, the innermost other ring
/// around it, if any. Errs with two edges that touch; only rings whose edges
/// do not touch nest.
pub(super) fn nesting(rings: &Rings) -> Result<Vec<Option<usize>>, (usize, usize)> {
    if rings.points.len() <= FEW_POINTS {
        every_pair(rings)
    } else {
        by_sweep(rings)
    }
}

/// [`nesting`] by testing every pair of edges, and every ring's first point
/// against every other ring.
pub(super) fn every_pair(rings: &Rings) -> Result<Vec<Option<usize>>, (usize, usize)> {
    for e in 0..rings.points.len() {
        for f in e + 1..rings.points.len() {
            rings.test(e, f)?;
        }
    }
    let around = |r: usize| -> Vec<usize> {
        let first = rings.points[rings.starts[r]];
        (0..rings.count())
            .filter(|&s| s != r && point_in_ring(first, rings.points_of(s)))
            .collect()
    };
    let around: Vec<Vec<usize>> = (0..rings.count()).map(around).collect();
    // Of the rings around a ring, the innermost has the most rings around it.
    Ok(around
        .iter()
        .map(|outer| outer.iter().copied().max_by_key(|&s| around[s].len()))
        .collect())
}

/// [`nesting`] by the three steps of the module documentation.
pub(super) fn by_sweep(rings: &Rings) -> Result<Vec<Option<usize>>, (usize, usize)> {
    close_points(rings)?;
    let mut parents = vec![None; rings.count()];
    Sweep::new(rings, |p| p).run(Some(&mut parents))?;
    Sweep::new(rings, |p| DVec2::new(p.y, -p.x)).run(None)?;
    Ok(parents)
}

/// Step 1: tests the edges of every two points less than [`NEAR`] apart.
fn close_points(rings: &Rings) -> Result<(), (usize, usize)> {
    let cell = |p: DVec2| ((p.x / NEAR).floor() as i64, (p.y / NEAR).floor() as i64);
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::with_capacity(rings.points.len());
    for (v, &p) in rings.points.iter().enumerate() {
        let (x, y) = cell(p);
        for key in [x - 1, x, x + 1]
            .into_iter()
            .flat_map(|x| [(x, y - 1), (x, y), (x, y + 1)])
        {
            for &u in grid.get(&key).into_iter().flatten() {
                if p.distance(rings.points[u]) < NEAR {
                    for e in rings.edges_at(v) {
                        for f in rings.edges_at(u) {
                            rings.test(e, f)?;
                        }
                    }
                }
            }
        }
        grid.entry((x, y)).or_default().push(v);
    }
    Ok(())
}

/// Steps 2 and 3: a vertical line sweeping the points, in a frame that is
/// the plane itself or the plane turned by 90°.
struct Sweep<'a> {
    rings: &'a Rings,
    /// The points in the frame.
    at: Vec<DVec2>,
    /// The edges crossing the sweep line, from the lowest up.
    line: VecDeque<usize>,
}

impl<'a> Sweep<'a> {
    fn new(rings: &'a Rings, frame: impl Fn(DVec2) -> DVec2) -> Self {
        // Adding 0.0 turns -0.0 into 0.0, so that the order below agrees
        // with comparing the numbers.
        let at = rings.points.iter().map(|&p| frame(p) + 0.0).collect();
        Sweep {
            rings,
            at,
            line: VecDeque::new(),
        }
    }

    /// The order in which the sweep line meets points `a` and `b`: by x,
    /// then by y, then by index.
    fn order(&self, a: usize, b: usize) -> Ordering {
        let (p, q) = (self.at[a], self.at[b]);
        p.x.total_cmp(&q.x).then(p.y.total_cmp(&q.y)).then(a.cmp(&b))
    }

    /// The other end point of edge `e`, which ends at point `v`.
    fn other_end(&self, e: usize, v: usize) -> usize {
        if e == v { self.rings.next[e] } else { e }
    }

    /// The end points of edge `e`, the one the sweep line meets first first.
    fn ends(&self, e: usize) -> (DVec2, DVec2) {
        let f = self.rings.next[e];
        match self.order(e, f) {
            Ordering::Greater => (self.at[f], self.at[e]),
            _ => (self.at[e], self.at[f]),
        }
    }

    /// The height of edge `e` on the sweep line through `p`, exact at its end
    /// points; for a vertical edge, its height nearest to `p`.
    fn height(&self, e: usize, p: DVec2) -> f64 {
        let (a, b) = self.ends(e);
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

    /// The first place on the line whose edge is not below `p`.
    fn place(&self, p: DVec2) -> usize {
        self.line.partition_point(|&e| self.height(e, p) < p.y)
    }

    /// Meets every point in turn. `parents` receives how the rings nest.
    fn run(mut self, mut parents: Option<&mut Vec<Option<usize>>>) -> Result<(), (usize, usize)> {
        let mut points: Vec<usize> = (0..self.at.len()).collect();
        points.sort_unstable_by(|&a, &b| self.order(a, b));
        let mut met = vec![false; self.rings.count()];
        for v in points {
            let at = self.meet(v)?;
            let ring = self.rings.ring[v];
            if let Some(parents) = parents.as_deref_mut()
                && !met[ring]
            {
                // The ring's first point: the edge right below it tells what
                // it lies in. Rings are counter-clockwise, so the inside of
                // the edge's ring lies above the edge if the edge runs to the
                // right, and below it otherwise.
                parents[ring] = at.checked_sub(1).map(|i| self.line[i]).and_then(|e| {
                    let other = self.rings.ring[e];
                    let rightwards = self.at[self.rings.next[e]].x > self.at[e].x;
                    if rightwards { Some(other) } else { parents[other] }
                });
            }
            met[ring] = true;
        }
        Ok(())
    }

    /// Moves the sweep line to point `v`: its edges that end there leave the
    /// line, those that start there join it, and the edges near `v` are
    /// tested. Returns the place of `v` on the line.
    fn meet(&mut self, v: usize) -> Result<usize, (usize, usize)> {
        let p = self.at[v];
        let edges = self.rings.edges_at(v);
        let starts = |sweep: &Self, e: usize| sweep.order(v, sweep.other_end(e, v)) == Ordering::Less;
        for e in edges {
            if !starts(self, e) {
                self.leave(e, p)?;
            }
        }
        let at = self.place(p);
        let mut joining: Vec<usize> = edges.into_iter().filter(|&e| starts(self, e)).collect();
        if let [lower, upper] = joining[..] {
            let (a, b) = (self.at[self.other_end(lower, v)], self.at[self.other_end(upper, v)]);
            if (a - p).perp_dot(b - p) < 0.0 {
                joining.swap(0, 1);
            }
        }
        let top = at + joining.len();
        for (i, e) in (at..).zip(joining) {
            self.line.insert(i, e);
        }
        if top > at {
            if at > 0 {
                self.rings.test(self.line[at - 1], self.line[at])?;
            }
            if top < self.line.len() {
                self.rings.test(self.line[top - 1], self.line[top])?;
            }
        }
        self.near(v, (0..at).rev())?;
        self.near(v, top..self.line.len())?;
        Ok(at)
    }

    /// Takes edge `e`, which ends at `p`, off the line and tests the edges
    /// that become neighbours.
    fn leave(&mut self, e: usize, p: DVec2) -> Result<(), (usize, usize)> {
        let from = self.place(p);
        let found = self
            .line
            .range(from..)
            .take_while(|&&f| self.height(f, p) == p.y)
            .position(|&f| f == e)
            .map(|i| from + i)
            // Only rounding in an invalid ring can put the edge elsewhere.
            .or_else(|| self.line.iter().position(|&f| f == e));
        if let Some(i) = found {
            self.line.remove(i);
            if i > 0 && i < self.line.len() {
                self.rings.test(self.line[i - 1], self.line[i])?;
            }
        }
        Ok(())
    }

    /// Tests the edges at `places` on the line, nearest to point `v` first,
    /// as long as they lie less than [`NEAR`] above or below it: against the
    /// edges of `v` and against each other.
    fn near(&self, v: usize, places: impl Iterator<Item = usize>) -> Result<(), (usize, usize)> {
        let p = self.at[v];
        let edges = self.rings.edges_at(v);
        let mut seen: Vec<usize> = Vec::new();
        for i in places {
            let t = self.line[i];
            if (self.height(t, p) - p.y).abs() >= NEAR {
                break;
            }
            if edges.contains(&t) {
                continue;
            }
            for &e in edges.iter().chain(&seen) {
                self.rings.test(e, t)?;
            }
            seen.push(t);
        }
        Ok(())
    }
}
