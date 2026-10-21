//! The spatial index of snap targets (INP-008, INP-017, NFR-PERF-005).
//!
//! A [`SnapIndex`] holds the nodes and edges of one storey in two R-trees,
//! so that snapping looks only at the targets near the cursor instead of
//! scanning the whole storey. It follows the model's revisions
//! incrementally: every target belongs to an owner, and a new revision
//! removes and inserts only the owners whose targets changed, compared by a
//! fingerprint of their content. The first build, and a revision that
//! changes more than a quarter of the owners, bulk-load the trees instead.
//!
//! Targets come from [`SnapSource`]s: the plan's elements (cut rings,
//! projected and overhead lines) and the walls' reference lines. Elements
//! that are hidden or excluded from snapping, themselves or through their
//! layer, provide none; locked elements stay snappable while they reject
//! edits (INP-017). A whole source, such as a linked reference model, can
//! be switched off with [`SnapIndex::set_source_excluded`].

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use archicraft_geometry::plan::{PlanDrawing, PlanElement, polygon_rings};
use archicraft_model::glam::DVec2;
use archicraft_model::tolerance::Welder;
use archicraft_model::{Element, ElementId, MODEL_TOLERANCE, Project, StoreyId};
use rstar::{AABB, PointDistance, RTree, RTreeObject};

use super::snap::closest_on_segment;

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

/// A node in the index.
#[derive(Clone, Copy, Debug, PartialEq)]
struct IndexedPoint {
    point: DVec2,
    key: TargetKey,
}

/// An edge in the index.
#[derive(Clone, Copy, Debug, PartialEq)]
struct IndexedEdge {
    edge: [DVec2; 2],
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

/// Collects the targets of one source, in order.
#[derive(Debug)]
pub struct TargetSink {
    owner: TargetOwner,
    source: u8,
    points: Vec<IndexedPoint>,
    edges: Vec<IndexedEdge>,
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

    pub fn node(&mut self, class: SourceClass, point: DVec2) {
        let key = self.key(self.points.len(), class);
        self.points.push(IndexedPoint { point, key });
    }

    pub fn edge(&mut self, class: SourceClass, edge: [DVec2; 2]) {
        let key = self.key(self.edges.len(), class);
        self.edges.push(IndexedEdge { edge, key });
    }
}

/// Something that provides snap targets (INP-008).
pub trait SnapSource {
    fn owner(&self) -> TargetOwner;

    /// Adds this source's nodes and edges to `sink`. Of coincident targets
    /// the one added first is taken (NFR-ACC-003).
    fn targets(&self, sink: &mut TargetSink);
}

/// A wall's reference line: its points are stored in the model, so they
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
    }
}

/// An element's plan: the corners and sides of its cut rings, then its
/// projected and overhead lines.
impl SnapSource for PlanElement {
    fn owner(&self) -> TargetOwner {
        TargetOwner::Element(self.id)
    }

    fn targets(&self, sink: &mut TargetSink) {
        for polygon in &self.cut {
            for ring in polygon_rings(polygon) {
                for i in 0..ring.len() {
                    sink.node(SourceClass::Cut, ring[i]);
                    sink.edge(SourceClass::Cut, [ring[i], ring[(i + 1) % ring.len()]]);
                }
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
    fingerprint: u64,
}

impl OwnerTargets {
    /// A hash over the bits of every target and its key, so that equal
    /// content gives equal fingerprints whatever allocation it came from.
    fn fingerprint(&self) -> u64 {
        let mut h = DefaultHasher::new();
        (self.sources, self.points.len()).hash(&mut h);
        let points = self.points.iter().map(|p| (p.key, [p.point, p.point]));
        for (key, ends) in points.chain(self.edges.iter().map(|e| (e.key, e.edge))) {
            (key.source, key.index, key.class).hash(&mut h);
            for p in ends {
                (p.x.to_bits(), p.y.to_bits()).hash(&mut h);
            }
        }
        h.finish()
    }
}

/// Inserts the targets of one owner into the trees.
fn insert_targets(points: &mut RTree<IndexedPoint>, edges: &mut RTree<IndexedEdge>, targets: &OwnerTargets) {
    for p in &targets.points {
        points.insert(*p);
    }
    for e in &targets.edges {
        edges.insert(*e);
    }
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
    points: RTree<IndexedPoint>,
    edges: RTree<IndexedEdge>,
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
            };
            source.targets(&mut sink);
            order.insert((owner, targets.sources), position as u32);
            targets.sources += 1;
            targets.points.append(&mut sink.points);
            targets.edges.append(&mut sink.edges);
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
            let included = || owners.iter().filter(|(owner, _)| !self.excluded.contains(owner));
            self.points = RTree::bulk_load(included().flat_map(|(_, t)| t.points.iter().copied()).collect());
            self.edges = RTree::bulk_load(included().flat_map(|(_, t)| t.edges.iter().copied()).collect());
            self.stats.full_builds += 1;
        } else {
            for owner in gone.iter().chain(&changed) {
                self.remove_owner(*owner);
            }
            for owner in &changed {
                if !self.excluded.contains(owner) {
                    insert_targets(&mut self.points, &mut self.edges, &owners[owner]);
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
            insert_targets(&mut self.points, &mut self.edges, targets);
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
            for p in &old.points {
                self.points.remove(p);
            }
            for e in &old.edges {
                self.edges.remove(e);
            }
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
        let reach = radius + 2.0 * MODEL_TOLERANCE;
        let mut found: Vec<&IndexedPoint> = self
            .points
            .locate_within_distance(cursor.to_array(), reach * reach)
            .collect();
        found.sort_by_key(|p| self.rank(&p.key));
        let mut nodes = Welder::new();
        for p in found {
            nodes.weld(p.point);
        }
        nodes.into_points()
    }

    /// The edges whose closest point lies within `radius` of `cursor`, in
    /// source order.
    pub fn edges_near(&self, cursor: DVec2, radius: f64) -> Vec<[DVec2; 2]> {
        let reach = radius + MODEL_TOLERANCE;
        let mut found: Vec<&IndexedEdge> = self
            .edges
            .locate_within_distance(cursor.to_array(), reach * reach)
            .filter(|e| closest_on_segment(cursor, e.edge).distance(cursor) <= radius)
            .collect();
        found.sort_by_key(|e| self.rank(&e.key));
        found.into_iter().map(|e| e.edge).collect()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::time::Instant;

    use archicraft_model::template::default_metric_project;
    use archicraft_model::{Command, Layer, samples};

    use super::*;
    use crate::document::Document;
    use crate::input::snap::{collect, segment_intersection};
    use crate::input::{
        Expect, InputContext, Modifiers, PointInput, Resolved, Snap, SnapKind, SurfaceFrame, candidates, snap,
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
        nodes: Vec<DVec2>,
        edges: Vec<[DVec2; 2]>,
    }

    impl BruteForce {
        fn from_plan(plan: &PlanDrawing, project: &Project, storey: StoreyId) -> Self {
            let mut nodes = Welder::new();
            let mut edges = Vec::new();
            for element in project.elements_on(storey).filter(|e| project.is_visible(e)) {
                if let Some(wall) = element.as_wall() {
                    nodes.weld(wall.start);
                    nodes.weld(wall.end);
                    edges.push([wall.start, wall.end]);
                }
            }
            for element in &plan.elements {
                for polygon in &element.cut {
                    for ring in polygon_rings(polygon) {
                        for i in 0..ring.len() {
                            nodes.weld(ring[i]);
                            edges.push([ring[i], ring[(i + 1) % ring.len()]]);
                        }
                    }
                }
                for segment in element.projected.iter().chain(&element.overhead) {
                    for &p in segment {
                        nodes.weld(p);
                    }
                    edges.push(*segment);
                }
            }
            Self {
                nodes: nodes.into_points(),
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
            nodes.extend_from_slice(pending);
            let near: Vec<[DVec2; 2]> = (self.edges.iter().copied())
                .filter(|e| closest_on_segment(cursor, *e).distance(cursor) <= radius)
                .collect();
            collect(&nodes, &near, &[], cursor, radius, grid, previous)
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
            expects: Expect::Point,
            radius: 0.2,
            drag_threshold: 0.06,
            units: &ws.doc.project.settings.units,
            number_format: &ws.prefs.number_format(),
            angle_increment_deg: 45.0,
            snap_step: 0.0,
            frame: SurfaceFrame::plan(0.0),
            user_origin: DVec2::ZERO,
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
                .filter(|p| brute.nodes.iter().all(|n| n.distance(*p) > 0.05))
                .collect();
            for i in 0..500 {
                // Half anywhere, half near a node, a crossing of edges or an
                // edge's midpoint, where targets compete.
                let at = match i % 6 {
                    1 => *random.pick(&brute.nodes),
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
