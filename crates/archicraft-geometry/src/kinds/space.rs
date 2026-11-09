//! The space generator (SPC-001): the boundary with its holes extruded
//! between the space's base and its storey-linked top, one body without a
//! material, which the 3D views draw translucent. Spaces are no
//! construction elements, so the plan neither cuts nor projects the body.

use archicraft_model::Space;

use super::{EvalContext, GeometryKind};
use crate::GeometryError;
use crate::evaluate::{BodyPart, ElementGeometry};
use crate::extrude::extrude;

impl GeometryKind for Space {
    fn evaluate(&self, ctx: &EvalContext<'_>) -> Result<ElementGeometry, GeometryError> {
        let (bottom, top) = self.local_extent(ctx)?;
        let (outer, holes) = self.boundary.to_points();
        let mesh = extrude(&outer, &holes, bottom, top)?;
        Ok(ElementGeometry {
            parts: vec![BodyPart {
                name: "Body".to_owned(),
                material: None,
                mesh,
            }],
            axis: Vec::new(),
            children: Vec::new(),
        })
    }
}
