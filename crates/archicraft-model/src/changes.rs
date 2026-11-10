//! What each edit of a project changed (NFR-PERF-006, VIEW-001).
//!
//! The caches derived from a project (its geometry, the plans, the 3D scene)
//! update what an edit changed instead of everything. Each of them remembers
//! the project revision it last saw and asks the project's [`ChangeLog`]
//! what changed since: the elements an edit inserted, replaced or removed,
//! the storeys it moved, and whether it changed something everything
//! depends on. Stamps rather than a queue of changes, so that every
//! consumer diffs against its own last revision and none has to drain
//! anything.
//!
//! [`crate::command::Edit::apply`], the only place that advances
//! [`Project::revision`](crate::project::Project::revision), stamps every
//! change with the revision after its edit, for undo and redo alike. The
//! log keeps the latest stamp per element and per storey and is never
//! pruned, so it stays as large as the elements and storeys a session
//! touched. It is not saved: a project read from a file starts with an
//! empty log, and a consumer evaluates it in full once.

use std::collections::BTreeMap;

use crate::ids::{ElementId, StoreyId};
use crate::storey::{Storey, StoreyList};

/// The latest stamp of an element in a [`ChangeLog`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ElementStamp {
    /// The project revision after the last edit that inserted, replaced or
    /// removed the element.
    pub revision: u64,
    /// True if that edit removed the element: a tombstone. Inserting it
    /// again (an undo) replaces the tombstone.
    pub removed: bool,
}

/// The changes of a project, stamped edit by edit (NFR-PERF-006): the
/// project's [`Project::changes`](crate::project::Project::changes).
///
/// A cache: two logs always compare equal, so the log never takes part in
/// the project's equality, and the project's serde form leaves it out. The
/// stamps are kept in ordered maps, so that the queries list in a
/// deterministic order; a query is linear in the entries stamped so far.
#[derive(Clone, Debug, Default)]
pub struct ChangeLog {
    elements: BTreeMap<ElementId, ElementStamp>,
    /// The revision after the last edit that moved each storey.
    storeys: BTreeMap<StoreyId, u64>,
    /// The revision after the last edit that changed what everything
    /// derived from the project depends on, or 0.
    everything: u64,
}

impl PartialEq for ChangeLog {
    /// Always true: the log is a cache of what the edits did, not part of
    /// the project.
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl ChangeLog {
    /// True if no edit stamped anything yet.
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty() && self.storeys.is_empty() && self.everything == 0
    }

    /// The latest stamp of `element`, if an edit ever inserted, replaced or
    /// removed it.
    pub fn element(&self, element: ElementId) -> Option<ElementStamp> {
        self.elements.get(&element).copied()
    }

    /// The elements an edit inserted, replaced or removed after `revision`,
    /// with their latest stamps, in id order: what a cache that last saw
    /// `revision` has to update. A tombstone names an element that is no
    /// longer in the project.
    pub fn elements_since(&self, revision: u64) -> impl Iterator<Item = (ElementId, ElementStamp)> + '_ {
        self.elements
            .iter()
            .filter(move |(_, stamp)| stamp.revision > revision)
            .map(|(id, stamp)| (*id, *stamp))
    }

    /// The revision after the last edit that moved `storey`, if one did.
    pub fn storey(&self, storey: StoreyId) -> Option<u64> {
        self.storeys.get(&storey).copied()
    }

    /// The storeys an edit moved after `revision`, in id order: the storeys
    /// whose elevation, height or position in the list changed (a top
    /// linked relative to its home storey reaches a storey by position,
    /// STR-006), and the storeys inserted or deleted (STR-002, STR-003,
    /// STR-004). A rename or a new cut-plane height moves no storey.
    pub fn storeys_since(&self, revision: u64) -> impl Iterator<Item = StoreyId> + '_ {
        self.storeys
            .iter()
            .filter(move |(_, stamp)| **stamp > revision)
            .map(|(id, _)| *id)
    }

    /// True if an edit after `revision` changed what everything derived
    /// from the project depends on, such as the tessellation tolerances of
    /// the project settings (IFX-034): a cache that last saw `revision`
    /// starts over.
    pub fn everything_changed_since(&self, revision: u64) -> bool {
        self.everything > revision
    }

    /// Stamps `element`, which an edit inserted or replaced, or removed if
    /// `removed`.
    pub(crate) fn stamp_element(&mut self, element: ElementId, revision: u64, removed: bool) {
        self.elements.insert(element, ElementStamp { revision, removed });
    }

    /// Stamps the storeys an edit moved by replacing the storey list `old`
    /// with `new`: a storey stays unstamped only if the other list holds it
    /// at the same index with the same elevation and height, so the storeys
    /// in one list only are stamped too.
    pub(crate) fn stamp_storeys(&mut self, old: &StoreyList, new: &StoreyList, revision: u64) {
        let kept = |storey: &Storey, there: Option<&Storey>| {
            there.is_some_and(|s| s.id == storey.id && s.elevation == storey.elevation && s.height == storey.height)
        };
        for (list, other) in [(old, new), (new, old)] {
            for (index, storey) in list.iter().enumerate() {
                if !kept(storey, other.as_slice().get(index)) {
                    self.storeys.insert(storey.id, revision);
                }
            }
        }
    }

    /// Stamps a change that everything derived from the project depends on.
    pub(crate) fn stamp_everything(&mut self, revision: u64) {
        self.everything = revision;
    }
}
