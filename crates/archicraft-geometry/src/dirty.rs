//! What the edits since an evaluation dirtied (NFR-PERF-006, VIEW-001,
//! NFR-ACC-003): the elements whose geometry
//! [`ModelGeometry::evaluate_incremental`] evaluates again, found through the
//! project's change log, its relation store and the contact index of the
//! previous geometry, with a scan over the elements only when a storey moved.
//!
//! An element's geometry follows from its own data, the storeys it stands on
//! and reaches, and, for a wall, the junction inputs ([`WallInput`]: storey,
//! ends, thickness, reference line, absolute bottom and top) of the walls
//! whose reference lines touch its ends. So the dirty set of the edits since
//! a previous geometry holds:
//!
//! 1. the elements an edit inserted, replaced or removed: the change log's
//!    stamps and tombstones since the previous revision;
//! 2. for the storeys the edits moved, their storey dependents
//!    ([`RelationStore::storey_dependents`]) and every element whose vertical
//!    extent differs from the one under the storey list the previous
//!    geometry was evaluated from, as the storey dependents miss the tops
//!    linked relative to a storey further than the next (until
//!    archicraft-lppht);
//! 3. the walls whose reference lines touch the old or the new reference line
//!    of a wall among them whose junction input changed
//!    ([`ContactIndex::touching_segment`]). A footprint depends on the inputs
//!    of the walls touching its ends, never on their footprints, so one level
//!    of contact is enough, and a wall whose input stayed, such as one that
//!    is dirty by contact or through a relation only, dirties no neighbour.
//!    Contact ignores vertical overlap, a superset that also covers a relink
//!    or a storey edit changing the overlap. The old inputs come from the
//!    previous geometry, so a wall that moved or went dirties its neighbours
//!    at its old place too;
//! 4. the closure of [`RelationStore::dependents_of`] over all of them, which
//!    runs both ways for hosts and junctions, the path of openings
//!    (host-openings).
//!
//! Undo dirties what the edit dirtied: the change log stamps the same
//! elements and storeys, and the old and the new state enter alike.
//!
//! [`ContactIndex::touching_segment`]: crate::contact::ContactIndex::touching_segment
//! [`RelationStore::storey_dependents`]: archicraft_model::RelationStore::storey_dependents
//! [`RelationStore::dependents_of`]: archicraft_model::RelationStore::dependents_of

use std::collections::BTreeSet;

use archicraft_model::{ElementId, Project, StoreyId};

use crate::evaluate::ModelGeometry;
use crate::kinds::wall::wall_input;
use crate::wall::WallInput;

/// What the edits since a previous evaluation dirtied, as [`dirty_set`]
/// finds it (NFR-PERF-006).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirtySet {
    /// True if the previous geometry cannot be updated: it was never
    /// evaluated, it belongs to another project or to a newer revision of
    /// this one, or an edit since changed what everything depends on, the
    /// settings ([`ChangeLog::everything_changed_since`]). Then every
    /// element is evaluated again, and the sets below hold everything of the
    /// project and of the previous geometry.
    ///
    /// [`ChangeLog::everything_changed_since`]: archicraft_model::ChangeLog::everything_changed_since
    pub everything: bool,
    /// The elements to evaluate again, or to drop as they are gone, in id
    /// order; only elements of the project or of the previous geometry.
    pub elements: BTreeSet<ElementId>,
    /// The walls among them whose junction input changed, in id order: the
    /// walls whose old and new reference lines contact was expanded from,
    /// and whose entries in the contact index are replaced.
    pub walls: BTreeSet<ElementId>,
    /// The storeys the edits moved ([`ChangeLog::storeys_since`]).
    ///
    /// [`ChangeLog::storeys_since`]: archicraft_model::ChangeLog::storeys_since
    pub storeys: BTreeSet<StoreyId>,
}

/// The dirty set of the edits to `project` since `previous` was evaluated
/// (see the [module documentation](self)).
pub fn dirty_set(project: &Project, previous: &ModelGeometry) -> DirtySet {
    let since = previous.revision;
    let changes = &project.changes;
    let Some(source) = previous.source().filter(|source| {
        source.project == project.id && since <= project.revision && !changes.everything_changed_since(since)
    }) else {
        return everything(project, previous);
    };

    let mut elements: BTreeSet<ElementId> = changes.elements_since(since).map(|(id, _)| id).collect();
    let storeys: BTreeSet<StoreyId> = changes.storeys_since(since).collect();
    if !storeys.is_empty() {
        for storey in &storeys {
            elements.extend(project.relations.storey_dependents(*storey));
        }
        let moved = project.elements.values().filter(|e| {
            let before = e.kind.vertical_extent(e.storey, &source.storeys);
            !same_extent(before, project.vertical_extent(e))
        });
        elements.extend(moved.map(|e| e.id));
    }

    let contacts = previous.contacts();
    let mut walls = BTreeSet::new();
    let mut touching = Vec::new();
    for &id in &elements {
        let old = contacts.wall(id);
        let new = project.element(id).and_then(|e| wall_input(project, e));
        if same_input(old, new.as_ref()) {
            continue;
        }
        walls.insert(id);
        for wall in old.into_iter().chain(new.as_ref()) {
            touching.extend(contacts.touching_segment(wall.storey, wall.start, wall.end));
        }
    }
    elements.extend(touching);

    let mut open: Vec<ElementId> = elements.iter().copied().collect();
    while let Some(id) = open.pop() {
        for dependent in project.relations.dependents_of(id) {
            if elements.insert(dependent) {
                open.push(dependent);
            }
        }
    }
    elements.retain(|id| project.elements.contains_key(id) || source.homes.contains_key(id));
    DirtySet {
        everything: false,
        elements,
        walls,
        storeys,
    }
}

/// The dirty set of a full evaluation: every element, wall and storey of
/// the project and of `previous`.
fn everything(project: &Project, previous: &ModelGeometry) -> DirtySet {
    let source = previous.source();
    let mut elements: BTreeSet<ElementId> = project.elements.keys().copied().collect();
    elements.extend(previous.elements.keys());
    elements.extend(previous.problems.keys());
    elements.extend(source.into_iter().flat_map(|s| s.homes.keys()));
    let mut walls: BTreeSet<ElementId> = project
        .elements
        .values()
        .filter(|e| e.as_wall().is_some())
        .map(|e| e.id)
        .collect();
    walls.extend(previous.contacts().walls().map(|w| w.id));
    let mut storeys: BTreeSet<StoreyId> = project.storeys.iter().map(|s| s.id).collect();
    storeys.extend(
        source
            .into_iter()
            .flat_map(|s| s.storeys.iter().map(|storey| storey.id)),
    );
    DirtySet {
        everything: true,
        elements,
        walls,
        storeys,
    }
}

/// True if two vertical extents are the same bit for bit.
fn same_extent(a: Option<(f64, f64)>, b: Option<(f64, f64)>) -> bool {
    let bits = |extent: Option<(f64, f64)>| extent.map(|(bottom, top)| (bottom.to_bits(), top.to_bits()));
    bits(a) == bits(b)
}

/// True if two junction inputs are the same bit for bit, so that no
/// footprint can tell them apart. `None` is no input: the element is no
/// wall, or a storey it needs is missing ([`wall_input`]).
fn same_input(old: Option<&WallInput>, new: Option<&WallInput>) -> bool {
    match (old, new) {
        (None, None) => true,
        (Some(a), Some(b)) => a.storey == b.storey && a.reference_line == b.reference_line && bits(a) == bits(b),
        _ => false,
    }
}

/// The numbers of a junction input that junction resolution reads, the face
/// offsets of the reference line among them, as bits.
fn bits(wall: &WallInput) -> [u64; 9] {
    let (exterior, interior) = wall.reference_line.face_offsets(wall.thickness);
    [
        wall.start.x,
        wall.start.y,
        wall.end.x,
        wall.end.y,
        wall.thickness,
        exterior,
        interior,
        wall.bottom,
        wall.top,
    ]
    .map(f64::to_bits)
}
