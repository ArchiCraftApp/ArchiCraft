//! Columns (SRS 4.13; COL-001, COL-003).
//!
//! A column stands on its home storey with its axis vertical through
//! [`Column::position`], the [`Anchor`] point of its cross-section, which is
//! turned by [`Column::rotation`] about the axis. Its base and top are a
//! [`VerticalLink`] (ELM-006), so a top linked to a storey follows storey
//! edits (STR-006). Each end is cut horizontally or by a plane at an angle
//! ([`ColumnCut`]); the geometry crate sweeps the section between the two
//! planes.
//!
//! **Plan coordinates.** A section point `p` (section coordinates: centred
//! on its bounding box for a rectangle or circle, the profile's coordinates
//! for a profile; see [`crate::profile`]) lies in plan at
//! `position + rotate(p - anchor point, rotation)`: with no rotation the
//! section's x and y run along the world's X and Y.
//!
//! **Profile sections (PRF-004).** A column with [`CrossSection::Profile`]
//! refers to the profile by ID, so its size and its anchor point are the
//! project's: the methods that need them take the profile
//! ([`Column::profile`] looks it up). The [`Kind::plan_bounds`] hook does not
//! see the project, so for a profile column it gives the axis point only, and
//! [`Project::plan_bounds`] gives the bounds of the profile's box in plan,
//! which a profile edit moves without touching the column. The column's own
//! material stays when a profile is chosen, so switching back to a
//! rectangle or circle keeps it; the profile's regions bring their own.

use glam::DVec2;
use serde::{Deserialize, Serialize};

use super::member::{Anchor, CrossSection};
use super::{Kind, KindDescriptor, KindTag, StoreyLinkRef, Vertical, VerticalLink, plan_bounds_of, vertical};
use crate::element::{Element, IfcClass, Position, PredefinedType, StructuralFunction, TopLink};
use crate::fields::FieldSpec;
use crate::ids::{MaterialId, ProfileId, StoreyId};
use crate::message::Message;
use crate::profile::{Profile, ProfileUsage};
use crate::project::{DefaultsSlot, Project};
use crate::storey::StoreyList;
use crate::template::TemplateContext;
use crate::transform::{PlanTransform, TransformError};
use crate::validate::{self, MAX_HEIGHT, MAX_THICKNESS, MIN_HEIGHT, MIN_THICKNESS};

/// The largest angle of an end cut, exclusive, in degrees. The SRS gives no
/// limit (COL-003); 60° keeps the plane well away from the axis.
pub const MAX_CUT_ANGLE_DEG: f64 = 60.0;

/// An end cut of a column (COL-003).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum ColumnCut {
    /// A horizontal plane, the default.
    #[default]
    Horizontal,
    /// A plane tilted by the angle in radians about the column's local x
    /// axis (the section's x axis after the rotation): it rises by
    /// `tan(angle)` per metre along the local y axis. Less than
    /// [`MAX_CUT_ANGLE_DEG`] either way. The same angle at both ends cuts
    /// parallel planes.
    Angle(f64),
}

impl ColumnCut {
    /// How far the plane rises per metre along the column's local y axis:
    /// zero for a horizontal cut.
    pub fn slope(self) -> f64 {
        match self {
            ColumnCut::Horizontal => 0.0,
            ColumnCut::Angle(angle) => angle.tan(),
        }
    }

    /// The cut of the mirror image: a mirror reverses the local y axis, so
    /// the plane tilts the other way.
    fn mirrored(self) -> Self {
        match self {
            ColumnCut::Horizontal => ColumnCut::Horizontal,
            ColumnCut::Angle(angle) => ColumnCut::Angle(-angle),
        }
    }
}

/// A column (COL-001, COL-003). See the module documentation for the plan
/// coordinates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Column {
    /// The anchor point in plan: where the axis stands.
    pub position: DVec2,
    pub section: CrossSection,
    pub material: MaterialId,
    /// The point of the section the axis runs through.
    pub anchor: Anchor,
    /// The turn of the section about the axis, in radians counter-clockwise.
    pub rotation: f64,
    pub link: VerticalLink,
    pub top_cut: ColumnCut,
    pub bottom_cut: ColumnCut,
}

impl Column {
    /// The profile of a profile section in `project`, if it has it.
    pub fn profile<'p>(&self, project: &'p Project) -> Option<&'p Profile> {
        project.profile(self.section.profile()?)
    }

    /// The section's bounding box in section coordinates as (min, max). For
    /// a profile section it is the bounds of `profile`, the column's profile
    /// ([`Column::profile`]), and `None` without it; `profile` is ignored for
    /// other sections.
    pub fn section_bounds(&self, profile: Option<&Profile>) -> Option<(DVec2, DVec2)> {
        match self.section {
            CrossSection::Profile(_) => profile?.bounds(),
            _ => self.section.bounds(),
        }
    }

    /// The anchor point in section coordinates: the anchor's point of the
    /// section's bounding box, or the insertion origin of `profile` for
    /// [`Anchor::ProfileOrigin`] (PRF-004). `profile` is as for
    /// [`Column::section_bounds`].
    pub fn anchor_point(&self, profile: Option<&Profile>) -> Option<DVec2> {
        match (self.section, self.anchor) {
            (CrossSection::Profile(_), Anchor::ProfileOrigin) => profile.map(|p| p.origin),
            _ => Some(self.anchor.point(self.section_bounds(profile)?)),
        }
    }

    /// The plan point of the section point `p` (section coordinates), for
    /// the anchor point `anchor` ([`Column::anchor_point`]).
    pub fn to_plan(&self, anchor: DVec2, p: DVec2) -> DVec2 {
        self.position + DVec2::from_angle(self.rotation).rotate(p - anchor)
    }

    /// Smallest and largest plan coordinates of the section: the rotated
    /// corners of its bounding box, or a circle's centre plus and minus its
    /// radius. `profile` is as for [`Column::section_bounds`]; `None` for a
    /// profile section without it.
    pub fn plan_bounds_with(&self, profile: Option<&Profile>) -> Option<(DVec2, DVec2)> {
        let anchor = self.anchor_point(profile)?;
        let (min, max) = self.section_bounds(profile)?;
        if let CrossSection::Circular { diameter } = self.section {
            let centre = self.to_plan(anchor, DVec2::ZERO);
            let radius = DVec2::splat(diameter / 2.0);
            return Some((centre - radius, centre + radius));
        }
        plan_bounds_of([min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)].map(|p| self.to_plan(anchor, p)))
    }
}

/// The column tool's defaults in the template (Appendix D.2): 0.30 × 0.30 m
/// reinforced concrete on the structure layer, anchored at the centre, cut
/// horizontally at both ends, the top linked to the home storey + 1.
fn template_defaults(context: &TemplateContext<'_>) -> DefaultsSlot {
    DefaultsSlot {
        layer: context.layer("Structure"),
        position: Position::Undefined,
        structural: StructuralFunction::Undefined,
        ifc_class: Column::DESCRIPTOR.default_class,
        predefined_type: None,
        kind: Column {
            position: DVec2::ZERO,
            section: CrossSection::Rectangular {
                width: 0.3,
                height: 0.3,
            },
            material: context.material("Reinforced concrete"),
            anchor: Anchor::Centre,
            rotation: 0.0,
            link: VerticalLink {
                base_offset: 0.0,
                top: TopLink::Relative {
                    storeys: 1,
                    offset: 0.0,
                },
            },
            top_cut: ColumnCut::Horizontal,
            bottom_cut: ColumnCut::Horizontal,
        }
        .into(),
    }
}

impl Project {
    /// A new column at `position` on `storey`, turned by `rotation` radians,
    /// with the column tool's defaults (ELM-001). `nth` numbers columns
    /// created together.
    pub fn new_column(&self, storey: StoreyId, position: DVec2, rotation: f64, nth: u32) -> Element {
        let mut element = self
            .new_element(KindTag::Column, storey, nth)
            .unwrap_or_else(|| unreachable!("columns have template defaults"));
        if let Some(column) = element.as_column_mut() {
            (column.position, column.rotation) = (position, rotation);
        }
        element
    }

    /// Smallest and largest plan coordinates `element` covers: the
    /// [`Kind::plan_bounds`] hook, except for a profile column, whose bounds
    /// need its profile from this project (PRF-004; see the module
    /// documentation). `None` if it has no points.
    pub fn plan_bounds(&self, element: &Element) -> Option<(DVec2, DVec2)> {
        match element.as_column() {
            Some(column) if column.section.profile().is_some() => column.plan_bounds_with(column.profile(self)),
            _ => element.kind.plan_bounds(),
        }
    }
}

impl Vertical for Column {
    fn link_of(element: &Element) -> Option<VerticalLink> {
        element.as_column().map(|c| c.link)
    }

    fn set_link_of(element: &mut Element, link: VerticalLink) {
        if let Some(column) = element.as_column_mut() {
            column.link = link;
        }
    }
}

/// The base and top fields (ELM-005, ELM-006). The column's own fields come
/// with its tool and properties (archicraft-k553).
const FIELDS: [FieldSpec; 6] = vertical::fields::<Column>("model-error-column-height-range");

impl Kind for Column {
    /// ELM-011: `IfcColumn` by default, `IfcMember` selectable; ELM-019:
    /// names `C-001`, `C-002`, …
    const DESCRIPTOR: KindDescriptor = KindDescriptor {
        label_key: "model-kind-column",
        noun_key: "model-kind-column-noun",
        node_prefix: "Column",
        code: "column",
        name_prefix: "C",
        default_class: IfcClass::Column,
        ifc_classes: IfcClass::COLUMN_CLASSES,
        predefined_types: PredefinedType::for_class,
        has_tool: true,
        has_body: true,
        ifcx_node: true,
        construction: true,
        template_defaults: Some(template_defaults),
        fields: &FIELDS,
    };

    fn material(&self) -> Option<MaterialId> {
        Some(self.material)
    }

    fn set_material(&mut self, material: MaterialId) {
        self.material = material;
    }

    /// The profile of a profile section (PRF-004).
    fn profiles(&self) -> Vec<ProfileId> {
        self.section.profile().into_iter().collect()
    }

    fn replace_profile(&mut self, from: ProfileId, to: ProfileId) -> bool {
        match &mut self.section {
            CrossSection::Profile(profile) if *profile == from => {
                *profile = to;
                true
            }
            _ => false,
        }
    }

    /// Only profiles usable by columns (PRF-004).
    fn accepts_profile(&self, usage: ProfileUsage) -> bool {
        usage.columns
    }

    /// Range checks (ELM-017): the position within the coordinate range,
    /// every section dimension between [`MIN_THICKNESS`] and
    /// [`MAX_THICKNESS`], a finite rotation, cut angles below
    /// [`MAX_CUT_ANGLE_DEG`], the base and top as for walls, and a linked
    /// storey that exists. Cut planes that cross within the section depend
    /// on the height and are found by the geometry. A profile section's
    /// profile is checked by [`validate::validate_prototype`] (PRF-004);
    /// [`Anchor::ProfileOrigin`] on another section is stored as
    /// [`Anchor::Centre`].
    fn validate(&self, _home: StoreyId, storeys: &StoreyList) -> Result<Self, Message> {
        validate::check_point(self.position)?;
        if self
            .section
            .dimensions()
            .iter()
            .any(|d| !(d.is_finite() && (MIN_THICKNESS..=MAX_THICKNESS).contains(d)))
        {
            return Err(Message::new("model-error-column-section-range")
                .length("min", MIN_THICKNESS)
                .length("max", MAX_THICKNESS));
        }
        if !self.rotation.is_finite() {
            return Err(Message::new("model-error-column-rotation"));
        }
        let max_angle = MAX_CUT_ANGLE_DEG.to_radians();
        for cut in [self.top_cut, self.bottom_cut] {
            if let ColumnCut::Angle(angle) = cut
                && !(angle.is_finite() && angle.abs() < max_angle)
            {
                return Err(Message::new("model-error-column-cut-angle").angle("max", max_angle));
            }
        }
        if let TopLink::Fixed { height } = self.link.top
            && !(height.is_finite() && (MIN_HEIGHT..=MAX_HEIGHT).contains(&height))
        {
            return Err(Message::new("model-error-column-height-range")
                .length("min", MIN_HEIGHT)
                .length("max", MAX_HEIGHT));
        }
        validate::check_vertical_link(&self.link)?;
        if self.link.missing_storey(storeys).is_some() {
            return Err(Message::new("model-error-column-top-storey-missing"));
        }
        let mut column = self.clone();
        if column.anchor == Anchor::ProfileOrigin && column.section.profile().is_none() {
            column.anchor = Anchor::Centre;
        }
        Ok(column)
    }

    /// Moves the position; a rotation turns the section with it, and a
    /// mirror gives the mirror image: a rectangle or circle is symmetric
    /// about its x axis, so the image is the section turned to the mirrored
    /// x axis with its anchor flipped across that axis and its cuts tilted
    /// the other way. A profile need not be symmetric and its mirror image is
    /// another profile, so a profile column refuses a mirror and stays
    /// unchanged.
    fn transform(&mut self, transform: &PlanTransform) -> Result<(), TransformError> {
        if transform.is_mirror() && self.section.profile().is_some() {
            return Err(TransformError::Unsupported);
        }
        self.position = transform.apply(self.position);
        if transform.is_translation() {
            return Ok(());
        }
        if !transform.is_mirror() {
            let turn = transform.apply_vector(DVec2::X);
            self.rotation += turn.y.atan2(turn.x);
            return Ok(());
        }
        let x_axis = transform.apply_vector(DVec2::from_angle(self.rotation));
        self.rotation = x_axis.y.atan2(x_axis.x);
        self.anchor = self.anchor.flipped_vertically();
        self.top_cut = self.top_cut.mirrored();
        self.bottom_cut = self.bottom_cut.mirrored();
        Ok(())
    }

    fn vertical_extent(&self, home: StoreyId, storeys: &StoreyList) -> Option<(f64, f64)> {
        self.link.extent(home, storeys)
    }

    fn storey_links(&self) -> Vec<StoreyLinkRef> {
        self.link.storey_links()
    }

    fn relative_top(&self) -> Option<i32> {
        self.link.relative_top()
    }

    fn rehome(&mut self, home: StoreyId, storeys: &StoreyList, _target: StoreyId, target_elevation: f64) -> Option<()> {
        self.link.rehome(home, storeys, target_elevation)
    }

    fn relink(&mut self, home: StoreyId, storeys: &StoreyList, target: StoreyId) -> Option<()> {
        self.link.relink(home, storeys, target)
    }

    fn unlink_storey(&mut self, home: StoreyId, storeys: &StoreyList, storey: StoreyId) -> bool {
        self.link.unlink_storey(home, storeys, storey)
    }

    fn fix_top(&mut self, height: f64) -> bool {
        self.link.fix_top(height)
    }

    /// The rotated section's corners, or a circle's centre plus and minus
    /// its radius. A profile column's size is its profile's, which this hook
    /// does not see: it gives the axis point only, and
    /// [`Project::plan_bounds`] the profile's box (PRF-004).
    fn plan_bounds(&self) -> Option<(DVec2, DVec2)> {
        self.plan_bounds_with(None).or(Some((self.position, self.position)))
    }
}
