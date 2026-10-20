//! Slabs (SRS 4.12; SLB-001…003).

use glam::DVec2;
use serde::{Deserialize, Serialize};

use super::{Kind, KindDescriptor};
use crate::element::IfcClass;
use crate::ids::{MaterialId, StoreyId};
use crate::storey::StoreyList;
use crate::transform::{PlanTransform, TransformError};
use crate::validate;

/// Which face of a slab its elevation refers to (SLB-002).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum SlabReference {
    #[default]
    Top,
    Bottom,
}

impl SlabReference {
    pub fn code(self) -> &'static str {
        match self {
            SlabReference::Top => "top",
            SlabReference::Bottom => "bottom",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "top" => Some(SlabReference::Top),
            "bottom" => Some(SlabReference::Bottom),
            _ => None,
        }
    }
}

/// A slab with a straight-edged outline and holes (SLB-001…003).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Slab {
    /// Outline, counter-clockwise, without repeating the first point.
    pub outline: Vec<DVec2>,
    /// Holes, clockwise, inside the outline.
    pub holes: Vec<Vec<DVec2>>,
    pub thickness: f64,
    pub reference: SlabReference,
    /// Elevation of the reference face relative to the home storey.
    pub offset: f64,
    pub material: MaterialId,
}

/// The ring transformed point by point. A mirror reverses the orientation,
/// so the ring is reversed as well, keeping its first point first.
fn transform_ring(ring: &mut [DVec2], transform: &PlanTransform) {
    for p in ring.iter_mut() {
        *p = transform.apply(*p);
    }
    if transform.is_mirror() && ring.len() > 1 {
        ring[1..].reverse();
    }
}

impl Kind for Slab {
    /// ELM-011: `IfcSlab` by default, `IfcFooting` and `IfcCovering`
    /// selectable; ELM-019: names `SL-001`, `SL-002`, …
    const DESCRIPTOR: KindDescriptor = KindDescriptor {
        label_key: "model-kind-slab",
        noun_key: "model-kind-slab-noun",
        node_prefix: "Slab",
        code: "slab",
        name_prefix: "SL",
        default_class: IfcClass::Slab,
        ifc_classes: &[IfcClass::Slab, IfcClass::Footing, IfcClass::Covering],
        has_tool: true,
    };

    fn material(&self) -> MaterialId {
        self.material
    }

    fn set_material(&mut self, material: MaterialId) {
        self.material = material;
    }

    /// Normalises the outline and holes (EDT-018).
    fn validate(&self, _storeys: &StoreyList) -> Result<Self, String> {
        validate::validate_slab(self)
    }

    /// Keeps the outline counter-clockwise and the holes clockwise, also
    /// when mirrored (SLB-001).
    fn transform(&mut self, transform: &PlanTransform) -> Result<(), TransformError> {
        transform_ring(&mut self.outline, transform);
        for hole in &mut self.holes {
            transform_ring(hole, transform);
        }
        Ok(())
    }

    /// The reference face is at the offset above the home storey (SLB-002).
    fn vertical_extent(&self, home: StoreyId, storeys: &StoreyList) -> Option<(f64, f64)> {
        let reference = storeys.get(home)?.elevation + self.offset;
        Some(match self.reference {
            SlabReference::Top => (reference - self.thickness, reference),
            SlabReference::Bottom => (reference, reference + self.thickness),
        })
    }

    fn rehome(&mut self, home: StoreyId, storeys: &StoreyList, target_elevation: f64) -> Option<()> {
        let home = storeys.get(home)?.elevation;
        self.offset = home + self.offset - target_elevation;
        Some(())
    }
}
