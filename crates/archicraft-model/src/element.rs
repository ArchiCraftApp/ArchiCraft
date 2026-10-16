//! Elements (SRS 3.2, 4.9, 4.11, 4.12).

use glam::DVec2;
use serde::{Deserialize, Serialize};

use crate::ids::{ElementId, LayerId, MaterialId, StoreyId};

/// IFC class of an element (ELM-011, Table 5-1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IfcClass {
    Wall,
    Slab,
    Footing,
    Covering,
}

impl IfcClass {
    /// The IFC 4.3 class name written as `bsi::ifc::class.code`.
    pub fn code(self) -> &'static str {
        match self {
            IfcClass::Wall => "IfcWall",
            IfcClass::Slab => "IfcSlab",
            IfcClass::Footing => "IfcFooting",
            IfcClass::Covering => "IfcCovering",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        [IfcClass::Wall, IfcClass::Slab, IfcClass::Footing, IfcClass::Covering]
            .into_iter()
            .find(|c| c.code() == code)
    }
}

/// Position of an element relative to the building envelope (ELM-012),
/// written as `IsExternal` when known (IFX-022).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Position {
    #[default]
    Undefined,
    Exterior,
    Interior,
}

impl Position {
    pub const ALL: [Position; 3] = [Position::Undefined, Position::Exterior, Position::Interior];

    pub fn code(self) -> &'static str {
        match self {
            Position::Undefined => "undefined",
            Position::Exterior => "exterior",
            Position::Interior => "interior",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.code() == code)
    }

    pub fn is_external(self) -> Option<bool> {
        match self {
            Position::Undefined => None,
            Position::Exterior => Some(true),
            Position::Interior => Some(false),
        }
    }
}

/// Where the top of a vertical element is (ELM-006).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum TopLink {
    /// A fixed height above the element's base.
    Fixed { height: f64 },
    /// An offset above a specific storey's elevation.
    Storey { storey: StoreyId, offset: f64 },
    /// An offset above the storey `storeys` positions above the home storey
    /// (for example "home + 1").
    Relative { storeys: i32, offset: f64 },
}

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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ElementKind {
    Wall(Wall),
    Slab(Slab),
}

/// The tool that creates an element kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KindTag {
    Wall,
    Slab,
}

impl KindTag {
    pub fn label(self) -> &'static str {
        match self {
            KindTag::Wall => "Wall",
            KindTag::Slab => "Slab",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            KindTag::Wall => "wall",
            KindTag::Slab => "slab",
        }
    }

    /// Prefix of automatic element names (ELM-019).
    pub fn name_prefix(self) -> &'static str {
        match self {
            KindTag::Wall => "W",
            KindTag::Slab => "SL",
        }
    }

    /// IFC classes selectable for this kind (ELM-011).
    pub fn ifc_classes(self) -> &'static [IfcClass] {
        match self {
            KindTag::Wall => &[IfcClass::Wall, IfcClass::Footing],
            KindTag::Slab => &[IfcClass::Slab, IfcClass::Footing, IfcClass::Covering],
        }
    }
}

impl ElementKind {
    pub fn tag(&self) -> KindTag {
        match self {
            ElementKind::Wall(_) => KindTag::Wall,
            ElementKind::Slab(_) => KindTag::Slab,
        }
    }

    pub fn material(&self) -> MaterialId {
        match self {
            ElementKind::Wall(w) => w.material,
            ElementKind::Slab(s) => s.material,
        }
    }

    pub fn set_material(&mut self, material: MaterialId) {
        match self {
            ElementKind::Wall(w) => w.material = material,
            ElementKind::Slab(s) => s.material = material,
        }
    }

    /// Translates the element in plan.
    pub fn translate(&mut self, delta: DVec2) {
        match self {
            ElementKind::Wall(w) => {
                w.start += delta;
                w.end += delta;
            }
            ElementKind::Slab(s) => {
                for p in s.outline.iter_mut().chain(s.holes.iter_mut().flatten()) {
                    *p += delta;
                }
            }
        }
    }
}

/// Any element placed in the model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub id: ElementId,
    /// User-visible name or ID, for example `W-012` (ELM-004).
    pub name: String,
    /// Home storey (ELM-005).
    pub storey: StoreyId,
    pub layer: LayerId,
    pub ifc_class: IfcClass,
    pub position: Position,
    /// Locked elements reject edits (ELM-013).
    pub locked: bool,
    pub kind: ElementKind,
}

impl Element {
    pub fn tag(&self) -> KindTag {
        self.kind.tag()
    }

    pub fn as_wall(&self) -> Option<&Wall> {
        match &self.kind {
            ElementKind::Wall(w) => Some(w),
            ElementKind::Slab(_) => None,
        }
    }

    pub fn as_slab(&self) -> Option<&Slab> {
        match &self.kind {
            ElementKind::Slab(s) => Some(s),
            ElementKind::Wall(_) => None,
        }
    }
}
