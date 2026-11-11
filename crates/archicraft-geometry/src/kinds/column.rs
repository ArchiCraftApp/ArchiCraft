//! The column generator (COL-001, COL-003, PRF-004, IFX-035): the
//! cross-section, anchored at the column's position and turned by its
//! rotation, swept up the vertical axis between the two end cuts, with the
//! axis from the bottom cut to the top cut.
//!
//! The generator reads only its element, the storeys and the profile of a
//! profile section. An edit of that profile stamps the columns that use it
//! in the project's change log, so incremental evaluation re-evaluates them
//! (PRF-004: editing a profile updates every element that uses it).

use archicraft_model::kinds::{Column, ColumnCut, CrossSection};
use archicraft_model::message::Message;
use archicraft_model::profile::ParametricShape;
use archicraft_model::{Element, Project};
use glam::{DVec2, DVec3};

use super::{EvalContext, GeometryKind};
use crate::GeometryError;
use crate::evaluate::{BodyPart, ElementGeometry};
use crate::section::{Section, parametric_section, profile_section};
use crate::sweep::{CutPlane, MemberFrame, frame_for_column, sweep};

/// The column's section in section coordinates and its anchor point there:
/// a rectangle or circle centred on its bounding box and made of the
/// column's material, or the column's profile through [`profile_section`],
/// in profile coordinates with the profile's region roles and materials and
/// anchored at its insertion origin for `Anchor::ProfileOrigin`.
fn section(column: &Column, project: &Project) -> Result<(Section, DVec2), GeometryError> {
    let tessellation = &project.settings.tessellation;
    let missing_profile = || GeometryError::InvalidPolygon(Message::new("model-error-element-profile-missing"));
    let (section, profile) = match column.section {
        CrossSection::Rectangular { width, height } => (
            parametric_section(&ParametricShape::Rectangle { width, height }, column.material, tessellation)?,
            None,
        ),
        CrossSection::Circular { diameter } => (
            parametric_section(&ParametricShape::Circle { diameter }, column.material, tessellation)?,
            None,
        ),
        CrossSection::Profile(_) => {
            let profile = column.profile(project).ok_or_else(missing_profile)?;
            (profile_section(profile, tessellation)?, Some(profile))
        }
    };
    let anchor = column.anchor_point(profile).ok_or_else(missing_profile)?;
    Ok((section, anchor))
}

/// The plan footprint of `element` if it is a column (SPC-001): the outer
/// loop of every region of its section, the same section its body is swept
/// from, in plan with its anchor point at the column's position and turned
/// by its rotation, counter-clockwise. A column stands vertical, so this is
/// its outline at every height whatever its end cuts. The holes of a
/// hollow section are left out: they lie inside the column. `None` for an
/// element that is no column and for a column without a section, such as
/// one whose profile is missing.
pub fn plan_footprint(project: &Project, element: &Element) -> Option<Vec<Vec<DVec2>>> {
    let column = element.as_column()?;
    let (section, anchor) = section(column, project).ok()?;
    Some(
        section
            .regions
            .iter()
            .map(|region| region.outer.points.iter().map(|&p| column.to_plan(anchor, p)).collect())
            .collect(),
    )
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

    /// One part `Body`, or `Region_1`, `Region_2`, … for a profile of
    /// several regions, swept from the section moved so that its anchor
    /// point lies on the axis, in the column's frame at its position on the
    /// storey's floor (`frame_for_column`), between the bottom cut at the
    /// base and the top cut at the top. The axis joins the points where the
    /// anchor axis meets the two cuts.
    fn evaluate(&self, ctx: &EvalContext<'_>) -> Result<ElementGeometry, GeometryError> {
        let (bottom, top) = self.local_extent(ctx)?;
        let (section, anchor) = section(self, ctx.project)?;
        let section = section.transformed(-anchor, 0.0);
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
