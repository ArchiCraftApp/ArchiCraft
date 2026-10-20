//! Slabs (SRS 4.12; SLB-001…003).

use glam::DVec2;
use serde::{Deserialize, Serialize};

use super::{Kind, KindDescriptor};
use crate::element::IfcClass;
use crate::ids::MaterialId;
use crate::storey::StoreyList;
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

impl Kind for Slab {
    /// ELM-011: `IfcSlab` by default, `IfcFooting` and `IfcCovering`
    /// selectable; ELM-019: names `SL-001`, `SL-002`, …
    const DESCRIPTOR: KindDescriptor = KindDescriptor {
        label: "Slab",
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
}
