//! The slab generator (SLB-001, SLB-002, SLB-003): the outline with its
//! holes extruded between the slab's bottom and top.

use archicraft_model::Slab;

use super::{EvalContext, GeometryKind};
use crate::GeometryError;
use crate::evaluate::{BodyPart, ElementGeometry};
use crate::extrude::extrude;

impl GeometryKind for Slab {
    fn evaluate(&self, ctx: &EvalContext<'_>) -> Result<ElementGeometry, GeometryError> {
        let (bottom, top) = self.local_extent(ctx)?;
        let (outer, holes) = self.outline.to_points();
        let mesh = extrude(&outer, &holes, bottom, top)?;
        Ok(ElementGeometry {
            parts: vec![BodyPart {
                name: "Body".to_owned(),
                material: Some(self.material),
                mesh,
            }],
            axis: Vec::new(),
            children: Vec::new(),
        })
    }
}
