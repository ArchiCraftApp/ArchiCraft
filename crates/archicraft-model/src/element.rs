//! Elements (SRS 3.2, 4.9): the envelope every kind shares. The kinds'
//! parameter payloads live in [`crate::kinds`].

use glam::DVec2;
use serde::{Deserialize, Serialize};

use crate::ids::{ElementId, LayerId, MaterialId, StoreyId};
use crate::kinds::{Slab, Wall};

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
