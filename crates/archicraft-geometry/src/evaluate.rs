//! Evaluation of element parameters into 3D bodies (WAL-027, IFX-033,
//! NFR-REL-006).
//!
//! Bodies are generated in the coordinates of the element's home storey: X
//! and Y as in the model, Z relative to the storey elevation. That is how they
//! are written to IFCX, below a storey node that carries the elevation
//! (IFX-031).
//!
//! The bodies of each kind come from its generator in [`crate::kinds`]. This
//! module is the generic path around them: the junction pre-pass for walls,
//! the extent check, one `catch_unwind` per element and the collection of
//! problems. An evaluation updates the geometry of an earlier revision,
//! evaluating again only what the edits since dirtied ([`crate::dirty`],
//! NFR-PERF-006).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use archicraft_model::{
    Element, ElementId, MODEL_TOLERANCE, MaterialId, Project, ProjectId, Rgb, StoreyId, StoreyList,
};
use glam::DVec3;

use crate::GeometryError;
use crate::contact::{ContactIndex, Junctions};
use crate::dirty::dirty_set;
use crate::kinds::{self, EvalContext};
use crate::mesh::TriMesh;
use crate::message::Message;
use crate::wall::Footprint;

pub use crate::kinds::wall::{wall_input, wall_inputs};

/// The colour of a body without a material, like a space's (SPC-001): a
/// light blue-grey. The 3D views draw such a body in it at
/// [`MATERIAL_LESS_OPACITY`], and IFCX writes both as the body's own
/// presentation instead of a material.
pub const MATERIAL_LESS_COLOR: Rgb = Rgb([0.72, 0.79, 0.86]);

/// The opacity of a body without a material: translucent, so that it never
/// hides what it holds.
pub const MATERIAL_LESS_OPACITY: f64 = 0.25;

/// One closed, single-material part of an element body (IFX-017).
#[derive(Clone, Debug, PartialEq)]
pub struct BodyPart {
    /// Child name in IFCX: `Body` for single-material bodies.
    pub name: String,
    /// `None` for a kind that is not built of a material, like a space
    /// (SPC-001): the plan cuts no region from it and it appears in
    /// [`MATERIAL_LESS_COLOR`] at [`MATERIAL_LESS_OPACITY`].
    pub material: Option<MaterialId>,
    pub mesh: TriMesh,
}

/// The bodies of one part of a compound element (IFX-016), in storey
/// coordinates like the element's own.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PartGeometry {
    /// The part's role in the model's `Kind::parts`, for example `Part_1`
    /// or `Flight_1`: the child node name in IFCX, stable per kind so that
    /// the part's id survives edits.
    pub role: String,
    pub parts: Vec<BodyPart>,
    /// Reference line of the part, if it has one; empty otherwise.
    pub axis: Vec<DVec3>,
}

/// The evaluated geometry of one element, in storey coordinates.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ElementGeometry {
    pub parts: Vec<BodyPart>,
    /// Reference line of walls (IFX-035); empty for other kinds.
    pub axis: Vec<DVec3>,
    /// The bodies of the parts of a compound element, one per entry of the
    /// model's `Kind::parts` in its order (IFX-016); empty for simple kinds.
    pub children: Vec<PartGeometry>,
}

impl ElementGeometry {
    /// The element's own body parts followed by those of its children:
    /// everything that is drawn, picked and measured (VIEW-001).
    pub fn bodies(&self) -> impl Iterator<Item = &BodyPart> {
        self.parts
            .iter()
            .chain(self.children.iter().flat_map(|child| child.parts.iter()))
    }

    pub fn bounds(&self) -> Option<(DVec3, DVec3)> {
        self.bodies()
            .filter_map(|p| p.mesh.bounds())
            .reduce(|(lo, hi), (l, h)| (lo.min(l), hi.max(h)))
    }

    pub fn volume(&self) -> f64 {
        self.bodies().map(|p| p.mesh.signed_volume()).sum()
    }
}

/// The evaluated geometry of a whole project.
///
/// An evaluation updates the geometry of an earlier revision of the project
/// ([`ModelGeometry::evaluate_incremental`]): it evaluates again only what
/// the edits since dirtied ([`crate::dirty`]), keeps everything else and
/// says what changed, so that the views can follow an edit within the
/// budget of NFR-PERF-006 (VIEW-001).
#[derive(Clone, Debug, Default)]
pub struct ModelGeometry {
    /// The project revision this geometry was evaluated from.
    pub revision: u64,
    /// The geometry of every element without a problem. Shared, so that an
    /// evaluation can keep the geometry of unchanged elements without copying
    /// it, and consumers can tell it is unchanged by pointer.
    pub elements: HashMap<ElementId, Arc<ElementGeometry>>,
    /// Elements whose geometry failed, with the reason as a `geometry-`
    /// message (UI-012, NFR-L10N-001).
    pub problems: BTreeMap<ElementId, Message>,
    /// The elements this evaluation evaluated again or dropped, in id
    /// order: what a consumer of the geometry it was updated from has to
    /// update (NFR-PERF-006). After a full evaluation, every element of the
    /// project and of that geometry.
    pub changed: BTreeSet<ElementId>,
    /// The storeys whose content this evaluation changed: the storeys the
    /// edits moved and the old and new home storeys of the elements in
    /// [`changed`](Self::changed). After a full evaluation, also every
    /// storey of the project and of the geometry it was updated from.
    pub changed_storeys: BTreeSet<StoreyId>,
    /// The walls of the evaluated project and their reference lines by
    /// storey, kept for the next evaluation (NFR-PERF-006).
    contacts: ContactIndex,
    /// The candidate tests of the last junction pass.
    junction_tests: usize,
    /// What this geometry was evaluated from, for the next evaluation;
    /// `None` if it never was.
    source: Option<Source>,
}

/// What a [`ModelGeometry`] was evaluated from besides its revision, which
/// the next evaluation compares the project with ([`crate::dirty`]).
#[derive(Clone, Debug)]
pub(crate) struct Source {
    pub(crate) project: ProjectId,
    /// The storey list: an element whose vertical extent the next storey
    /// list moves is dirty.
    pub(crate) storeys: StoreyList,
    /// The home storey of every element, for the storeys an element leaves
    /// when it is removed or moves to another storey.
    pub(crate) homes: HashMap<ElementId, StoreyId>,
}

impl ModelGeometry {
    /// Evaluates every element: [`ModelGeometry::evaluate_incremental`] from
    /// a geometry that was never evaluated. A failing element is listed in
    /// `problems` and does not stop the others (NFR-REL-006). Wall junctions
    /// are resolved through a [`ContactIndex`] of the walls (NFR-PERF-006).
    pub fn evaluate(project: &Project) -> Self {
        Self::evaluate_incremental(project, &Self::default())
    }

    /// The geometry of `project`, updated from `previous`, the geometry of
    /// an earlier revision of it (NFR-PERF-006, VIEW-001): only the elements
    /// of the [`dirty_set`] of the edits since are evaluated again, through
    /// their kind's generator, and only the dirty walls' junctions resolved
    /// again, end by end from the same candidates as in a full evaluation, so
    /// the result is a full evaluation's bit for bit. Every other element
    /// keeps its `Arc` and its problem; [`changed`](Self::changed) and
    /// [`changed_storeys`](Self::changed_storeys) say what changed.
    ///
    /// Everything is evaluated instead when `previous` cannot be updated
    /// ([`DirtySet::everything`](crate::DirtySet::everything)): it was never
    /// evaluated, it belongs to another project or to a newer revision, or an
    /// edit since changed the settings. The change log is not saved, so the
    /// first evaluation after a load is a full one. `previous` has to come
    /// from the same session as `project`: a project read again from its file
    /// starts its revisions over.
    pub fn evaluate_incremental(project: &Project, previous: &Self) -> Self {
        let dirty = dirty_set(project, previous);
        let (contacts, elements, problems, mut homes) = if dirty.everything {
            let contacts = ContactIndex::new(wall_inputs(project));
            let capacity = project.elements.len();
            (
                contacts,
                HashMap::with_capacity(capacity),
                BTreeMap::new(),
                HashMap::with_capacity(capacity),
            )
        } else {
            let mut contacts = previous.contacts.clone();
            for &id in &dirty.walls {
                contacts.remove(id);
                if let Some(wall) = project.element(id).and_then(|e| wall_input(project, e)) {
                    contacts.insert(wall);
                }
            }
            let homes = previous.source.as_ref().map(|s| s.homes.clone()).unwrap_or_default();
            (contacts, previous.elements.clone(), previous.problems.clone(), homes)
        };
        let Junctions {
            footprints,
            candidate_tests,
        } = contacts.resolve(dirty.elements.iter().copied());
        let mut geometry = Self {
            revision: project.revision,
            elements,
            problems,
            changed: BTreeSet::new(),
            changed_storeys: dirty.storeys,
            contacts,
            junction_tests: candidate_tests,
            source: None,
        };
        let old_homes = previous.source.as_ref().map(|s| &s.homes);
        for id in dirty.elements {
            geometry.elements.remove(&id);
            geometry.problems.remove(&id);
            geometry.changed_storeys.extend(old_homes.and_then(|old| old.get(&id)));
            match project.element(id) {
                Some(element) => {
                    geometry.store(project, element, footprints.get(&id));
                    homes.insert(id, element.storey);
                    geometry.changed_storeys.insert(element.storey);
                }
                None => {
                    homes.remove(&id);
                }
            }
            geometry.changed.insert(id);
        }
        geometry.source = Some(Source {
            project: project.id,
            storeys: project.storeys.clone(),
            homes,
        });
        geometry
    }

    /// Evaluates `element`, a wall with its junction-resolved `footprint`,
    /// into [`Self::elements`] or, if it fails or panics, [`Self::problems`]
    /// (NFR-REL-006).
    fn store(&mut self, project: &Project, element: &Element, footprint: Option<&Footprint>) {
        let result = catch_unwind(AssertUnwindSafe(|| evaluate_element(project, element, footprint)));
        match result {
            Ok(Ok(geometry)) => {
                self.elements.insert(element.id, Arc::new(geometry));
            }
            Ok(Err(e)) => {
                tracing::warn!(element = %element.id, name = %element.name, "geometry failed: {e}");
                self.problems.insert(element.id, e.message());
            }
            Err(_) => {
                tracing::error!(element = %element.id, name = %element.name, "geometry evaluation panicked");
                self.problems
                    .insert(element.id, Message::new("geometry-evaluation-panicked"));
            }
        }
    }

    pub fn get(&self, id: ElementId) -> Option<&ElementGeometry> {
        self.elements.get(&id).map(Arc::as_ref)
    }

    /// What this geometry was evaluated from; `None` if it never was.
    pub(crate) fn source(&self) -> Option<&Source> {
        self.source.as_ref()
    }

    /// The walls this geometry was evaluated from, indexed for contact
    /// queries.
    pub fn contacts(&self) -> &ContactIndex {
        &self.contacts
    }

    /// How often the junction pass of this evaluation tested a wall end
    /// against another wall (see [`Junctions::candidate_tests`]).
    pub fn junction_candidate_tests(&self) -> usize {
        self.junction_tests
    }
}

/// Evaluates one element through its kind's generator. `footprint` is the
/// junction-resolved footprint of a wall, if available. An element of a kind
/// without a body, such as a text or a dimension, evaluates to an empty
/// geometry without asking its generator: no part, no axis and no problem
/// (ANN-002). An element whose height is below the model tolerance
/// (NFR-ACC-003) has no body. Validation keeps elements at least 1 mm high,
/// and storey commands refuse to make a linked element lower (NFR-REL-005),
/// but a storey list changed outside the commands can still shorten them, so
/// such heights do reach this point (tests/storey.rs).
pub fn evaluate_element(
    project: &Project,
    element: &Element,
    footprint: Option<&Footprint>,
) -> Result<ElementGeometry, GeometryError> {
    if !element.kind.descriptor().has_body {
        return Ok(ElementGeometry::default());
    }
    let ctx = EvalContext {
        project,
        element,
        footprint,
    };
    let (bottom, top) = kinds::local_extent(&ctx)?;
    if top - bottom < MODEL_TOLERANCE {
        return Err(GeometryError::InvalidExtent);
    }
    kinds::evaluate(&ctx)
}
