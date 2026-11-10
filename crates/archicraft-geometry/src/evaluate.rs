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
//! problems.

use std::collections::{BTreeMap, HashMap};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use archicraft_model::{Element, ElementId, MODEL_TOLERANCE, MaterialId, Project, Rgb};
use glam::DVec3;

use crate::GeometryError;
use crate::contact::{ContactIndex, Junctions};
use crate::kinds::{self, EvalContext};
use crate::mesh::TriMesh;
use crate::message::Message;
use crate::wall::Footprint;

pub use crate::kinds::wall::wall_inputs;

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
    /// The walls of the evaluated project and their reference lines by
    /// storey, kept for the next evaluation (NFR-PERF-006).
    contacts: ContactIndex,
    /// The candidate tests of the last junction pass.
    junction_tests: usize,
}

impl ModelGeometry {
    /// Evaluates every element. A failing element is listed in `problems`
    /// and does not stop the others (NFR-REL-006). Wall junctions are
    /// resolved through a [`ContactIndex`] of the walls (NFR-PERF-006).
    pub fn evaluate(project: &Project) -> Self {
        let contacts = ContactIndex::new(wall_inputs(project));
        let Junctions {
            footprints: wall_footprints,
            candidate_tests,
        } = contacts.resolve_all();
        let mut elements = HashMap::with_capacity(project.elements.len());
        let mut problems = BTreeMap::new();
        for element in project.elements.values() {
            let result = catch_unwind(AssertUnwindSafe(|| {
                evaluate_element(project, element, wall_footprints.get(&element.id))
            }));
            match result {
                Ok(Ok(geometry)) => {
                    elements.insert(element.id, Arc::new(geometry));
                }
                Ok(Err(e)) => {
                    tracing::warn!(element = %element.id, name = %element.name, "geometry failed: {e}");
                    problems.insert(element.id, e.message());
                }
                Err(_) => {
                    tracing::error!(element = %element.id, name = %element.name, "geometry evaluation panicked");
                    problems.insert(element.id, Message::new("geometry-evaluation-panicked"));
                }
            }
        }
        Self {
            revision: project.revision,
            elements,
            problems,
            contacts,
            junction_tests: candidate_tests,
        }
    }

    pub fn get(&self, id: ElementId) -> Option<&ElementGeometry> {
        self.elements.get(&id).map(Arc::as_ref)
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
