//! The wall generator (WAL-027, IFX-035): the junction-resolved footprint
//! extruded between the wall's base and top, with the reference line as the
//! axis.

use archicraft_model::validate::validate_ring;
use archicraft_model::{Element, Message, Project, Wall};
use glam::DVec2;

use super::{EvalContext, GeometryKind};
use crate::GeometryError;
use crate::evaluate::{BodyPart, ElementGeometry};
use crate::extrude::extrude;
use crate::wall::{Footprint, WallInput};

/// Wall data for junction resolution ([`crate::wall::footprints`]), which
/// runs before the generators and hands each wall its footprint through
/// [`EvalContext::footprint`].
pub fn wall_inputs(project: &Project) -> Vec<WallInput> {
    project
        .elements
        .values()
        .filter_map(|e| wall_input(project, e))
        .collect()
}

/// The junction input of `element`: `None` unless it is a wall whose home
/// and linked storeys are in the project.
pub fn wall_input(project: &Project, element: &Element) -> Option<WallInput> {
    let wall = element.as_wall()?;
    let (bottom, top) = project.vertical_extent(element)?;
    Some(WallInput {
        id: element.id,
        storey: element.storey,
        start: wall.start,
        end: wall.end,
        thickness: wall.thickness,
        reference_line: wall.reference_line,
        bottom,
        top,
    })
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

/// The footprint the body of `wall` is extruded from (WAL-027): its
/// junction-resolved footprint `joined` ([`crate::wall::footprints`]), or
/// its footprint with square ends if it has none or a junction folds a very
/// short wall into a ring that is no valid polygon. The ring is cleaned and
/// counter-clockwise ([`validate_ring`]). Detected space boundaries follow
/// it (SPC-001), so that they agree with the plan and the 3D views.
///
/// # Errors
///
/// The message of [`validate_ring`] if the square footprint is needed and is
/// no valid polygon.
pub fn body_footprint(wall: &Wall, joined: Option<&Footprint>) -> Result<Vec<DVec2>, Message> {
    let square = || {
        validate_ring(&square_footprint(
            wall.start,
            wall.end,
            wall.reference_line.face_offsets(wall.thickness),
        ))
    };
    match joined {
        Some(joined) => validate_ring(joined).or_else(|_| square()),
        None => square(),
    }
}

impl GeometryKind for Wall {
    /// Keeps the stored offsets exact rather than subtracting elevations.
    fn local_extent(&self, ctx: &EvalContext<'_>) -> Result<(f64, f64), GeometryError> {
        let (bottom, top) = ctx.vertical_extent()?;
        Ok((self.base_offset, self.base_offset + (top - bottom)))
    }

    fn evaluate(&self, ctx: &EvalContext<'_>) -> Result<ElementGeometry, GeometryError> {
        let (bottom, top) = self.local_extent(ctx)?;
        let ring = body_footprint(self, ctx.footprint).map_err(GeometryError::InvalidPolygon)?;
        let mesh = extrude(&ring, &[], bottom, top)?;
        Ok(ElementGeometry {
            parts: vec![BodyPart {
                name: "Body".to_owned(),
                material: Some(self.material),
                mesh,
            }],
            axis: vec![self.start.extend(bottom), self.end.extend(bottom)],
            children: Vec::new(),
        })
    }
}
