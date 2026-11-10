//! The wall contact index (NFR-PERF-006, NFR-ACC-003): the walls whose
//! reference lines touch a point or a segment, found through one R-tree per
//! storey instead of a scan over every pair of walls.
//!
//! Reference lines touch if their points are closer than [`JOIN_TOLERANCE`]
//! (decision D-6). The tree holds each wall's reference line as its bounding
//! box inflated by that tolerance, so every point that touches the line lies
//! in the box; rstar's envelope tests include the boundary, so no touching
//! wall is lost there. The boxes only select candidates: the exact tests of
//! the queries and of junction resolution ([`crate::wall`]) decide, with
//! their strict comparisons. Walls on different storeys never join, so each
//! storey has its own tree.
//!
//! [`ModelGeometry::evaluate`](crate::ModelGeometry::evaluate) bulk-loads
//! the index once, resolves every wall's junctions through it and keeps it,
//! so that incremental evaluation can update it wall by wall with
//! [`ContactIndex::insert`] and [`ContactIndex::remove`].

use std::collections::HashMap;

use archicraft_model::tolerance::JOIN_TOLERANCE;
use archicraft_model::{ElementId, StoreyId};
use glam::DVec2;
use rstar::primitives::{GeomWithData, Rectangle};
use rstar::{AABB, RTree};

use crate::plan::segments_intersect;
use crate::wall::{Footprint, WallInput, resolve_footprint, touches};

/// A wall's reference line in the tree: its bounding box inflated by
/// [`JOIN_TOLERANCE`].
type Entry = GeomWithData<Rectangle<[f64; 2]>, ElementId>;

fn entry(wall: &WallInput) -> Entry {
    let margin = DVec2::splat(JOIN_TOLERANCE);
    let lower = wall.start.min(wall.end) - margin;
    let upper = wall.start.max(wall.end) + margin;
    GeomWithData::new(Rectangle::from_corners(lower.to_array(), upper.to_array()), wall.id)
}

/// The walls of a project by element id, with their reference lines in one
/// R-tree per storey.
#[derive(Clone, Debug, Default)]
pub struct ContactIndex {
    walls: HashMap<ElementId, WallInput>,
    trees: HashMap<StoreyId, RTree<Entry>>,
}

/// The result of a junction pass ([`ContactIndex::resolve`]).
#[derive(Clone, Debug, Default)]
pub struct Junctions {
    /// The junction-resolved footprint of every resolved wall.
    pub footprints: HashMap<ElementId, Footprint>,
    /// How often a wall end was tested against another wall: the
    /// candidates the index found for the end, without the wall itself.
    /// Linear in the walls, where a pair scan tests every other wall of the
    /// storey.
    pub candidate_tests: usize,
}

impl ContactIndex {
    /// Bulk-loads the index from `walls`. Element ids are unique; of two
    /// walls with the same id, the later one is kept.
    pub fn new(walls: impl IntoIterator<Item = WallInput>) -> Self {
        let walls: HashMap<ElementId, WallInput> = walls.into_iter().map(|w| (w.id, w)).collect();
        let mut entries: HashMap<StoreyId, Vec<Entry>> = HashMap::new();
        for wall in walls.values() {
            entries.entry(wall.storey).or_default().push(entry(wall));
        }
        let trees = entries
            .into_iter()
            .map(|(storey, entries)| (storey, RTree::bulk_load(entries)))
            .collect();
        Self { walls, trees }
    }

    /// The number of walls.
    pub fn len(&self) -> usize {
        self.walls.len()
    }

    pub fn is_empty(&self) -> bool {
        self.walls.is_empty()
    }

    /// The wall with element id `id`.
    pub fn wall(&self, id: ElementId) -> Option<&WallInput> {
        self.walls.get(&id)
    }

    /// Adds `wall`, or replaces the wall with its id and returns that one.
    pub fn insert(&mut self, wall: WallInput) -> Option<WallInput> {
        let old = self.remove(wall.id);
        self.trees.entry(wall.storey).or_default().insert(entry(&wall));
        self.walls.insert(wall.id, wall);
        old
    }

    /// Removes the wall with element id `id` and returns it.
    pub fn remove(&mut self, id: ElementId) -> Option<WallInput> {
        let wall = self.walls.remove(&id)?;
        if let Some(tree) = self.trees.get_mut(&wall.storey) {
            let removed = tree.remove(&entry(&wall));
            debug_assert!(removed.is_some(), "wall {id} was not in its storey's tree");
            if tree.size() == 0 {
                self.trees.remove(&wall.storey);
            }
        }
        Some(wall)
    }

    /// The walls on `storey` whose reference line touches `p`, in element-id
    /// order: an end of the line or the line itself is closer than
    /// [`JOIN_TOLERANCE`] (NFR-ACC-003). These are the walls a wall end at
    /// `p` can join; junction resolution also requires their vertical
    /// extents to overlap.
    pub fn touching_point(&self, storey: StoreyId, p: DVec2) -> Vec<ElementId> {
        self.candidates(storey, p)
            .into_iter()
            .filter(|other| touches(p, other.start, other.end))
            .map(|other| other.id)
            .collect()
    }

    /// The walls on `storey` whose reference line touches the segment from
    /// `a` to `b`, in element-id order: the two cross, or an end of one
    /// touches the other as in [`touching_point`](Self::touching_point). A
    /// wall with this very reference line is among them. They include every
    /// wall whose junctions a wall from `a` to `b` takes part in: the walls
    /// with an end on it and those its ends lie on.
    pub fn touching_segment(&self, storey: StoreyId, a: DVec2, b: DVec2) -> Vec<ElementId> {
        let Some(tree) = self.trees.get(&storey) else {
            return Vec::new();
        };
        let mut found: Vec<ElementId> = tree
            .locate_in_envelope_intersecting(&AABB::from_corners(a.min(b).to_array(), a.max(b).to_array()))
            .filter_map(|e| self.walls.get(&e.data))
            .filter(|other| {
                segments_intersect(a, b, other.start, other.end)
                    || touches(a, other.start, other.end)
                    || touches(b, other.start, other.end)
                    || touches(other.start, a, b)
                    || touches(other.end, a, b)
            })
            .map(|other| other.id)
            .collect();
        found.sort_unstable();
        found
    }

    /// Resolves the junctions of the walls `ids` (WAL-020): each end against
    /// the walls the index finds at it, in element-id order. Ids that are
    /// not in the index are skipped.
    pub fn resolve(&self, ids: impl IntoIterator<Item = ElementId>) -> Junctions {
        let mut junctions = Junctions::default();
        for id in ids {
            let Some(wall) = self.walls.get(&id) else {
                continue;
            };
            let footprint = resolve_footprint(
                wall,
                |p| self.candidates(wall.storey, p),
                &mut junctions.candidate_tests,
            );
            junctions.footprints.insert(id, footprint);
        }
        junctions
    }

    /// Resolves the junctions of every wall.
    pub fn resolve_all(&self) -> Junctions {
        self.resolve(self.walls.keys().copied())
    }

    /// The walls on `storey` whose inflated box contains `p`, in element-id
    /// order, so that the result does not depend on the shape of the tree.
    fn candidates(&self, storey: StoreyId, p: DVec2) -> Vec<&WallInput> {
        let Some(tree) = self.trees.get(&storey) else {
            return Vec::new();
        };
        let mut found: Vec<&WallInput> = tree
            .locate_in_envelope_intersecting(&AABB::from_point(p.to_array()))
            .filter_map(|e| self.walls.get(&e.data))
            .collect();
        found.sort_unstable_by_key(|w| w.id);
        found
    }
}

#[cfg(test)]
mod tests {
    use archicraft_model::ReferenceLine;

    use super::*;

    fn wall(storey: StoreyId, start: (f64, f64), end: (f64, f64)) -> WallInput {
        WallInput {
            id: ElementId::new(),
            storey,
            start: DVec2::new(start.0, start.1),
            end: DVec2::new(end.0, end.1),
            thickness: 0.3,
            reference_line: ReferenceLine::Center,
            bottom: 0.0,
            top: 3.0,
        }
    }

    fn sorted(mut ids: Vec<ElementId>) -> Vec<ElementId> {
        ids.sort_unstable();
        ids
    }

    /// NFR-ACC-003: a point touches the walls whose reference line, or an
    /// end of it, is closer than the model tolerance; 20 µm away, or on
    /// another storey, it touches none.
    #[test]
    fn a_point_finds_the_walls_whose_reference_line_it_touches() {
        let (storey, other_storey) = (StoreyId::new(), StoreyId::new());
        let through = wall(storey, (0.0, 0.0), (10.0, 0.0));
        let ending = wall(storey, (5.0, 4.0), (5.0, 0.000_005));
        let starting = wall(storey, (5.0, -0.000_005), (5.0, -4.0));
        let elsewhere = wall(other_storey, (5.0, 0.0), (5.0, 4.0));
        let index = ContactIndex::new([through.clone(), ending.clone(), starting.clone(), elsewhere.clone()]);

        let at_node = index.touching_point(storey, DVec2::new(5.0, 0.0));
        assert_eq!(at_node, sorted(vec![through.id, ending.id, starting.id]));
        assert_eq!(
            index.touching_point(storey, DVec2::new(2.0, 0.000_005)),
            vec![through.id]
        );
        assert!(index.touching_point(storey, DVec2::new(2.0, 0.000_020)).is_empty());
        assert!(index.touching_point(storey, DVec2::new(10.000_020, 0.0)).is_empty());
        assert_eq!(
            index.touching_point(other_storey, DVec2::new(5.0, 2.0)),
            vec![elsewhere.id]
        );
        assert!(index.touching_point(StoreyId::new(), DVec2::new(5.0, 0.0)).is_empty());
    }

    /// NFR-ACC-003: a segment touches the walls that cross it, whose end
    /// lies on it and on whose reference line its own ends lie, within the
    /// model tolerance, and no wall farther away.
    #[test]
    fn a_segment_finds_the_walls_it_touches_or_crosses() {
        let storey = StoreyId::new();
        let crossing = wall(storey, (3.0, -2.0), (3.0, 2.0));
        let branch = wall(storey, (6.0, 0.000_005), (6.0, 4.0));
        let carrier = wall(storey, (-1.0, -3.0), (-1.0, 3.0));
        let corner = wall(storey, (10.000_005, 0.0), (14.0, 3.0));
        // Not touching: 20 µm beside the segment, and a skew wall whose box
        // overlaps the segment's but whose line passes 0.45 m from its end.
        let parallel = wall(storey, (0.0, 0.000_020), (8.0, 0.000_020));
        let apart = wall(storey, (6.0, 0.000_020), (6.0, 4.0));
        let skew = wall(storey, (9.0, 1.0), (13.0, -1.0));
        let index = ContactIndex::new([
            crossing.clone(),
            branch.clone(),
            carrier.clone(),
            corner.clone(),
            parallel,
            apart,
            skew,
        ]);
        let found = index.touching_segment(storey, DVec2::new(-1.0, 0.0), DVec2::new(10.0, 0.0));
        assert_eq!(found, sorted(vec![crossing.id, branch.id, carrier.id, corner.id]));
    }

    /// A zero-length wall has a point as its box: it is found at its point,
    /// within the tolerance, and nowhere else.
    #[test]
    fn a_zero_length_wall_is_found_at_its_point() {
        let storey = StoreyId::new();
        let dot = wall(storey, (2.0, 3.0), (2.0, 3.0));
        let index = ContactIndex::new([dot.clone()]);
        assert_eq!(index.touching_point(storey, DVec2::new(2.0, 3.0)), vec![dot.id]);
        assert_eq!(index.touching_point(storey, DVec2::new(2.000_005, 3.0)), vec![dot.id]);
        assert!(index.touching_point(storey, DVec2::new(2.0, 3.000_020)).is_empty());
        let across = index.touching_segment(storey, DVec2::new(0.0, 3.000_005), DVec2::new(4.0, 3.000_005));
        assert_eq!(across, vec![dot.id]);
    }

    /// A ring with an interior wall and a T junction, the walls of the
    /// insert and remove tests.
    fn plan(storey: StoreyId) -> Vec<WallInput> {
        vec![
            wall(storey, (0.0, 0.0), (12.0, 0.0)),
            wall(storey, (12.0, 0.0), (12.0, 9.0)),
            wall(storey, (12.0, 9.0), (0.0, 9.0)),
            wall(storey, (0.0, 9.0), (0.0, 0.0)),
            wall(storey, (6.0, 0.0), (6.0, 9.0)),
            wall(storey, (6.0, 4.0), (12.0, 4.0)),
        ]
    }

    fn assert_same_footprints(actual: &Junctions, expected: &Junctions) {
        assert_eq!(actual.footprints.len(), expected.footprints.len());
        for (id, footprint) in &expected.footprints {
            assert_eq!(actual.footprints.get(id), Some(footprint), "{id}");
        }
        assert_eq!(actual.candidate_tests, expected.candidate_tests);
    }

    /// Insert and remove keep the index equal to one bulk-loaded from the
    /// same walls: the same queries and the same junctions. Replacing a wall
    /// moves it; a removed wall is found nowhere.
    #[test]
    fn insert_and_remove_keep_the_index_equal_to_a_bulk_loaded_one() {
        let (storey, upper) = (StoreyId::new(), StoreyId::new());
        let walls: Vec<WallInput> = plan(storey).into_iter().chain(plan(upper)).collect();
        let mut index = ContactIndex::default();
        for wall in walls.iter().rev() {
            assert!(index.insert(wall.clone()).is_none());
        }
        assert_eq!(index.len(), walls.len());
        assert_same_footprints(&index.resolve_all(), &ContactIndex::new(walls.clone()).resolve_all());

        // Move the interior cross wall off its T junction and back.
        let cross = walls[5].clone();
        let moved = WallInput {
            start: DVec2::new(6.5, 4.0),
            ..cross.clone()
        };
        assert_eq!(index.insert(moved.clone()).map(|w| w.start), Some(cross.start));
        assert_eq!(index.wall(cross.id).map(|w| w.start), Some(moved.start));
        assert!(!index.touching_point(storey, DVec2::new(9.0, 4.0)).is_empty());
        let interior = walls[4].id;
        assert_eq!(index.touching_point(storey, DVec2::new(6.0, 4.0)), vec![interior]);
        let mut expected: Vec<WallInput> = walls.clone();
        expected[5] = moved;
        assert_same_footprints(&index.resolve_all(), &ContactIndex::new(expected).resolve_all());
        index.insert(cross.clone());
        assert_same_footprints(&index.resolve_all(), &ContactIndex::new(walls.clone()).resolve_all());

        // Remove a ring wall: it is found nowhere, and the rest resolve as
        // without it.
        let ring = walls[0].clone();
        assert_eq!(index.remove(ring.id).map(|w| w.id), Some(ring.id));
        assert!(index.remove(ring.id).is_none());
        assert!(index.wall(ring.id).is_none());
        assert!(!index.touching_point(storey, DVec2::new(3.0, 0.0)).contains(&ring.id));
        let rest: Vec<WallInput> = walls.iter().filter(|w| w.id != ring.id).cloned().collect();
        assert_same_footprints(&index.resolve_all(), &ContactIndex::new(rest).resolve_all());

        // Emptying a storey leaves the other one as it was.
        for wall in &walls[1..6] {
            index.remove(wall.id);
        }
        assert!(index.touching_point(storey, DVec2::new(12.0, 0.0)).is_empty());
        assert_same_footprints(
            &index.resolve_all(),
            &ContactIndex::new(plan_ids(&walls[6..])).resolve_all(),
        );
    }

    fn plan_ids(walls: &[WallInput]) -> Vec<WallInput> {
        walls.to_vec()
    }

    /// Resolving some walls gives their entries of the full pass, so a pass
    /// over the walls an edit touches matches a full one.
    #[test]
    fn resolving_some_walls_gives_their_entries_of_the_full_pass() {
        let walls = plan(StoreyId::new());
        let index = ContactIndex::new(walls.clone());
        let all = index.resolve_all();
        let some = index.resolve([walls[4].id, walls[5].id, ElementId::new()]);
        assert_eq!(some.footprints.len(), 2);
        for id in [walls[4].id, walls[5].id] {
            assert_eq!(some.footprints[&id], all.footprints[&id]);
        }
        assert!(some.candidate_tests < all.candidate_tests);
    }

    /// NFR-PERF-006: an end is tested only against the other walls whose
    /// boxes contain it. In an L junction of two walls that is the other wall
    /// at the shared corner and nothing at the free ends: 2 tests, where a
    /// pair scan makes 4.
    #[test]
    fn the_candidate_tests_count_the_other_walls_at_each_end() {
        let storey = StoreyId::new();
        let index = ContactIndex::new([
            wall(storey, (0.0, 0.0), (5.0, 0.0)),
            wall(storey, (5.0, 0.0), (5.0, 4.0)),
        ]);
        assert_eq!(index.resolve_all().candidate_tests, 2);
        assert_eq!(ContactIndex::default().resolve_all().candidate_tests, 0);
    }
}
