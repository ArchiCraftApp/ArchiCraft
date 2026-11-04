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
//! [`TopLink`]: crate::element::TopLink

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ids::{ElementId, RelationId, StoreyId};

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
}

/// The explicit relations of a project (SRS 3.3), keyed by their identity,
/// so that they iterate in one order whatever the order they were recorded
/// in and a file written from them is the same (IFX-007). Derived relations
/// are not stored. Serialises as the list of its relations.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(from = "Vec<Relation>", into = "Vec<Relation>")]
pub struct RelationStore {
    explicit: BTreeMap<RelationId, Relation>,
}

impl RelationStore {
    /// Records an explicit relation. Returns whether it was new; a derived
    /// relation is not stored and gives `false`.
    pub fn insert(&mut self, relation: Relation) -> bool {
        if relation.is_derived() {
            return false;
        }
        self.explicit.insert(relation.id(), relation).is_none()
    }

    /// Removes the relation with the identity `id` and returns it.
    pub fn remove(&mut self, id: RelationId) -> Option<Relation> {
        self.explicit.remove(&id)
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
    /// file node stores (IFX-060).
    pub fn starting_at(&self, element: ElementId) -> impl Iterator<Item = &Relation> {
        self.explicit
            .values()
            .filter(move |r| r.ends().is_some_and(|(first, _)| first == element))
    }

    pub fn len(&self) -> usize {
        self.explicit.len()
    }

    pub fn is_empty(&self) -> bool {
        self.explicit.is_empty()
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
