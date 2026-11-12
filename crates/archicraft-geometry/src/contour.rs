//! Contour detection (INP-018): the planar graph of the lines of a plan and
//! the closed regions a click picks from it.
//!
//! A [`PlanSegmentSource`] supplies straight segments in the 2D coordinates
//! of one plane: a storey's [`PlanDrawing`] for now; the 2D drafting curves
//! and a working plane of the 3D views are later sources. [`ContourGraph`]
//! nodes them within [`MODEL_TOLERANCE`] (NFR-ACC-003): endpoints closer than
//! the tolerance are one vertex, crossing points snap to the vertices near
//! them, a segment is split at every vertex that lies on it, and overlapping
//! collinear pieces become one edge. The bounded faces of the resulting
//! half-edge structure, without their antennas, are the regions
//! ([`ContourGraph::region_at`]); a line that joins a face's boundary to a
//! closed loop inside the face (a bridge) is left out of the boundary too,
//! and the loop becomes an island of the face. The runs of edges through the vertices
//! where two edges meet are the chains: the clicked edge
//! ([`ContourGraph::edge_at`], up to the first corner) and the chain of
//! connected edges ([`ContourGraph::chain_at`]).
//!
//! The segments are put into a canonical order before noding, so the graph
//! does not depend on the order or direction in which a source supplies
//! them. Two segments parallel within
//! [`ANGLE_TOLERANCE`](archicraft_model::ANGLE_TOLERANCE) that cross at so
//! shallow an angle that neither has an endpoint on the other are not split
//! at their crossing.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::collections::hash_map::Entry;

use archicraft_model::outline::Ring;
use archicraft_model::tolerance::{Welder, is_parallel};
use archicraft_model::{ElementId, MODEL_TOLERANCE};
use geo::Polygon;
use glam::DVec2;
use rstar::primitives::{GeomWithData, Line, Rectangle};
use rstar::{AABB, RTree};

use crate::plan::{PlanDrawing, polygon_rings, ring_contains, segment_distance};

/// The curve a segment approximates, so that a contour traced over its edges
/// can be rebuilt with the exact curve (INP-018: contour detection preserves
/// arcs). No source supplies curves yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CurveRef {
    /// The element the curve belongs to.
    pub element: ElementId,
    /// The curve's number among the element's curves.
    pub index: u32,
}

/// A straight segment of a source, and an edge of a [`ContourGraph`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub points: [DVec2; 2],
    /// The curve the segment is a piece of; `None` for a straight line.
    pub source: Option<CurveRef>,
}

impl Segment {
    /// A straight segment from `a` to `b`.
    pub fn new(a: DVec2, b: DVec2) -> Self {
        Self {
            points: [a, b],
            source: None,
        }
    }
}

/// Which lines of a source bound regions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContourOptions {
    /// Wall reference lines (WAL-007) as well as the drawn outlines.
    pub reference_lines: bool,
}

/// Supplies the straight segments a [`ContourGraph`] is built from, in the
/// 2D coordinates of one plane (INP-018).
pub trait PlanSegmentSource {
    fn segments(&self, options: &ContourOptions) -> Vec<Segment>;
}

impl PlanSegmentSource for [Segment] {
    fn segments(&self, _options: &ContourOptions) -> Vec<Segment> {
        self.to_vec()
    }
}

/// The outlines of the union of all cut regions, so that joined walls of
/// different materials have one boundary (the plan merges cut regions only
/// per material), the projected and overhead lines and, if asked for, the
/// wall reference lines.
impl PlanSegmentSource for PlanDrawing {
    fn segments(&self, options: &ContourOptions) -> Vec<Segment> {
        let cut: Vec<Polygon<f64>> = self
            .cut_regions
            .iter()
            .flat_map(|region| region.polygons.iter().cloned())
            .collect();
        let mut segments = Vec::new();
        for polygon in &geo::unary_union(&cut) {
            for ring in polygon_rings(polygon) {
                segments.extend((0..ring.len()).map(|i| Segment::new(ring[i], ring[(i + 1) % ring.len()])));
            }
        }
        for element in &self.elements {
            let lines = element.projected.iter().chain(&element.overhead);
            segments.extend(lines.map(|[a, b]| Segment::new(*a, *b)));
            if options.reference_lines {
                segments.extend(element.reference_line.map(|[a, b]| Segment::new(a, b)));
            }
        }
        segments
    }
}

/// A closed region of a contour graph: a bounded face without its antennas
/// (edges that end inside it) and bridges (edges that join its boundary to a
/// closed loop inside it), with the components and bridged loops it
/// encloses. No ring runs along an edge twice.
#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    /// The boundary, counter-clockwise, without repeating the first point.
    pub outer: Vec<DVec2>,
    /// The outer boundaries of the enclosed components, clockwise, like the
    /// holes of a slab.
    pub islands: Vec<Vec<DVec2>>,
}

impl Region {
    /// The area inside the boundary and outside the islands.
    pub fn area(&self) -> f64 {
        signed_area(&self.outer) + self.islands.iter().map(|island| signed_area(island)).sum::<f64>()
    }
}

/// A run of connected edges of a contour graph: the clicked edge or a chain
/// of connected edges (INP-018).
#[derive(Clone, Debug, PartialEq)]
pub struct Chain {
    /// The ends and corners, without the vertices where the chain runs
    /// straight on. An open chain runs from its lexically first end (smaller
    /// x, then smaller y); a closed one runs counter-clockwise from its
    /// lexically first corner and does not repeat it.
    pub points: Vec<DVec2>,
    /// True if the chain returns to its start.
    pub closed: bool,
}

/// The signed area of a ring of points, positive counter-clockwise.
fn signed_area(ring: &[DVec2]) -> f64 {
    Ring::<()>::from_points(ring.iter().copied()).signed_area()
}

/// A ring of a cycle of half-edges, without its antennas and bridges.
#[derive(Clone, Debug)]
struct Face {
    ring: Vec<DVec2>,
    /// Signed: positive for a bounded face, negative for the outer boundary
    /// of a component or of a loop bridged to the bounded face around it.
    area: f64,
    component: usize,
    /// For a bounded face, the boundaries it encloses directly.
    islands: Vec<usize>,
    /// True for the boundary of a loop bridged to the bounded face around
    /// it, which is among that face's islands from the start.
    bridged: bool,
}

/// An edge of the noded graph between two vertices.
#[derive(Clone, Copy, Debug)]
struct Edge {
    ends: [usize; 2],
    source: Option<CurveRef>,
}

/// Bounding boxes with the index of what they bound.
type BoxIndex = RTree<GeomWithData<Rectangle<[f64; 2]>, usize>>;

/// The planar graph of a source's segments (INP-018).
#[derive(Clone, Debug)]
pub struct ContourGraph {
    vertices: Vec<DVec2>,
    edges: Vec<Edge>,
    faces: Vec<Face>,
    /// The bounding boxes of the bounded faces.
    bounded: BoxIndex,
    /// The edges at each vertex.
    incident: Vec<Vec<usize>>,
    /// The edges as lines, with their indices.
    lines: RTree<GeomWithData<Line<[f64; 2]>, usize>>,
}

impl ContourGraph {
    /// Nodes the segments of `source` into a planar graph and finds its
    /// faces.
    pub fn build<S: PlanSegmentSource + ?Sized>(source: &S, options: &ContourOptions) -> Self {
        let mut segments = source.segments(options);
        segments.retain(|s| s.points.iter().all(|p| p.is_finite()));
        for s in &mut segments {
            if lexical(s.points[1], s.points[0]).is_lt() {
                s.points.reverse();
            }
        }
        segments.sort_by(|s, t| {
            lexical(s.points[0], t.points[0])
                .then(lexical(s.points[1], t.points[1]))
                .then(s.source.cmp(&t.source))
        });
        let (vertices, edges) = node(&segments);
        let faces = faces(&vertices, &edges);
        let bounded = RTree::bulk_load(
            faces
                .iter()
                .enumerate()
                .filter(|(_, face)| face.area > 0.0)
                .map(|(k, face)| GeomWithData::new(Rectangle::from_aabb(bounds(&face.ring)), k))
                .collect(),
        );
        let mut incident = vec![Vec::new(); vertices.len()];
        for (e, edge) in edges.iter().enumerate() {
            for v in edge.ends {
                incident[v].push(e);
            }
        }
        let lines = RTree::bulk_load(
            edges
                .iter()
                .enumerate()
                .map(|(e, edge)| {
                    let [a, b] = edge.ends.map(|v| vertices[v].to_array());
                    GeomWithData::new(Line::new(a, b), e)
                })
                .collect(),
        );
        let mut graph = Self {
            vertices,
            edges,
            faces,
            bounded,
            incident,
            lines,
        };
        graph.assign_islands();
        graph
    }

    /// The edges of the graph, each with the source of the first segment
    /// that has one.
    pub fn edges(&self) -> impl Iterator<Item = Segment> + '_ {
        self.edges.iter().map(|edge| Segment {
            points: edge.ends.map(|v| self.vertices[v]),
            source: edge.source,
        })
    }

    /// Every region of the graph.
    pub fn regions(&self) -> Vec<Region> {
        (0..self.faces.len())
            .filter(|&k| self.faces[k].area > 0.0)
            .map(|k| self.region(k))
            .collect()
    }

    /// The smallest region that contains `p`, or `None` if no region does
    /// (INP-018).
    pub fn region_at(&self, p: DVec2) -> Option<Region> {
        self.smallest_face_around(p, None).map(|k| self.region(k))
    }

    /// The straight run of edges through the edge nearest to `p` within
    /// `radius`, up to the corners and the vertices where other than two
    /// edges meet: the clicked edge (INP-018).
    pub fn edge_at(&self, p: DVec2, radius: f64) -> Option<Chain> {
        self.nearest_edge(p, radius).map(|e| self.chain(e, true))
    }

    /// The run of edges through the edge nearest to `p` within `radius`, up
    /// to the vertices where other than two edges meet: the chain of
    /// connected edges (INP-018).
    pub fn chain_at(&self, p: DVec2, radius: f64) -> Option<Chain> {
        self.nearest_edge(p, radius).map(|e| self.chain(e, false))
    }

    /// The edge nearest to `p` within `radius`; of equally near ones the
    /// first.
    fn nearest_edge(&self, p: DVec2, radius: f64) -> Option<usize> {
        self.lines
            .locate_within_distance(p.to_array(), radius * radius)
            .map(|entry| {
                let [a, b] = self.edges[entry.data].ends.map(|v| self.vertices[v]);
                (segment_distance(p, a, b), entry.data)
            })
            .filter(|&(distance, _)| distance <= radius)
            .min_by(|s, t| s.0.total_cmp(&t.0).then(s.1.cmp(&t.1)))
            .map(|(_, e)| e)
    }

    /// The chain through edge `e`, walking on from both of its ends while two
    /// edges meet at the vertex and, if `straight`, the chain runs straight
    /// on there.
    fn chain(&self, e: usize, straight: bool) -> Chain {
        let [a, b] = self.edges[e].ends;
        let mut path = vec![a, b];
        self.walk(e, straight, &mut path);
        if path[path.len() - 1] != a {
            let mut back = vec![b, a];
            self.walk(e, straight, &mut back);
            back.reverse();
            back.extend_from_slice(&path[2..]);
            path = back;
        }
        // Back at the start, also where more than two edges meet.
        let closed = path.len() > 3 && path[0] == path[path.len() - 1];
        if closed {
            path.pop();
        }
        let all: Vec<DVec2> = path.iter().map(|&v| self.vertices[v]).collect();
        let n = all.len();
        let mut points: Vec<DVec2> = (0..n)
            .filter(|&k| {
                let end = !closed && (k == 0 || k == n - 1);
                end || !runs_straight(all[(k + n - 1) % n], all[k], all[(k + 1) % n])
            })
            .map(|k| all[k])
            .collect();
        if closed {
            if signed_area(&points) < 0.0 {
                points.reverse();
            }
            let first = (0..points.len()).min_by(|&j, &k| lexical(points[j], points[k]));
            points.rotate_left(first.unwrap_or(0));
        } else if lexical(points[points.len() - 1], points[0]).is_lt() {
            points.reverse();
        }
        Chain { points, closed }
    }

    /// Walks on from the last vertex of `path`, reached along edge `e`, and
    /// appends the vertices it passes, until it reaches a vertex where other
    /// than two edges meet, a corner if `straight`, or the start of `path`.
    fn walk(&self, mut e: usize, straight: bool, path: &mut Vec<usize>) {
        loop {
            let v = path[path.len() - 1];
            let &[f, g] = self.incident[v].as_slice() else {
                return;
            };
            let next = if f == e { g } else { f };
            let [u, w] = self.edges[next].ends;
            let w = if u == v { w } else { u };
            let previous = path[path.len() - 2];
            if straight && !runs_straight(self.vertices[previous], self.vertices[v], self.vertices[w]) {
                return;
            }
            path.push(w);
            if w == path[0] {
                return;
            }
            e = next;
        }
    }

    /// The bounded face of smallest area whose boundary contains `p`, other
    /// than the faces of the component `other_than`.
    fn smallest_face_around(&self, p: DVec2, other_than: Option<usize>) -> Option<usize> {
        self.bounded
            .locate_in_envelope_intersecting(&AABB::from_point(p.to_array()))
            .map(|entry| entry.data)
            .filter(|&k| Some(self.faces[k].component) != other_than && ring_contains(&self.faces[k].ring, p))
            .min_by(|&j, &k| self.faces[j].area.total_cmp(&self.faces[k].area).then(j.cmp(&k)))
    }

    /// Gives every component boundary to the smallest face of another
    /// component around it. Components do not touch, so any vertex of the
    /// boundary decides.
    fn assign_islands(&mut self) {
        for k in 0..self.faces.len() {
            let face = &self.faces[k];
            if face.area < 0.0
                && !face.bridged
                && let Some(around) = self.smallest_face_around(face.ring[0], Some(face.component))
            {
                self.faces[around].islands.push(k);
            }
        }
    }

    fn region(&self, k: usize) -> Region {
        let face = &self.faces[k];
        Region {
            outer: without_straight_vertices(face.ring.clone()),
            islands: face
                .islands
                .iter()
                .map(|&island| without_straight_vertices(self.faces[island].ring.clone()))
                .collect(),
        }
    }
}

/// True if the path `a b c` runs straight on at `b`: its two pieces are
/// parallel ([`is_parallel`]) without turning back, and `b` lies within the
/// model tolerance of the line `a c`, so that a long sliver keeps its
/// corners.
fn runs_straight(a: DVec2, b: DVec2, c: DVec2) -> bool {
    let (u, v) = (b - a, c - b);
    is_parallel(u, v) && u.dot(v) > 0.0 && segment_distance(b, a, c) < MODEL_TOLERANCE
}

/// Orders points by x, then y.
fn lexical(a: DVec2, b: DVec2) -> Ordering {
    a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y))
}

fn bounds(ring: &[DVec2]) -> AABB<[f64; 2]> {
    let (lo, hi) = ring.iter().fold((DVec2::INFINITY, DVec2::NEG_INFINITY), |(lo, hi), p| {
        (lo.min(*p), hi.max(*p))
    });
    AABB::from_corners(lo.to_array(), hi.to_array())
}

/// The box around the segment `a b`, grown by `margin`.
fn envelope(a: DVec2, b: DVec2, margin: f64) -> AABB<[f64; 2]> {
    let margin = DVec2::splat(margin);
    AABB::from_corners((a.min(b) - margin).to_array(), (a.max(b) + margin).to_array())
}

/// The point where the segments `a b` and `c d` cross, if they are not
/// parallel.
fn crossing(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> Option<DVec2> {
    let (r, s) = (b - a, d - c);
    if is_parallel(r, s) {
        return None;
    }
    let q = c - a;
    let denominator = r.perp_dot(s);
    let t = q.perp_dot(s) / denominator;
    let u = q.perp_dot(r) / denominator;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then(|| a + r * t)
}

/// Nodes the canonically ordered segments: the welded vertices and the
/// edges between them, without duplicates.
fn node(segments: &[Segment]) -> (Vec<DVec2>, Vec<Edge>) {
    // Endpoints first, so that crossing points snap to them.
    let mut welder: Welder<DVec2> = Welder::new();
    let mut welded: Vec<Edge> = Vec::new();
    for s in segments {
        let [a, b] = s.points.map(|p| welder.weld(p));
        if a != b {
            welded.push(Edge {
                ends: [a, b],
                source: s.source,
            });
        }
    }
    let position = |welder: &Welder<DVec2>, [a, b]: [usize; 2]| (welder.points()[a], welder.points()[b]);

    // Crossings, welded to the vertices within the tolerance.
    let index: BoxIndex = RTree::bulk_load(
        welded
            .iter()
            .enumerate()
            .map(|(i, edge)| {
                let (a, b) = position(&welder, edge.ends);
                GeomWithData::new(Rectangle::from_corners(a.to_array(), b.to_array()), i)
            })
            .collect(),
    );
    let mut splits: Vec<Vec<usize>> = vec![Vec::new(); welded.len()];
    for i in 0..welded.len() {
        let (a, b) = position(&welder, welded[i].ends);
        let mut others: Vec<usize> = index
            .locate_in_envelope_intersecting(&envelope(a, b, 0.0))
            .map(|entry| entry.data)
            .filter(|&j| j > i)
            .collect();
        others.sort_unstable();
        for j in others {
            let (c, d) = position(&welder, welded[j].ends);
            if let Some(x) = crossing(a, b, c, d) {
                let v = welder.weld(x);
                splits[i].push(v);
                splits[j].push(v);
            }
        }
    }

    // Every vertex within the tolerance of a segment splits it: endpoints
    // on another segment (T junctions, collinear overlaps) and crossings.
    let vertices = welder.into_points();
    let index: RTree<GeomWithData<[f64; 2], usize>> = RTree::bulk_load(
        vertices
            .iter()
            .enumerate()
            .map(|(v, p)| GeomWithData::new(p.to_array(), v))
            .collect(),
    );
    let mut edges: Vec<Edge> = Vec::new();
    let mut seen: HashMap<[usize; 2], usize> = HashMap::new();
    for (Edge { ends: [a, b], source }, mut on) in welded.into_iter().zip(splits) {
        let (pa, pb) = (vertices[a], vertices[b]);
        on.extend(
            index
                .locate_in_envelope(&envelope(pa, pb, MODEL_TOLERANCE))
                .map(|entry| entry.data)
                .filter(|&v| segment_distance(vertices[v], pa, pb) < MODEL_TOLERANCE),
        );
        let direction = pb - pa;
        let mut along: Vec<(f64, usize)> = on
            .into_iter()
            .filter(|&v| v != a && v != b)
            .map(|v| ((vertices[v] - pa).dot(direction), v))
            .collect();
        along.sort_by(|s, t| s.0.total_cmp(&t.0).then(s.1.cmp(&t.1)));
        along.dedup_by_key(|(_, v)| *v);
        let mut from = a;
        for to in along.into_iter().map(|(_, v)| v).chain([b]) {
            let key = if from < to { [from, to] } else { [to, from] };
            match seen.entry(key) {
                Entry::Vacant(entry) => {
                    entry.insert(edges.len());
                    edges.push(Edge {
                        ends: [from, to],
                        source,
                    });
                }
                Entry::Occupied(entry) => {
                    let edge = &mut edges[*entry.get()];
                    edge.source = edge.source.or(source);
                }
            }
            from = to;
        }
    }
    (vertices, edges)
}

/// The faces of the noded graph. Half-edge `2 e` runs along edge `e`, `2 e + 1`
/// against it. Leaving the head of a half-edge, a face turns into the
/// outgoing half-edge that precedes the reverse one in counter-clockwise
/// order, which keeps the face on the left: bounded faces run
/// counter-clockwise (positive area), the outer boundaries of components
/// clockwise. A cycle that runs along a bridge out to a loop and back splits
/// into its rings ([`rings_of`]): a bounded face's outer ring and the loops
/// bridged to it, its islands, or the boundaries of a component's bridged
/// parts.
fn faces(vertices: &[DVec2], edges: &[Edge]) -> Vec<Face> {
    let origin = |h: usize| edges[h / 2].ends[h % 2];
    let direction = |h: usize| vertices[origin(h ^ 1)] - vertices[origin(h)];
    let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); vertices.len()];
    for h in 0..2 * edges.len() {
        outgoing[origin(h)].push(h);
    }
    let mut rank = vec![0; 2 * edges.len()];
    for list in &mut outgoing {
        list.sort_by(|&g, &h| {
            let (u, v) = (direction(g), direction(h));
            u.to_angle()
                .total_cmp(&v.to_angle())
                .then(u.length_squared().total_cmp(&v.length_squared()))
                .then(g.cmp(&h))
        });
        for (k, &h) in list.iter().enumerate() {
            rank[h] = k;
        }
    }
    let next = |h: usize| {
        let reverse = h ^ 1;
        let around = &outgoing[origin(reverse)];
        around[(rank[reverse] + around.len() - 1) % around.len()]
    };

    let mut components = Components::new(vertices.len());
    for edge in edges {
        components.join(edge.ends[0], edge.ends[1]);
    }
    let mut visited = vec![false; 2 * edges.len()];
    let mut faces = Vec::new();
    for start in 0..2 * edges.len() {
        if visited[start] {
            continue;
        }
        let mut cycle = Vec::new();
        let mut h = start;
        while !visited[h] {
            visited[h] = true;
            cycle.push(h);
            h = next(h);
        }
        let component = components.find(origin(start));
        let rings: Vec<(Vec<DVec2>, f64)> = rings_of(&cycle)
            .into_iter()
            .map(|ring| {
                let ring: Vec<DVec2> = ring.into_iter().map(|h| vertices[origin(h)]).collect();
                let area = signed_area(&ring);
                (ring, area)
            })
            .filter(|(ring, area)| ring.len() >= 3 && *area != 0.0)
            .collect();
        // A cycle has one counter-clockwise ring at most, the outer ring of
        // a bounded face; should rounding give it more, the largest holds
        // the clockwise ones.
        let outer = rings
            .iter()
            .enumerate()
            .filter(|(_, (_, area))| *area > 0.0)
            .max_by(|(_, (_, s)), (_, (_, t))| s.total_cmp(t))
            .map(|(k, _)| k);
        let first = faces.len();
        faces.extend(rings.into_iter().map(|(ring, area)| Face {
            ring,
            area,
            component,
            islands: Vec::new(),
            bridged: area < 0.0 && outer.is_some(),
        }));
        if let Some(outer) = outer {
            let islands: Vec<usize> = (first..faces.len()).filter(|&k| faces[k].bridged).collect();
            faces[first + outer].islands = islands;
        }
    }
    faces
}

/// The rings of a cycle of half-edges: the cycle without the edges it runs
/// along in both directions, split where it leaves them. A half-edge whose
/// reverse follows it right away runs into a dead end and back (an antenna);
/// one whose reverse comes later runs out along a bridge, around the ring
/// between them and back, and that ring is cut out of the cycle. Each edge
/// left is in one ring, once.
fn rings_of(cycle: &[usize]) -> Vec<Vec<usize>> {
    let mut rings = Vec::new();
    let mut kept: Vec<usize> = Vec::with_capacity(cycle.len());
    // The position of each half-edge in `kept`.
    let mut at: HashMap<usize, usize> = HashMap::new();
    for &h in cycle {
        match at.remove(&(h ^ 1)) {
            Some(k) => {
                let ring = kept.split_off(k + 1);
                for g in &ring {
                    at.remove(g);
                }
                kept.pop();
                if !ring.is_empty() {
                    rings.push(ring);
                }
            }
            None => {
                at.insert(h, kept.len());
                kept.push(h);
            }
        }
    }
    if !kept.is_empty() {
        rings.push(kept);
    }
    rings
}

/// The ring without the vertices where it runs straight on (within the
/// model tolerance of the line between their neighbours).
fn without_straight_vertices(mut ring: Vec<DVec2>) -> Vec<DVec2> {
    loop {
        let before = ring.len();
        let mut k = 0;
        while ring.len() > 3 && k < ring.len() {
            let n = ring.len();
            if segment_distance(ring[k], ring[(k + n - 1) % n], ring[(k + 1) % n]) < MODEL_TOLERANCE {
                ring.remove(k);
            } else {
                k += 1;
            }
        }
        if ring.len() == before {
            return ring;
        }
    }
}

/// Union-find over the vertices: the connected components of the graph.
struct Components(Vec<usize>);

impl Components {
    fn new(n: usize) -> Self {
        Self((0..n).collect())
    }

    fn find(&mut self, v: usize) -> usize {
        let mut root = v;
        while self.0[root] != root {
            root = self.0[root];
        }
        let mut v = v;
        while self.0[v] != root {
            v = std::mem::replace(&mut self.0[v], root);
        }
        root
    }

    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        self.0[a.max(b)] = a.min(b);
    }
}
