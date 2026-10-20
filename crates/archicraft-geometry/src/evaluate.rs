//! Evaluation of element parameters into 3D bodies (WAL-027, IFX-033,
//! NFR-REL-006).
//!
//! Bodies are generated in the coordinates of the element's home storey: X
//! and Y as in the model, Z relative to the storey elevation. That is how they
//! are written to IFCX, below a storey node that carries the elevation
//! (IFX-031).

use std::collections::{BTreeMap, HashMap};
use std::panic::{AssertUnwindSafe, catch_unwind};

use archicraft_model::validate::validate_ring;
use archicraft_model::{Element, ElementId, ElementKind, MODEL_TOLERANCE, MaterialId, Project};
use glam::{DVec2, DVec3};

use crate::GeometryError;
use crate::extrude::extrude;
use crate::mesh::TriMesh;
use crate::wall::{Footprint, WallInput, footprints};

/// One closed, single-material part of an element body (IFX-017).
#[derive(Clone, Debug, PartialEq)]
pub struct BodyPart {
    /// Child name in IFCX: `Body` for single-material bodies.
    pub name: String,
    pub material: MaterialId,
    pub mesh: TriMesh,
}

/// The evaluated geometry of one element, in storey coordinates.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ElementGeometry {
    pub parts: Vec<BodyPart>,
    /// Reference line of walls (IFX-035); empty for other kinds.
    pub axis: Vec<DVec3>,
}

impl ElementGeometry {
    pub fn bounds(&self) -> Option<(DVec3, DVec3)> {
        self.parts
            .iter()
            .filter_map(|p| p.mesh.bounds())
            .reduce(|(lo, hi), (l, h)| (lo.min(l), hi.max(h)))
    }

    pub fn volume(&self) -> f64 {
        self.parts.iter().map(|p| p.mesh.signed_volume()).sum()
    }
}

/// The evaluated geometry of a whole project.
#[derive(Clone, Debug, Default)]
pub struct ModelGeometry {
    /// The project revision this geometry was evaluated from.
    pub revision: u64,
    pub elements: HashMap<ElementId, ElementGeometry>,
    /// Elements whose geometry failed, with the reason (UI-012).
    pub problems: BTreeMap<ElementId, String>,
}

impl ModelGeometry {
    /// Evaluates every element. A failing element is listed in `problems`
    /// and does not stop the others (NFR-REL-006).
    pub fn evaluate(project: &Project) -> Self {
        let wall_footprints = footprints(&wall_inputs(project));
        let mut elements = HashMap::with_capacity(project.elements.len());
        let mut problems = BTreeMap::new();
        for element in project.elements.values() {
            let result = catch_unwind(AssertUnwindSafe(|| {
                evaluate_element(project, element, wall_footprints.get(&element.id))
            }));
            match result {
                Ok(Ok(geometry)) => {
                    elements.insert(element.id, geometry);
                }
                Ok(Err(e)) => {
                    tracing::warn!(element = %element.id, name = %element.name, "geometry failed: {e}");
                    problems.insert(element.id, e.to_string());
                }
                Err(_) => {
                    tracing::error!(element = %element.id, name = %element.name, "geometry evaluation panicked");
                    problems.insert(element.id, "Geometry evaluation failed unexpectedly.".to_owned());
                }
            }
        }
        Self {
            revision: project.revision,
            elements,
            problems,
        }
    }

    pub fn get(&self, id: ElementId) -> Option<&ElementGeometry> {
        self.elements.get(&id)
    }
}

/// Wall data for junction resolution.
pub fn wall_inputs(project: &Project) -> Vec<WallInput> {
    project
        .elements
        .values()
        .filter_map(|e| {
            let wall = e.as_wall()?;
            let (bottom, top) = project.vertical_extent(e)?;
            Some(WallInput {
                id: e.id,
                storey: e.storey,
                start: wall.start,
                end: wall.end,
                thickness: wall.thickness,
                reference_line: wall.reference_line,
                bottom,
                top,
            })
        })
        .collect()
}

/// Bottom and top of an element relative to its home storey.
fn local_extent(project: &Project, element: &Element) -> Result<(f64, f64), GeometryError> {
    let home = project
        .storeys
        .get(element.storey)
        .ok_or(GeometryError::MissingReference("home storey"))?
        .elevation;
    let (bottom, top) = project
        .vertical_extent(element)
        .ok_or(GeometryError::MissingReference("linked storey"))?;
    match &element.kind {
        // Keep the stored offsets exact rather than subtracting elevations.
        ElementKind::Wall(wall) => Ok((wall.base_offset, wall.base_offset + (top - bottom))),
        ElementKind::Slab(_) => Ok((bottom - home, top - home)),
    }
}

fn square_footprint(start: DVec2, end: DVec2, offsets: (f64, f64)) -> Footprint {
    let normal = (end - start).normalize_or_zero().perp();
    let (ext, int) = offsets;
    [
        start + normal * ext,
        end + normal * ext,
        end + normal * int,
        start + normal * int,
    ]
}

/// Evaluates one element. `footprint` is the junction-resolved footprint of a
/// wall, if available. An element whose height is below the model tolerance
/// (NFR-ACC-003) has no body. Validation keeps elements at least 1 mm high,
/// but moving a storey shortens the walls linked to it without validating
/// them again, so such heights do reach this point (tests/storey.rs).
pub fn evaluate_element(
    project: &Project,
    element: &Element,
    footprint: Option<&Footprint>,
) -> Result<ElementGeometry, GeometryError> {
    let (bottom, top) = local_extent(project, element)?;
    if top - bottom < MODEL_TOLERANCE {
        return Err(GeometryError::InvalidExtent);
    }
    match &element.kind {
        ElementKind::Wall(wall) => {
            let square = square_footprint(wall.start, wall.end, wall.reference_line.face_offsets(wall.thickness));
            let joined = footprint.copied().unwrap_or(square);
            // A junction can fold a very short wall; fall back to square ends.
            let ring = validate_ring(&joined)
                .or_else(|_| validate_ring(&square))
                .map_err(GeometryError::InvalidPolygon)?;
            let mesh = extrude(&ring, &[], bottom, top)?;
            Ok(ElementGeometry {
                parts: vec![BodyPart {
                    name: "Body".to_owned(),
                    material: wall.material,
                    mesh,
                }],
                axis: vec![wall.start.extend(bottom), wall.end.extend(bottom)],
            })
        }
        ElementKind::Slab(slab) => {
            let mesh = extrude(&slab.outline, &slab.holes, bottom, top)?;
            Ok(ElementGeometry {
                parts: vec![BodyPart {
                    name: "Body".to_owned(),
                    material: slab.material,
                    mesh,
                }],
                axis: Vec::new(),
            })
        }
    }
}
