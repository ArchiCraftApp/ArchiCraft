//! Walls (SRS 4.11; WAL-001, WAL-003).

use glam::DVec2;
use serde::{Deserialize, Serialize};

use super::{Kind, KindDescriptor, StoreyLinkRef, VerticalLink, plan_bounds_of};
use crate::element::{IfcClass, TopLink};
use crate::ids::{MaterialId, StoreyId};
use crate::message::Message;
use crate::storey::StoreyList;
use crate::transform::{PlanTransform, TransformError};
use crate::validate;

/// Location of a wall's reference line within its thickness (WAL-003).
///
/// The reference line runs from start to end. The exterior side is on the
/// right of that direction and the interior side on the left, so walls drawn
/// counter-clockwise around a building have their exterior faces outside.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ReferenceLine {
    Outside,
    Center,
    Inside,
    /// Distance of the reference line from the exterior face, towards the
    /// interior face.
    Custom(f64),
}

impl ReferenceLine {
    /// Offsets of the exterior and interior faces from the reference line,
    /// measured along the left-hand normal of the reference line.
    pub fn face_offsets(self, thickness: f64) -> (f64, f64) {
        let from_exterior = match self {
            ReferenceLine::Outside => 0.0,
            ReferenceLine::Center => thickness / 2.0,
            ReferenceLine::Inside => thickness,
            ReferenceLine::Custom(offset) => offset,
        };
        (-from_exterior, thickness - from_exterior)
    }

    pub fn label(self) -> &'static str {
        match self {
            ReferenceLine::Outside => "Exterior face",
            ReferenceLine::Center => "Centre",
            ReferenceLine::Inside => "Interior face",
            ReferenceLine::Custom(_) => "Custom offset",
        }
    }

    /// Next location, for cycling while drawing (WAL-007).
    pub fn next(self) -> Self {
        match self {
            ReferenceLine::Outside => ReferenceLine::Center,
            ReferenceLine::Center => ReferenceLine::Inside,
            ReferenceLine::Inside | ReferenceLine::Custom(_) => ReferenceLine::Outside,
        }
    }
}

/// A straight wall (WAL-001, WAL-003).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wall {
    pub start: DVec2,
    pub end: DVec2,
    pub thickness: f64,
    pub reference_line: ReferenceLine,
    /// Base offset relative to the home storey elevation.
    pub base_offset: f64,
    pub top: TopLink,
    pub material: MaterialId,
}

impl Wall {
    pub fn length(&self) -> f64 {
        self.start.distance(self.end)
    }

    pub fn direction(&self) -> DVec2 {
        (self.end - self.start).normalize_or_zero()
    }

    /// The wall's base offset and top as a [`VerticalLink`] (ELM-006).
    pub fn link(&self) -> VerticalLink {
        VerticalLink {
            base_offset: self.base_offset,
            top: self.top,
        }
    }

    pub fn set_link(&mut self, link: VerticalLink) {
        self.base_offset = link.base_offset;
        self.top = link.top;
    }

    /// Applies `change` to the wall's vertical link. The [`VerticalLink`]
    /// helpers leave the link unchanged when they fail.
    fn change_link<T>(&mut self, change: impl FnOnce(&mut VerticalLink) -> T) -> T {
        let mut link = self.link();
        let result = change(&mut link);
        self.set_link(link);
        result
    }
}

impl Kind for Wall {
    /// ELM-011: `IfcWall` by default, `IfcFooting` selectable; ELM-019:
    /// names `W-001`, `W-002`, …
    const DESCRIPTOR: KindDescriptor = KindDescriptor {
        label_key: "model-kind-wall",
        noun_key: "model-kind-wall-noun",
        node_prefix: "Wall",
        code: "wall",
        name_prefix: "W",
        default_class: IfcClass::Wall,
        ifc_classes: &[IfcClass::Wall, IfcClass::Footing],
        has_tool: true,
    };

    fn material(&self) -> MaterialId {
        self.material
    }

    fn set_material(&mut self, material: MaterialId) {
        self.material = material;
    }

    fn validate(&self, storeys: &StoreyList) -> Result<Self, Message> {
        validate::validate_wall(self)?;
        if self.link().missing_storey(storeys).is_some() {
            return Err(Message::new("model-error-wall-top-storey-missing"));
        }
        Ok(self.clone())
    }

    fn transform(&mut self, transform: &PlanTransform) -> Result<(), TransformError> {
        let (start, end) = (transform.apply(self.start), transform.apply(self.end));
        // A mirror swaps the sides of the reference line. Swapping the ends
        // keeps the exterior face on the right, now on the mirrored exterior
        // side (WAL-003).
        (self.start, self.end) = if transform.is_mirror() {
            (end, start)
        } else {
            (start, end)
        };
        Ok(())
    }

    fn vertical_extent(&self, home: StoreyId, storeys: &StoreyList) -> Option<(f64, f64)> {
        self.link().extent(home, storeys)
    }

    fn storey_links(&self) -> Vec<StoreyLinkRef> {
        self.link().storey_links()
    }

    fn rehome(&mut self, home: StoreyId, storeys: &StoreyList, target_elevation: f64) -> Option<()> {
        self.change_link(|link| link.rehome(home, storeys, target_elevation))
    }

    fn relink(&mut self, home: StoreyId, storeys: &StoreyList, target: StoreyId) -> Option<()> {
        self.change_link(|link| link.relink(home, storeys, target))
    }

    fn unlink_storey(&mut self, home: StoreyId, storeys: &StoreyList, storey: StoreyId) -> bool {
        self.change_link(|link| link.unlink_storey(home, storeys, storey))
    }

    /// The corners of the unjoined footprint: both faces at both ends.
    fn plan_bounds(&self) -> Option<(DVec2, DVec2)> {
        let normal = self.direction().perp();
        let (exterior, interior) = self.reference_line.face_offsets(self.thickness);
        plan_bounds_of(
            [self.start, self.end]
                .into_iter()
                .flat_map(|p| [p + normal * exterior, p + normal * interior]),
        )
    }
}
