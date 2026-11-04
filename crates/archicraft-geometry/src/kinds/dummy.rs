//! The generator of the test-only element kind, behind the cargo feature
//! `test-kinds` (see [`crate::kinds`]): a box over its plan rectangle
//! between its base and top. Asked for parts, it adds one box per part the
//! model declares, stacked on top of the body and half as high, each over an
//! equal strip of the rectangle along X (IFX-016). Asked to panic, it
//! panics, which the evaluation contains per element (NFR-REL-006).

use archicraft_model::MaterialId;
use archicraft_model::kinds::Dummy;
use glam::DVec2;

use super::{EvalContext, GeometryKind};
use crate::GeometryError;
use crate::evaluate::{BodyPart, ElementGeometry, PartGeometry};
use crate::extrude::extrude;

/// A box over the plan rectangle from `lo` to `hi` between `bottom` and
/// `top`.
fn body(lo: DVec2, hi: DVec2, bottom: f64, top: f64, material: Option<MaterialId>) -> Result<BodyPart, GeometryError> {
    let ring = [lo, DVec2::new(hi.x, lo.y), hi, DVec2::new(lo.x, hi.y)];
    Ok(BodyPart {
        name: "Body".to_owned(),
        material,
        mesh: extrude(&ring, &[], bottom, top)?,
    })
}

impl GeometryKind for Dummy {
    fn evaluate(&self, ctx: &EvalContext<'_>) -> Result<ElementGeometry, GeometryError> {
        if self.panic_on_evaluate {
            panic!("the test kind was asked to panic");
        }
        let (bottom, top) = self.local_extent(ctx)?;
        let far = self.origin + self.size;
        let specs = ctx.element.kind.parts();
        let strips = specs.len() as f64;
        let children = specs
            .into_iter()
            .enumerate()
            .map(|(i, spec)| {
                let x0 = self.origin.x + self.size.x * i as f64 / strips;
                let x1 = self.origin.x + self.size.x * (i + 1) as f64 / strips;
                let lo = DVec2::new(x0, self.origin.y);
                let hi = DVec2::new(x1, far.y);
                Ok(PartGeometry {
                    role: spec.role,
                    parts: vec![body(lo, hi, top, top + (top - bottom) / 2.0, self.material)?],
                    axis: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>, GeometryError>>()?;
        Ok(ElementGeometry {
            parts: vec![body(self.origin, far, bottom, top, self.material)?],
            axis: Vec::new(),
            children,
        })
    }
}
