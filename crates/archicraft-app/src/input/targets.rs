//! The spatial index of snap targets (INP-008, INP-017, NFR-PERF-005).
//!
//! A [`SnapIndex`] holds the nodes, edges and arcs of one storey in R-trees,
//! so that snapping looks only at the targets near the cursor instead of
//! scanning the whole storey. It follows the model's revisions
//! incrementally: every target belongs to an owner, and a new revision
//! removes and inserts only the owners whose targets changed, compared by a
//! fingerprint of their content. The first build, and a revision that
//! changes more than a quarter of the owners, bulk-load the trees instead.
//!
//! Targets come from [`SnapSource`]s: the plan's elements (cut rings,
//! projected and overhead lines), the walls' reference lines and the slabs'
//! outline centroids. Only area elements offer centroids (INP-024): a
//! wall's cut centroid would lie within the snap radius of its faces on a
//! thin wall and, at node level, hide their midpoints (INP-009). A node
//! carries its kind within the node level (INP-009): a plain node, a
//! handle, a centre, an axis point or a centroid. Elements
//! that are hidden or excluded from snapping, themselves or through their
//! layer, provide none; locked elements stay snappable while they reject
//! edits (INP-017). A whole source, such as a linked reference model, can
//! be switched off with [`SnapIndex::set_source_excluded`].
//!
//! Edges come in chains: a wall's reference line, a slab's outline, a cut
//! ring. [`SnapIndex::hover`] finds the edge under the cursor and divides
//! the stretch the division scope gives (INP-010).

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use archicraft_geometry::plan::{PlanDrawing, PlanElement, outline_centroid, polygon_rings};
use archicraft_model::glam::DVec2;
use archicraft_model::tolerance::{Welder, coincident};
use archicraft_model::{Element, ElementId, MODEL_TOLERANCE, Project, StoreyId};
use rstar::{AABB, PointDistance, RTree, RTreeObject};

use super::division::{Chain, DivisionMode, DivisionScope, Hovered, InputSettings};
use super::snap::{ArcTarget, Snap, SnapKind, closest_on_segment, segment_intersection};

/// How far beyond the snap radius a hovered edge stays hovered, as a
/// multiple of the radius (INP-010).
pub const HOVER_HYSTERESIS: f64 = 1.5;

/// What a snap target belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TargetOwner {
    Element(ElementId),
    /// A guide line, by its number.
    Guide(u64),
    /// A linked reference model, by its number (IFX-084).
    Reference(u64),
}

/// The kind of geometry a target comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SourceClass {
    /// A wall's reference line (WAL-007).
    ReferenceLine,
    /// An outline stored in the model, such as a slab's.
    Outline,
    /// A ring of a cut polygon in the plan.
    Cut,
    /// A projected line below the cut plane.
    Projected,
    /// An overhead line above the cut plane.
    Overhead,
    /// A guide line.
    Guide,
    /// A linked reference model's geometry (IFX-084).
    Reference,
}

/// Where a target sits among all targets: its owner, which of the owner's
/// sources gave it, and its number within that source's nodes or edges.
#[derive(Clone, Copy, Debug, PartialEq)]
struct TargetKey {
    owner: TargetOwner,
    source: u8,
    index: u32,
    class: SourceClass,
}

/// An edge of the index (INP-010): its owner, the owner's source that gave
/// it and its number there. It names the same edge across revisions while
/// the owner's sources give their edges in the same order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EdgeKey {
    pub owner: TargetOwner,
    source: u8,
    index: u32,
}

/// A node in the index, with its kind within the node level.
#[derive(Clone, Copy, Debug, PartialEq)]
struct IndexedPoint {
    point: DVec2,
    kind: SnapKind,
    key: TargetKey,
}

/// An edge in the index, with the number of its chain within its source.
#[derive(Clone, Copy, Debug, PartialEq)]
struct IndexedEdge {
    edge: [DVec2; 2],
    key: TargetKey,
    chain: u32,
}

impl IndexedEdge {
    fn edge_key(&self) -> EdgeKey {
        EdgeKey {
            owner: self.key.owner,
            source: self.key.source,
            index: self.key.index,
        }
    }

    fn distance(&self, p: DVec2) -> f64 {
        closest_on_segment(p, self.edge).distance(p)
    }
}

/// An arc or a circle in the index.
#[derive(Clone, Copy, Debug, PartialEq)]
struct IndexedArc {
    arc: ArcTarget,
    key: TargetKey,
}

impl RTreeObject for IndexedPoint {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_point(self.point.to_array())
    }
}

impl PointDistance for IndexedPoint {
    fn distance_2(&self, point: &[f64; 2]) -> f64 {
        self.point.distance_squared(DVec2::from_array(*point))
    }
}

impl RTreeObject for IndexedEdge {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_corners(self.edge[0].to_array(), self.edge[1].to_array())
    }
}

impl PointDistance for IndexedEdge {
    fn distance_2(&self, point: &[f64; 2]) -> f64 {
        let p = DVec2::from_array(*point);
        closest_on_segment(p, self.edge).distance_squared(p)
    }
}

/// The whole circle: no arc reaches beyond it.
impl RTreeObject for IndexedArc {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        let r = DVec2::splat(self.arc.radius);
        AABB::from_corners((self.arc.centre - r).to_array(), (self.arc.centre + r).to_array())
    }
}

impl PointDistance for IndexedArc {
    fn distance_2(&self, point: &[f64; 2]) -> f64 {
        self.arc.reach(DVec2::from_array(*point)).powi(2)
    }
}

/// Collects the targets of one source, in order.
#[derive(Debug)]
pub struct TargetSink {
    owner: TargetOwner,
    source: u8,
    points: Vec<IndexedPoint>,
    edges: Vec<IndexedEdge>,
    arcs: Vec<IndexedArc>,
    chains: u32,
}

impl TargetSink {
    fn key(&self, index: usize, class: SourceClass) -> TargetKey {
        TargetKey {
            owner: self.owner,
            source: self.source,
            index: index as u32,
            class,
        }
    }

    fn point(&mut self, class: SourceClass, kind: SnapKind, point: DVec2) {
        let key = self.key(self.points.len(), class);
        self.points.push(IndexedPoint { point, kind, key });
    }

    /// An element node: a corner or an end.
    pub fn node(&mut self, class: SourceClass, point: DVec2) {
        self.point(class, SnapKind::Node, point);
    }

    /// The hook for handles (INP-008): a node that a selected element's
    /// handle provides. Wall ends stay plain nodes.
    pub fn handle(&mut self, class: SourceClass, point: DVec2) {
        self.point(class, SnapKind::Handle, point);
    }

    /// An axis point of a column or a beam (INP-008).
    pub fn axis_point(&mut self, class: SourceClass, point: DVec2) {
        self.point(class, SnapKind::AxisPoint, point);
    }

    /// The centroid of a polygon (INP-024).
    pub fn centroid(&mut self, class: SourceClass, point: DVec2) {
        self.point(class, SnapKind::Centroid, point);
    }

    /// An edge that is a chain of its own, such as a wall's reference line.
    pub fn edge(&mut self, class: SourceClass, edge: [DVec2; 2]) {
        let chain = self.next_chain();
        self.chain_edge(class, edge, chain);
    }

    /// The edges of a closed ring, one chain (INP-010): a slab's outline or
    /// a cut ring.
    pub fn ring(&mut self, class: SourceClass, ring: &[DVec2]) {
        let chain = self.next_chain();
        for i in 0..ring.len() {
            self.chain_edge(class, [ring[i], ring[(i + 1) % ring.len()]], chain);
        }
    }

    fn next_chain(&mut self) -> u32 {
        self.chains += 1;
        self.chains - 1
    }

    fn chain_edge(&mut self, class: SourceClass, edge: [DVec2; 2], chain: u32) {
        let key = self.key(self.edges.len(), class);
        self.edges.push(IndexedEdge { edge, key, chain });
    }

    /// A circle or an arc: its centre and its tangent points from the
    /// previous point are targets (INP-008).
    pub fn arc(&mut self, class: SourceClass, arc: ArcTarget) {
        let key = self.key(self.arcs.len(), class);
        self.arcs.push(IndexedArc { arc, key });
    }
}

/// Something that provides snap targets (INP-008).
pub trait SnapSource {
    fn owner(&self) -> TargetOwner;

    /// Adds this source's nodes and edges to `sink`. Of coincident targets
    /// the one added first is taken (NFR-ACC-003).
    fn targets(&self, sink: &mut TargetSink);
}

/// A wall's reference line, a slab's outline centroid (INP-024) and its
/// outline and holes as rings (INP-010): they come from the model, so they
/// come before the points computed for the plan.
impl SnapSource for Element {
    fn owner(&self) -> TargetOwner {
        TargetOwner::Element(self.id)
    }

    fn targets(&self, sink: &mut TargetSink) {
        if let Some(wall) = self.as_wall() {
            sink.node(SourceClass::ReferenceLine, wall.start);
            sink.node(SourceClass::ReferenceLine, wall.end);
            sink.edge(SourceClass::ReferenceLine, [wall.start, wall.end]);
        }
        if let Some(slab) = self.as_slab() {
            if let Some(centroid) = outline_centroid(&slab.outline, &slab.holes) {
                sink.centroid(SourceClass::Outline, centroid);
            }
            for ring in std::iter::once(&slab.outline).chain(&slab.holes) {
                sink.ring(SourceClass::Outline, ring);
            }
        }
    }
}

/// An element's plan: the corners and sides of its cut rings, then its
/// projected and overhead lines. Cut polygons offer no centroids: those
/// come from area elements' outlines (INP-024).
impl SnapSource for PlanElement {
    fn owner(&self) -> TargetOwner {
        TargetOwner::Element(self.id)
    }

    fn targets(&self, sink: &mut TargetSink) {
        for polygon in &self.cut {
            for ring in polygon_rings(polygon) {
                for &p in &ring {
                    sink.node(SourceClass::Cut, p);
                }
                sink.ring(SourceClass::Cut, &ring);
            }
        }
        let lines = (self.projected.iter().map(|s| (SourceClass::Projected, s)))
            .chain(self.overhead.iter().map(|s| (SourceClass::Overhead, s)));
        for (class, segment) in lines {
            for &p in segment {
                sink.node(class, p);
            }
            sink.edge(class, *segment);
        }
    }
}

/// The targets of one owner and the fingerprint of their content.
#[derive(Clone, Debug, Default)]
struct OwnerTargets {
    sources: u8,
    points: Vec<IndexedPoint>,
    edges: Vec<IndexedEdge>,
    arcs: Vec<IndexedArc>,
    fingerprint: u64,
}

impl OwnerTargets {
    /// A hash over the bits of every target and its key, so that equal
    /// content gives equal fingerprints whatever allocation it came from.
    fn fingerprint(&self) -> u64 {
        let mut h = DefaultHasher::new();
        (self.sources, self.points.len(), self.edges.len()).hash(&mut h);
        let points = self.points.iter().map(|p| (p.key, [p.point, p.point]));
        for (key, ends) in points.chain(self.edges.iter().map(|e| (e.key, e.edge))) {
            (key.source, key.index, key.class).hash(&mut h);
            for p in ends {
                (p.x.to_bits(), p.y.to_bits()).hash(&mut h);
            }
        }
        for p in &self.points {
            p.kind.hash(&mut h);
        }
        for e in &self.edges {
            e.chain.hash(&mut h);
        }
        for IndexedArc { arc, key } in &self.arcs {
            (key.source, key.index, key.class).hash(&mut h);
            let numbers = [arc.centre.x, arc.centre.y, arc.radius, arc.start, arc.sweep];
            numbers.map(f64::to_bits).hash(&mut h);
        }
        h.finish()
    }
}

/// The R-trees of the targets.
#[derive(Clone, Debug, Default)]
struct Trees {
    points: RTree<IndexedPoint>,
    edges: RTree<IndexedEdge>,
    arcs: RTree<IndexedArc>,
}

impl Trees {
    fn bulk_load<'a>(owners: impl Iterator<Item = &'a OwnerTargets> + Clone) -> Self {
        Self {
            points: RTree::bulk_load(owners.clone().flat_map(|t| t.points.iter().copied()).collect()),
            edges: RTree::bulk_load(owners.clone().flat_map(|t| t.edges.iter().copied()).collect()),
            arcs: RTree::bulk_load(owners.flat_map(|t| t.arcs.iter().copied()).collect()),
        }
    }

    /// Inserts the targets of one owner.
    fn insert(&mut self, targets: &OwnerTargets) {
        for p in &targets.points {
            self.points.insert(*p);
        }
        for e in &targets.edges {
            self.edges.insert(*e);
        }
        for a in &targets.arcs {
            self.arcs.insert(*a);
        }
    }

    /// Removes the targets of one owner.
    fn remove(&mut self, targets: &OwnerTargets) {
        for p in &targets.points {
            self.points.remove(p);
        }
        for e in &targets.edges {
            self.edges.remove(e);
        }
        for a in &targets.arcs {
            self.arcs.remove(a);
        }
    }
}

/// Nodes in the order given, nodes within the model tolerance merged into
/// the first one with the best kind of them (NFR-ACC-003, INP-009).
pub(super) fn weld_nodes(nodes: impl IntoIterator<Item = Snap>) -> Vec<Snap> {
    let mut welder = Welder::new();
    let mut kinds: Vec<SnapKind> = Vec::new();
    for node in nodes {
        let group = welder.weld(node.point);
        match kinds.get_mut(group) {
            Some(kind) => *kind = (*kind).min(node.kind),
            None => kinds.push(node.kind),
        }
    }
    let points = welder.into_points().into_iter();
    points.zip(kinds).map(|(point, kind)| Snap { point, kind }).collect()
}

/// How often the index was built and updated, for tests and diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IndexStats {
    /// Bulk loads of all targets.
    pub full_builds: u64,
    /// Owners inserted by incremental updates, new or changed, and by
    /// switching their source back on.
    pub inserted_owners: u64,
    /// Owners removed by incremental updates, gone or changed, and by
    /// switching their source off.
    pub removed_owners: u64,
}

/// The snap targets of one storey in R-trees (INP-008, NFR-PERF-005).
#[derive(Clone, Debug, Default)]
pub struct SnapIndex {
    revision: Option<u64>,
    trees: Trees,
    owners: HashMap<TargetOwner, OwnerTargets>,
    /// The position of each source in the order the sources were given.
    order: HashMap<(TargetOwner, u8), u32>,
    /// Owners whose targets are kept but not in the trees (INP-017).
    excluded: HashSet<TargetOwner>,
    stats: IndexStats,
}

impl SnapIndex {
    /// The model revision the index shows, if it was built.
    pub fn revision(&self) -> Option<u64> {
        self.revision
    }

    pub fn stats(&self) -> IndexStats {
        self.stats
    }

    /// Brings the index to the project's revision: the reference lines of
    /// the walls that provide snaps in model order, then the elements of
    /// `plan`, the storey's plan. Hidden and excluded elements provide none;
    /// locked elements stay targets (INP-017).
    pub fn update(&mut self, project: &Project, storey: StoreyId, plan: &PlanDrawing) {
        if self.revision == Some(project.revision) {
            return;
        }
        let snappable = |id: ElementId| project.element(id).is_some_and(|e| project.provides_snaps(e));
        let references = project
            .elements_on(storey)
            .filter(|e| project.provides_snaps(e))
            .map(|e| e as &dyn SnapSource);
        let plans = plan
            .elements
            .iter()
            .filter(|e| snappable(e.id))
            .map(|e| e as &dyn SnapSource);
        let sources: Vec<&dyn SnapSource> = references.chain(plans).collect();
        self.sync(project.revision, &sources);
    }

    /// Brings the index to `sources` at `revision`. Of coincident targets,
    /// those of earlier sources are taken.
    pub fn sync(&mut self, revision: u64, sources: &[&dyn SnapSource]) {
        let mut owners: HashMap<TargetOwner, OwnerTargets> = HashMap::new();
        let mut order = HashMap::with_capacity(sources.len());
        for (position, source) in sources.iter().enumerate() {
            let owner = source.owner();
            let targets = owners.entry(owner).or_default();
            let mut sink = TargetSink {
                owner,
                source: targets.sources,
                points: Vec::new(),
                edges: Vec::new(),
                arcs: Vec::new(),
                chains: 0,
            };
            source.targets(&mut sink);
            order.insert((owner, targets.sources), position as u32);
            targets.sources += 1;
            targets.points.append(&mut sink.points);
            targets.edges.append(&mut sink.edges);
            targets.arcs.append(&mut sink.arcs);
        }
        for targets in owners.values_mut() {
            targets.fingerprint = targets.fingerprint();
        }

        let changed: Vec<TargetOwner> = owners
            .iter()
            .filter(|(owner, t)| {
                self.owners
                    .get(owner)
                    .is_none_or(|old| old.fingerprint != t.fingerprint)
            })
            .map(|(owner, _)| *owner)
            .collect();
        let gone: Vec<TargetOwner> = self
            .owners
            .keys()
            .filter(|owner| !owners.contains_key(owner))
            .copied()
            .collect();
        let edits = changed.len() + gone.len();
        if self.revision.is_none() || edits * 4 > owners.len() {
            let included = owners.iter().filter(|(owner, _)| !self.excluded.contains(owner));
            self.trees = Trees::bulk_load(included.map(|(_, t)| t));
            self.stats.full_builds += 1;
        } else {
            for owner in gone.iter().chain(&changed) {
                self.remove_owner(*owner);
            }
            for owner in &changed {
                if !self.excluded.contains(owner) {
                    self.trees.insert(&owners[owner]);
                    self.stats.inserted_owners += 1;
                }
            }
        }
        self.owners = owners;
        self.order = order;
        self.revision = Some(revision);
    }

    /// Switches all targets of `source` off or back on (INP-017), for example
    /// a linked reference model's. Only that source's targets leave or
    /// enter the trees; the switch holds across revisions until it is
    /// switched back.
    pub fn set_source_excluded(&mut self, source: TargetOwner, excluded: bool) {
        if excluded {
            if self.excluded.contains(&source) {
                return;
            }
            self.remove_owner(source);
            self.excluded.insert(source);
        } else if self.excluded.remove(&source)
            && let Some(targets) = self.owners.get(&source)
        {
            self.trees.insert(targets);
            self.stats.inserted_owners += 1;
        }
    }

    /// Removes the targets of `owner` from the trees, unless they are not
    /// there because the owner is new or excluded.
    fn remove_owner(&mut self, owner: TargetOwner) {
        if self.excluded.contains(&owner) {
            return;
        }
        if let Some(old) = self.owners.get(&owner) {
            self.trees.remove(old);
            self.stats.removed_owners += 1;
        }
    }

    /// The order of a target among all targets: its source's position, then
    /// its number in the source.
    fn rank(&self, key: &TargetKey) -> (u32, u32) {
        (self.order[&(key.owner, key.source)], key.index)
    }

    /// The nodes within `radius` of `cursor` in source order, nodes within
    /// the model tolerance merged into the first one (NFR-ACC-003). The
    /// search reaches beyond the radius by twice the tolerance, so a node
    /// within the radius merges with the same node as in the whole storey.
    pub fn nodes_near(&self, cursor: DVec2, radius: f64) -> Vec<DVec2> {
        self.targets_near(cursor, radius).iter().map(|s| s.point).collect()
    }

    /// The nodes of [`Self::nodes_near`] with their kinds: a merged node has
    /// the best kind of the nodes merged into it (INP-009).
    pub fn targets_near(&self, cursor: DVec2, radius: f64) -> Vec<Snap> {
        let reach = radius + 2.0 * MODEL_TOLERANCE;
        let mut found: Vec<&IndexedPoint> = self
            .trees
            .points
            .locate_within_distance(cursor.to_array(), reach * reach)
            .collect();
        found.sort_by_key(|p| self.rank(&p.key));
        weld_nodes(found.into_iter().map(|p| Snap {
            point: p.point,
            kind: p.kind,
        }))
    }

    /// The edges whose closest point lies within `radius` of `cursor`, in
    /// source order.
    pub fn edges_near(&self, cursor: DVec2, radius: f64) -> Vec<[DVec2; 2]> {
        let reach = radius + MODEL_TOLERANCE;
        let mut found: Vec<&IndexedEdge> = self
            .trees
            .edges
            .locate_within_distance(cursor.to_array(), reach * reach)
            .filter(|e| closest_on_segment(cursor, e.edge).distance(cursor) <= radius)
            .collect();
        found.sort_by_key(|e| self.rank(&e.key));
        found.into_iter().map(|e| e.edge).collect()
    }

    /// The arcs whose centre or circle lies within `radius` of `cursor`, in
    /// source order: every arc with a centre or a tangent point that near.
    pub fn arcs_near(&self, cursor: DVec2, radius: f64) -> Vec<ArcTarget> {
        let reach = radius + MODEL_TOLERANCE;
        let mut found: Vec<&IndexedArc> = self
            .trees
            .arcs
            .locate_within_distance(cursor.to_array(), reach * reach)
            .filter(|a| a.arc.reach(cursor) <= reach)
            .collect();
        found.sort_by_key(|a| self.rank(&a.key));
        found.into_iter().map(|a| a.arc).collect()
    }

    /// The edge named `key`, unless it is gone or its owner is excluded.
    fn edge(&self, key: EdgeKey) -> Option<&IndexedEdge> {
        if self.excluded.contains(&key.owner) {
            return None;
        }
        let edges = &self.owners.get(&key.owner)?.edges;
        edges.iter().find(|e| e.edge_key() == key)
    }

    /// The edge under the cursor (INP-010): the edge `kept` from the last
    /// position while the cursor stays within [`HOVER_HYSTERESIS`] times
    /// `radius` of it, otherwise the edge nearest the cursor within
    /// `radius`. Of edges equally near within the model tolerance, such as
    /// a wall's reference line and the slab outline on it, an edge of a
    /// `selection` element is hovered, also over a kept edge that is not,
    /// and otherwise the first in source order.
    fn hovered_edge(
        &self,
        cursor: DVec2,
        radius: f64,
        kept: Option<EdgeKey>,
        selection: &[ElementId],
    ) -> Option<&IndexedEdge> {
        let selected = |e: &&IndexedEdge| matches!(e.key.owner, TargetOwner::Element(id) if selection.contains(&id));
        let kept = (kept.and_then(|key| self.edge(key))).filter(|e| e.distance(cursor) <= HOVER_HYSTERESIS * radius);
        if let Some(edge) = kept.filter(|e| selection.is_empty() || selected(e)) {
            return Some(edge);
        }
        let reach = radius + MODEL_TOLERANCE;
        let near: Vec<(&IndexedEdge, f64)> = (self.trees.edges)
            .locate_within_distance(cursor.to_array(), reach * reach)
            .map(|e| (e, e.distance(cursor)))
            .filter(|(_, d)| *d <= radius)
            .collect();
        let nearest = near.iter().map(|(_, d)| *d).fold(f64::INFINITY, f64::min);
        let coincident: Vec<&IndexedEdge> = (near.into_iter())
            .filter(|(_, d)| *d <= nearest + MODEL_TOLERANCE)
            .map(|(e, _)| e)
            .collect();
        let rank = |e: &&IndexedEdge| self.rank(&e.key);
        (coincident.iter().copied().filter(selected).min_by_key(rank))
            .or(kept)
            .or_else(|| coincident.into_iter().min_by_key(rank))
    }

    /// The hovered edge at `cursor` and the division points of `settings`'
    /// mode on the stretch its scope gives (INP-010): the edge itself, its
    /// part around the cursor between intersections with other owners'
    /// edges, or the owner's whole chain. `kept` is the edge hovered at the
    /// last position, which the cursor keeps within [`HOVER_HYSTERESIS`]
    /// times `radius`; of coincident edges, one of a `selection` element is
    /// hovered, so the selected slab's outline under walls' reference lines
    /// can be divided. Only this one stretch is divided, into at most
    /// [`super::MAX_DIVISION_POINTS`] points.
    pub fn hover(
        &self,
        cursor: DVec2,
        radius: f64,
        kept: Option<EdgeKey>,
        selection: &[ElementId],
        settings: InputSettings,
    ) -> Option<Hovered> {
        let edge = self.hovered_edge(cursor, radius, kept, selection)?;
        let span = match settings.scope {
            DivisionScope::BetweenNodes => Chain::segment(edge.edge),
            DivisionScope::BetweenIntersections => Chain::segment(self.between_intersections(edge, cursor)),
            DivisionScope::WholeElement => self.chain_of(edge),
        };
        let mode = settings.division.sanitized();
        Some(Hovered {
            key: edge.edge_key(),
            edge: edge.edge,
            points: span.divide(mode, cursor),
            span,
            kind: if mode == DivisionMode::Half {
                SnapKind::Midpoint
            } else {
                SnapKind::Division
            },
        })
    }

    /// The part of `edge` around the cursor's projection between its
    /// intersections with the edges of other owners, the pairwise
    /// intersection of snapping restricted to this edge (INP-010).
    fn between_intersections(&self, edge: &IndexedEdge, cursor: DVec2) -> [DVec2; 2] {
        let [a, b] = edge.edge;
        let along = |p: DVec2| super::Curve::Segment(edge.edge).parameter(p);
        let at = along(cursor);
        let (mut from, mut to) = (0.0_f64, 1.0_f64);
        let crossing = (self.trees.edges)
            .locate_in_envelope_intersecting(&edge.envelope())
            .filter(|other| other.key.owner != edge.key.owner)
            .filter_map(|other| segment_intersection(edge.edge, other.edge));
        for t in crossing.map(along) {
            if t <= at {
                from = from.max(t);
            } else {
                to = to.min(t);
            }
        }
        [a + (b - a) * from, a + (b - a) * to]
    }

    /// The whole chain `edge` belongs to (INP-010): all edges of its owner's
    /// source with its chain number, in order.
    fn chain_of(&self, edge: &IndexedEdge) -> Chain {
        let edges: Vec<[DVec2; 2]> = (self.owners.get(&edge.key.owner))
            .map(|t| &t.edges[..])
            .unwrap_or_default()
            .iter()
            .filter(|e| e.key.source == edge.key.source && e.chain == edge.chain)
            .map(|e| e.edge)
            .collect();
        let (Some(first), Some(last)) = (edges.first(), edges.last()) else {
            return Chain::segment(edge.edge);
        };
        let closed = edges.len() > 2 && coincident(last[1], first[0]);
        let mut points: Vec<DVec2> = edges.iter().map(|e| e[0]).collect();
        if !closed {
            points.push(last[1]);
        }
        Chain { points, closed }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::f64::consts::PI;
    use std::time::Instant;

    use archicraft_model::template::default_metric_project;
    use archicraft_model::{Command, ElementKind, Layer, samples};

    use super::*;
    use crate::document::Document;
    use crate::input::snap::{ArcTarget, Targets, collect, segment_intersection};
    use crate::input::{
        DivisionMode, DivisionScope, Expect, InputContext, Modifiers, PointInput, Resolved, Snap, SnapKind, SnapLevel,
        SurfaceFrame, TrackerMode, candidates, snap,
    };
    use crate::persistence::prefs::Preferences;
    use crate::workspace::Workspace;

    /// Explicit nodes and edges, for tests of snapping.
    struct Explicit<'a> {
        nodes: &'a [DVec2],
        edges: &'a [[DVec2; 2]],
    }

    impl SnapSource for Explicit<'_> {
        fn owner(&self) -> TargetOwner {
            TargetOwner::Guide(0)
        }

        fn targets(&self, sink: &mut TargetSink) {
            for &p in self.nodes {
                sink.node(SourceClass::Guide, p);
            }
            for &e in self.edges {
                sink.edge(SourceClass::Guide, e);
            }
        }
    }

    /// An index of the given nodes and edges.
    pub(crate) fn index_of(nodes: &[DVec2], edges: &[[DVec2; 2]]) -> SnapIndex {
        let mut index = SnapIndex::default();
        index.sync(0, &[&Explicit { nodes, edges }]);
        index
    }

    /// The path the index replaces: every node and edge of the storey,
    /// collected each frame and scanned for each cursor position.
    struct BruteForce {
        nodes: Vec<Snap>,
        edges: Vec<[DVec2; 2]>,
    }

    impl BruteForce {
        fn from_plan(plan: &PlanDrawing, project: &Project, storey: StoreyId) -> Self {
            let mut nodes = Vec::new();
            let node = |point| Snap {
                point,
                kind: SnapKind::Node,
            };
            let centroid = |point| Snap {
                point,
                kind: SnapKind::Centroid,
            };
            let mut edges = Vec::new();
            for element in project.elements_on(storey).filter(|e| project.is_visible(e)) {
                if let Some(wall) = element.as_wall() {
                    nodes.extend([node(wall.start), node(wall.end)]);
                    edges.push([wall.start, wall.end]);
                }
                if let Some(slab) = element.as_slab() {
                    nodes.extend(outline_centroid(&slab.outline, &slab.holes).map(centroid));
                    for ring in std::iter::once(&slab.outline).chain(&slab.holes) {
                        edges.extend((0..ring.len()).map(|i| [ring[i], ring[(i + 1) % ring.len()]]));
                    }
                }
            }
            for element in &plan.elements {
                for polygon in &element.cut {
                    for ring in polygon_rings(polygon) {
                        for i in 0..ring.len() {
                            nodes.push(node(ring[i]));
                            edges.push([ring[i], ring[(i + 1) % ring.len()]]);
                        }
                    }
                }
                for segment in element.projected.iter().chain(&element.overhead) {
                    nodes.extend(segment.map(node));
                    edges.push(*segment);
                }
            }
            Self {
                nodes: weld_nodes(nodes),
                edges,
            }
        }

        fn candidates(
            &self,
            pending: &[DVec2],
            cursor: DVec2,
            radius: f64,
            grid: f64,
            previous: Option<DVec2>,
        ) -> Vec<Snap> {
            let mut nodes = self.nodes.clone();
            nodes.extend(pending.iter().map(|&point| Snap {
                point,
                kind: SnapKind::Node,
            }));
            let distance = |e: &[DVec2; 2]| closest_on_segment(cursor, *e).distance(cursor);
            let near: Vec<[DVec2; 2]> = (self.edges.iter().copied()).filter(|e| distance(e) <= radius).collect();
            // The hovered edge in the default settings: the first of the
            // nearest edges, divided at its midpoint (INP-010).
            let nearest = near.iter().map(distance).fold(f64::INFINITY, f64::min);
            let divisions: Vec<Snap> = (near.iter())
                .find(|e| distance(e) <= nearest + MODEL_TOLERANCE)
                .map(|[a, b]| Snap {
                    point: (*a + *b) / 2.0,
                    kind: SnapKind::Midpoint,
                })
                .into_iter()
                .collect();
            let targets = Targets {
                nodes: &nodes,
                edges: &near,
                arcs: &[],
                guides: &[],
                divisions: &divisions,
            };
            collect(&targets, cursor, radius, grid, previous)
        }
    }

    /// A seeded generator of numbers in [0, 1) (SplitMix64).
    struct Random(u64);

    impl Random {
        fn next(&mut self) -> f64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
        }

        fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
            &items[(self.next() * items.len() as f64) as usize]
        }

        fn between(&mut self, lo: DVec2, hi: DVec2) -> DVec2 {
            lo + (hi - lo) * DVec2::new(self.next(), self.next())
        }
    }

    fn storeys(project: &Project) -> Vec<StoreyId> {
        project.storeys.as_slice().iter().map(|s| s.id).collect()
    }

    /// Resolves `cursor` on `storey` for the first point of a wall, as a
    /// plan view does each frame: the index from the workspace, then point
    /// input.
    fn resolve(ws: &mut Workspace, input: &mut PointInput, storey: StoreyId, cursor: DVec2) -> Resolved {
        let targets = ws.snap_index(storey);
        let ctx = InputContext {
            project: &ws.doc.project,
            storey,
            targets: &targets,
            pending: &[],
            previous: None,
            guides: &[],
            selection: &ws.selection.iter().copied().collect::<Vec<_>>(),
            input: ws.prefs.input,
            expects: Expect::Point,
            tracker: TrackerMode::Relative,
            tracker_xyz: false,
            radius: 0.2,
            drag_threshold: 0.06,
            units: &ws.doc.project.settings.units,
            number_format: &ws.prefs.number_format(),
            angle_increment_deg: 45.0,
            constraint_reference: crate::input::ConstraintReference::XAxis,
            frame_rotation: 0.0,
            snap_step: 0.0,
            frame: SurfaceFrame::plan(0.0),
            user_origin: crate::input::Frame2d::default(),
            marked: None,
        };
        input.resolve(cursor, Modifiers::default(), &ctx)
    }

    fn workspace(project: Project) -> Workspace {
        Workspace::new(Document::new(project, 50), Preferences::default())
    }

    /// NFR-PERF-005, INP-008, INP-009: on both storeys of two_storey_shell
    /// the index gives the same candidates as scanning every target, for 500
    /// seeded cursor positions at three radii, with and without a previous
    /// point and pending points.
    #[test]
    fn the_index_snaps_like_the_brute_force_scan() {
        let project = samples::two_storey_shell();
        let mut doc = Document::new(project.clone(), 10);
        let mut random = Random(4_007);
        let pending = [DVec2::new(5.0, 6.0), DVec2::new(2.5, 4.0)];
        let mut kinds = Vec::new();
        for storey in storeys(&project) {
            let plan = doc.plan(storey);
            let brute = BruteForce::from_plan(&plan, &project, storey);
            let mut index = SnapIndex::default();
            index.update(&project, storey, &plan);
            let (lo, hi) = plan.bounds().unwrap();
            let pad = DVec2::splat(1.0);
            let crossings: Vec<DVec2> = (brute.edges.iter().enumerate())
                .flat_map(|(i, a)| brute.edges[i + 1..].iter().filter_map(|b| segment_intersection(*a, *b)))
                .filter(|p| brute.nodes.iter().all(|n| n.point.distance(*p) > 0.05))
                .collect();
            for i in 0..500 {
                // Half anywhere, half near a node, a crossing of edges or an
                // edge's midpoint, where targets compete.
                let at = match i % 6 {
                    1 => random.pick(&brute.nodes).point,
                    3 if !crossings.is_empty() => *random.pick(&crossings),
                    5 => {
                        let [a, b] = random.pick(&brute.edges);
                        (*a + *b) / 2.0
                    }
                    _ => random.between(lo - pad, hi + pad),
                };
                let cursor = if i % 2 == 0 {
                    at
                } else {
                    at + random.between(DVec2::splat(-0.3), DVec2::splat(0.3))
                };
                let previous = (i % 3 != 0).then(|| random.between(lo - pad, hi + pad));
                let pending: &[DVec2] = if i % 5 == 0 { &pending } else { &[] };
                for radius in [0.05, 0.2, 1.0] {
                    let grid = if i % 7 == 0 { 0.05 } else { 0.0 };
                    let expected = brute.candidates(pending, cursor, radius, grid, previous);
                    let found = candidates(&index, pending, cursor, radius, grid, previous, &[]);
                    assert_eq!(found, expected, "{cursor} r {radius} previous {previous:?}");
                    kinds.push(snap(&index, pending, cursor, radius, grid, previous).kind);
                }
            }
        }
        for kind in [
            SnapKind::Node,
            SnapKind::Centroid,
            SnapKind::Intersection,
            SnapKind::Midpoint,
            SnapKind::Perpendicular,
            SnapKind::Edge,
            SnapKind::Grid,
            SnapKind::Free,
        ] {
            assert!(kinds.contains(&kind), "no {kind:?} among the cases");
        }
    }

    /// NFR-PERF-005: across 200 resolves the index is built once per storey
    /// and revision; adding one wall inserts exactly its owner and makes its
    /// nodes targets, and undoing it removes them again.
    #[test]
    fn the_index_is_built_once_and_updated_per_owner() {
        let mut ws = workspace(samples::two_storey_shell());
        let mut input = PointInput::default();
        let all = storeys(&ws.doc.project);
        let (ground, upper) = (all[0], all[1]);
        let mut random = Random(17);
        for _ in 0..200 {
            for &storey in &all {
                let cursor = random.between(DVec2::splat(-1.0), DVec2::new(13.0, 10.0));
                resolve(&mut ws, &mut input, storey, cursor);
            }
        }
        let built = IndexStats {
            full_builds: 1,
            ..IndexStats::default()
        };
        for &storey in &all {
            assert_eq!(ws.snap_index(storey).stats(), built);
        }

        let end = DVec2::new(24.0, 20.0);
        let near_end = end + DVec2::new(0.05, 0.03);
        assert_eq!(resolve(&mut ws, &mut input, ground, near_end).snap.kind, SnapKind::Free);
        let wall = ws.doc.project.new_wall(ground, DVec2::new(20.0, 20.0), end, 9);
        assert!(ws.execute(Command::AddElements(vec![wall])));
        let snapped = resolve(&mut ws, &mut input, ground, near_end).snap;
        assert_eq!((snapped.kind, snapped.point), (SnapKind::Node, end));
        let added = IndexStats {
            full_builds: 1,
            inserted_owners: 1,
            removed_owners: 0,
        };
        assert_eq!(ws.snap_index(ground).stats(), added);
        assert_eq!(ws.snap_index(upper).stats(), built, "nothing changed upstairs");

        ws.undo();
        assert_eq!(resolve(&mut ws, &mut input, ground, near_end).snap.kind, SnapKind::Free);
        assert_eq!(
            ws.snap_index(ground).stats(),
            IndexStats {
                removed_owners: 1,
                ..added
            }
        );
    }

    /// INP-017: a locked wall and a wall on a locked layer keep providing
    /// their nodes and edges as snap targets; a wall on a hidden layer
    /// provides none.
    #[test]
    fn locked_walls_stay_targets_and_hidden_ones_do_not() {
        let mut project = default_metric_project();
        let storey = project.storeys.ground();
        let locked_layer = Layer {
            locked: true,
            ..Layer::new("Locked")
        };
        let hidden_layer = Layer {
            visible: false,
            ..Layer::new("Hidden")
        };
        let mut locked = project.new_wall(storey, DVec2::ZERO, DVec2::new(4.0, 0.0), 0);
        locked.locked = true;
        let mut on_locked_layer = project.new_wall(storey, DVec2::new(0.0, 3.0), DVec2::new(4.0, 3.0), 1);
        on_locked_layer.layer = locked_layer.id;
        let mut hidden = project.new_wall(storey, DVec2::new(0.0, 6.0), DVec2::new(4.0, 6.0), 2);
        hidden.layer = hidden_layer.id;
        project.layers.extend([locked_layer, hidden_layer]);
        let walls = [&locked, &on_locked_layer, &hidden].map(|w| {
            let w = w.as_wall().unwrap();
            [w.start, w.end]
        });
        let mut ws = workspace(project);
        assert!(ws.execute(Command::AddElements(vec![locked, on_locked_layer, hidden])));
        let index = ws.snap_index(storey);

        for [start, end] in &walls[..2] {
            assert!(index.nodes_near(*start, 0.01).contains(start), "{start}");
            assert!(index.nodes_near(*end, 0.01).contains(end), "{end}");
            let middle = (*start + *end) / 2.0;
            assert!(index.edges_near(middle, 0.01).contains(&[*start, *end]), "{middle}");
            let s = snap(&index, &[], *end + DVec2::new(0.02, 0.01), 0.1, 0.0, None);
            assert_eq!((s.kind, s.point), (SnapKind::Node, *end));
        }
        let [start, end] = walls[2];
        assert!(index.nodes_near(start, 0.5).is_empty());
        assert!(index.edges_near((start + end) / 2.0, 0.5).is_empty());
        let s = snap(&index, &[], end + DVec2::new(0.02, 0.01), 0.1, 0.0, None);
        assert_eq!(s.kind, SnapKind::Free);
    }

    /// The ends of `wall` and whether the index offers them as nodes and
    /// its middle as an edge.
    fn offers(index: &SnapIndex, wall: &Element) -> (bool, bool) {
        let w = wall.as_wall().unwrap();
        let nodes = [w.start, w.end].iter().all(|p| index.nodes_near(*p, 0.01).contains(p));
        let edge = (index.edges_near((w.start + w.end) / 2.0, 0.01)).contains(&[w.start, w.end]);
        (nodes, edge)
    }

    /// INP-017: an excluded wall and a wall on an excluded layer provide no
    /// nodes or edges while they stay visible; a locked wall still does.
    /// Excluding a wall by command drops only its owner from the index.
    #[test]
    fn excluded_walls_and_layers_provide_no_targets() {
        let mut project = default_metric_project();
        let storey = project.storeys.ground();
        let excluded_layer = Layer {
            snap_excluded: true,
            ..Layer::new("No snaps")
        };
        let mut excluded = project.new_wall(storey, DVec2::ZERO, DVec2::new(4.0, 0.0), 0);
        excluded.snap_excluded = true;
        let mut on_excluded_layer = project.new_wall(storey, DVec2::new(0.0, 3.0), DVec2::new(4.0, 3.0), 1);
        on_excluded_layer.layer = excluded_layer.id;
        let mut locked = project.new_wall(storey, DVec2::new(0.0, 6.0), DVec2::new(4.0, 6.0), 2);
        locked.locked = true;
        project.layers.push(excluded_layer);
        // Enough other walls that excluding one updates the index in place.
        let others = (3..8).map(|n| {
            let y = 3.0 * f64::from(n);
            project.new_wall(storey, DVec2::new(0.0, y), DVec2::new(4.0, y), n)
        });
        let walls: Vec<Element> = [excluded, on_excluded_layer, locked]
            .into_iter()
            .chain(others)
            .collect();
        let mut ws = workspace(project);
        assert!(ws.execute(Command::AddElements(walls.clone())));
        let index = ws.snap_index(storey);
        for wall in &walls[..2] {
            assert!(ws.doc.project.is_visible(wall), "{}", wall.name);
            assert_eq!(offers(&index, wall), (false, false), "{}", wall.name);
            let end = wall.as_wall().unwrap().end;
            let s = snap(&index, &[], end + DVec2::new(0.02, 0.01), 0.1, 0.0, None);
            assert_eq!(s.kind, SnapKind::Free, "{}", wall.name);
        }
        for wall in &walls[2..] {
            assert_eq!(offers(&index, wall), (true, true), "{}", wall.name);
        }

        let before = index.stats();
        let locked = Element {
            snap_excluded: true,
            ..walls[2].clone()
        };
        assert!(ws.execute(Command::UpdateElements(vec![locked])));
        let index = ws.snap_index(storey);
        assert_eq!(offers(&index, &walls[2]), (false, false));
        for wall in &walls[3..] {
            assert_eq!(offers(&index, wall), (true, true), "{}", wall.name);
        }
        assert_eq!(
            index.stats(),
            IndexStats {
                removed_owners: before.removed_owners + 1,
                ..before
            }
        );
    }

    /// A stub of a linked reference model's targets (IFX-084).
    struct Reference(Vec<DVec2>);

    impl SnapSource for Reference {
        fn owner(&self) -> TargetOwner {
            TargetOwner::Reference(1)
        }

        fn targets(&self, sink: &mut TargetSink) {
            for &p in &self.0 {
                sink.node(SourceClass::Reference, p);
            }
            sink.edge(SourceClass::Reference, [self.0[0], self.0[1]]);
        }
    }

    /// INP-017: switching a reference source off removes all its targets and
    /// switching it back restores them, without a full rebuild; the switch
    /// holds across later revisions.
    #[test]
    fn a_reference_source_can_be_switched_off_and_on() {
        let reference = Reference(vec![DVec2::new(10.0, 0.0), DVec2::new(10.0, 5.0)]);
        let walls = [DVec2::ZERO, DVec2::new(4.0, 0.0)];
        let explicit = Explicit {
            nodes: &walls,
            edges: &[[walls[0], walls[1]]],
        };
        let mut index = SnapIndex::default();
        index.sync(1, &[&explicit, &reference]);
        let offered = |index: &SnapIndex| {
            let nodes = reference
                .0
                .iter()
                .filter(|p| index.nodes_near(**p, 0.01).contains(p))
                .count();
            let edges = index.edges_near(DVec2::new(10.0, 2.5), 0.01).len();
            (nodes, edges)
        };
        assert_eq!(offered(&index), (2, 1));
        let built = index.stats();

        index.set_source_excluded(TargetOwner::Reference(1), true);
        assert_eq!(offered(&index), (0, 0));
        assert_eq!(index.nodes_near(walls[1], 0.01), vec![walls[1]], "other sources stay");
        index.sync(2, &[&explicit, &reference]);
        assert_eq!(offered(&index), (0, 0), "the switch holds across revisions");

        index.set_source_excluded(TargetOwner::Reference(1), false);
        assert_eq!(offered(&index), (2, 1));
        assert_eq!(index.stats().full_builds, built.full_builds);
        index.sync(3, &[&explicit, &reference]);
        assert_eq!(offered(&index), (2, 1));
        assert_eq!(index.stats().full_builds, built.full_builds);
    }

    /// A synthetic source of arcs, axis points and handles, which no
    /// element provides yet.
    #[derive(Default)]
    struct Synthetic {
        arcs: Vec<ArcTarget>,
        axis_points: Vec<DVec2>,
        handles: Vec<DVec2>,
    }

    impl SnapSource for Synthetic {
        fn owner(&self) -> TargetOwner {
            TargetOwner::Guide(1)
        }

        fn targets(&self, sink: &mut TargetSink) {
            for &arc in &self.arcs {
                sink.arc(SourceClass::Guide, arc);
            }
            for &p in &self.axis_points {
                sink.axis_point(SourceClass::Guide, p);
            }
            for &p in &self.handles {
                sink.handle(SourceClass::Guide, p);
            }
        }
    }

    fn index_with(sources: &[&dyn SnapSource]) -> SnapIndex {
        let mut index = SnapIndex::default();
        index.sync(0, sources);
        index
    }

    fn assert_candidates(found: &[Snap], expected: &[(SnapKind, DVec2)]) {
        assert_eq!(found.len(), expected.len(), "{found:?}");
        for (snap, (kind, point)) in found.iter().zip(expected) {
            assert_eq!(snap.kind, *kind, "{found:?}");
            assert!(snap.point.distance(*point) < 1e-9, "{snap:?} is not at {point}");
        }
    }

    /// INP-008: a circle offers its centre and both tangent points from the
    /// previous point, at c + r(cos(θ ± φ), sin(θ ± φ)); an arc offers only
    /// the tangent point inside its sweep, either way round; from inside
    /// the circle there is none.
    #[test]
    fn circles_and_arcs_offer_centres_and_tangent_points() {
        let centre = DVec2::new(1.0, 2.0);
        let previous = Some(centre + DVec2::new(2.0, 0.0));
        // |p - c| = 2r: φ = 60°.
        let (upper, lower) = (
            centre + DVec2::new(0.5, 0.75_f64.sqrt()),
            centre + DVec2::new(0.5, -(0.75_f64.sqrt())),
        );
        let cursor = centre + DVec2::new(0.5, 0.0);
        let circle = ArcTarget::circle(centre, 1.0);
        let index = index_with(&[&Synthetic {
            arcs: vec![circle],
            ..Synthetic::default()
        }]);
        assert_candidates(
            &candidates(&index, &[], cursor, 1.0, 0.0, previous, &[]),
            &[
                (SnapKind::Centre, centre),
                (SnapKind::Tangent, upper),
                (SnapKind::Tangent, lower),
            ],
        );
        let near_upper = upper + DVec2::new(0.03, 0.02);
        let s = snap(&index, &[], near_upper, 0.1, 0.0, previous);
        assert_eq!((s.kind, s.kind.label()), (SnapKind::Tangent, "tangent"));
        assert!(s.point.distance(upper) < 1e-12);
        assert_eq!(snap(&index, &[], near_upper, 0.1, 0.0, None).kind, SnapKind::Free);
        let inside = Some(centre + DVec2::new(0.2, 0.1));
        assert_candidates(
            &candidates(&index, &[], cursor, 1.0, 0.0, inside, &[]),
            &[(SnapKind::Centre, centre)],
        );

        let half = |sweep: f64| ArcTarget {
            centre,
            radius: 1.0,
            start: 0.0,
            sweep,
        };
        for (arc, kept) in [(half(PI), upper), (half(-PI), lower)] {
            let index = index_with(&[&Synthetic {
                arcs: vec![arc],
                ..Synthetic::default()
            }]);
            assert_candidates(
                &candidates(&index, &[], cursor, 1.0, 0.0, previous, &[]),
                &[(SnapKind::Centre, centre), (SnapKind::Tangent, kept)],
            );
        }
    }

    /// INP-024: the ground slab of two_storey_shell, 12 m by 9 m, offers its
    /// centroid.
    #[test]
    fn the_ground_slab_offers_its_centroid() {
        let mut ws = workspace(samples::two_storey_shell());
        let ground = ws.doc.project.storeys.ground();
        let index = ws.snap_index(ground);
        let s = snap(&index, &[], DVec2::new(6.05, 4.47), 0.2, 0.0, None);
        assert_eq!((s.kind, s.kind.label()), (SnapKind::Centroid, "centroid"));
        assert!(s.point.distance(DVec2::new(6.0, 4.5)) < 1e-12, "{s:?}");
    }

    /// INP-024: a cut polygon offers no centroid, only its corners and
    /// sides: centroids belong to area elements such as slabs.
    #[test]
    fn a_cut_polygon_offers_no_centroid() {
        use archicraft_geometry::geo::{LineString, Polygon};
        let l_shape = Polygon::new(
            LineString::from(vec![
                (0.0, 0.0),
                (6.0, 0.0),
                (6.0, 3.0),
                (3.0, 3.0),
                (3.0, 6.0),
                (0.0, 6.0),
            ]),
            Vec::new(),
        );
        let element = PlanElement {
            id: ElementId::new(),
            cut: vec![l_shape],
            projected: Vec::new(),
            overhead: Vec::new(),
            reference_line: None,
        };
        let index = index_with(&[&element]);
        // The centroid (2.5, 2.5) is 0.71 from the inner corner (3, 3).
        let centroid = DVec2::splat(2.5);
        let near = candidates(&index, &[], centroid, 0.2, 0.0, None, &[]);
        assert!(near.is_empty(), "{near:?}");
        let found = candidates(&index, &[], centroid, 1.0, 0.0, None, &[]);
        assert!(!found.is_empty());
        assert!(found.iter().all(|s| s.kind != SnapKind::Centroid), "{found:?}");
    }

    /// INP-008, INP-009: axis points and handles are node-level candidates
    /// with their own labels; a handle on a plain node stays a node.
    #[test]
    fn axis_points_and_handles_are_node_candidates() {
        let (axis, handle, corner) = (DVec2::new(1.0, 1.0), DVec2::new(3.0, 1.0), DVec2::new(5.0, 1.0));
        let plain = Explicit {
            nodes: &[corner],
            edges: &[],
        };
        let synthetic = Synthetic {
            axis_points: vec![axis],
            handles: vec![handle, corner],
            ..Synthetic::default()
        };
        let index = index_with(&[&plain, &synthetic]);
        for (point, kind, label) in [
            (axis, SnapKind::AxisPoint, "axis point"),
            (handle, SnapKind::Handle, "handle"),
            (corner, SnapKind::Node, "node"),
        ] {
            let found = candidates(&index, &[], point + DVec2::new(0.04, 0.03), 0.2, 0.0, None, &[]);
            assert_candidates(&found, &[(kind, point)]);
            assert_eq!((kind.level(), kind.label()), (Some(SnapLevel::Node), label));
        }
    }

    /// The division points `resolved` shows, which must be on a hovered edge.
    fn ticks(resolved: &Resolved) -> &[DVec2] {
        &resolved.hovered.as_ref().expect("an edge is hovered").points
    }

    fn assert_points(found: &[DVec2], expected: &[DVec2]) {
        assert_eq!(found.len(), expected.len(), "{found:?} is not {expected:?}");
        for (f, e) in found.iter().zip(expected) {
            assert!(f.distance(*e) < 1e-9, "{found:?} is not {expected:?}");
        }
    }

    /// INP-010: with the cursor near a wall's reference line, away from its
    /// nodes, the hovered edge is that line with exactly the active mode's
    /// division points: three for four equal parts. Switching the mode
    /// replaces them; the edge stays hovered up to 1.5 times the snap radius
    /// from it, and beyond that the ticks disappear.
    #[test]
    fn hovering_an_edge_shows_the_active_modes_division_points() {
        let project = default_metric_project();
        let storey = project.storeys.ground();
        let line = [DVec2::ZERO, DVec2::new(4.0, 0.0)];
        let wall = project.new_wall(storey, line[0], line[1], 0);
        let mut ws = workspace(project);
        assert!(ws.execute(Command::AddElements(vec![wall])));
        let mut input = PointInput::default();
        ws.prefs.input.division = DivisionMode::Equal(4);
        let cursor = DVec2::new(1.3, -0.05);
        let resolved = resolve(&mut ws, &mut input, storey, cursor);
        let hovered = resolved.hovered.clone().unwrap();
        assert_eq!(hovered.edge, line);
        assert_eq!(hovered.kind, SnapKind::Division);
        assert_points(ticks(&resolved), &[1.0, 2.0, 3.0].map(|x| DVec2::new(x, 0.0)));

        ws.prefs.input.division = DivisionMode::Percent(25.0);
        let resolved = resolve(&mut ws, &mut input, storey, cursor);
        assert_points(ticks(&resolved), &[DVec2::new(1.0, 0.0)]);
        ws.prefs.input.division = DivisionMode::Half;
        let resolved = resolve(&mut ws, &mut input, storey, cursor);
        assert_points(ticks(&resolved), &[DVec2::new(2.0, 0.0)]);
        assert_eq!(resolved.hovered.unwrap().kind, SnapKind::Midpoint);

        // The radius is 0.2: hovered from 0.25 only when hovered before.
        let farther = DVec2::new(1.3, -0.25);
        assert!(resolve(&mut ws, &mut input, storey, farther).hovered.is_some());
        let fresh = resolve(&mut ws, &mut PointInput::default(), storey, farther);
        assert_eq!(fresh.hovered, None, "not hovered from scratch");
        let beyond = resolve(&mut ws, &mut input, storey, DVec2::new(1.3, -0.31));
        assert_eq!(beyond.hovered, None, "beyond 1.5 times the radius");
        assert!(resolve(&mut ws, &mut input, storey, farther).hovered.is_none());
    }

    fn scope(division: DivisionMode, scope: DivisionScope) -> crate::input::InputSettings {
        crate::input::InputSettings { division, scope }
    }

    /// INP-010: on the ground storey of two_storey_shell, between nodes
    /// divides only the hovered south reference line; between intersections
    /// divides its part on the cursor's side of x = 5, where the interior
    /// wall's reference line meets it; the whole element divides a cut
    /// ring or, with the walls switched off, the ground slab's whole
    /// 42 m perimeter.
    #[test]
    fn the_scope_limits_the_division_points() {
        let mut ws = workspace(samples::two_storey_shell());
        let ground = ws.doc.project.storeys.ground();
        let index = ws.snap_index(ground);
        let south = [DVec2::ZERO, DVec2::new(12.0, 0.0)];
        let at = |x: f64| DVec2::new(x, 0.0);
        let hover = |index: &SnapIndex, cursor: DVec2, settings| index.hover(cursor, 0.2, None, &[], settings).unwrap();

        let nodes = hover(&index, DVec2::new(2.0, -0.05), InputSettings::default());
        assert_eq!(nodes.edge, south);
        assert!(!nodes.span.closed);
        assert_points(&nodes.span.points, &south);
        assert_points(&nodes.points, &[at(6.0)]);

        let between = scope(DivisionMode::Half, DivisionScope::BetweenIntersections);
        let west = hover(&index, DVec2::new(2.0, -0.05), between);
        assert_eq!(west.edge, south);
        assert_points(&west.span.points, &[at(0.0), at(5.0)]);
        assert_points(&west.points, &[at(2.5)]);
        let east = hover(&index, DVec2::new(8.0, -0.05), between);
        assert_points(&east.span.points, &[at(5.0), at(12.0)]);
        assert_points(&east.points, &[at(8.5)]);
        let thirds = scope(DivisionMode::Equal(3), DivisionScope::BetweenIntersections);
        assert_points(
            &hover(&index, DVec2::new(1.0, -0.05), thirds).points,
            &[at(5.0 / 3.0), at(10.0 / 3.0)],
        );

        let whole = scope(DivisionMode::Equal(3), DivisionScope::WholeElement);
        let line = hover(&index, DVec2::new(2.0, -0.05), whole);
        assert!(!line.span.closed, "a reference line is its own chain");
        assert_points(&line.span.points, &south);
        let inner_face = hover(&index, DVec2::new(3.0, 0.3), whole);
        assert!(inner_face.span.closed);
        assert_eq!(inner_face.span.points.len(), 4, "the south wall's cut ring");
        assert_eq!(inner_face.points.len(), 2);

        let mut slab_only = (*index).clone();
        for wall in ws.doc.project.elements_on(ground).filter(|e| e.as_wall().is_some()) {
            slab_only.set_source_excluded(TargetOwner::Element(wall.id), true);
        }
        let perimeter = hover(&slab_only, DVec2::new(3.0, -0.05), whole);
        assert!(perimeter.span.closed);
        assert!((perimeter.span.length() - 42.0).abs() < 1e-9);
        assert_points(&perimeter.points, &[DVec2::new(12.0, 2.0), DVec2::new(5.0, 9.0)]);
    }

    /// INP-010: on the ground storey of two_storey_shell with all walls
    /// present, the ground slab's outline lies under the walls' reference
    /// lines. With nothing selected the south reference line is hovered;
    /// selecting the slab makes its outline the hovered one of the
    /// coincident edges, also over the reference line kept from the last
    /// position, so the whole element divides its 42 m perimeter. An edge
    /// nearer the cursor than the selected element's still wins.
    #[test]
    fn a_selected_elements_edge_wins_among_coincident_edges() {
        let mut ws = workspace(samples::two_storey_shell());
        let ground = ws.doc.project.storeys.ground();
        ws.prefs.input = scope(DivisionMode::Equal(3), DivisionScope::WholeElement);
        let slab = (ws.doc.project.elements_on(ground))
            .find(|e| matches!(e.kind, ElementKind::Slab(_)))
            .map(|e| e.id)
            .unwrap();
        let mut input = PointInput::default();
        let cursor = DVec2::new(3.0, -0.05);
        let line = resolve(&mut ws, &mut input, ground, cursor).hovered.unwrap();
        assert_eq!(line.edge, [DVec2::ZERO, DVec2::new(12.0, 0.0)]);
        assert!(!line.span.closed, "the south reference line");

        ws.selection.insert(slab);
        let perimeter = resolve(&mut ws, &mut input, ground, cursor).hovered.unwrap();
        assert_eq!(perimeter.key.owner, TargetOwner::Element(slab));
        assert!(perimeter.span.closed);
        assert!((perimeter.span.length() - 42.0).abs() < 1e-9);
        assert_points(&perimeter.points, &[DVec2::new(12.0, 2.0), DVec2::new(5.0, 9.0)]);
        let kept = resolve(&mut ws, &mut input, ground, DVec2::new(3.0, -0.25)).hovered;
        assert_eq!(
            kept.map(|h| h.key),
            Some(perimeter.key),
            "kept within 1.5 times the radius"
        );

        // At y = 0.19 the south wall's inner face at y = 0.3 is nearer than
        // the outline, both within the radius.
        let inner = resolve(&mut ws, &mut PointInput::default(), ground, DVec2::new(3.0, 0.19));
        assert_ne!(inner.hovered.unwrap().key.owner, TargetOwner::Element(slab));
    }

    /// INP-010: the hovered edge is kept by its owner and number, so it
    /// survives an incremental update of the index that adds another wall.
    #[test]
    fn the_hovered_edge_is_named_across_revisions() {
        let mut ws = workspace(samples::two_storey_shell());
        let ground = ws.doc.project.storeys.ground();
        let mut input = PointInput::default();
        let cursor = DVec2::new(2.0, -0.05);
        let before = resolve(&mut ws, &mut input, ground, cursor).hovered.unwrap();
        let wall = ws
            .doc
            .project
            .new_wall(ground, DVec2::new(20.0, 20.0), DVec2::new(24.0, 20.0), 9);
        assert!(ws.execute(Command::AddElements(vec![wall])));
        // From 0.25, within 1.5 times the radius: kept only by its key.
        let kept = resolve(&mut ws, &mut input, ground, DVec2::new(2.0, -0.25)).hovered;
        assert_eq!(kept.map(|h| h.key), Some(before.key));
        assert_eq!(ws.snap_index(ground).stats().full_builds, 1, "updated in place");
    }

    /// The average time of `count` resolves at seeded cursor positions in
    /// `lo`..`hi`, after the index is built.
    fn average_resolve(ws: &mut Workspace, storey: StoreyId, lo: DVec2, hi: DVec2, count: u32) -> f64 {
        let mut input = PointInput::default();
        let mut random = Random(5);
        resolve(ws, &mut input, storey, lo);
        let started = Instant::now();
        for _ in 0..count {
            let cursor = random.between(lo, hi);
            resolve(ws, &mut input, storey, cursor);
        }
        started.elapsed().as_secs_f64() / f64::from(count)
    }

    /// NFR-PERF-005 (loose bound; the frame-time assertion is separate): a
    /// resolve on two_storey_shell takes under 1 ms on average.
    #[test]
    fn resolving_on_two_storey_shell_takes_under_a_millisecond() {
        let mut ws = workspace(samples::two_storey_shell());
        let ground = ws.doc.project.storeys.ground();
        let average = average_resolve(&mut ws, ground, DVec2::splat(-1.0), DVec2::new(13.0, 10.0), 1_000);
        assert!(average < 1e-3, "{:.3} ms per resolve", average * 1e3);
    }

    /// NFR-PERF-005 (loose bound): a resolve on a storey of 5,000 walls
    /// takes under 5 ms on average.
    #[test]
    fn resolving_among_5000_walls_takes_under_5_ms() {
        let project = default_metric_project();
        let storey = project.storeys.ground();
        let walls = (0..5_000)
            .map(|n| {
                let at = DVec2::new(f64::from(n % 50), f64::from(n / 50)) * 2.0;
                project.new_wall(storey, at, at + DVec2::X, n)
            })
            .collect();
        let mut ws = workspace(project);
        assert!(ws.execute(Command::AddElements(walls)));
        let average = average_resolve(&mut ws, storey, DVec2::splat(-1.0), DVec2::new(101.0, 201.0), 200);
        assert!(average < 5e-3, "{:.3} ms per resolve", average * 1e3);
    }

    /// The index uses the rstar that geo brings, so only one copy is
    /// compiled (docs/technology-choices.md).
    #[test]
    fn one_rstar_version_is_compiled() {
        let versions = locked_versions(include_str!("../../../../Cargo.lock"), "rstar");
        assert_eq!(versions.len(), 1, "{versions:?}");
        assert!(versions[0].starts_with("\"0.12."), "{versions:?}");
    }

    /// The quoted versions of the package `name` in a Cargo.lock. Line based, so a
    /// checkout with CRLF line endings (Windows) reads the same as one with LF.
    fn locked_versions<'a>(lock: &'a str, name: &str) -> Vec<&'a str> {
        let name_line = format!("name = \"{name}\"");
        lock.split("[[package]]")
            .filter(|p| p.lines().any(|l| l == name_line))
            .filter_map(|p| p.lines().find_map(|l| l.strip_prefix("version = ")))
            .collect()
    }

    #[test]
    fn locked_versions_ignore_line_endings() {
        let lf = "[[package]]\nname = \"rstar\"\nversion = \"0.12.2\"\n\n[[package]]\nname = \"rstar-x\"\nversion = \"1.0.0\"\n";
        let crlf = lf.replace('\n', "\r\n");
        assert_eq!(locked_versions(lf, "rstar"), ["\"0.12.2\""]);
        assert_eq!(locked_versions(&crlf, "rstar"), ["\"0.12.2\""]);
    }
}
