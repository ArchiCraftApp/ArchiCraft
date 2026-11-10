//! The change log of a project (NFR-PERF-006): every edit stamps the
//! elements and storeys it changes with the revision after it, so that the
//! caches derived from a project (geometry, plans, the 3D scene) can update
//! only what changed since the revision they last saw. Undo and redo stamp
//! the same things again (UND-001). The log is a cache: it takes no part in
//! the project's equality and is never serialised.

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::{BTreeMap, BTreeSet};

use archicraft_model::command::{Edit, StoreyElements};
use archicraft_model::glam::DVec2;
use archicraft_model::template::default_metric_project;
use archicraft_model::{
    Command, Element, ElementId, ElementStamp, History, Layer, Project, Relation, Storey, StoreyId, StoreyList,
};
use common::wall;

/// The elements stamped after `revision`, each with whether its stamp is a
/// tombstone.
fn stamped(p: &Project, revision: u64) -> BTreeMap<ElementId, bool> {
    p.changes
        .elements_since(revision)
        .map(|(id, stamp)| (id, stamp.removed))
        .collect()
}

/// The storeys stamped after `revision`.
fn storeys_stamped(p: &Project, revision: u64) -> BTreeSet<StoreyId> {
    p.changes.storeys_since(revision).collect()
}

fn set<T: Ord + Copy>(items: &[T]) -> BTreeSet<T> {
    items.iter().copied().collect()
}

/// Four walls W-001 to W-004 on the ground storey, not yet added.
fn four_walls(p: &Project) -> Vec<Element> {
    let ground = p.storeys.ground();
    (0..4)
        .map(|n| {
            let y = f64::from(n);
            p.new_wall(ground, DVec2::new(0.0, y), DVec2::new(5.0, y), n)
        })
        .collect()
}

/// The steps of a command's round trip: the command, its undo and its redo.
#[derive(Clone, Copy, Debug)]
enum Step {
    Execute,
    Undo,
    Redo,
}

const ROUND_TRIP: [Step; 3] = [Step::Execute, Step::Undo, Step::Redo];

fn run(p: &mut Project, h: &mut History, command: &Command, step: Step) {
    match step {
        Step::Execute => h.execute(p, command.clone()).unwrap(),
        Step::Undo => {
            h.undo(p).unwrap();
        }
        Step::Redo => {
            h.redo(p).unwrap();
        }
    }
}

/// Executes `command`, undoes and redoes it, and checks that each of the
/// three stamps exactly the elements `expected`, with the revisions of that
/// step, and a tombstone exactly for the elements the step removed.
#[track_caller]
fn assert_stamps_elements(p: &mut Project, h: &mut History, command: Command, expected: &[ElementId]) {
    for step in ROUND_TRIP {
        let before = p.revision;
        run(p, h, &command, step);
        let stamps = stamped(p, before);
        assert_eq!(
            stamps.keys().copied().collect::<BTreeSet<_>>(),
            set(expected),
            "{step:?}"
        );
        for (id, removed) in stamps {
            let stamp = p.changes.element(id).unwrap();
            assert!(
                stamp.revision > before && stamp.revision <= p.revision,
                "{step:?}: {stamp:?} is a revision of the step ({before}, {}]",
                p.revision
            );
            assert_eq!(removed, p.element(id).is_none(), "{step:?}: the tombstone of {id}");
        }
    }
}

/// NFR-PERF-006: an element edit stamps its element with the revision after
/// it; a removal leaves a tombstone, and an edit that fails stamps nothing.
#[test]
fn an_element_edit_stamps_its_element_with_the_revision_after_it() {
    let mut p = default_metric_project();
    assert!(p.changes.is_empty(), "a new project starts with an empty log");
    let w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    let id = w.id;
    let changed = |p: &Project| {
        Some(ElementStamp {
            revision: p.revision,
            removed: false,
        })
    };

    let remove = Edit::InsertElement {
        index: 0,
        element: w.clone(),
    }
    .apply(&mut p)
    .unwrap();
    assert_eq!(p.changes.element(id), changed(&p));

    let mut moved = w.clone();
    moved.kind.translate(DVec2::new(0.0, 2.0));
    Edit::ReplaceElement(moved).apply(&mut p).unwrap();
    assert_eq!(p.changes.element(id), changed(&p));

    remove.apply(&mut p).unwrap();
    let tombstone = Some(ElementStamp {
        revision: p.revision,
        removed: true,
    });
    assert_eq!(p.changes.element(id), tombstone, "a removal leaves a tombstone");

    let before = p.revision;
    assert!(Edit::RemoveElement { id }.apply(&mut p).is_err());
    assert_eq!(p.revision, before);
    assert_eq!(p.changes.element(id), tombstone, "a failing edit stamps nothing");
    assert_eq!(stamped(&p, before), BTreeMap::new());
}

/// NFR-PERF-006, UND-001: adding, updating, moving and deleting elements
/// stamp exactly the elements they change, and their undo and redo stamp
/// the same elements again, so a cache that saw the state before the edit
/// and one that saw the state before its undo update the same elements.
#[test]
fn element_commands_their_undo_and_redo_stamp_exactly_their_elements() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let walls = four_walls(&p);
    let ids: Vec<ElementId> = walls.iter().map(|w| w.id).collect();

    assert_stamps_elements(&mut p, &mut h, Command::AddElements(walls.clone()), &ids);

    let mut thicker = p.element(ids[0]).unwrap().clone();
    thicker.as_wall_mut().unwrap().thickness = 0.4;
    assert_stamps_elements(&mut p, &mut h, Command::UpdateElements(vec![thicker]), &ids[..1]);

    let delta = DVec2::new(0.0, 10.0);
    let moved = vec![ids[1], ids[2]];
    assert_stamps_elements(&mut p, &mut h, Command::MoveElements { ids: moved, delta }, &ids[1..3]);

    assert_stamps_elements(&mut p, &mut h, Command::DeleteElements(vec![ids[3]]), &ids[3..]);
    assert_eq!(p.changes.element(ids[3]).map(|s| s.removed), Some(true));
}

/// NFR-PERF-006: a command that changes no element stamps none.
#[test]
fn commands_on_other_data_stamp_no_element() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let walls = four_walls(&p);
    h.execute(&mut p, Command::AddElements(walls)).unwrap();
    let mut info = p.info.clone();
    info.name = "Renamed".to_owned();
    let first = p.storeys.as_slice()[1].id;
    let commands = [
        Command::SetInfo(info),
        Command::AddLayer(Layer::new("Extra")),
        Command::RenameStorey {
            storey: first,
            name: "01 Upper floor".to_owned(),
        },
    ];
    for command in commands {
        let before = p.revision;
        h.execute(&mut p, command.clone()).unwrap();
        assert_eq!(stamped(&p, before), BTreeMap::new(), "{command:?}");
    }
}

/// A project with the storeys B1 (-3 m), 00 (0 m), 01 (3 m) and 02 (6 m),
/// each 3 m high, made by command, with the history and the ids in that
/// order.
fn four_storeys() -> (Project, History, [StoreyId; 4]) {
    let mut p = default_metric_project();
    let mut h = History::default();
    let ground = p.storeys.ground();
    let first = p.storeys.as_slice()[1].id;
    let basement = Storey::new("B1", 0.0, 3.0);
    let second = Storey::new("02", 0.0, 3.0);
    let ids = [basement.id, ground, first, second.id];
    let below = Command::InsertStorey {
        reference: ground,
        above: false,
        storey: basement,
    };
    let above = Command::InsertStorey {
        reference: first,
        above: true,
        storey: second,
    };
    h.execute(&mut p, below).unwrap();
    h.execute(&mut p, above).unwrap();
    let elevations: Vec<f64> = p.storeys.iter().map(|s| s.elevation).collect();
    assert_eq!(elevations, [-3.0, 0.0, 3.0, 6.0]);
    (p, h, ids)
}

/// The storeys whose elevation, height or position differ between `old`
/// and `new`, and the storeys in one list only: what a storey edit moves.
fn moved_storeys(old: &StoreyList, new: &StoreyList) -> BTreeSet<StoreyId> {
    let ids: BTreeSet<StoreyId> = old.iter().chain(new.iter()).map(|s| s.id).collect();
    ids.into_iter()
        .filter(|&id| match (old.get(id), new.get(id)) {
            (Some(a), Some(b)) => {
                a.elevation != b.elevation || a.height != b.height || old.index_of(id) != new.index_of(id)
            }
            _ => true,
        })
        .collect()
}

/// A storey edit on the storeys of [`four_storeys`] and the storeys it
/// moves.
struct StoreyCase {
    name: &'static str,
    /// The command, given the storeys B1, 00, 01 and 02 and a new storey.
    command: fn([StoreyId; 4], &Storey) -> Command,
    /// The storeys it moves, given the same storeys and the new one's id.
    moves: fn([StoreyId; 4], StoreyId) -> Vec<StoreyId>,
}

/// NFR-PERF-006, UND-001: a storey edit stamps exactly the storeys whose
/// elevation, height or position in the list it changes, and the storey it
/// inserts or deletes; a rename or a cut-plane change moves no storey. Undo
/// and redo stamp the same storeys again.
#[test]
fn storey_edits_stamp_exactly_the_storeys_they_move() {
    let cases = [
        StoreyCase {
            name: "the top storey up, alone: the storey below gets higher",
            command: |[_, _, _, s], _| Command::SetStoreyElevation {
                storey: s,
                elevation: 6.5,
                shift_above: false,
            },
            moves: |[_, _, f, s], _| vec![f, s],
        },
        StoreyCase {
            name: "a storey up with those above: the storey below gets higher",
            command: |[_, _, f, _], _| Command::SetStoreyElevation {
                storey: f,
                elevation: 3.5,
                shift_above: true,
            },
            moves: |[_, g, f, s], _| vec![g, f, s],
        },
        StoreyCase {
            name: "the ground storey higher, only the next one moves",
            command: |[_, g, _, _], _| Command::SetStoreyHeight {
                storey: g,
                height: 3.5,
                shift_above: false,
            },
            moves: |[_, g, f, _], _| vec![g, f],
        },
        StoreyCase {
            name: "the ground storey higher, all above move",
            command: |[_, g, _, _], _| Command::SetStoreyHeight {
                storey: g,
                height: 4.0,
                shift_above: true,
            },
            moves: |[_, g, f, s], _| vec![g, f, s],
        },
        StoreyCase {
            name: "a storey inserted above 01: 02 moves up and one place on",
            command: |[_, _, f, _], new| Command::InsertStorey {
                reference: f,
                above: true,
                storey: new.clone(),
            },
            moves: |[_, _, _, s], new| vec![new, s],
        },
        StoreyCase {
            name: "a storey inserted below the ground storey: B1 moves down, the others one place on",
            command: |[_, g, _, _], new| Command::InsertStorey {
                reference: g,
                above: false,
                storey: new.clone(),
            },
            moves: |[b, g, f, s], new| vec![new, b, g, f, s],
        },
        StoreyCase {
            name: "the top storey deleted",
            command: |[_, _, _, s], _| Command::DeleteStorey {
                storey: s,
                elements: StoreyElements::Delete,
            },
            moves: |[_, _, _, s], _| vec![s],
        },
        StoreyCase {
            name: "the ground storey deleted: the storeys above move down and one place back",
            command: |[_, g, _, _], _| Command::DeleteStorey {
                storey: g,
                elements: StoreyElements::Delete,
            },
            moves: |[_, g, f, s], _| vec![g, f, s],
        },
        StoreyCase {
            name: "a rename",
            command: |[_, _, f, _], _| Command::RenameStorey {
                storey: f,
                name: "01 Upper floor".to_owned(),
            },
            moves: |_, _| Vec::new(),
        },
        StoreyCase {
            name: "a cut-plane height",
            command: |[_, _, f, _], _| Command::SetStoreyCutPlane {
                storey: f,
                height: Some(1.5),
            },
            moves: |_, _| Vec::new(),
        },
    ];
    for case in cases {
        let (mut p, mut h, ids) = four_storeys();
        let new = Storey::new("New", 0.0, 3.0);
        let command = (case.command)(ids, &new);
        let expected = set(&(case.moves)(ids, new.id));
        let name = case.name;
        for step in ROUND_TRIP {
            let before = p.revision;
            let old = p.storeys.clone();
            run(&mut p, &mut h, &command, step);
            assert_eq!(storeys_stamped(&p, before), expected, "{name}, {step:?}");
            assert_eq!(
                moved_storeys(&old, &p.storeys),
                expected,
                "{name}, {step:?}: the lists differ"
            );
            for id in &expected {
                let revision = p.changes.storey(*id).unwrap();
                assert!(revision > before && revision <= p.revision, "{name}, {step:?}");
            }
        }
    }
}

/// NFR-PERF-006: a settings edit (tessellation) invalidates everything
/// derived from the project; no other command does.
#[test]
fn only_a_settings_edit_invalidates_everything() {
    let (mut p, mut h, [_, ground, first, _]) = four_storeys();
    let start = p.revision;
    let mut info = p.info.clone();
    info.name = "Renamed".to_owned();
    let walls = four_walls(&p);
    let ids: Vec<ElementId> = walls.iter().map(|w| w.id).collect();
    let others = [
        Command::AddElements(walls),
        Command::MoveElements {
            ids,
            delta: DVec2::new(1.0, 0.0),
        },
        Command::SetInfo(info),
        Command::AddLayer(Layer::new("Extra")),
        Command::SetStoreyElevation {
            storey: first,
            elevation: 3.5,
            shift_above: true,
        },
        Command::SetStoreyHeight {
            storey: ground,
            height: 3.0,
            shift_above: true,
        },
    ];
    for command in others {
        h.execute(&mut p, command).unwrap();
    }
    h.undo(&mut p).unwrap();
    assert!(!p.changes.everything_changed_since(start));

    let before = p.revision;
    let mut settings = p.settings.clone();
    settings.tessellation.chord_deviation = 0.001;
    h.execute(&mut p, Command::SetSettings(settings)).unwrap();
    assert!(p.changes.everything_changed_since(before));
    assert!(!p.changes.everything_changed_since(p.revision));
    let command = Command::SetSettings(p.settings.clone());
    for step in [Step::Undo, Step::Redo] {
        let before = p.revision;
        run(&mut p, &mut h, &command, step);
        assert!(p.changes.everything_changed_since(before), "{step:?}");
    }
}

/// NFR-PERF-006: a relation decides what its ends depend on (SRS 3.3), so
/// recording or removing it stamps both ends; deleting an element removes
/// its relations, which stamps the other ends too.
#[test]
fn a_relation_edit_stamps_both_ends() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let walls = four_walls(&p);
    let [a, b, c] = [walls[0].id, walls[1].id, walls[2].id];
    h.execute(&mut p, Command::AddElements(walls)).unwrap();

    let junction = Relation::Junction { a, b };
    assert_stamps_elements(&mut p, &mut h, Command::InsertRelation(junction), &[a, b]);
    assert_stamps_elements(&mut p, &mut h, Command::RemoveRelation(junction.id()), &[a, b]);

    h.execute(&mut p, Command::InsertRelation(Relation::Host { hosted: c, host: a }))
        .unwrap();
    let before = p.revision;
    h.execute(&mut p, Command::DeleteElements(vec![a])).unwrap();
    assert_eq!(stamped(&p, before), BTreeMap::from([(a, true), (c, false)]));
}

/// The change log is a cache: projects that differ only in their logs are
/// equal, the log is not part of the serde form, and a project read back
/// from it starts with an empty log.
#[test]
fn the_change_log_takes_no_part_in_equality_or_the_serde_form() {
    let mut p = default_metric_project();
    let pristine = p.clone();
    let mut h = History::default();
    let walls = four_walls(&p);
    h.execute(&mut p, Command::AddElements(walls)).unwrap();
    h.undo(&mut p).unwrap();
    assert!(!p.changes.is_empty());
    assert!(pristine.changes.is_empty());
    assert_eq!(
        Project {
            revision: pristine.revision,
            ..p.clone()
        },
        pristine,
        "projects that differ only in their logs are equal"
    );

    let json = serde_json::to_value(&p).unwrap();
    assert!(json.get("changes").is_none(), "the log is not serialised");
    let back: Project = serde_json::from_value(json).unwrap();
    assert!(back.changes.is_empty(), "a project read back starts with an empty log");
}
