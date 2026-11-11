//! Incremental evaluation (NFR-PERF-006, VIEW-001, NFR-ACC-003): after some
//! edits, `ModelGeometry::evaluate_incremental` evaluates only what the edits
//! dirtied, keeps every other element as it was, and gives what a full
//! evaluation gives, bit for bit.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use archicraft_geometry::evaluate::wall_inputs;
use archicraft_geometry::wall::WallInput;
use archicraft_geometry::{ModelGeometry, dirty_set};
use archicraft_model::command::{StoreyElements, relink_to_storey};
use archicraft_model::glam::DVec2;
use archicraft_model::kinds::{Anchor, ColumnCut, CrossSection, Dummy};
use archicraft_model::samples::{self, FixtureBuilder};
use archicraft_model::{
    Command, Element, ElementId, History, JOIN_TOLERANCE, Position, Project, ReferenceLine, Relation, Storey, StoreyId,
    StoreyList, TopLink,
};

/// Executes `command`, which the test expects to be accepted.
fn execute(project: &mut Project, history: &mut History, command: Command) {
    if let Err(e) = history.execute(project, command) {
        panic!("the command was refused: {e}");
    }
}

/// Fails unless `actual` holds the evaluated elements and the problems of
/// `expected`, bit for bit: per element the same body parts (`TriMesh ==`),
/// children and axes.
fn assert_same_geometry(actual: &ModelGeometry, expected: &ModelGeometry, context: &str) {
    assert_eq!(actual.revision, expected.revision, "{context}: revision");
    assert_eq!(actual.problems, expected.problems, "{context}: problems");
    let ids = |g: &ModelGeometry| g.elements.keys().copied().collect::<BTreeSet<ElementId>>();
    assert_eq!(ids(actual), ids(expected), "{context}: the evaluated elements");
    for (id, geometry) in &expected.elements {
        assert!(
            actual.elements[id].as_ref() == geometry.as_ref(),
            "{context}: the geometry of {id} differs from a full evaluation"
        );
    }
}

/// The home storey of every element.
fn homes(project: &Project) -> HashMap<ElementId, StoreyId> {
    project.elements.values().map(|e| (e.id, e.storey)).collect()
}

/// True if `p` touches the segment from `a` to `b`: it is closer than the
/// model tolerance to its start, its end or the segment (NFR-ACC-003), the
/// test junction resolution makes.
fn near(p: DVec2, a: DVec2, b: DVec2) -> bool {
    let ab = b - a;
    let length2 = ab.length_squared();
    let t = if length2 > 0.0 {
        ((p - a).dot(ab) / length2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    a.distance(p) < JOIN_TOLERANCE || b.distance(p) < JOIN_TOLERANCE || p.distance(a + ab * t) < JOIN_TOLERANCE
}

/// True if the segments `a b` and `c d` cross: the ends of each lie on
/// different sides of the other's line, an end on the line counting as right
/// of it.
fn cross(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> bool {
    let side = |p: DVec2, q: DVec2, r: DVec2| (q - p).perp_dot(r - p) > 0.0;
    side(c, d, a) != side(c, d, b) && side(a, b, c) != side(a, b, d)
}

/// The walls on `storey` whose reference lines touch the segment from `a` to
/// `b`, by a scan over every wall: the two cross, or an end of one touches
/// the other. Vertical overlap does not matter.
fn touching_by_scan(walls: &[WallInput], storey: StoreyId, a: DVec2, b: DVec2) -> BTreeSet<ElementId> {
    walls
        .iter()
        .filter(|w| w.storey == storey)
        .filter(|w| {
            cross(a, b, w.start, w.end)
                || near(a, w.start, w.end)
                || near(b, w.start, w.end)
                || near(w.start, a, b)
                || near(w.end, a, b)
        })
        .map(|w| w.id)
        .collect()
}

/// The reference line of the wall `id`.
fn line(project: &Project, id: ElementId) -> (DVec2, DVec2) {
    let wall = project.elements[&id].as_wall().expect("a wall");
    (wall.start, wall.end)
}

/// The interior wall on the middle column line of a stand-in storey: it runs
/// from ring to ring, and cross walls on both sides end on it in T
/// junctions.
fn middle_column_wall(project: &Project, storey: StoreyId) -> ElementId {
    let mut columns: Vec<(f64, ElementId)> = project
        .elements_on(storey)
        .filter(|e| e.position == Position::Interior)
        .filter_map(|e| {
            let wall = e.as_wall()?;
            let vertical = (wall.start.x - wall.end.x).abs() < 1e-9;
            (vertical && wall.start.distance(wall.end) > 4.0).then_some((wall.start.x, e.id))
        })
        .collect();
    columns.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert!(columns.len() >= 3, "the stand-in has column walls");
    columns[columns.len() / 2].1
}

/// NFR-PERF-006, NFR-ACC-003: moving one interior wall of the RM-M stand-in
/// dirties exactly the wall and the walls touching its old or its new
/// reference line, as a scan over every wall finds them; nothing on another
/// storey and no relation adds to them.
#[test]
fn dirty_set_after_one_edit_is_local_for_a_moved_wall() {
    let mut project = samples::stand_in(5_000);
    let mut history = History::default();
    let storey = project.storeys.as_slice()[2].id;
    let wall = middle_column_wall(&project, storey);
    let (old_start, old_end) = line(&project, wall);
    let geometry = ModelGeometry::evaluate(&project);

    // Half a room east: the line now crosses the cross walls of the column
    // east of it, whose ends it carried before.
    let delta = DVec2::new(2.0, 0.0);
    execute(
        &mut project,
        &mut history,
        Command::MoveElements { ids: vec![wall], delta },
    );
    let walls = wall_inputs(&project);
    let mut expected = BTreeSet::from([wall]);
    expected.extend(touching_by_scan(&walls, storey, old_start, old_end));
    expected.extend(touching_by_scan(&walls, storey, old_start + delta, old_end + delta));

    let dirty = dirty_set(&project, &geometry);
    assert!(!dirty.everything);
    assert_eq!(dirty.elements, expected);
    assert_eq!(
        dirty.walls,
        BTreeSet::from([wall]),
        "only the moved wall's junction input changed"
    );
    assert!(dirty.storeys.is_empty());
    assert!(
        expected.len() > 10,
        "{} walls touch the old or the new line",
        expected.len()
    );
    assert!(expected.len() < 100, "{} dirty elements of 5,000", expected.len());

    let updated = ModelGeometry::evaluate_incremental(&project, &geometry);
    assert_same_geometry(&updated, &ModelGeometry::evaluate(&project), "after the move");
    assert_eq!(updated.changed, expected);
}

/// The elements a storey edit from the list `old` to the project's moves,
/// by a scan over every element: those homed on a moved storey, those whose
/// top is linked to one by identity, and those whose vertical extent the new
/// list changes, as a relative top link carries them across moved storeys.
fn moved_by_scan(project: &Project, old: &StoreyList, moved: &BTreeSet<StoreyId>) -> BTreeSet<ElementId> {
    project
        .elements
        .values()
        .filter(|e| {
            moved.contains(&e.storey)
                || e.kind.storey_links().iter().any(|link| moved.contains(&link.storey))
                || e.kind.vertical_extent(e.storey, old) != project.vertical_extent(e)
        })
        .map(|e| e.id)
        .collect()
}

/// STR-006, NFR-PERF-006: a storey elevation change dirties exactly the
/// elements homed on or linked to the storeys the change log stamps, also
/// a wall whose top is linked two storeys up from a storey that stays
/// (the interim for archicraft-lppht), and none of the other elements.
#[test]
fn dirty_set_after_one_edit_is_local_for_a_storey_elevation_change() {
    let mut project = samples::stand_in(5_000);
    let mut history = History::default();
    let storeys: Vec<StoreyId> = project.storeys.iter().map(|s| s.id).collect();
    // Three free-standing walls on the lowest storey, north of the plan:
    // their tops reach the storey two up, the fourth storey by identity
    // and the storey above.
    let north = wall_inputs(&project)
        .iter()
        .map(|w| w.start.y.max(w.end.y))
        .fold(0.0, f64::max)
        + 10.0;
    let tops = [
        TopLink::Relative {
            storeys: 2,
            offset: 0.0,
        },
        TopLink::Storey {
            storey: storeys[3],
            offset: 0.0,
        },
        TopLink::Relative {
            storeys: 1,
            offset: 0.0,
        },
    ];
    let extra: Vec<Element> = tops
        .iter()
        .enumerate()
        .map(|(i, top)| {
            let x = 4.0 * i as f64;
            let mut wall = project.new_wall(storeys[0], DVec2::new(x, north), DVec2::new(x + 2.0, north), 0);
            if let Some(w) = wall.as_wall_mut() {
                w.top = *top;
            }
            wall
        })
        .collect();
    let [two_up, by_identity, one_up] = [extra[0].id, extra[1].id, extra[2].id];
    execute(&mut project, &mut history, Command::AddElements(extra));

    let mut geometry = ModelGeometry::evaluate(&project);
    let edits = [
        // The second storey's height and the third's elevation change.
        (storeys[2], 0.25, false),
        // The third storey and those above move up together.
        (storeys[2], 0.5, true),
    ];
    for (storey, raise, shift_above) in edits {
        let old = project.storeys.clone();
        let elevation = old.get(storey).expect("the storey").elevation + raise;
        execute(
            &mut project,
            &mut history,
            Command::SetStoreyElevation {
                storey,
                elevation,
                shift_above,
            },
        );
        let moved: BTreeSet<StoreyId> = project.changes.storeys_since(geometry.revision).collect();
        assert!(!moved.contains(&storeys[0]), "the lowest storey stays");
        let expected = moved_by_scan(&project, &old, &moved);
        let context = format!("raise by {raise}, shift above {shift_above}");

        let dirty = dirty_set(&project, &geometry);
        assert_eq!(dirty.elements, expected, "{context}");
        assert_eq!(dirty.storeys, moved, "{context}");
        assert!(
            dirty.elements.contains(&two_up),
            "{context}: a top two storeys up moves"
        );
        assert_eq!(dirty.elements.contains(&by_identity), shift_above, "{context}");
        assert!(!dirty.elements.contains(&one_up), "{context}: the storey above stays");
        let lowest = project
            .elements_on(storeys[0])
            .filter(|e| dirty.elements.contains(&e.id));
        assert_eq!(lowest.count(), if shift_above { 2 } else { 1 }, "{context}");

        let updated = ModelGeometry::evaluate_incremental(&project, &geometry);
        assert_same_geometry(&updated, &ModelGeometry::evaluate(&project), &context);
        geometry = updated;
    }
}

/// NFR-PERF-006: deleting a wall dirties the wall and its former neighbours,
/// the walls whose reference lines touched it, and their junctions become
/// those of a full evaluation.
#[test]
fn dirty_set_after_one_edit_is_local_for_a_deleted_wall() {
    let mut project = samples::stand_in(5_000);
    let mut history = History::default();
    let storey = project.storeys.as_slice()[3].id;
    let wall = middle_column_wall(&project, storey);
    let (start, end) = line(&project, wall);
    let neighbours = touching_by_scan(&wall_inputs(&project), storey, start, end);
    assert!(neighbours.contains(&wall) && neighbours.len() > 10, "{neighbours:?}");
    let geometry = ModelGeometry::evaluate(&project);

    execute(&mut project, &mut history, Command::DeleteElements(vec![wall]));
    let dirty = dirty_set(&project, &geometry);
    assert_eq!(dirty.elements, neighbours);
    assert_eq!(dirty.walls, BTreeSet::from([wall]));

    let updated = ModelGeometry::evaluate_incremental(&project, &geometry);
    assert!(updated.get(wall).is_none());
    assert_same_geometry(&updated, &ModelGeometry::evaluate(&project), "after the delete");
    assert_eq!(
        updated.changed, neighbours,
        "the deleted wall is dropped, its neighbours evaluated"
    );
}

/// NFR-PERF-006 'openings of affected neighbouring elements', SRS 3.3: a test
/// kind element hosted in a wall through `Relation::Host` is dirty when the
/// wall moves, and editing it dirties the wall, the path the openings of
/// host-openings take. The wall is dirty through the relation only, so its
/// neighbours are not.
#[test]
fn a_host_and_its_hosted_element_dirty_each_other() {
    let mut project = samples::stand_in(1_000);
    let mut history = History::default();
    let storey = project.storeys.as_slice()[1].id;
    let wall = middle_column_wall(&project, storey);
    let (start, end) = line(&project, wall);
    let middle = start.lerp(end, 0.5);
    let material = project.materials[0].id;
    let hosted = Dummy::new(middle - DVec2::splat(0.05), DVec2::splat(0.1), material).into_element(&project, storey, 0);
    let hosted_id = hosted.id;
    execute(&mut project, &mut history, Command::AddElements(vec![hosted]));
    execute(
        &mut project,
        &mut history,
        Command::InsertRelation(Relation::Host {
            hosted: hosted_id,
            host: wall,
        }),
    );
    let geometry = ModelGeometry::evaluate(&project);

    execute(
        &mut project,
        &mut history,
        Command::MoveElements {
            ids: vec![wall],
            delta: DVec2::new(0.5, 0.0),
        },
    );
    let dirty = dirty_set(&project, &geometry);
    assert!(
        dirty.elements.contains(&hosted_id),
        "moving the host dirties the hosted element"
    );
    let geometry = ModelGeometry::evaluate_incremental(&project, &geometry);
    assert_same_geometry(&geometry, &ModelGeometry::evaluate(&project), "after moving the host");
    assert!(geometry.changed.contains(&hosted_id));

    execute(
        &mut project,
        &mut history,
        Command::MoveElements {
            ids: vec![hosted_id],
            delta: DVec2::new(0.0, 0.25),
        },
    );
    let dirty = dirty_set(&project, &geometry);
    assert_eq!(
        dirty.elements,
        BTreeSet::from([hosted_id, wall]),
        "editing the hosted element dirties its host"
    );
    assert!(dirty.walls.is_empty(), "no junction input changed");
    let geometry = ModelGeometry::evaluate_incremental(&project, &geometry);
    assert_same_geometry(
        &geometry,
        &ModelGeometry::evaluate(&project),
        "after moving the hosted element",
    );
}

/// NFR-PERF-006 'recomputing junctions': the walls joined to a wall whose
/// thickness changes get new footprints, the same as a full evaluation's:
/// the cross walls ending on it stop at its new faces.
#[test]
fn the_walls_joined_to_an_edited_wall_get_the_footprints_of_a_full_evaluation() {
    let mut project = samples::stand_in(1_000);
    let mut history = History::default();
    let storey = project.storeys.as_slice()[1].id;
    let wall = middle_column_wall(&project, storey);
    let (start, end) = line(&project, wall);
    let joined: Vec<ElementId> = wall_inputs(&project)
        .iter()
        .filter(|w| w.id != wall && w.storey == storey && (near(w.start, start, end) || near(w.end, start, end)))
        .map(|w| w.id)
        .collect();
    assert!(joined.len() > 4, "{} walls end on the wall", joined.len());
    let before = ModelGeometry::evaluate(&project);

    let mut thicker = project.elements[&wall].clone();
    if let Some(w) = thicker.as_wall_mut() {
        w.thickness = 0.3;
    }
    execute(&mut project, &mut history, Command::UpdateElements(vec![thicker]));
    let after = ModelGeometry::evaluate_incremental(&project, &before);
    let full = ModelGeometry::evaluate(&project);
    assert_same_geometry(&after, &full, "after the thickness change");
    for id in joined {
        assert!(after.changed.contains(&id), "{id} was evaluated again");
        assert_ne!(after.get(id), before.get(id), "{id}: its T junction moved");
        assert_eq!(after.get(id), full.get(id), "{id}");
    }
}

/// NFR-PERF-006: an edit that keeps a wall's junction input, such as a new
/// name, a layer or a material, dirties that wall only.
#[test]
fn an_edit_that_keeps_a_walls_junction_input_dirties_only_the_wall() {
    let mut project = samples::stand_in(1_000);
    let mut history = History::default();
    let wall = middle_column_wall(&project, project.storeys.as_slice()[0].id);
    let geometry = ModelGeometry::evaluate(&project);
    let mut renamed = project.elements[&wall].clone();
    renamed.name = "Renamed".to_owned();
    execute(&mut project, &mut history, Command::UpdateElements(vec![renamed]));
    let dirty = dirty_set(&project, &geometry);
    assert_eq!(dirty.elements, BTreeSet::from([wall]));
    assert!(dirty.walls.is_empty());
    let updated = ModelGeometry::evaluate_incremental(&project, &geometry);
    assert_same_geometry(&updated, &ModelGeometry::evaluate(&project), "after the rename");
}

/// NFR-PERF-006: an incremental step keeps the `Arc` of every element it
/// does not evaluate again, so that consumers see by pointer what is
/// unchanged; `changed` lists exactly the elements it evaluated again or
/// dropped, and `changed_storeys` the moved storeys and the old and new
/// home storeys of those.
#[test]
fn unchanged_elements_keep_their_arc_and_changed_lists_the_recomputed_ones() {
    let mut project = samples::stand_in(1_000);
    let mut history = History::default();
    let storeys: Vec<StoreyId> = project.storeys.iter().map(|s| s.id).collect();
    let moved_wall = middle_column_wall(&project, storeys[1]);
    let relinked = middle_column_wall(&project, storeys[3]);
    let deleted = middle_column_wall(&project, storeys[4]);
    let mut geometry = ModelGeometry::evaluate(&project);

    let relinked_to = relink_to_storey(&project, &project.elements[&relinked], storeys[2]).expect("relinkable");
    let edits = [
        Command::MoveElements {
            ids: vec![moved_wall],
            delta: DVec2::new(1.0, 0.0),
        },
        Command::UpdateElements(vec![relinked_to]),
        Command::DeleteElements(vec![deleted]),
        Command::SetStoreyHeight {
            storey: storeys[3],
            height: 3.5,
            shift_above: true,
        },
    ];
    for command in edits {
        let context = format!("{:?}", command.label());
        let homes_before = homes(&project);
        execute(&mut project, &mut history, command);
        let dirty = dirty_set(&project, &geometry);
        let updated = ModelGeometry::evaluate_incremental(&project, &geometry);
        assert_same_geometry(&updated, &ModelGeometry::evaluate(&project), &context);

        let mut recomputed = BTreeSet::new();
        for (id, element) in &updated.elements {
            let kept = geometry.elements.get(id).is_some_and(|old| Arc::ptr_eq(old, element));
            assert_eq!(kept, !dirty.elements.contains(id), "{context}: {id}");
            if !kept {
                recomputed.insert(*id);
            }
        }
        recomputed.extend(updated.problems.keys().filter(|id| dirty.elements.contains(id)));
        let dropped = homes_before.keys().filter(|id| !project.elements.contains_key(*id));
        recomputed.extend(dropped);
        assert_eq!(updated.changed, recomputed, "{context}");
        assert_eq!(updated.changed, dirty.elements, "{context}");

        let homes_after = homes(&project);
        let mut storeys_expected = dirty.storeys.clone();
        for id in &updated.changed {
            storeys_expected.extend(homes_before.get(id));
            storeys_expected.extend(homes_after.get(id));
        }
        assert_eq!(updated.changed_storeys, storeys_expected, "{context}");
        geometry = updated;
    }
    assert!(geometry.changed_storeys.len() > 1);
}

/// NFR-PERF-006: undoing an edit dirties what the edit dirtied, and redoing
/// it again, as the old and the new state enter the dirty set alike.
#[test]
fn undo_dirties_what_the_edit_dirtied() {
    let base = samples::stand_in(1_000);
    let storeys: Vec<StoreyId> = base.storeys.iter().map(|s| s.id).collect();
    let wall = middle_column_wall(&base, storeys[2]);
    let mut thicker = base.elements[&wall].clone();
    if let Some(w) = thicker.as_wall_mut() {
        w.thickness = 0.4;
    }
    let relinked = relink_to_storey(&base, &base.elements[&wall], storeys[1]).expect("relinkable");
    let (start, _) = line(&base, wall);
    let added = base.new_wall(storeys[2], start, start + DVec2::new(2.0, 0.0), 0);
    let edits = [
        Command::MoveElements {
            ids: vec![wall],
            delta: DVec2::new(0.0, 1.0),
        },
        Command::DeleteElements(vec![wall]),
        Command::UpdateElements(vec![thicker]),
        Command::UpdateElements(vec![relinked]),
        Command::AddElements(vec![added]),
        Command::SetStoreyElevation {
            storey: storeys[2],
            elevation: 6.5,
            shift_above: false,
        },
    ];
    for command in edits {
        let context = format!("{:?}", command.label());
        let mut project = base.clone();
        let mut history = History::default();
        let geometry = ModelGeometry::evaluate(&project);
        execute(&mut project, &mut history, command);
        let edit = dirty_set(&project, &geometry);
        assert!(!edit.elements.is_empty(), "{context}");
        let geometry = ModelGeometry::evaluate_incremental(&project, &geometry);
        assert!(history.undo(&mut project).is_some());
        assert_eq!(
            dirty_set(&project, &geometry).elements,
            edit.elements,
            "{context}: undo"
        );
        let geometry = ModelGeometry::evaluate_incremental(&project, &geometry);
        assert_same_geometry(&geometry, &ModelGeometry::evaluate(&project), &context);
        assert!(history.redo(&mut project).is_some());
        assert_eq!(
            dirty_set(&project, &geometry).elements,
            edit.elements,
            "{context}: redo"
        );
    }
}

/// NFR-PERF-006: a geometry that cannot be updated is evaluated again in
/// full, with every element and storey changed: one never evaluated, one
/// of another project, one newer than the project, and one before a change
/// of the settings, which everything depends on. An unchanged project keeps
/// everything.
#[test]
fn a_full_evaluation_runs_when_the_previous_geometry_cannot_be_updated() {
    let mut project = samples::stand_in(1_000);
    let mut history = History::default();
    let all: BTreeSet<ElementId> = project.elements.keys().copied().collect();
    let all_storeys: BTreeSet<StoreyId> = project.storeys.iter().map(|s| s.id).collect();

    let never = ModelGeometry::default();
    assert!(dirty_set(&project, &never).everything);
    let first = ModelGeometry::evaluate_incremental(&project, &never);
    assert_eq!(first.changed, all);
    assert_eq!(first.changed_storeys, all_storeys);

    let other_project = samples::stand_in(100);
    let other = ModelGeometry::evaluate(&other_project);
    let dirty = dirty_set(&project, &other);
    assert!(dirty.everything);
    let updated = ModelGeometry::evaluate_incremental(&project, &other);
    assert_same_geometry(&updated, &first, "from another project");
    let dropped: BTreeSet<ElementId> = other_project.elements.keys().copied().collect();
    assert_eq!(updated.changed, all.union(&dropped).copied().collect());

    let mut ahead = project.clone();
    let wall = middle_column_wall(&ahead, ahead.storeys.as_slice()[0].id);
    execute(&mut ahead, &mut History::default(), Command::DeleteElements(vec![wall]));
    let newer = ModelGeometry::evaluate(&ahead);
    assert!(dirty_set(&project, &newer).everything);
    assert_same_geometry(
        &ModelGeometry::evaluate_incremental(&project, &newer),
        &first,
        "from a newer revision",
    );

    let unchanged = ModelGeometry::evaluate_incremental(&project, &first);
    let dirty = dirty_set(&project, &first);
    assert!(!dirty.everything && dirty.elements.is_empty() && dirty.storeys.is_empty());
    assert!(unchanged.changed.is_empty() && unchanged.changed_storeys.is_empty());
    for (id, element) in &unchanged.elements {
        assert!(Arc::ptr_eq(element, &first.elements[id]));
    }

    let mut settings = project.settings.clone();
    settings.tessellation.chord_deviation *= 2.0;
    execute(&mut project, &mut history, Command::SetSettings(settings));
    assert!(dirty_set(&project, &first).everything);
    let updated = ModelGeometry::evaluate_incremental(&project, &first);
    assert_same_geometry(&updated, &ModelGeometry::evaluate(&project), "after a settings change");
    assert_eq!(updated.changed, all);
    assert_eq!(updated.changed_storeys, all_storeys);
}

/// A xorshift64 generator. Every sequence of the property test has its own
/// seed, which each of its assertions prints, so that a failing sequence can
/// be replayed.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // splitmix64 spreads small seeds; xorshift needs a state other than 0.
        let mut z = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        Self((z ^ (z >> 31)).max(1))
    }

    fn bits(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// A number below `n`, which must not be 0.
    fn below(&mut self, n: usize) -> usize {
        (self.bits() % n as u64) as usize
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())]
    }

    fn coin(&mut self) -> bool {
        self.bits() & 1 == 1
    }
}

/// The edits of the property test.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Op {
    Move,
    Thickness,
    ReferenceLine,
    Delete,
    Add,
    StoreyElevation,
    StoreyHeight,
    RelinkTop,
    RelinkHome,
    Undo,
    Redo,
    InsertStorey,
    DeleteStorey,
    Host,
    Failing,
    Settings,
    AddColumn,
    ColumnEdit,
}

/// The edits with their weights, out of 100.
const OPS: [(Op, usize); 18] = [
    (Op::Move, 14),
    (Op::Thickness, 6),
    (Op::ReferenceLine, 6),
    (Op::Delete, 6),
    (Op::Add, 10),
    (Op::StoreyElevation, 6),
    (Op::StoreyHeight, 4),
    (Op::RelinkTop, 8),
    (Op::RelinkHome, 4),
    (Op::Undo, 10),
    (Op::Redo, 6),
    (Op::InsertStorey, 2),
    (Op::DeleteStorey, 2),
    (Op::Host, 4),
    (Op::Failing, 1),
    (Op::Settings, 1),
    (Op::AddColumn, 4),
    (Op::ColumnEdit, 6),
];

/// One edit of the property test: what it was, whether the project took
/// it, and a description for the failure messages.
struct Edit {
    op: Op,
    accepted: bool,
    description: String,
}

/// A project the property test edits at random, through commands like a
/// user.
struct Session {
    project: Project,
    history: History,
    rng: Rng,
    /// The identities of the elements and storeys the edits add, which
    /// follow from the seed.
    ids: FixtureBuilder,
}

impl Session {
    fn new(base: &Project, seed: u64) -> Self {
        Self {
            project: base.clone(),
            history: History::default(),
            rng: Rng::new(seed),
            ids: FixtureBuilder::new(&format!("incremental-{seed}")),
        }
    }

    fn op(&mut self) -> Op {
        let total: usize = OPS.iter().map(|(_, weight)| weight).sum();
        let mut n = self.rng.below(total);
        for (op, weight) in OPS {
            if n < weight {
                return op;
            }
            n -= weight;
        }
        unreachable!("the weights add up to the total")
    }

    /// A random element, a wall four times out of five.
    fn element(&mut self) -> Option<Element> {
        let any = self.rng.below(5) == 0;
        let ids: Vec<ElementId> = self
            .project
            .elements
            .values()
            .filter(|e| any || e.as_wall().is_some())
            .map(|e| e.id)
            .collect();
        (!ids.is_empty()).then(|| self.project.elements[&ids[self.rng.below(ids.len())]].clone())
    }

    fn wall(&mut self) -> Option<Element> {
        let ids: Vec<ElementId> = self
            .project
            .elements
            .values()
            .filter(|e| e.as_wall().is_some())
            .map(|e| e.id)
            .collect();
        (!ids.is_empty()).then(|| self.project.elements[&ids[self.rng.below(ids.len())]].clone())
    }

    fn storey(&mut self) -> Storey {
        let storeys = self.project.storeys.as_slice();
        storeys[self.rng.below(storeys.len())].clone()
    }

    /// Executes `command`; a refused command changes nothing.
    fn run(&mut self, command: Command) -> bool {
        self.history.execute(&mut self.project, command).is_ok()
    }

    /// Applies one random edit.
    fn edit(&mut self) -> Edit {
        let op = self.op();
        let (accepted, description) = match op {
            Op::Move => self.moved(),
            Op::Thickness | Op::ReferenceLine | Op::RelinkTop => self.changed_wall(op),
            Op::Delete => match self.element() {
                Some(element) => (self.run(Command::DeleteElements(vec![element.id])), element.name),
                None => (false, String::new()),
            },
            Op::Add => self.added(),
            Op::StoreyElevation | Op::StoreyHeight => self.moved_storey(op),
            Op::RelinkHome => match self.element() {
                Some(element) => {
                    let target = self.storey();
                    let accepted = match relink_to_storey(&self.project, &element, target.id) {
                        Some(moved) => self.run(Command::UpdateElements(vec![moved])),
                        None => false,
                    };
                    (accepted, format!("{} to {}", element.name, target.name))
                }
                None => (false, String::new()),
            },
            Op::Undo => (self.history.undo(&mut self.project).is_some(), String::new()),
            Op::Redo => (self.history.redo(&mut self.project).is_some(), String::new()),
            Op::InsertStorey => {
                let reference = self.storey();
                let above = self.rng.coin();
                let storey = Storey {
                    id: StoreyId::from_uuid(self.ids.id("storey")),
                    ..Storey::new("Inserted", 0.0, self.rng.pick(&[2.5, 3.0]))
                };
                let accepted = self.run(Command::InsertStorey {
                    reference: reference.id,
                    above,
                    storey,
                });
                let side = if above { "above" } else { "below" };
                (accepted, format!("{side} {}", reference.name))
            }
            Op::DeleteStorey => {
                let storey = self.storey();
                let target = self.storey();
                let elements = if self.rng.coin() {
                    StoreyElements::Delete
                } else {
                    StoreyElements::MoveTo(target.id)
                };
                let accepted = self.run(Command::DeleteStorey {
                    storey: storey.id,
                    elements,
                });
                (accepted, format!("{} ({elements:?})", storey.name))
            }
            Op::Host | Op::Failing => self.hosted(op),
            Op::AddColumn => self.added_column(),
            Op::ColumnEdit => self.changed_column(),
            Op::Settings => {
                let mut settings = self.project.settings.clone();
                settings.tessellation.chord_deviation = self.rng.pick(&[0.001, 0.002, 0.004]);
                (self.run(Command::SetSettings(settings)), String::new())
            }
        };
        Edit {
            op,
            accepted,
            description,
        }
    }

    /// Moves an element, mostly a wall, by a grid step or by a distance
    /// around the model tolerance.
    fn moved(&mut self) -> (bool, String) {
        let Some(element) = self.element() else {
            return (false, String::new());
        };
        let axis = if self.rng.coin() { DVec2::X } else { DVec2::Y };
        let sign = if self.rng.coin() { 1.0 } else { -1.0 };
        let delta = axis * sign * self.rng.pick(&[4.0, 2.0, 1.0, 0.5, 0.000_005, 0.000_02]);
        let accepted = self.run(Command::MoveElements {
            ids: vec![element.id],
            delta,
        });
        (accepted, format!("{} by {delta}", element.name))
    }

    /// Changes a wall's thickness, reference line or top link.
    fn changed_wall(&mut self, op: Op) -> (bool, String) {
        let Some(mut element) = self.wall() else {
            return (false, String::new());
        };
        let storey = self.storey().id;
        let Some(wall) = element.as_wall_mut() else {
            unreachable!("a wall");
        };
        let what = match op {
            Op::Thickness => {
                wall.thickness = self.rng.pick(&[0.1, 0.115, 0.175, 0.25, 0.3, 0.365, 0.49]);
                format!("{}", wall.thickness)
            }
            Op::ReferenceLine => {
                wall.reference_line = self.rng.pick(&[
                    ReferenceLine::Outside,
                    ReferenceLine::Center,
                    ReferenceLine::Inside,
                    ReferenceLine::Custom(0.05),
                ]);
                format!("{:?}", wall.reference_line)
            }
            _ => {
                wall.top = match self.rng.below(3) {
                    0 => TopLink::Fixed {
                        height: self.rng.pick(&[2.5, 3.0, 4.5]),
                    },
                    1 => TopLink::Storey {
                        storey,
                        offset: self.rng.pick(&[0.0, -0.3]),
                    },
                    _ => TopLink::Relative {
                        storeys: self.rng.pick(&[-1, 1, 2, 3]),
                        offset: self.rng.pick(&[0.0, 0.3]),
                    },
                };
                format!("{:?}", wall.top)
            }
        };
        let description = format!("{} to {what}", element.name);
        (self.run(Command::UpdateElements(vec![element])), description)
    }

    /// Adds a wall from an end or the middle of a wall along a grid axis.
    fn added(&mut self) -> (bool, String) {
        let (storey, start) = match self.wall() {
            Some(element) => {
                let wall = element.as_wall().expect("a wall");
                let at = self.rng.pick(&[0.0, 0.5, 1.0]);
                (element.storey, wall.start.lerp(wall.end, at))
            }
            None => (self.storey().id, DVec2::ZERO),
        };
        let direction = self.rng.pick(&[DVec2::X, DVec2::NEG_X, DVec2::Y, DVec2::NEG_Y]);
        let end = start + direction * self.rng.pick(&[2.0, 4.0]);
        let mut wall = self.project.new_wall(storey, start, end, 0);
        wall.id = ElementId::from_uuid(self.ids.id("wall"));
        (
            self.run(Command::AddElements(vec![wall])),
            format!("from {start} to {end}"),
        )
    }

    /// Adds a column (COL-001) at an end or the middle of a wall, turned or
    /// not, rectangular or circular.
    fn added_column(&mut self) -> (bool, String) {
        let (storey, at) = match self.wall() {
            Some(element) => {
                let wall = element.as_wall().expect("a wall");
                let at = self.rng.pick(&[0.0, 0.5, 1.0]);
                (element.storey, wall.start.lerp(wall.end, at))
            }
            None => (self.storey().id, DVec2::ZERO),
        };
        let rotation = self.rng.pick(&[0.0, 0.3, std::f64::consts::FRAC_PI_2]);
        let mut column = self.project.new_column(storey, at, rotation, 0);
        column.id = ElementId::from_uuid(self.ids.id("column"));
        if self.rng.coin() {
            column.as_column_mut().expect("a column").section = CrossSection::Circular { diameter: 0.4 };
        }
        (
            self.run(Command::AddElements(vec![column])),
            format!("at {at}, turned by {rotation}"),
        )
    }

    /// Changes a column's rotation, anchor, section, end cuts or top link.
    fn changed_column(&mut self) -> (bool, String) {
        let ids: Vec<ElementId> = self
            .project
            .elements
            .values()
            .filter(|e| e.as_column().is_some())
            .map(|e| e.id)
            .collect();
        if ids.is_empty() {
            return (false, String::new());
        }
        let mut element = self.project.elements[&ids[self.rng.below(ids.len())]].clone();
        let storey = self.storey().id;
        let what = self.rng.below(6);
        let cuts = [ColumnCut::Horizontal, ColumnCut::Angle(0.2), ColumnCut::Angle(-0.35)];
        let column = element.as_column_mut().expect("a column");
        let change = match what {
            0 => {
                column.rotation = self.rng.pick(&[0.0, 0.3, 1.2]);
                format!("rotation {}", column.rotation)
            }
            1 => {
                column.anchor = self.rng.pick(&Anchor::ALL);
                format!("{:?}", column.anchor)
            }
            2 => {
                column.section = self.rng.pick(&[
                    CrossSection::Rectangular {
                        width: 0.3,
                        height: 0.3,
                    },
                    CrossSection::Rectangular {
                        width: 0.25,
                        height: 0.5,
                    },
                    CrossSection::Circular { diameter: 0.4 },
                ]);
                format!("{:?}", column.section)
            }
            3 => {
                column.top_cut = self.rng.pick(&cuts);
                format!("top {:?}", column.top_cut)
            }
            4 => {
                column.bottom_cut = self.rng.pick(&cuts);
                format!("bottom {:?}", column.bottom_cut)
            }
            _ => {
                column.link.top = match self.rng.below(3) {
                    0 => TopLink::Fixed {
                        height: self.rng.pick(&[2.5, 3.0]),
                    },
                    1 => TopLink::Storey { storey, offset: 0.0 },
                    _ => TopLink::Relative {
                        storeys: self.rng.pick(&[1, 2]),
                        offset: 0.0,
                    },
                };
                format!("{:?}", column.link.top)
            }
        };
        let description = format!("{} to {change}", element.name);
        (self.run(Command::UpdateElements(vec![element])), description)
    }

    /// Changes a storey's elevation or height, with or without the storeys
    /// above.
    fn moved_storey(&mut self, op: Op) -> (bool, String) {
        let storey = self.storey();
        let shift_above = self.rng.coin();
        let change = self.rng.pick(&[-3.0, -1.0, -0.5, -0.25, 0.25, 0.5, 1.0, 3.0]);
        let command = if op == Op::StoreyElevation {
            Command::SetStoreyElevation {
                storey: storey.id,
                elevation: storey.elevation + change,
                shift_above,
            }
        } else {
            Command::SetStoreyHeight {
                storey: storey.id,
                height: storey.height + change,
                shift_above,
            }
        };
        let description = format!("{} by {change}, shift above {shift_above}", storey.name);
        (self.run(command), description)
    }

    /// Adds a small test kind element in the middle of a wall: hosted in
    /// it through a relation (SRS 3.3), or one whose generator panics, which
    /// gives it a problem (NFR-REL-006).
    fn hosted(&mut self, op: Op) -> (bool, String) {
        let Some(host) = self.wall() else {
            return (false, String::new());
        };
        let wall = host.as_wall().expect("a wall");
        let middle = wall.start.lerp(wall.end, 0.5);
        let mut kind = Dummy::new(
            middle - DVec2::splat(0.05),
            DVec2::splat(0.1),
            self.project.materials[0].id,
        );
        kind.panic_on_evaluate = op == Op::Failing;
        let mut element = kind.into_element(&self.project, host.storey, 0);
        element.id = ElementId::from_uuid(self.ids.id("hosted"));
        let id = element.id;
        let mut accepted = self.run(Command::AddElements(vec![element]));
        if op == Op::Host {
            accepted = accepted
                && self.run(Command::InsertRelation(Relation::Host {
                    hosted: id,
                    host: host.id,
                }));
        }
        (accepted, format!("in {}", host.name))
    }
}

/// Fails unless `updated`, evaluated incrementally from `previous`, lists
/// every element whose geometry or problem differs between the full
/// evaluations `full_before` and `full_after` in `changed`, keeps the `Arc`
/// of every other element, and lists the old and new home storeys of the
/// changed elements in `changed_storeys`.
fn assert_changes_listed(
    updated: &ModelGeometry,
    previous: &ModelGeometry,
    (full_before, full_after): (&ModelGeometry, &ModelGeometry),
    (homes_before, homes_after): (&HashMap<ElementId, StoreyId>, &HashMap<ElementId, StoreyId>),
    context: &str,
) {
    let ids = full_before
        .elements
        .keys()
        .chain(full_after.elements.keys())
        .chain(full_before.problems.keys())
        .chain(full_after.problems.keys());
    for id in ids {
        let differs =
            full_before.get(*id) != full_after.get(*id) || full_before.problems.get(id) != full_after.problems.get(id);
        assert!(
            !differs || updated.changed.contains(id),
            "{context}: {id} changed unlisted"
        );
    }
    for (id, element) in &updated.elements {
        if !updated.changed.contains(id) {
            let kept = previous.elements.get(id).is_some_and(|old| Arc::ptr_eq(old, element));
            assert!(kept, "{context}: {id} is unchanged but has a new Arc");
        }
    }
    for id in &updated.changed {
        for home in [homes_before.get(id), homes_after.get(id)].into_iter().flatten() {
            assert!(updated.changed_storeys.contains(home), "{context}: the home of {id}");
        }
    }
}

/// NFR-PERF-006, VIEW-001: 200 seeded random edit sequences of 1 to 20 edits
/// on `stand_in(1_000)` (moves, thickness, reference line, delete, add,
/// storey elevation and height, relinking the top and the home storey,
/// undo, redo, inserted and deleted storeys, hosted and failing elements,
/// settings, added columns and their rotation, anchor, section, end cuts
/// and top link (COL-001, COL-003)), evaluated incrementally after every edit and once for the
/// whole sequence, give the elements, parts, children, axes and problems of
/// a fresh `ModelGeometry::evaluate`, bit for bit (`TriMesh ==`). Each
/// assertion prints the seed and the edits so far. The sequences take every
/// kind of edit, and most steps update the previous geometry rather than
/// evaluate everything again.
#[test]
fn incremental_equals_full() {
    let base = samples::stand_in(1_000);
    let base_geometry = ModelGeometry::evaluate(&base);
    let base_homes = homes(&base);
    let mut accepted: HashMap<Op, usize> = HashMap::new();
    let (mut steps, mut updates) = (0, 0);
    for seed in 1..=200_u64 {
        let mut session = Session::new(&base, seed);
        let length = 1 + session.rng.below(20);
        let mut log = Vec::new();
        let (mut geometry, mut full, mut homes_before) =
            (base_geometry.clone(), base_geometry.clone(), base_homes.clone());
        for step in 1..=length {
            let edit = session.edit();
            if edit.accepted {
                *accepted.entry(edit.op).or_default() += 1;
            }
            log.push(format!("{:?} {} ({})", edit.op, edit.description, edit.accepted));
            let context = format!("seed {seed}, edit {step} of {length}: {}", log.join("; "));
            steps += 1;
            if !dirty_set(&session.project, &geometry).everything {
                updates += 1;
            }
            let updated = ModelGeometry::evaluate_incremental(&session.project, &geometry);
            let fresh = ModelGeometry::evaluate(&session.project);
            assert_same_geometry(&updated, &fresh, &context);
            let homes_after = homes(&session.project);
            assert_changes_listed(
                &updated,
                &geometry,
                (&full, &fresh),
                (&homes_before, &homes_after),
                &context,
            );
            (geometry, full, homes_before) = (updated, fresh, homes_after);
        }
        let at_once = ModelGeometry::evaluate_incremental(&session.project, &base_geometry);
        let context = format!("seed {seed}, all {length} edits at once: {}", log.join("; "));
        assert_same_geometry(&at_once, &full, &context);
    }
    for (op, _) in OPS {
        assert!(
            accepted.get(&op).is_some_and(|n| *n > 0),
            "no {op:?} edit was accepted: {accepted:?}"
        );
    }
    assert!(
        updates * 10 > steps * 9,
        "{updates} of {steps} steps updated the previous geometry"
    );
}
