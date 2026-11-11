//! Which regions of a custom profile overlap (PRF-004), in time that grows
//! with their points and with the crossings of their outlines up to a
//! limit, beyond which it gives up (NFR-SEC-003).
//!
//! Two regions overlap when an edge of one crosses an edge of the other
//! *deeply*, each having an end point more than [`MODEL_TOLERANCE`] on either
//! side of the other's line, or when their intersection is larger than a
//! strip `MODEL_TOLERANCE` wide along the shorter outer ring. Regions that
//! touch, sharing edges or points or lying in each other's holes, do not. A
//! boolean intersection takes time with the crossings, millions for two
//! crossed combs, and in geo 0.33 with the square of the points for long
//! slanted edges even where nothing crosses: 0.4 s for a comb of 10,000
//! points touching a block. [`first_overlap`] instead
//!
//! 1. puts the points on a grid of 2^-26 m (15 nm), where the tests below are
//!    exact in 128-bit integers. A valid region keeps its shape there: its
//!    points and the edges that do not follow each other lie at least the
//!    tolerance apart and its corners are at least 0.1°, far more than 15 nm
//!    can change.
//! 2. sweeps a line once across the points of all regions in the order of x,
//!    then y, keeping each region's edges that cross it in order of height.
//!    Edges of one region never cross, so no order needs swaps. At each end
//!    point, against each other region whose x-range it lies in, and at each
//!    crossing found so far, the edges of the two regions through the point
//!    and next above and below it are tested against each other. A deep
//!    crossing settles that pair of regions; any other crossing becomes a
//!    point that the sweep visits.
//! 3. sums, for each pair of regions whose boxes overlap and that no deep
//!    crossing settled, the area of their intersection by Green's theorem
//!    over the pieces into which their crossings and the points where their
//!    outlines touch cut the edges: the pieces of either outline inside the
//!    other region, and those of the first that run along an edge of the
//!    second the same way. Which side of the other region a piece runs on,
//!    and the order of the pieces along an edge, are exact; only the sum is
//!    in floating point.
//!
//! The sweep finds every crossing of two regions before it passes it. Were Z
//! the first one it misses, of edges e of one region and f of another, take
//! the region between e and f left of Z. An edge of the two inside it can
//! leave it only at a point the sweep visits: its end point, or its crossing
//! with e or f before Z, which the sweep finds in time. At the last such
//! point in the region before Z, which lies in the x-ranges of e and f, no
//! edge of the two lies between it and e or f, so e and f are tested there.
//! A crossing that is not deep has an end point of one edge within the
//! tolerance of the other edge (where the end point near the other edge's
//! line lies beyond that edge, that edge's own end point lies nearer still),
//! and a point lies within the tolerance of at most 21 edges of a valid
//! region (the nearest points of edges that share no point lie pairwise the
//! tolerance apart, so there are at most 7 such edges). That bounds a pair's
//! crossings at 42 per point of the two, which still lets 32 regions that
//! copy each other within the tolerance cross millions of times, where
//! regions that only touch cross a few times per point they share. So the
//! sweep gives up after `limit` crossings that are not deep. With n points
//! in R regions and c crossings, each point and crossing finds its place on
//! up to R lines and tests a few edges there: O((R n + c) log n), and adding
//! or removing an edge, once for each edge, moves up to n indices of its
//! region's line.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::ops::Range;

use glam::{DVec2, I64Vec2};

use crate::tolerance::MODEL_TOLERANCE;

/// Grid cells per metre, 2^26. Points less than 16 m (2^30 cells) right of
/// and above the origin keep every product below within `i128`.
const SCALE: f64 = (1u64 << 26) as f64;

/// The rings of a valid region on the grid. Edge `e` runs from point `e` to
/// point `next[e]`, and the region lies on its left.
#[derive(Debug)]
pub(super) struct GridRegion {
    points: Vec<I64Vec2>,
    next: Vec<usize>,
    prev: Vec<usize>,
    /// The smallest and largest corner of the box of its points.
    bounds: (I64Vec2, I64Vec2),
}

impl GridRegion {
    /// The rings of a valid region, outer ring counter-clockwise and holes
    /// clockwise, whose points lie up to 16 m right of and above `origin`.
    pub(super) fn new<'a>(rings: impl IntoIterator<Item = &'a [DVec2]>, origin: DVec2) -> Self {
        let mut region = GridRegion {
            points: Vec::new(),
            next: Vec::new(),
            prev: Vec::new(),
            bounds: (I64Vec2::MAX, I64Vec2::MIN),
        };
        for ring in rings {
            let (start, n) = (region.points.len(), ring.len());
            for (i, &p) in ring.iter().enumerate() {
                let p = ((p - origin) * SCALE).round();
                let p = I64Vec2::new(p.x as i64, p.y as i64);
                region.bounds = (region.bounds.0.min(p), region.bounds.1.max(p));
                region.points.push(p);
                region.next.push(start + (i + 1) % n);
                region.prev.push(start + (i + n - 1) % n);
            }
        }
        region
    }

    /// The end points of edge `e`, first the one the sweep meets first.
    fn ends(&self, e: usize) -> (I64Vec2, I64Vec2) {
        let (a, b) = (self.points[e], self.points[self.next[e]]);
        if lex(a, b) == Ordering::Less { (a, b) } else { (b, a) }
    }

    /// Edge `e` as a vector, in the direction of its ring.
    fn dir(&self, e: usize) -> I64Vec2 {
        self.points[self.next[e]] - self.points[e]
    }

    /// Which way an edge of this region leaving its point `w` in direction
    /// `d` runs: into the region, out of it, or along one of its edges at `w`.
    fn corner(&self, w: usize, d: I64Vec2) -> Side {
        let out = self.dir(w);
        let back = self.points[self.prev[w]] - self.points[w];
        let along = |a: I64Vec2| cross(a, d) == 0 && dot(a, d) > 0;
        if along(out) {
            return Side::Along;
        }
        if along(back) {
            return Side::Against;
        }
        // The region lies counter-clockwise from `out` to `back`.
        let (turn, from_out, to_back) = (cross(out, back), cross(out, d), cross(d, back));
        let inside = match turn.cmp(&0) {
            Ordering::Greater => from_out > 0 && to_back > 0,
            Ordering::Less => from_out > 0 || to_back > 0,
            Ordering::Equal => from_out > 0,
        };
        if inside { Side::Inside } else { Side::Outside }
    }
}

/// What [`first_overlap`] finds.
#[derive(Debug, PartialEq)]
pub(super) enum Overlap {
    /// No two regions overlap.
    None,
    /// The first two regions, by their indices, that overlap.
    Regions(usize, usize),
    /// The outlines cross less than the tolerance deep in more places than
    /// the limit.
    TooManyCrossings,
}

/// The first pair of valid `regions`, by their indices, that overlap (see
/// the module documentation), unless their outlines cross in more than
/// `limit` places that are not deep. `strip(i, j)` is the largest area
/// regions `i` and `j` may share, in square metres.
pub(super) fn first_overlap(regions: &[GridRegion], limit: usize, strip: impl Fn(usize, usize) -> f64) -> Overlap {
    let mut sweep = Sweep::new(regions, limit);
    sweep.run();
    if sweep.crossings.len() > limit {
        return Overlap::TooManyCrossings;
    }
    let n = regions.len();
    for i in 0..n {
        for j in i + 1..n {
            let overlaps = match &sweep.pairs[i * n + j] {
                None => false,
                Some(pair) if pair.deep => true,
                Some(_) => sweep.shared_area(i, j) > strip(i, j),
            };
            if overlaps {
                return Overlap::Regions(i, j);
            }
        }
    }
    Overlap::None
}

/// The pairs of edges the sweep of [`first_overlap`] tests on `regions`,
/// and the crossings that are not deep it finds, up to `limit`.
#[cfg(test)]
pub(super) fn sweep_work(regions: &[GridRegion], limit: usize) -> (usize, usize) {
    let mut sweep = Sweep::new(regions, limit);
    sweep.run();
    (sweep.tested, sweep.crossings.len())
}

/// The order in which the sweep meets grid points: by x, then by y.
fn lex(a: I64Vec2, b: I64Vec2) -> Ordering {
    a.x.cmp(&b.x).then(a.y.cmp(&b.y))
}

/// The cross and dot products of differences of grid points, exact in
/// `i64`: their coordinates are below 2^30 in magnitude.
fn cross(a: I64Vec2, b: I64Vec2) -> i128 {
    (a.x * b.y - a.y * b.x) as i128
}

fn dot(a: I64Vec2, b: I64Vec2) -> i128 {
    (a.x * b.x + a.y * b.y) as i128
}

/// Whether `a` and `b` have opposite signs, neither zero.
fn opposite(a: i128, b: i128) -> bool {
    (a > 0 && b < 0) || (a < 0 && b > 0)
}

/// A point the sweep visits, `num / den` with `den` > 0: an end point, with
/// `den` 1, or the crossing of two edges.
#[derive(Clone, Copy, Debug)]
struct Probe {
    num: [i128; 2],
    den: i128,
}

impl Probe {
    fn at(p: I64Vec2) -> Self {
        Probe {
            num: [p.x as i128, p.y as i128],
            den: 1,
        }
    }

    /// The crossing of the lines through `p0`, `p1` and `q0`, `q1`, which
    /// are not parallel.
    fn crossing((p0, p1): (I64Vec2, I64Vec2), (q0, q1): (I64Vec2, I64Vec2)) -> Self {
        let (d, g) = (p1 - p0, q1 - q0);
        let (mut den, mut t) = (cross(d, g), cross(q0 - p0, g));
        if den < 0 {
            (den, t) = (-den, -t);
        }
        Probe {
            num: [
                p0.x as i128 * den + d.x as i128 * t,
                p0.y as i128 * den + d.y as i128 * t,
            ],
            den,
        }
    }

    /// The order in which the sweep meets the probe and grid point `p`.
    fn cmp_point(&self, p: I64Vec2) -> Ordering {
        let (x, y) = (p.x as i128 * self.den, p.y as i128 * self.den);
        self.num[0].cmp(&x).then(self.num[1].cmp(&y))
    }

    /// Where the probe lies against the line from `a` to `b`: `Greater` on
    /// its left, which is above it if `b` comes after `a` in the sweep.
    fn side(&self, (a, b): (I64Vec2, I64Vec2)) -> Ordering {
        let d = b - a;
        let q = [
            self.num[0] - a.x as i128 * self.den,
            self.num[1] - a.y as i128 * self.den,
        ];
        (d.x as i128 * q[1] - d.y as i128 * q[0]).cmp(&0)
    }

    fn approx(&self) -> DVec2 {
        DVec2::new(self.num[0] as f64, self.num[1] as f64) / self.den as f64
    }
}

/// Where a point of one region lies against the other region.
#[derive(Clone, Copy, Debug)]
enum Contact {
    Inside,
    Outside,
    /// Inside the edge with this index.
    OnEdge(usize),
    /// At the point with this index.
    AtPoint(usize),
}

/// Where the outline of the other region meets an edge between its ends.
#[derive(Clone, Copy, Debug)]
enum Split {
    /// It crosses the other region's edge with this index there.
    Crossing(usize, Probe),
    /// The other region's point with this index lies on it.
    Point(usize),
}

/// Where a piece of an edge runs against the other region.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Side {
    Inside,
    Outside,
    /// Along an edge of the other region, the same way.
    Along,
    /// Along an edge of the other region, the other way.
    Against,
}

/// Two regions whose boxes overlap, as the sweep finds them.
struct Pair {
    /// An edge of the first crosses an edge of the second deeply.
    deep: bool,
    /// Where each region's points lie against the other region.
    contacts: [Vec<Contact>; 2],
    /// Where the other region's outline meets each region's edges.
    splits: [Vec<(usize, Split)>; 2],
}

struct Sweep<'a> {
    regions: &'a [GridRegion],
    /// The distinct points of all regions in the order the sweep meets them.
    points: Vec<I64Vec2>,
    /// The regions with a point at each of them, with the point's index:
    /// those of point `i` are `at[starts[i]..starts[i + 1]]`.
    at: Vec<(usize, usize)>,
    starts: Vec<usize>,
    /// Each region's edges that cross the sweep line, from the lowest up.
    lines: Vec<Vec<usize>>,
    /// The pair of regions `i` < `j` at `i * regions.len() + j`, if their
    /// boxes overlap.
    pairs: Vec<Option<Pair>>,
    /// How many pairs no deep crossing settled yet.
    open: usize,
    /// The most crossings that are not deep to find before giving up.
    limit: usize,
    /// The crossings found: a region, its edge, a region of higher index
    /// and its edge.
    crossings: HashSet<[usize; 4]>,
    /// The crossings to visit before each point, the first after them.
    pending: Vec<Vec<[usize; 4]>>,
    /// How many pairs of edges [`Sweep::test`] has tested.
    #[cfg(test)]
    tested: usize,
}

impl<'a> Sweep<'a> {
    fn new(regions: &'a [GridRegion], limit: usize) -> Self {
        let mut all: Vec<(I64Vec2, usize, usize)> = regions
            .iter()
            .enumerate()
            .flat_map(|(r, region)| region.points.iter().enumerate().map(move |(v, &p)| (p, r, v)))
            .collect();
        all.sort_unstable_by(|a, b| lex(a.0, b.0).then(a.1.cmp(&b.1)));
        let (mut points, mut at, mut starts) = (Vec::new(), Vec::with_capacity(all.len()), Vec::new());
        for (p, r, v) in all {
            if points.last() != Some(&p) {
                points.push(p);
                starts.push(at.len());
            }
            at.push((r, v));
        }
        starts.push(at.len());
        let n = regions.len();
        let mut pairs: Vec<Option<Pair>> = (0..n * n).map(|_| None).collect();
        let mut open = 0;
        for i in 0..n {
            for j in i + 1..n {
                let ((a_min, a_max), (b_min, b_max)) = (regions[i].bounds, regions[j].bounds);
                if a_max.cmplt(b_min).any() || b_max.cmplt(a_min).any() {
                    continue;
                }
                pairs[i * n + j] = Some(Pair {
                    deep: false,
                    contacts: [i, j].map(|r| vec![Contact::Outside; regions[r].points.len()]),
                    splits: [Vec::new(), Vec::new()],
                });
                open += 1;
            }
        }
        let pending = vec![Vec::new(); points.len()];
        Sweep {
            regions,
            points,
            at,
            starts,
            lines: vec![Vec::new(); n],
            pairs,
            open,
            limit,
            crossings: HashSet::new(),
            pending,
            #[cfg(test)]
            tested: 0,
        }
    }

    /// The pair of regions `r` and `s`, in either order, if their boxes
    /// overlap and no deep crossing settled it.
    fn open_pair(&mut self, r: usize, s: usize) -> Option<&mut Pair> {
        let n = self.regions.len();
        self.pairs[r.min(s) * n + r.max(s)].as_mut().filter(|pair| !pair.deep)
    }

    /// Meets every point in turn, and before each one the crossings found
    /// before it, until no pair is left open or more crossings than the
    /// limit are found.
    fn run(&mut self) {
        for i in 0..self.points.len() {
            if self.open == 0 || self.crossings.len() > self.limit {
                return;
            }
            while let Some([r, e, s, f]) = self.pending[i].pop() {
                let probe = Probe::crossing(self.regions[r].ends(e), self.regions[s].ends(f));
                let near = |t: usize| self.near(t, self.locate(t, &probe));
                let (first, second) = (near(r), near(s));
                self.visit((r, first), (s, second), i);
            }
            self.meet(i);
        }
    }

    /// The places on region `r`'s line from which edges do not lie below
    /// `probe`, and from which they lie above it.
    fn locate(&self, r: usize, probe: &Probe) -> (usize, usize) {
        let (line, region) = (&self.lines[r], &self.regions[r]);
        let lo = line.partition_point(|&e| probe.side(region.ends(e)) == Ordering::Greater);
        let hi = lo + line[lo..].partition_point(|&e| probe.side(region.ends(e)) == Ordering::Equal);
        (lo, hi)
    }

    /// Moves the sweep line to point `i`: the edges ending there leave it,
    /// those starting there join it, and the point's contacts with the
    /// regions whose x-range it lies in are noted.
    fn meet(&mut self, i: usize) {
        let p = self.points[i];
        let probe = Probe::at(p);
        let here = self.at[self.starts[i]..self.starts[i + 1]].to_vec();
        for &(r, v) in &here {
            let region = &self.regions[r];
            // Edges of one region pass through its own points only if they
            // end there.
            let (lo, hi) = self.locate(r, &probe);
            self.lines[r].drain(lo..hi);
            let mut starting: Vec<usize> = [region.prev[v], v]
                .into_iter()
                .filter(|&e| region.ends(e).0 == p)
                .collect();
            if let [lower, upper] = starting[..] {
                let (l, u) = (region.ends(lower), region.ends(upper));
                if cross(l.1 - l.0, u.1 - u.0) < 0 {
                    starting.swap(0, 1);
                }
            }
            self.lines[r].splice(lo..lo, starting);
        }
        for &(r, v) in &here {
            let own = self.near(r, self.locate(r, &probe));
            for s in 0..self.regions.len() {
                let (min, max) = self.regions[s].bounds;
                if s == r || p.x < min.x || p.x > max.x || self.open_pair(r, s).is_none() {
                    continue;
                }
                let w = here.iter().find(|&&(t, _)| t == s).map(|&(_, w)| w);
                let (lo, hi) = self.locate(s, &probe);
                let contact = match w {
                    Some(w) => Contact::AtPoint(w),
                    None if hi > lo => Contact::OnEdge(self.lines[s][lo]),
                    // The region lies above an edge that runs to the right.
                    None if lo > 0 && self.regions[s].dir(self.lines[s][lo - 1]).x > 0 => Contact::Inside,
                    None => Contact::Outside,
                };
                let side = usize::from(r > s);
                let Some(pair) = self.open_pair(r, s) else { continue };
                pair.contacts[side][v] = contact;
                if let Contact::OnEdge(f) = contact {
                    pair.splits[1 - side].push((f, Split::Point(v)));
                }
                // Two regions with a point here are tested once.
                if w.is_none() || r < s {
                    let other = self.near(s, (lo, hi));
                    self.visit((r, own.clone()), (s, other), i + 1);
                }
            }
        }
    }

    /// The places on region `t`'s line of the edges through a point and next
    /// above and below it, from where the point lies on it, [`Sweep::locate`].
    fn near(&self, t: usize, (lo, hi): (usize, usize)) -> Range<usize> {
        lo.saturating_sub(1)..(hi + 1).min(self.lines[t].len())
    }

    /// Tests the edges of two regions at their places on the lines, those
    /// through a point and next above and below it, against each other.
    /// Crossings found must lie after point `next - 1`.
    fn visit(&mut self, a: (usize, Range<usize>), b: (usize, Range<usize>), next: usize) {
        let ((r, first), (s, second)) = if a.0 < b.0 { (a, b) } else { (b, a) };
        for i in first {
            for j in second.clone() {
                if self.open_pair(r, s).is_none() || self.crossings.len() > self.limit {
                    return;
                }
                self.test([r, self.lines[r][i], s, self.lines[s][j]], next);
            }
        }
    }

    /// Tests edge `e` of region `r` against edge `f` of region `s` > `r`: a
    /// deep crossing settles the pair, another crossing is noted and, if
    /// new, visited later.
    fn test(&mut self, key: [usize; 4], next: usize) {
        #[cfg(test)]
        {
            self.tested += 1;
        }
        let [r, e, s, f] = key;
        let ((p0, p1), (q0, q1)) = (self.regions[r].ends(e), self.regions[s].ends(f));
        let (s0, s1) = (cross(q1 - q0, p0 - q0), cross(q1 - q0, p1 - q0));
        let (t0, t1) = (cross(p1 - p0, q0 - p0), cross(p1 - p0, q1 - p0));
        if !opposite(s0, s1) || !opposite(t0, t1) || self.crossings.contains(&key) {
            return;
        }
        // The distances of the end points from the other edge's line, on
        // the grid.
        let depth = |a: i128, b: i128, edge: I64Vec2| a.abs().min(b.abs()) as f64 / edge.as_dvec2().length();
        let tolerance = MODEL_TOLERANCE * SCALE;
        let deep = depth(s0, s1, q1 - q0) > tolerance && depth(t0, t1, p1 - p0) > tolerance;
        let probe = Probe::crossing((p0, p1), (q0, q1));
        let Some(pair) = self.open_pair(r, s) else { return };
        if deep {
            pair.deep = true;
            self.open -= 1;
            return;
        }
        pair.splits[0].push((e, Split::Crossing(f, probe)));
        pair.splits[1].push((f, Split::Crossing(e, probe)));
        self.crossings.insert(key);
        let at = self
            .points
            .partition_point(|&p| probe.cmp_point(p) == Ordering::Greater);
        // The sweep finds every crossing before it passes it (see the module
        // documentation); tests check that here.
        debug_assert!(at >= next, "the sweep passed a crossing before finding it");
        self.pending[at.max(next)].push(key);
    }

    /// The area regions `i` < `j` both cover, in square metres, once the
    /// sweep met every point and found no deep crossing between them.
    fn shared_area(&mut self, i: usize, j: usize) -> f64 {
        let (all, n) = (self.regions, self.regions.len());
        let Some(pair) = self.pairs[i * n + j].as_mut() else {
            // Their boxes lie apart.
            return 0.0;
        };
        let regions = [&all[i], &all[j]];
        // Areas from the middle of the two boxes, to keep the products small.
        let (lo, hi) = (
            regions[0].bounds.0.min(regions[1].bounds.0),
            regions[0].bounds.1.max(regions[1].bounds.1),
        );
        let middle = (lo.as_dvec2() + hi.as_dvec2()) / 2.0;
        let mut twice = 0.0;
        for r in 0..2 {
            let (region, other) = (regions[r], regions[1 - r]);
            let position = |s: &Split| match *s {
                Split::Crossing(_, probe) => probe.approx(),
                Split::Point(w) => other.points[w].as_dvec2(),
            };
            // Each piece takes its side from the split it starts at, so the
            // splits of an edge are put in order exactly: by the fraction of
            // the edge from its first point to them, `num / den`.
            let fraction = |e: usize, s: &Split| {
                let (start, d) = (region.points[e], region.dir(e));
                match *s {
                    Split::Crossing(f, _) => {
                        let (q0, g) = (other.points[f], other.dir(f));
                        let (num, den) = (cross(q0 - start, g), cross(d, g));
                        if den < 0 { (-num, -den) } else { (num, den) }
                    }
                    Split::Point(w) => (dot(other.points[w] - start, d), dot(d, d)),
                }
            };
            let mut splits: Vec<(usize, (i128, i128), Split)> = std::mem::take(&mut pair.splits[r])
                .into_iter()
                .map(|(e, s)| (e, fraction(e, &s), s))
                .collect();
            splits.sort_unstable_by(|(e, (a, b), _), (f, (c, d), _)| e.cmp(f).then((a * d).cmp(&(c * b))));
            let mut splits = splits.iter().map(|(e, _, s)| (*e, *s)).peekable();
            for e in 0..region.points.len() {
                let d = region.dir(e);
                let mut side = match pair.contacts[r][e] {
                    Contact::Inside => Side::Inside,
                    Contact::Outside => Side::Outside,
                    Contact::OnEdge(f) => along_edge(other.dir(f), d),
                    Contact::AtPoint(w) => other.corner(w, d),
                };
                let mut from = region.points[e].as_dvec2() - middle;
                let mut piece = |side: Side, to: DVec2| {
                    if side == Side::Inside || (side == Side::Along && r == 0) {
                        twice += from.perp_dot(to);
                    }
                    from = to;
                };
                while let Some((_, split)) = splits.next_if(|(f, _)| *f == e) {
                    piece(side, position(&split) - middle);
                    side = match split {
                        Split::Crossing(f, _) => along_edge(other.dir(f), d),
                        Split::Point(w) => other.corner(w, d),
                    };
                }
                piece(side, region.points[region.next[e]].as_dvec2() - middle);
            }
        }
        twice / 2.0 / (SCALE * SCALE)
    }
}

/// Which way direction `d` runs from a point inside an edge of the other
/// region running in direction `g`: into the region on its left, out of it,
/// or along it.
fn along_edge(g: I64Vec2, d: I64Vec2) -> Side {
    match cross(g, d).cmp(&0) {
        Ordering::Greater => Side::Inside,
        Ordering::Less => Side::Outside,
        Ordering::Equal if dot(g, d) > 0 => Side::Along,
        Ordering::Equal => Side::Against,
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::TAU;

    use super::*;

    fn region(rings: &[Vec<DVec2>]) -> GridRegion {
        GridRegion::new(rings.iter().map(Vec::as_slice), DVec2::splat(-1.0))
    }

    fn square(min: DVec2, size: f64) -> Vec<DVec2> {
        vec![
            min,
            min + DVec2::new(size, 0.0),
            min + DVec2::splat(size),
            min + DVec2::new(0.0, size),
        ]
    }

    /// The area two regions share, or `None` for a deep crossing.
    fn shared(a: &[Vec<DVec2>], b: &[Vec<DVec2>]) -> Option<f64> {
        let regions = [region(a), region(b)];
        let mut sweep = Sweep::new(&regions, usize::MAX);
        sweep.run();
        match &sweep.pairs[1] {
            None => Some(0.0),
            Some(pair) if pair.deep => None,
            Some(_) => Some(sweep.shared_area(0, 1)),
        }
    }

    /// PRF-004: squares overlapping at a corner or crossed by a plate cross
    /// deeply; squares that share an edge, part of one or a corner, one in
    /// the other or in the other's hole, and two whose edges cross less
    /// than MODEL_TOLERANCE from an end do not, and share their common area.
    #[test]
    fn deep_crossings_are_told_from_touching_edges() {
        let a = vec![square(DVec2::ZERO, 1.0)];
        assert_eq!(shared(&a, &[square(DVec2::splat(0.5), 1.0)]), None);
        let plate = vec![
            DVec2::new(-0.1, 0.4),
            DVec2::new(1.1, 0.4),
            DVec2::new(1.1, 0.6),
            DVec2::new(-0.1, 0.6),
        ];
        assert_eq!(shared(&a, &[plate]), None);
        let close = |area: Option<f64>, expected: f64| area.is_some_and(|area| (area - expected).abs() < 1e-7);
        assert!(close(shared(&a, &[square(DVec2::new(1.0, 0.0), 1.0)]), 0.0));
        assert!(close(shared(&a, &[square(DVec2::new(1.0, 0.5), 1.0)]), 0.0));
        assert!(close(shared(&a, &[square(DVec2::splat(1.0), 1.0)]), 0.0));
        assert!(close(shared(&a, &[square(DVec2::splat(0.25), 0.5)]), 0.25));
        assert!(close(shared(&a, &a), 1.0));
        let mut hole = square(DVec2::splat(0.2), 0.6);
        hole.reverse();
        let holed = vec![square(DVec2::ZERO, 1.0), hole];
        assert!(close(shared(&holed, &[square(DVec2::splat(0.3), 0.4)]), 0.0));
        assert!(close(shared(&holed, &[square(DVec2::splat(0.2), 0.6)]), 0.0));
        assert!(close(shared(&holed, &a), 0.64));
        let shallow = 0.5 * MODEL_TOLERANCE;
        let nudged = square(DVec2::new(1.0 - shallow, 0.5), 1.0);
        assert!(close(shared(&a, &[nudged]), shallow * 0.5));
        assert_eq!(
            shared(&a, &[square(DVec2::new(1.0 - 3.0 * MODEL_TOLERANCE, 0.5), 1.0)]),
            None
        );
    }

    /// The outer ring and the holes of a region.
    type Region = (Vec<DVec2>, Vec<Vec<DVec2>>);

    /// PRF-004, NFR-SEC-003: the sweep gives up once more crossings that
    /// are not deep than the limit are found: two squares that cross
    /// shallowly twice are checked with a limit of 2 and not with 1.
    #[test]
    fn crossings_beyond_the_limit_are_reported() {
        let regions = [
            region(&[square(DVec2::ZERO, 1.0)]),
            region(&[square(DVec2::new(1.0 - 0.5 * MODEL_TOLERANCE, 0.5), 1.0)]),
        ];
        let strip = |_, _| 1e-4;
        assert_eq!(first_overlap(&regions, 2, strip), Overlap::None);
        assert_eq!(first_overlap(&regions, 1, strip), Overlap::TooManyCrossings);
        assert_eq!(first_overlap(&regions, 2, |_, _| 1e-6), Overlap::Regions(0, 1));
    }

    /// PRF-004: crossings of an edge with both edges at a point of the
    /// other region that lies 1e-17 m from it, 1e-9 grid cells apart, keep
    /// their order along the edge, which a floating-point order swaps,
    /// giving the rest of the edge the wrong side: a region reaching into
    /// a triangle there shares 0.013 m² with it, and a small triangle whose
    /// corner lies there only touches it.
    #[test]
    fn crossings_a_hair_apart_keep_their_order() {
        let ring = |points: &[(i64, i64)]| -> Vec<DVec2> {
            let ring: Vec<DVec2> = points
                .iter()
                .map(|&(x, y)| DVec2::new(x as f64, y as f64) / SCALE)
                .collect();
            super::super::validate_ring(&ring).unwrap()
        };
        let hair = [
            (288_805_479, 126_864_386),
            (293_135_622, 128_805_485),
            (290_000_001, 130_000_007),
        ];
        let triangle = |top: i64| ring(&[(0, 0), (566_263_160, 253_842_119), (0, top)]);
        let reaching = ring(&[
            (288_805_479, 10_000_000),
            (660_000_000, 10_000_000),
            (660_000_000, 200_000_000),
            (530_000_000, 200_000_000),
            (505_000_000, 226_379_221),
            (505_000_000, 230_000_000),
            (495_000_000, 230_000_000),
            (495_000_000, 221_896_462),
            hair[1],
            hair[2],
            hair[0],
        ]);
        let shared = |a: &[DVec2], b: &[DVec2]| {
            let regions = [GridRegion::new([a], DVec2::ZERO), GridRegion::new([b], DVec2::ZERO)];
            let mut sweep = Sweep::new(&regions, usize::MAX);
            sweep.run();
            assert!(!sweep.pairs[1].as_ref().unwrap().deep);
            assert!(!sweep.crossings.is_empty());
            sweep.shared_area(0, 1)
        };
        let area = shared(&triangle(200_000_000), &reaching);
        assert!((area - 0.0130).abs() < 1e-4, "{area}");
        let area = shared(&triangle(600_000_000), &ring(&hair));
        assert!(area.abs() < 1e-9, "{area}");
    }

    /// A small deterministic random generator (splitmix64).
    struct Random(u64);

    impl Random {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }

        fn pick<T: Copy>(&mut self, items: &[T]) -> T {
            items[self.below(items.len())]
        }

        /// A number from -1 to 1.
        fn signed(&mut self) -> f64 {
            (self.next() >> 11) as f64 / (1_u64 << 52) as f64 - 1.0
        }
    }

    /// A random region about the origin, its outer ring and holes: a star
    /// of 3 to 14 points or a rectangle, with points on a grid of 0.25 m,
    /// where outlines meet in every way, or anywhere; sometimes with a hole,
    /// a smaller copy of the ring. It may be invalid.
    fn random_region(random: &mut Random) -> Region {
        let on_grid = random.below(3) > 0;
        let snap = |p: DVec2| if on_grid { (p * 4.0).round() / 4.0 } else { p };
        let outer: Vec<DVec2> = if random.below(3) == 0 {
            let size = DVec2::new(random.signed().abs(), random.signed().abs()) * 2.0 + 0.25;
            square(DVec2::ZERO, 1.0)
                .into_iter()
                .map(|p| snap(p * size - size / 2.0))
                .collect()
        } else {
            let n = 3 + random.below(12);
            (0..n)
                .map(|i| {
                    let angle = TAU * (i as f64 + 0.4 * random.signed()) / n as f64;
                    snap(DVec2::from_angle(angle) * (0.5 + 1.5 * random.signed().abs()))
                })
                .collect()
        };
        let holes = if random.below(4) == 0 {
            vec![outer.iter().rev().map(|&p| snap(p * 0.5)).collect()]
        } else {
            Vec::new()
        };
        (outer, holes)
    }

    /// Whether edges `e` of `a` and `f` of `b` cross, and whether deeply,
    /// by the definitions of the module documentation.
    fn crossing(a: &GridRegion, e: usize, b: &GridRegion, f: usize) -> (bool, bool) {
        let ((p0, p1), (q0, q1)) = (a.ends(e), b.ends(f));
        let side = |a: I64Vec2, b: I64Vec2, p: I64Vec2| cross(b - a, p - a);
        let (s0, s1, t0, t1) = (side(q0, q1, p0), side(q0, q1, p1), side(p0, p1, q0), side(p0, p1, q1));
        let crosses = opposite(s0, s1) && opposite(t0, t1);
        let tolerance = MODEL_TOLERANCE * SCALE;
        let far = |s: i128, edge: I64Vec2| s.abs() as f64 / edge.as_dvec2().length() > tolerance;
        let deep = crosses && far(s0, q1 - q0) && far(s1, q1 - q0) && far(t0, p1 - p0) && far(t1, p1 - p0);
        (crosses, deep)
    }

    /// PRF-004, NFR-SEC-003: on random pairs of regions that touch, share
    /// edges and points, cross shallowly or deeply, lie in each other or in
    /// each other's holes, also turned by random angles, the sweep finds a
    /// deep crossing exactly where testing every pair of edges does,
    /// otherwise every crossing, and the area of the intersection that a
    /// boolean intersection gives.
    #[test]
    fn the_sweep_agrees_with_testing_every_pair_of_edges() {
        use geo::{Area, BooleanOps};
        let mut random = Random(9_212);
        let (mut deep, mut shallow, mut sharing, mut touching) = (0, 0, 0, 0);
        for case in 0..6000 {
            let a = random_region(&mut random);
            let map = |(outer, holes): &Region, f: &dyn Fn(DVec2) -> DVec2| -> Region {
                let ring = |r: &Vec<DVec2>| r.iter().map(|&p| f(p)).collect();
                (ring(outer), holes.iter().map(ring).collect())
            };
            let grid_offset = |random: &mut Random, most: f64| {
                (DVec2::new(random.signed(), random.signed()) * most * 4.0).round() / 4.0
            };
            // Another region, the same one moved, mirrored about the
            // vertical through its rightmost point, or halved towards one of
            // its points.
            let b = match random.below(4) {
                0 => {
                    let by = grid_offset(&mut random, 2.5);
                    map(&random_region(&mut random), &|p| p + by)
                }
                1 => {
                    let by = grid_offset(&mut random, 1.0);
                    map(&a, &|p| p + by)
                }
                2 => {
                    let right = a.0.iter().map(|p| p.x).fold(f64::MIN, f64::max);
                    map(&a, &|p| DVec2::new(2.0 * right - p.x, p.y))
                }
                _ => {
                    let at = a.0[random.below(a.0.len())];
                    map(&a, &|p| at + (p - at) / 2.0)
                }
            };
            // Moved by about the tolerance, and turned.
            let nudge = random.pick(&[0.0, 0.0, 0.5, -0.5, 0.99, 1.01, -3.0]) * MODEL_TOLERANCE;
            let turn = DVec2::from_angle(random.pick(&[0.0, 0.0, 0.0, 1.0, 0.3, 1e-7]) * random.signed());
            let by = DVec2::new(nudge, nudge * 0.5);
            let (a_outer, a_holes) = map(&a, &|p| turn.rotate(p));
            let (b_outer, b_holes) = map(&b, &|p| turn.rotate(p + by));
            let (Ok(a), Ok(b)) = (
                super::super::validate_slab_polygon(&a_outer, &a_holes),
                super::super::validate_slab_polygon(&b_outer, &b_holes),
            ) else {
                continue;
            };
            let grid = |(outer, holes): &Region| {
                let rings = std::iter::once(outer).chain(holes).map(Vec::as_slice);
                GridRegion::new(rings, DVec2::splat(-6.0))
            };
            let regions = [grid(&a), grid(&b)];
            let (ga, gb) = (&regions[0], &regions[1]);
            let mut crossings = HashSet::new();
            let mut any_deep = false;
            for e in 0..ga.points.len() {
                for f in 0..gb.points.len() {
                    let (crosses, is_deep) = crossing(ga, e, gb, f);
                    any_deep |= is_deep;
                    if crosses {
                        crossings.insert((e, f));
                    }
                }
            }
            let mut sweep = Sweep::new(&regions, usize::MAX);
            sweep.run();
            let found_deep = sweep.pairs[1].as_ref().is_some_and(|pair| pair.deep);
            assert_eq!(found_deep, any_deep, "case {case}: deep crossing");
            if any_deep {
                deep += 1;
                continue;
            }
            let found: HashSet<(usize, usize)> = sweep.crossings.iter().map(|&[_, e, _, f]| (e, f)).collect();
            assert_eq!(found, crossings, "case {case}: crossings");
            let polygon = |(outer, holes): &Region| {
                let ring = |r: &Vec<DVec2>| geo::LineString::from(r.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>());
                geo::Polygon::new(ring(outer), holes.iter().map(ring).collect())
            };
            let expected = polygon(&a).intersection(&polygon(&b)).unsigned_area();
            let Some(pair) = &sweep.pairs[1] else {
                // Boxes apart.
                assert!(expected < 1e-9, "case {case}: {expected}");
                continue;
            };
            let contact = pair.splits.iter().any(|s| !s.is_empty())
                || pair
                    .contacts
                    .iter()
                    .flatten()
                    .any(|c| matches!(c, Contact::OnEdge(_) | Contact::AtPoint(_)));
            let area = sweep.shared_area(0, 1);
            assert!(
                (area - expected).abs() < 1e-6,
                "case {case}: area {area}, expected {expected}"
            );
            shallow += usize::from(!crossings.is_empty());
            sharing += usize::from(expected > 1e-3);
            touching += usize::from(crossings.is_empty() && expected < 1e-3 && contact);
        }
        assert!(
            deep > 2000 && shallow > 400 && sharing > 500 && touching > 300,
            "{deep} deep, {shallow} shallow, {sharing} sharing, {touching} touching"
        );
    }

    /// PRF-004, NFR-SEC-003: one sweep over several regions finds, for
    /// each pair, the deep crossings, the crossings and the shared area that
    /// a sweep over the pair alone finds, also where points of several
    /// regions coincide.
    #[test]
    fn one_sweep_over_all_regions_agrees_with_one_per_pair() {
        let mut random = Random(5_077);
        let (mut deep, mut sharing, mut touching) = (0, 0, 0);
        for case in 0..1500 {
            let (mut shapes, mut regions): (Vec<Region>, Vec<GridRegion>) = (Vec::new(), Vec::new());
            while regions.len() < 4 {
                // A new region, a copy of an earlier one moved on the grid
                // or one halved towards one of its points.
                let map = |(outer, holes): &Region, f: &dyn Fn(DVec2) -> DVec2| -> Region {
                    let ring = |r: &Vec<DVec2>| r.iter().map(|&p| f(p)).collect();
                    (ring(outer), holes.iter().map(ring).collect())
                };
                let earlier = (!shapes.is_empty()).then(|| shapes[random.below(shapes.len())].clone());
                let shape = match (earlier, random.below(3)) {
                    (Some(earlier), 1) => {
                        let by = (DVec2::new(random.signed(), random.signed()) * 8.0).round() / 4.0;
                        map(&earlier, &|p| p + by)
                    }
                    (Some(earlier), 2) => {
                        let at = earlier.0[random.below(earlier.0.len())];
                        map(&earlier, &|p| at + (p - at) / 2.0)
                    }
                    _ => {
                        let by = (DVec2::new(random.signed(), random.signed()) * 14.0).round() / 4.0;
                        map(&random_region(&mut random), &|p| p + by)
                    }
                };
                if let Ok((outer, holes)) = super::super::validate_slab_polygon(&shape.0, &shape.1) {
                    let rings = std::iter::once(&outer).chain(&holes).map(Vec::as_slice);
                    regions.push(GridRegion::new(rings, DVec2::splat(-6.0)));
                    shapes.push((outer, holes));
                }
            }
            let mut all = Sweep::new(&regions, usize::MAX);
            all.run();
            let n = regions.len();
            for i in 0..n {
                for j in i + 1..n {
                    let two = [&regions[i], &regions[j]].map(|r| GridRegion {
                        points: r.points.clone(),
                        next: r.next.clone(),
                        prev: r.prev.clone(),
                        bounds: r.bounds,
                    });
                    let mut alone = Sweep::new(&two, usize::MAX);
                    alone.run();
                    let (Some(together), Some(apart)) = (&all.pairs[i * n + j], &alone.pairs[1]) else {
                        assert!(
                            all.pairs[i * n + j].is_none() && alone.pairs[1].is_none(),
                            "case {case}"
                        );
                        continue;
                    };
                    assert_eq!(together.deep, apart.deep, "case {case}: {i}, {j} deep");
                    if apart.deep {
                        deep += 1;
                        continue;
                    }
                    let of = |sweep: &Sweep, a: usize, b: usize| -> HashSet<(usize, usize)> {
                        sweep
                            .crossings
                            .iter()
                            .filter(|k| k[0] == a && k[2] == b)
                            .map(|k| (k[1], k[3]))
                            .collect()
                    };
                    assert_eq!(of(&all, i, j), of(&alone, 0, 1), "case {case}: {i}, {j} crossings");
                    let touches = together.splits.iter().any(|s| !s.is_empty());
                    let (area, expected) = (all.shared_area(i, j), alone.shared_area(0, 1));
                    assert!(
                        (area - expected).abs() < 1e-12,
                        "case {case}: {i}, {j}: {area}, {expected}"
                    );
                    sharing += usize::from(expected > 1e-3);
                    touching += usize::from(expected < 1e-3 && touches);
                }
            }
        }
        assert!(
            deep > 2000 && sharing > 800 && touching > 50,
            "{deep} deep, {sharing} sharing, {touching} touching"
        );
    }
}
