//! Element outlines: closed rings of nodes and edges with data per edge
//! (SLB-003, SLB-005, SLB-008, EDT-008, EDT-015).
//!
//! Every polygon-based element stores one [`Outline`], so the node, edge and
//! polygon operations are written once against it instead of per kind
//! (NFR-USE-002). An outline is one list of rings addressed by [`RingIndex`]:
//! ring 0 is the outer ring, counter-clockwise, and rings 1.. are holes,
//! clockwise, so a hole is edited like the outline (SLB-003) and a part of an
//! element is addressed as (ring, node) or (ring, edge).
//!
//! Each [`Vertex`] carries the [`Edge`] that starts at it: edge `i` runs from
//! node `i` to node `i + 1`, wrapping to node 0 after the last. Inserting or
//! removing a node therefore keeps points and per-edge data in step, without
//! parallel arrays. The edge data is a type parameter ([`EdgeData`]); `()`
//! stands for edges without data.
//!
//! Lengths are metres in model coordinates. Conversions to and from
//! [`geo::Polygon`] copy coordinates unchanged, so a round trip is bit-exact.

use std::fmt::Debug;

use glam::DVec2;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// The index of a ring in an [`Outline`]: 0 is the outer ring, 1.. the holes.
pub type RingIndex = u16;

/// The index of a node, or of the edge starting at it, in a [`Ring`].
pub type NodeIndex = u32;

/// The ring index of the outer ring.
pub const OUTER_RING: RingIndex = 0;

/// Data attached to each edge of an outline, such as the angle of a slab edge
/// (SLB-005). Splitting an edge at a new node gives both halves a copy and
/// merging two edges at a removed node keeps the first one's data, unless a
/// kind overrides that.
pub trait EdgeData: Clone + Default + PartialEq + Debug + Serialize + DeserializeOwned {
    /// The data of the two edges an edge is split into.
    fn split(&self) -> (Self, Self) {
        (self.clone(), self.clone())
    }

    /// The data of the edge that replaces `first` and the `second` edge
    /// following it.
    fn merge(first: &Self, _second: &Self) -> Self {
        first.clone()
    }
}

impl EdgeData for () {}

/// The geometry of an edge between its two nodes. Only straight edges exist
/// so far; arcs follow with curved geometry.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeShape {
    /// A straight line.
    #[default]
    Line,
}

/// An edge: its shape and its data.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Edge<E = ()> {
    pub shape: EdgeShape,
    pub data: E,
}

/// A node of a ring and the edge that starts at it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vertex<E = ()> {
    pub point: DVec2,
    /// The edge to the next node.
    pub next: Edge<E>,
}

impl<E: EdgeData> Vertex<E> {
    /// A node at `point` whose edge is a straight line with default data.
    pub fn new(point: DVec2) -> Self {
        Self {
            point,
            next: Edge::default(),
        }
    }
}

/// An edge of a ring with its end points, as returned by [`Ring::edge`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment<'a, E = ()> {
    pub start: DVec2,
    pub end: DVec2,
    pub edge: &'a Edge<E>,
}

/// The smallest and largest corner of the axis-aligned bounding box of
/// `points`, or `None` for no points.
pub fn point_bounds(points: impl IntoIterator<Item = DVec2>) -> Option<(DVec2, DVec2)> {
    points
        .into_iter()
        .map(|p| (p, p))
        .reduce(|(lo, hi), (l, h)| (lo.min(l), hi.max(h)))
}

/// A closed ring of nodes, without repeating the first node at the end.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ring<E = ()> {
    pub vertices: Vec<Vertex<E>>,
}

impl<E> Default for Ring<E> {
    fn default() -> Self {
        Self { vertices: Vec::new() }
    }
}

impl<E: EdgeData> Ring<E> {
    /// A ring through `points` with straight edges and default data.
    pub fn from_points(points: impl IntoIterator<Item = DVec2>) -> Self {
        Self {
            vertices: points.into_iter().map(Vertex::new).collect(),
        }
    }

    /// The number of nodes, which is also the number of edges.
    pub fn len(&self) -> usize {
        self.vertices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    /// The nodes in order.
    pub fn points(&self) -> impl ExactSizeIterator<Item = DVec2> + '_ {
        self.vertices.iter().map(|v| v.point)
    }

    /// Node `index`, wrapping around: node `len()` is node 0.
    ///
    /// # Panics
    ///
    /// If the ring is empty.
    pub fn node(&self, index: NodeIndex) -> DVec2 {
        self.vertices[self.wrap(index)].point
    }

    /// Edge `index`, from node `index` to the node after it, wrapping around.
    ///
    /// # Panics
    ///
    /// If the ring is empty.
    pub fn edge(&self, index: NodeIndex) -> Segment<'_, E> {
        let i = self.wrap(index);
        let vertex = &self.vertices[i];
        Segment {
            start: vertex.point,
            end: self.vertices[(i + 1) % self.len()].point,
            edge: &vertex.next,
        }
    }

    /// The edges in order, the last one closing the ring.
    pub fn segments(&self) -> impl ExactSizeIterator<Item = Segment<'_, E>> + '_ {
        (0..self.len()).map(|i| Segment {
            start: self.vertices[i].point,
            end: self.vertices[(i + 1) % self.len()].point,
            edge: &self.vertices[i].next,
        })
    }

    fn wrap(&self, index: NodeIndex) -> usize {
        index as usize % self.len()
    }

    /// The signed area, positive for a counter-clockwise ring.
    pub fn signed_area(&self) -> f64 {
        self.segments().map(|s| s.start.perp_dot(s.end)).sum::<f64>() / 2.0
    }

    /// Whether the ring runs counter-clockwise (a positive signed area).
    pub fn is_counter_clockwise(&self) -> bool {
        self.signed_area() > 0.0
    }

    /// Whether `p` lies inside the ring (even-odd rule). Points on an edge
    /// may fall on either side.
    pub fn contains(&self, p: DVec2) -> bool {
        let mut inside = false;
        for s in self.segments() {
            let (a, b) = (s.start, s.end);
            if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
                inside = !inside;
            }
        }
        inside
    }

    /// The smallest and largest corner of the axis-aligned bounding box, or
    /// `None` for an empty ring.
    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        point_bounds(self.points())
    }

    /// Reverses the order of the nodes, as [`slice::reverse`] does, and keeps
    /// each edge's data on the edge between the same two nodes.
    pub fn reverse(&mut self) {
        let n = self.len();
        if n < 2 {
            return;
        }
        // Old edge i joins nodes i and i + 1; after reversing, node j is old
        // node n - 1 - j, so new edge j is old edge n - 2 - j (mod n).
        let edges: Vec<Edge<E>> = self.vertices.iter().map(|v| v.next.clone()).collect();
        self.vertices.reverse();
        for (j, vertex) in self.vertices.iter_mut().enumerate() {
            vertex.next = edges[(2 * n - 2 - j) % n].clone();
        }
    }

    /// Reverses the ring unless it already runs in the given direction. A
    /// ring without area keeps its order.
    pub fn orient(&mut self, counter_clockwise: bool) {
        let area = self.signed_area();
        if area != 0.0 && (area > 0.0) != counter_clockwise {
            self.reverse();
        }
    }

    /// The ring as a closed [`geo::LineString`]: the first node repeated at
    /// the end.
    pub fn to_geo(&self) -> geo::LineString<f64> {
        self.points()
            .chain(self.vertices.first().map(|v| v.point))
            .map(|p| geo::coord! { x: p.x, y: p.y })
            .collect()
    }

    /// The ring through the nodes of `line`, without its closing node, with
    /// straight edges and default data.
    pub fn from_geo(line: &geo::LineString<f64>) -> Self {
        let mut points: Vec<DVec2> = line.coords().map(|c| DVec2::new(c.x, c.y)).collect();
        if points.len() > 1 && points.first() == points.last() {
            points.pop();
        }
        Self::from_points(points)
    }
}

/// An outline: the outer ring followed by its holes (SLB-003).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Outline<E = ()> {
    /// Ring [`OUTER_RING`] is the outer ring, counter-clockwise; the others
    /// are holes, clockwise, inside it.
    pub rings: Vec<Ring<E>>,
}

impl<E: EdgeData> Outline<E> {
    /// The outline of the `outer` ring and the `holes`, oriented: the outer
    /// ring counter-clockwise, the holes clockwise. Coordinates and the
    /// order of the nodes are kept otherwise; nothing else is validated.
    /// [`Outline::from_point_rings`] keeps the rings as given instead.
    pub fn from_rings<P, H>(outer: P, holes: impl IntoIterator<Item = H>) -> Self
    where
        P: IntoIterator<Item = DVec2>,
        H: IntoIterator<Item = DVec2>,
    {
        let rings = std::iter::once(Ring::from_points(outer)).chain(holes.into_iter().map(Ring::from_points));
        let mut outline = Self { rings: rings.collect() };
        outline.normalize_orientation();
        outline
    }

    /// The outline through rings of points, ring 0 the outer ring, with
    /// straight edges and default data. The rings are kept as given, neither
    /// oriented nor validated; [`Outline::from_rings`] differs only in that
    /// it turns the outer ring counter-clockwise and the holes clockwise. It
    /// suits rings already oriented, such as those [`validate_slab_polygon`]
    /// returns.
    ///
    /// [`validate_slab_polygon`]: crate::validate::validate_slab_polygon
    pub fn from_point_rings<R: IntoIterator<Item = DVec2>>(rings: impl IntoIterator<Item = R>) -> Self {
        Self {
            rings: rings.into_iter().map(Ring::from_points).collect(),
        }
    }

    /// The nodes of the outer ring and of each hole, for code that works on
    /// lists of points; an outline without rings gives no points.
    pub fn to_points(&self) -> (Vec<DVec2>, Vec<Vec<DVec2>>) {
        let mut rings = self.rings.iter().map(|ring| ring.points().collect());
        (rings.next().unwrap_or_default(), rings.collect())
    }

    /// Orients the outer ring counter-clockwise and the holes clockwise.
    pub fn normalize_orientation(&mut self) {
        for (i, ring) in self.rings.iter_mut().enumerate() {
            ring.orient(i == usize::from(OUTER_RING));
        }
    }

    /// The outer ring.
    ///
    /// # Panics
    ///
    /// If the outline has no rings.
    pub fn outer(&self) -> &Ring<E> {
        &self.rings[usize::from(OUTER_RING)]
    }

    /// The holes, ring 1 first.
    pub fn holes(&self) -> &[Ring<E>] {
        self.rings.get(1..).unwrap_or_default()
    }

    /// Ring `index`, or `None` if there is no such ring.
    pub fn ring(&self, index: RingIndex) -> Option<&Ring<E>> {
        self.rings.get(usize::from(index))
    }

    pub fn ring_mut(&mut self, index: RingIndex) -> Option<&mut Ring<E>> {
        self.rings.get_mut(usize::from(index))
    }

    /// The rings with their indices.
    pub fn indexed_rings(&self) -> impl Iterator<Item = (RingIndex, &Ring<E>)> + '_ {
        (0..).zip(&self.rings)
    }

    /// The number of nodes in all rings together.
    pub fn node_count(&self) -> usize {
        self.rings.iter().map(Ring::len).sum()
    }

    /// The area of the outer ring less the areas of the holes, whatever the
    /// orientation of the rings.
    pub fn area(&self) -> f64 {
        let mut rings = self.rings.iter().map(|r| r.signed_area().abs());
        let outer = rings.next().unwrap_or(0.0);
        outer - rings.sum::<f64>()
    }

    /// Whether `p` lies inside the outer ring and outside every hole.
    pub fn contains(&self, p: DVec2) -> bool {
        self.rings.first().is_some_and(|outer| outer.contains(p)) && !self.holes().iter().any(|h| h.contains(p))
    }

    /// The bounding box of the outer ring, which bounds the holes as well.
    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        self.rings.first().and_then(Ring::bounds)
    }

    /// The outline as a [`geo::Polygon`]: the outer ring as the exterior and
    /// the holes as interiors, each closed and with coordinates unchanged.
    /// geo reorients nothing, so the rings keep their direction.
    pub fn to_geo(&self) -> geo::Polygon<f64> {
        let exterior = self
            .rings
            .first()
            .map(Ring::to_geo)
            .unwrap_or_else(|| geo::LineString(Vec::new()));
        geo::Polygon::new(exterior, self.holes().iter().map(Ring::to_geo).collect())
    }

    /// The outline of `polygon` with straight edges and default data. The
    /// rings keep their direction and coordinates; [`Self::normalize_orientation`]
    /// orients them.
    pub fn from_geo(polygon: &geo::Polygon<f64>) -> Self {
        let rings = std::iter::once(polygon.exterior())
            .chain(polygon.interiors())
            .map(Ring::from_geo);
        Self { rings: rings.collect() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Edge data that tells edges apart, so the tests see whether data
    /// follows its edge (NFR-USE-002).
    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    struct TestEdge {
        tag: u32,
    }

    impl EdgeData for TestEdge {}

    fn rectangle(min: DVec2, max: DVec2) -> Vec<DVec2> {
        vec![min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)]
    }

    /// The upper floor slab of `samples::two_storey_shell`: 12 x 9 m with
    /// the 3 x 1.2 m stair hole, both given counter-clockwise as there.
    fn shell<E: EdgeData>() -> Outline<E> {
        let stair = rectangle(DVec2::new(6.0, 1.0), DVec2::new(9.0, 2.2));
        Outline::from_rings(rectangle(DVec2::ZERO, DVec2::new(12.0, 9.0)), [stair])
    }

    fn tagged(mut outline: Outline<TestEdge>) -> Outline<TestEdge> {
        for (r, ring) in outline.rings.iter_mut().enumerate() {
            for (i, vertex) in ring.vertices.iter_mut().enumerate() {
                vertex.next.data.tag = (100 * r + i) as u32;
            }
        }
        outline
    }

    /// The query tests, written once and run for every edge data type.
    mod queries {
        use super::*;

        /// SLB-003: the outer ring counter-clockwise, the holes clockwise,
        /// whatever the input direction.
        pub fn from_rings_normalises_orientation<E: EdgeData>() {
            let outline: Outline<E> = shell();
            assert!(outline.outer().is_counter_clockwise());
            assert!(outline.holes().iter().all(|h| !h.is_counter_clockwise()));
            // The outer ring given clockwise and the hole counter-clockwise.
            let outer = vec![
                DVec2::ZERO,
                DVec2::new(0.0, 9.0),
                DVec2::new(12.0, 9.0),
                DVec2::new(12.0, 0.0),
            ];
            let outline: Outline<E> = Outline::from_rings(outer, [rectangle(DVec2::ONE, DVec2::splat(2.0))]);
            assert!(outline.outer().is_counter_clockwise());
            assert!(!outline.holes()[0].is_counter_clockwise());
            assert_eq!(
                outline.outer().node(0),
                DVec2::new(12.0, 0.0),
                "reversed as slice::reverse does"
            );
        }

        /// SLB-003: the stair hole is deducted from the 12 x 9 m outline.
        pub fn area_deducts_the_holes<E: EdgeData>() {
            let outline: Outline<E> = shell();
            assert!((outline.area() - 104.4).abs() < 1e-9, "{}", outline.area());
            assert!((outline.outer().signed_area() - 108.0).abs() < 1e-9);
            assert!((outline.holes()[0].signed_area() + 3.6).abs() < 1e-9);
        }

        pub fn contains_is_false_inside_a_hole<E: EdgeData>() {
            let outline: Outline<E> = shell();
            assert!(outline.contains(DVec2::new(1.0, 1.0)));
            assert!(outline.contains(DVec2::new(10.0, 8.0)));
            assert!(!outline.contains(DVec2::new(7.5, 1.6)), "inside the stair hole");
            assert!(outline.holes()[0].contains(DVec2::new(7.5, 1.6)));
            assert!(!outline.contains(DVec2::new(13.0, 1.0)), "outside the outline");
            assert!(!outline.contains(DVec2::new(-0.5, 4.0)));
        }

        pub fn node_and_edge_indices_wrap<E: EdgeData>() {
            let outline: Outline<E> = shell();
            let outer = outline.outer();
            assert_eq!(outer.len(), 4);
            assert_eq!(outer.node(4), outer.node(0));
            assert_eq!(outer.node(7), DVec2::new(0.0, 9.0));
            let last = outer.edge(3);
            assert_eq!(
                (last.start, last.end),
                (DVec2::new(0.0, 9.0), DVec2::ZERO),
                "the closing edge"
            );
            assert_eq!(outer.edge(5), outer.edge(1));
            assert_eq!(outer.edge(NodeIndex::MAX).start, outer.node(NodeIndex::MAX));
            let segments: Vec<_> = outer.segments().collect();
            assert_eq!(segments.len(), 4);
            assert!((0..4).all(|i| segments[i] == outer.edge(i as NodeIndex)));
            assert!(segments.iter().all(|s| s.edge.shape == EdgeShape::Line));
        }

        pub fn rings_are_addressed_by_ring_index<E: EdgeData>() {
            let outline: Outline<E> = shell();
            assert_eq!(outline.ring(OUTER_RING), Some(outline.outer()));
            assert_eq!(outline.ring(1), Some(&outline.holes()[0]));
            assert_eq!(outline.ring(2), None);
            let indices: Vec<RingIndex> = outline.indexed_rings().map(|(i, _)| i).collect();
            assert_eq!(indices, [0, 1]);
            assert_eq!(outline.node_count(), 8);
        }

        pub fn bounds_span_the_outer_ring<E: EdgeData>() {
            let outline: Outline<E> = shell();
            assert_eq!(outline.bounds(), Some((DVec2::ZERO, DVec2::new(12.0, 9.0))));
            assert_eq!(
                outline.holes()[0].bounds(),
                Some((DVec2::new(6.0, 1.0), DVec2::new(9.0, 2.2)))
            );
            assert_eq!(Ring::<E>::default().bounds(), None);
        }

        /// Coordinates that do not survive any rounding pass through geo
        /// unchanged, and the rings keep their direction.
        pub fn geo_round_trip_is_bit_exact<E: EdgeData>() {
            let third = 1.0 / 3.0;
            let outer = vec![
                DVec2::new(0.1, 0.2),
                DVec2::new(12.3 + third, 0.7),
                DVec2::new(1e5 - third, 9.0 + 1e-12),
            ];
            let hole = vec![
                DVec2::new(1.0 + third, 1.1),
                DVec2::new(1.5, 1.2 + 1e-9),
                DVec2::new(2.0, 1.0 + 0.1 + 0.2),
            ];
            let outline: Outline<E> = Outline::from_rings(outer, [hole]);
            let polygon = outline.to_geo();
            assert_eq!(polygon.exterior().0.len(), 4, "closed with the first point");
            assert_eq!(polygon.exterior().0.first(), polygon.exterior().0.last());
            assert_eq!(polygon.interiors().len(), 1);
            let back: Outline<E> = Outline::from_geo(&polygon);
            assert_eq!(back, outline);
            for (a, b) in back
                .rings
                .iter()
                .flat_map(|r| r.points())
                .zip(outline.rings.iter().flat_map(|r| r.points()))
            {
                assert_eq!((a.x.to_bits(), a.y.to_bits()), (b.x.to_bits(), b.y.to_bits()));
            }
        }

        pub fn reverse_keeps_the_nodes_of_each_edge<E: EdgeData>() {
            let outline: Outline<E> = shell();
            let mut ring = outline.outer().clone();
            let before: Vec<_> = ring.segments().map(|s| (s.start, s.end, s.edge.clone())).collect();
            ring.reverse();
            assert_eq!(ring.node(0), DVec2::new(0.0, 9.0));
            assert!(!ring.is_counter_clockwise());
            for s in ring.segments() {
                assert!(
                    before.contains(&(s.end, s.start, s.edge.clone())),
                    "edge {s:?} keeps its data"
                );
            }
            ring.reverse();
            assert_eq!(&ring, outline.outer());
        }
    }

    /// Runs every query test for one edge data type.
    macro_rules! query_tests {
        ($module:ident, $edge:ty) => {
            mod $module {
                use super::queries;

                #[test]
                fn from_rings_normalises_orientation() {
                    queries::from_rings_normalises_orientation::<$edge>();
                }

                #[test]
                fn area_deducts_the_holes() {
                    queries::area_deducts_the_holes::<$edge>();
                }

                #[test]
                fn contains_is_false_inside_a_hole() {
                    queries::contains_is_false_inside_a_hole::<$edge>();
                }

                #[test]
                fn node_and_edge_indices_wrap() {
                    queries::node_and_edge_indices_wrap::<$edge>();
                }

                #[test]
                fn rings_are_addressed_by_ring_index() {
                    queries::rings_are_addressed_by_ring_index::<$edge>();
                }

                #[test]
                fn bounds_span_the_outer_ring() {
                    queries::bounds_span_the_outer_ring::<$edge>();
                }

                #[test]
                fn geo_round_trip_is_bit_exact() {
                    queries::geo_round_trip_is_bit_exact::<$edge>();
                }

                #[test]
                fn reverse_keeps_the_nodes_of_each_edge() {
                    queries::reverse_keeps_the_nodes_of_each_edge::<$edge>();
                }
            }
        };
    }

    query_tests!(without_data, ());
    query_tests!(with_test_edges, super::TestEdge);

    /// The data of each edge stays with it through indexing and reversing,
    /// which edge data without values cannot show.
    #[test]
    fn edge_data_follows_its_edge() {
        let mut outline = tagged(shell());
        let hole = outline.ring(1).expect("the stair hole");
        assert_eq!(hole.edge(2).edge.data.tag, 102);
        assert_eq!(hole.edge(6).edge.data.tag, 102, "wraps");
        let ring = outline.ring_mut(OUTER_RING).expect("the outer ring");
        let (start, end) = (ring.node(1), ring.node(2));
        ring.reverse();
        let reversed = ring
            .segments()
            .find(|s| (s.start, s.end) == (end, start))
            .expect("edge 1 reversed");
        assert_eq!(reversed.edge.data.tag, 1);
        assert_eq!(ring.edge(0).edge.data.tag, 2, "old edge n - 2 - 0");
    }

    /// SLB-003: rings of points are kept as given, in order and direction,
    /// and come back as the same points: slabs made by `Project::new_slab`
    /// or read from IFCX leave orienting to validation.
    #[test]
    fn point_rings_are_kept_as_given() {
        let outer = vec![
            DVec2::ZERO,
            DVec2::new(0.0, 9.0),
            DVec2::new(12.0, 9.0),
            DVec2::new(12.0, 0.0),
        ];
        let hole = rectangle(DVec2::ONE, DVec2::splat(2.0));
        let outline: Outline = Outline::from_point_rings([outer.clone(), hole.clone()]);
        assert!(!outline.outer().is_counter_clockwise(), "not oriented");
        assert!(outline.holes()[0].is_counter_clockwise(), "not oriented");
        assert_eq!(outline.to_points(), (outer, vec![hole]));
        let empty: Outline = Outline::from_point_rings(Vec::<Vec<DVec2>>::new());
        assert_eq!(empty.to_points(), (Vec::new(), Vec::new()));
    }

    /// The command journal serialises elements, so outlines round-trip
    /// through JSON without loss.
    #[test]
    fn serde_json_round_trip_is_lossless() {
        let mut outline: Outline = shell();
        outline.rings[1].vertices[0].point = DVec2::new(6.0 + 1.0 / 3.0, 1.0 + 0.1 + 0.2);
        let json = serde_json::to_string(&outline).expect("serialises");
        let back: Outline = serde_json::from_str(&json).expect("deserialises");
        assert_eq!(back, outline);
        let tagged = tagged(shell());
        let json = serde_json::to_string(&tagged).expect("serialises");
        assert_eq!(
            serde_json::from_str::<Outline<TestEdge>>(&json).expect("deserialises"),
            tagged
        );
    }

    #[test]
    fn default_edge_data_splits_into_copies_and_merges_to_the_first() {
        let edge = TestEdge { tag: 7 };
        assert_eq!(edge.split(), (TestEdge { tag: 7 }, TestEdge { tag: 7 }));
        assert_eq!(TestEdge::merge(&edge, &TestEdge { tag: 9 }), edge);
    }
}
