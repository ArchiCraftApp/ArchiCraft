//! Relations between elements (SRS 3.3; IFX-060, UI-012, NFR-REL-004).
//!
//! The model is associative: a relation records that one element depends on
//! another, so that it can be found, re-evaluated and persisted. There are
//! two sorts. A *derived* relation, the storey link, is a pure function of
//! the element data (an element's home storey and its [`TopLink`]), which
//! the kind codecs already write to files (IFX-060); it is never stored
//! here. An *explicit* relation (host, junction, trim, solid operation,
//! annotation binding) has no other home: the [`RelationStore`] inside the
//! project holds it, it is part of the project's equality and it is
//! serialised, and the IFCX writer stores it in `archicraft::relations` on
//! the node of its first end.
//!
//! A relation carries its ends only. The epics that own a relation's meaning
//! (host openings, the junction engine, trims, solid operations, dimensions)
//! add their parameters; the file format keeps each relation an open object
//! so they can do so without a format change.
//!
//! The store also keeps a derived index, so that the dependency queries
//! ([`RelationStore::relations_of`], [`RelationStore::dependents_of`],
//! [`RelationStore::neighbours`], [`RelationStore::host_of`],
//! [`RelationStore::storey_dependents`], [`RelationStore::moved_by_storeys`])
//! do not scan the project: the explicit relations of each element, the
//! storey links of each element and each storey, and the elements whose top
//! is linked relative to their home storey. The index follows from the
//! explicit relations and the element data alone. It is rebuilt once after a load
//! ([`crate::project::Project::rebuild_indexes`]) and kept exact by every
//! edit: the store's own [`RelationStore::insert`] and
//! [`RelationStore::remove`], and [`RelationStore::update_element`] for each
//! element an edit inserts, removes or replaces, so undo and redo keep it
//! exact at the cost of the elements they touch.
//!
//! [`TopLink`]: crate::element::TopLink

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::element::Element;
use crate::ids::{ElementId, RelationId, StoreyId};
use crate::storey::StoreyList;

/// Namespace of the name-based (version 5) identities of relations.
pub const RELATION_NAMESPACE: Uuid = Uuid::from_u128(0x3b8e_1f52_7c4a_5d90_8e61_a2f4_0c9d_7b36);

/// Which end of a vertical element a derived storey link holds (ELM-005,
/// ELM-006).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum StoreyRole {
    /// The element's base follows its home storey.
    Base,
    /// The element's top follows a storey it is linked to.
    Top,
}

impl StoreyRole {
    fn code(self) -> &'static str {
        match self {
            StoreyRole::Base => "base",
            StoreyRole::Top => "top",
        }
    }
}

/// The file codes of the explicit relations, in declaration order
/// (IFX-060): what [`Relation::from_code`] accepts.
pub const EXPLICIT_CODES: &[&str] = &["host", "junction", "trim", "solid_operation", "annotation_binding"];

/// A relation of SRS 3.3, carrying its ends only. The first end is the
/// element that depends on the second (the hosted element on its host, the
/// trimmed element on what trims it, the annotation on what it measures); a
/// junction is symmetric and keeps the order it was recorded in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Relation {
    /// Derived: `element`'s base or top follows `storey` (ELM-005, ELM-006,
    /// STR-006). Never stored; the store only indexes it.
    StoreyLink {
        element: ElementId,
        storey: StoreyId,
        role: StoreyRole,
    },
    /// `hosted` is positioned relative to `host` and cuts it (OPN-001).
    Host { hosted: ElementId, host: ElementId },
    /// `a` and `b` meet and are cleaned up by the junction rules (WAL-020).
    Junction { a: ElementId, b: ElementId },
    /// `target` is trimmed by `by`, keeping one side (ROF-010).
    Trim { target: ElementId, by: ElementId },
    /// `target` is modified by the `operator` element (SOL-020).
    SolidOperation { target: ElementId, operator: ElementId },
    /// `annotation` references `target`'s geometry or datum (DIM-001).
    AnnotationBinding { annotation: ElementId, target: ElementId },
}

impl Relation {
    /// The stable code of the variant in files (IFX-060) and in the
    /// relation's identity.
    pub fn code(&self) -> &'static str {
        match self {
            Relation::StoreyLink { .. } => "storey_link",
            Relation::Host { .. } => "host",
            Relation::Junction { .. } => "junction",
            Relation::Trim { .. } => "trim",
            Relation::SolidOperation { .. } => "solid_operation",
            Relation::AnnotationBinding { .. } => "annotation_binding",
        }
    }

    /// The explicit relation with the code `code` from `first` to `second`
    /// (IFX-060); `None` for the derived storey link and unknown codes.
    pub fn from_code(code: &str, first: ElementId, second: ElementId) -> Option<Self> {
        Some(match code {
            "host" => Relation::Host {
                hosted: first,
                host: second,
            },
            "junction" => Relation::Junction { a: first, b: second },
            "trim" => Relation::Trim {
                target: first,
                by: second,
            },
            "solid_operation" => Relation::SolidOperation {
                target: first,
                operator: second,
            },
            "annotation_binding" => Relation::AnnotationBinding {
                annotation: first,
                target: second,
            },
            _ => return None,
        })
    }

    /// Message key of the relation's name (NFR-L10N-001).
    pub fn label_key(&self) -> &'static str {
        match self {
            Relation::StoreyLink { .. } => "model-relation-storey-link",
            Relation::Host { .. } => "model-relation-host",
            Relation::Junction { .. } => "model-relation-junction",
            Relation::Trim { .. } => "model-relation-trim",
            Relation::SolidOperation { .. } => "model-relation-solid-operation",
            Relation::AnnotationBinding { .. } => "model-relation-annotation-binding",
        }
    }

    /// True for a relation that follows from the element data and is never
    /// stored: the storey link.
    pub fn is_derived(&self) -> bool {
        matches!(self, Relation::StoreyLink { .. })
    }

    /// The element ends of an explicit relation, the first end (the
    /// dependent element, whose node stores the relation) first; `None` for
    /// a derived relation, whose second end is a storey.
    pub fn ends(&self) -> Option<(ElementId, ElementId)> {
        match *self {
            Relation::StoreyLink { .. } => None,
            Relation::Host { hosted, host } => Some((hosted, host)),
            Relation::Junction { a, b } => Some((a, b)),
            Relation::Trim { target, by } => Some((target, by)),
            Relation::SolidOperation { target, operator } => Some((target, operator)),
            Relation::AnnotationBinding { annotation, target } => Some((annotation, target)),
        }
    }

    /// The relation's identity: a version 5 UUID of its variant and its
    /// ends under [`RELATION_NAMESPACE`], so that recording the same
    /// relation again gives the same identity (IFX-010).
    pub fn id(&self) -> RelationId {
        let name = match *self {
            Relation::StoreyLink { element, storey, role } => {
                format!("{}/{element}/{storey}/{}", self.code(), role.code())
            }
            _ => {
                let (first, second) = self
                    .ends()
                    .unwrap_or_else(|| unreachable!("explicit relations have ends"));
                format!("{}/{first}/{second}", self.code())
            }
        };
        RelationId::from_uuid(Uuid::new_v5(&RELATION_NAMESPACE, name.as_bytes()))
    }

    /// True if the second end depends on the first as well as the first on
    /// the second (SRS 3.3): a host depends on the elements it hosts, which
    /// cut it, and the two elements of a junction are cleaned up together.
    /// A trimmed element, the target of a solid operation, an annotation and
    /// an element linked to a storey depend on the other end only.
    fn depends_both_ways(&self) -> bool {
        matches!(self, Relation::Host { .. } | Relation::Junction { .. })
    }
}

/// The derived storey links of `element` (STR-006): its base follows its
/// home storey, and its top each storey it is linked to by identity
/// ([`crate::kinds::Kind::storey_links`]). A top linked relative to the home
/// storey is no link of its own: it follows the home storey.
fn storey_links_of(element: &Element) -> Vec<(StoreyId, StoreyRole)> {
    std::iter::once((element.storey, StoreyRole::Base))
        .chain(
            element
                .kind
                .storey_links()
                .into_iter()
                .map(|link| (link.storey, StoreyRole::Top)),
        )
        .collect()
}

/// The home storey and the number of storeys of `element`'s top if it is
/// linked relative to the home storey (ELM-006, STR-006).
fn relative_top_of(element: &Element) -> Option<(StoreyId, i32)> {
    Some((element.storey, element.kind.relative_top()?))
}

/// What the store derives from its explicit relations and the elements
/// (SRS 3.3). Ordered maps, so that two indexes with the same content are
/// equal and the queries are deterministic; no entry is ever empty, so an
/// index kept edit by edit equals one rebuilt from scratch.
#[derive(Clone, Debug, Default, PartialEq)]
struct Index {
    /// The identities of the explicit relations each element is an end of.
    touching: BTreeMap<ElementId, BTreeSet<RelationId>>,
    /// The storey links of each element, its home storey first.
    links: BTreeMap<ElementId, Vec<(StoreyId, StoreyRole)>>,
    /// The elements with a storey link to each storey.
    linked: BTreeMap<StoreyId, BTreeSet<ElementId>>,
    /// The home storey and the number of storeys of each element whose top
    /// is linked relative to its home storey.
    relative_of: BTreeMap<ElementId, (StoreyId, i32)>,
    /// The elements whose top is linked relative to their home storey, by
    /// home storey and number of storeys.
    relative: BTreeMap<StoreyId, BTreeMap<i32, BTreeSet<ElementId>>>,
}

impl Index {
    fn add_relation(&mut self, id: RelationId, relation: &Relation) {
        if let Some((first, second)) = relation.ends() {
            for end in [first, second] {
                self.touching.entry(end).or_default().insert(id);
            }
        }
    }

    fn remove_relation(&mut self, id: RelationId, relation: &Relation) {
        if let Some((first, second)) = relation.ends() {
            for end in [first, second] {
                if let Some(ids) = self.touching.get_mut(&end) {
                    ids.remove(&id);
                    if ids.is_empty() {
                        self.touching.remove(&end);
                    }
                }
            }
        }
    }

    /// Replaces what the index holds of the element `element`: its storey
    /// links by `links` and its relative top by `relative`.
    fn set_element(
        &mut self,
        element: ElementId,
        links: Vec<(StoreyId, StoreyRole)>,
        relative: Option<(StoreyId, i32)>,
    ) {
        self.set_relative(element, relative);
        self.set_links(element, links);
    }

    /// Replaces the relative top of `element` (home storey and number of
    /// storeys) by `relative`.
    fn set_relative(&mut self, element: ElementId, relative: Option<(StoreyId, i32)>) {
        if let Some((home, n)) = self.relative_of.remove(&element)
            && let Some(by_n) = self.relative.get_mut(&home)
        {
            if let Some(elements) = by_n.get_mut(&n) {
                elements.remove(&element);
                if elements.is_empty() {
                    by_n.remove(&n);
                }
            }
            if by_n.is_empty() {
                self.relative.remove(&home);
            }
        }
        if let Some((home, n)) = relative {
            self.relative
                .entry(home)
                .or_default()
                .entry(n)
                .or_default()
                .insert(element);
            self.relative_of.insert(element, (home, n));
        }
    }

    /// Replaces the storey links of `element` by `links`.
    fn set_links(&mut self, element: ElementId, links: Vec<(StoreyId, StoreyRole)>) {
        for (storey, _) in self.links.remove(&element).unwrap_or_default() {
            if let Some(elements) = self.linked.get_mut(&storey) {
                elements.remove(&element);
                if elements.is_empty() {
                    self.linked.remove(&storey);
                }
            }
        }
        if links.is_empty() {
            return;
        }
        for (storey, _) in &links {
            self.linked.entry(*storey).or_default().insert(element);
        }
        self.links.insert(element, links);
    }
}

/// The relations of a project (SRS 3.3). It holds the explicit relations,
/// keyed by their identity, so that they iterate in one order whatever the
/// order they were recorded in and a file written from them is the same
/// (IFX-007); serialises as the list of them. Derived relations are not
/// stored: the store's index derives the storey links from the elements.
/// The store's equality compares the explicit relations only, so the
/// project's equality does too; [`RelationStore::same_index`] compares the
/// indexes.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(from = "Vec<Relation>", into = "Vec<Relation>")]
pub struct RelationStore {
    explicit: BTreeMap<RelationId, Relation>,
    /// Derived, so never serialised, also if the store stops serialising
    /// as its list of relations.
    #[serde(skip)]
    index: Index,
    /// The number of full rebuilds of the index, which a load does once.
    #[cfg(debug_assertions)]
    #[serde(skip)]
    rebuilds: usize,
}

impl PartialEq for RelationStore {
    /// Compares the explicit relations only: the index follows from them
    /// and the elements, which the project compares itself.
    fn eq(&self, other: &Self) -> bool {
        self.explicit == other.explicit
    }
}

impl RelationStore {
    /// Records an explicit relation. Returns whether it was new; a derived
    /// relation is not stored and gives `false`.
    pub fn insert(&mut self, relation: Relation) -> bool {
        if relation.is_derived() {
            return false;
        }
        let id = relation.id();
        if self.explicit.contains_key(&id) {
            return false;
        }
        self.explicit.insert(id, relation);
        self.index.add_relation(id, &relation);
        true
    }

    /// Removes the relation with the identity `id` and returns it.
    pub fn remove(&mut self, id: RelationId) -> Option<Relation> {
        let relation = self.explicit.remove(&id)?;
        self.index.remove_relation(id, &relation);
        Some(relation)
    }

    pub fn get(&self, id: RelationId) -> Option<&Relation> {
        self.explicit.get(&id)
    }

    pub fn contains(&self, id: RelationId) -> bool {
        self.explicit.contains_key(&id)
    }

    /// The explicit relations with their identities, in identity order.
    pub fn iter(&self) -> impl Iterator<Item = (RelationId, &Relation)> {
        self.explicit.iter().map(|(id, relation)| (*id, relation))
    }

    /// The explicit relations whose first end is `element`: the ones its
    /// file node stores (IFX-060), in identity order.
    pub fn starting_at(&self, element: ElementId) -> impl Iterator<Item = &Relation> {
        self.touching(element)
            .filter(move |r| r.ends().is_some_and(|(first, _)| first == element))
    }

    pub fn len(&self) -> usize {
        self.explicit.len()
    }

    pub fn is_empty(&self) -> bool {
        self.explicit.is_empty()
    }

    /// The identities of the explicit relations `element` is an end of, in
    /// identity order.
    pub(crate) fn ids_touching(&self, element: ElementId) -> impl Iterator<Item = RelationId> + '_ {
        self.index.touching.get(&element).into_iter().flatten().copied()
    }

    /// The explicit relations `element` is an end of, in identity order.
    fn touching(&self, element: ElementId) -> impl Iterator<Item = &Relation> {
        self.ids_touching(element).filter_map(|id| self.explicit.get(&id))
    }

    /// Every relation `element` takes part in (SRS 3.3): its explicit
    /// relations in identity order, then its derived storey links, the home
    /// storey first.
    pub fn relations_of(&self, element: ElementId) -> impl Iterator<Item = Relation> + '_ {
        let links = self.index.links.get(&element).into_iter().flatten();
        self.touching(element)
            .copied()
            .chain(links.map(move |&(storey, role)| Relation::StoreyLink { element, storey, role }))
    }

    /// The elements that depend directly on `element` through an explicit
    /// relation, which re-evaluating `element` affects (SRS 3.3; the closure
    /// over them is the caller's). A hosted element and its host depend on
    /// each other, as the hosted element is placed on its host and cuts it,
    /// and so do the two elements of a junction; an element trimmed by
    /// `element`, the target of a solid operation whose operator it is and
    /// an annotation that refers to it depend on it, and not the other way
    /// round. A storey link makes its element depend on a storey, which
    /// [`Self::storey_dependents`] answers. Never `element` itself.
    pub fn dependents_of(&self, element: ElementId) -> BTreeSet<ElementId> {
        let mut dependents = BTreeSet::new();
        for relation in self.touching(element) {
            let Some((first, second)) = relation.ends() else {
                continue;
            };
            if second == element {
                dependents.insert(first);
            }
            if first == element && relation.depends_both_ways() {
                dependents.insert(second);
            }
        }
        dependents.remove(&element);
        dependents
    }

    /// The elements that share an explicit relation with `element`,
    /// whichever way they depend on each other (SRS 3.3). Never `element`
    /// itself.
    pub fn neighbours(&self, element: ElementId) -> BTreeSet<ElementId> {
        let mut neighbours: BTreeSet<ElementId> = self
            .touching(element)
            .filter_map(Relation::ends)
            .flat_map(|(first, second)| [first, second])
            .collect();
        neighbours.remove(&element);
        neighbours
    }

    /// The host of `element` (OPN-001): the second end of the host relation
    /// whose hosted end it is. The commands give an element one host at most
    /// ([`crate::command::Command::InsertRelation`]); a file can name more,
    /// as long as the reader does not repair them (NFR-REL-004), and then
    /// this is the one first in identity order.
    pub fn host_of(&self, element: ElementId) -> Option<ElementId> {
        self.touching(element).find_map(|relation| match *relation {
            Relation::Host { hosted, host } if hosted == element => Some(host),
            _ => None,
        })
    }

    /// The elements that follow `storey` when it moves (STR-006, ELM-005,
    /// ELM-006): those homed on it and those whose top is linked to it by
    /// its identity. A top linked relative to the home storey ("home + 1")
    /// counts as a dependency on the home storey, whichever storey it
    /// reaches, also above the top storey, where
    /// [`crate::storey::StoreyList::relative_elevation`] extrapolates:
    /// which storey that is follows from the storey list, while the index
    /// follows from the element data alone, so that element edits keep it
    /// exact. For "home + 1" that is enough, as a storey edit that moves the
    /// elevation it reaches changes the home storey's elevation or height
    /// (the top storey's height being the nominal one the extrapolation
    /// repeats). A link further away also moves with the storeys between,
    /// while its home storey stays: [`Self::moved_by_storeys`] finds
    /// everything a storey edit moves.
    pub fn storey_dependents(&self, storey: StoreyId) -> BTreeSet<ElementId> {
        self.index.linked.get(&storey).cloned().unwrap_or_default()
    }

    /// The elements whose vertical extent can differ between the storey
    /// lists `before` and `after` (STR-006, ELM-005, ELM-006): what a
    /// storey edit, its undo or its redo moves, for checking the edit
    /// against the elements or evaluating them again. They are the storey
    /// dependents ([`Self::storey_dependents`]) of every storey whose
    /// elevation differs or that is in one list only, which holds every
    /// element whose base moves and every top linked to a storey by its
    /// identity, and the elements whose top is linked relative to their
    /// home storey where the elevation that link reaches differs
    /// ([`StoreyList::relative_elevation`], extrapolated past the top and
    /// the bottom storey): a link to a storey further than the next moves
    /// with the storeys between and with a change of the storey order,
    /// while the home storey can stay. A height alone moves nothing but the
    /// extrapolation past the top storey, which the second part compares.
    ///
    /// The elevations are compared exactly, so the result holds every
    /// element whose extent changes at all, and an element whose extent
    /// stays only if it is homed on or linked to a storey that moves and its
    /// other end moves alike. Either index, from before or after the edit,
    /// gives every element that is in the project before and after it, as a
    /// storey edit changes the links only of elements it keeps in place
    /// ([`crate::command::Command::DeleteStorey`]). The cost is the storeys
    /// of both lists and the distinct pairs of home storey and number of
    /// storeys of the relative links, not the elements.
    pub fn moved_by_storeys(&self, before: &StoreyList, after: &StoreyList) -> BTreeSet<ElementId> {
        let elevation = |list: &StoreyList, id: StoreyId| list.get(id).map(|s| s.elevation);
        let mut moved = BTreeSet::new();
        for id in before.iter().chain(after.iter()).map(|s| s.id) {
            if elevation(before, id) != elevation(after, id)
                && let Some(elements) = self.index.linked.get(&id)
            {
                moved.extend(elements);
            }
        }
        for (&home, by_n) in &self.index.relative {
            for (&n, elements) in by_n {
                if before.relative_elevation(home, n) != after.relative_elevation(home, n) {
                    moved.extend(elements);
                }
            }
        }
        moved
    }

    /// Replaces the derived storey links and the relative top of the
    /// element `id` by those of `element`, its new version, or drops them
    /// when the element is gone (`None`) (STR-006). Every edit that inserts, removes or replaces an
    /// element calls it ([`crate::command::Edit::apply`]), so the index stays
    /// exact through undo and redo at the cost of the elements an edit
    /// touches.
    pub fn update_element(&mut self, id: ElementId, element: Option<&Element>) {
        debug_assert!(element.is_none_or(|e| e.id == id), "the element's own id");
        self.index.set_element(
            id,
            element.map(storey_links_of).unwrap_or_default(),
            element.and_then(relative_top_of),
        );
    }

    /// Builds the index from scratch, from the explicit relations and
    /// `elements`, which are all the project's elements (SRS 3.3): once
    /// after a load, through [`crate::project::Project::rebuild_indexes`].
    pub fn rebuild<'a>(&mut self, elements: impl IntoIterator<Item = &'a Element>) {
        let mut index = Index::default();
        for (id, relation) in &self.explicit {
            index.add_relation(*id, relation);
        }
        for element in elements {
            index.set_element(element.id, storey_links_of(element), relative_top_of(element));
        }
        self.index = index;
        #[cfg(debug_assertions)]
        {
            self.rebuilds += 1;
        }
    }

    /// The number of times [`Self::rebuild`] ran on this store, in debug
    /// builds, for the test that a load rebuilds the index once.
    #[cfg(debug_assertions)]
    pub fn rebuild_count(&self) -> usize {
        self.rebuilds
    }

    /// True if the index equals `other`'s, which the store's equality leaves
    /// out: for checking that the index kept edit by edit is the one a
    /// rebuild from scratch gives.
    pub fn same_index(&self, other: &Self) -> bool {
        self.index == other.index
    }
}

impl From<Vec<Relation>> for RelationStore {
    fn from(relations: Vec<Relation>) -> Self {
        let mut store = Self::default();
        for relation in relations {
            store.insert(relation);
        }
        store
    }
}

impl From<RelationStore> for Vec<Relation> {
    fn from(store: RelationStore) -> Self {
        store.explicit.into_values().collect()
    }
}
