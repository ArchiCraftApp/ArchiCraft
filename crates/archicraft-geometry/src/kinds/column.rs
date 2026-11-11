//! The column generator (COL-001, COL-003, IFX-035): the cross-section,
//! anchored at the column's position and turned by its rotation, swept up
//! the vertical axis between the two end cuts, with the axis from the bottom
//! cut to the top cut.

use archicraft_model::kinds::{Column, ColumnCut, CrossSection};
use archicraft_model::profile::ParametricShape;
use glam::{DVec2, DVec3};

use super::{EvalContext, GeometryKind};
use crate::GeometryError;
use crate::evaluate::{BodyPart, ElementGeometry};
use crate::section::{Section, parametric_section};
use crate::sweep::{CutPlane, MemberFrame, frame_for_column, sweep};

/// The column's section in section coordinates, centred on its bounding
/// box and made of the column's material.
fn section(column: &Column, ctx: &EvalContext<'_>) -> Result<Section, GeometryError> {
    let shape = match column.section {
        CrossSection::Rectangular { width, height } => ParametricShape::Rectangle { width, height },
        CrossSection::Circular { diameter } => ParametricShape::Circle { diameter },
    };
    parametric_section(&shape, column.material, &ctx.project.settings.tessellation)
}

/// The plane of the end cut `cut` through the axis at `z` (COL-003): it
/// rises by the cut's slope per metre along the frame's `v`, so its normal
/// is `Z - slope · v`.
fn cut_plane(cut: ColumnCut, frame: &MemberFrame, z: f64) -> CutPlane {
    CutPlane {
        point: frame.point(DVec2::ZERO, z),
        normal: DVec3::Z - frame.v * cut.slope(),
    }
}

impl GeometryKind for Column {
    /// Keeps the stored base offset exact rather than subtracting
    /// elevations, as walls do.
    fn local_extent(&self, ctx: &EvalContext<'_>) -> Result<(f64, f64), GeometryError> {
        let (bottom, top) = ctx.vertical_extent()?;
        let base = self.link.base_offset;
        Ok((base, base + (top - bottom)))
    }

    /// One part `Body` swept from the section moved so that its anchor
    /// point lies on the axis, in the column's frame at its position on the
    /// storey's floor (`frame_for_column`), between the bottom cut at the
    /// base and the top cut at the top. The axis joins the points where the
    /// anchor axis meets the two cuts.
    fn evaluate(&self, ctx: &EvalContext<'_>) -> Result<ElementGeometry, GeometryError> {
        let (bottom, top) = self.local_extent(ctx)?;
        let section = section(self, ctx)?.transformed(-self.anchor_point(), 0.0);
        let frame = frame_for_column(self.position.extend(0.0), self.rotation);
        let regions = sweep(
            &section,
            &frame,
            &cut_plane(self.bottom_cut, &frame, bottom),
            &cut_plane(self.top_cut, &frame, top),
        )?;
        Ok(ElementGeometry {
            parts: regions
                .into_iter()
                .map(|region| BodyPart {
                    name: region.role,
                    material: Some(region.material),
                    mesh: region.mesh,
                })
                .collect(),
            axis: vec![self.position.extend(bottom), self.position.extend(top)],
            children: Vec::new(),
        })
    }
}
