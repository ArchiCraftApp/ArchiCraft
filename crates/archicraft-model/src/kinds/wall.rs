//! Walls (SRS 4.11; WAL-001, WAL-003).

use glam::DVec2;
use serde::{Deserialize, Serialize};

use crate::element::TopLink;
use crate::ids::MaterialId;

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
}
