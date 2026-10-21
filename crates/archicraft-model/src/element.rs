//! Elements (SRS 3.2, 4.9): the envelope every kind shares. The kinds'
//! parameter payloads, [`ElementKind`], [`KindTag`] and the kind registry
//! live in [`crate::kinds`].

use glam::DVec2;
use serde::{Deserialize, Serialize};

use crate::ids::{ElementId, LayerId, StoreyId};
pub use crate::kinds::{ElementKind, KindTag};
use crate::storey::StoreyList;
use crate::transform::PlanTransform;

/// IFC class of an element (ELM-011, Table 5-1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IfcClass {
    Wall,
    Slab,
    Footing,
    Covering,
    /// `IfcBuildingElementProxy`, for elements without a more specific
    /// class.
    BuildingElementProxy,
}

impl IfcClass {
    /// The IFC 4.3 class name written as `bsi::ifc::class.code`.
    pub fn code(self) -> &'static str {
        match self {
            IfcClass::Wall => "IfcWall",
            IfcClass::Slab => "IfcSlab",
            IfcClass::Footing => "IfcFooting",
            IfcClass::Covering => "IfcCovering",
            IfcClass::BuildingElementProxy => "IfcBuildingElementProxy",
        }
    }

    /// Message key of the class's name in choices (NFR-L10N-001).
    pub fn label_key(self) -> &'static str {
        match self {
            IfcClass::Wall => "model-ifc-class-wall",
            IfcClass::Slab => "model-ifc-class-slab",
            IfcClass::Footing => "model-ifc-class-footing",
            IfcClass::Covering => "model-ifc-class-covering",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        [
            IfcClass::Wall,
            IfcClass::Slab,
            IfcClass::Footing,
            IfcClass::Covering,
            IfcClass::BuildingElementProxy,
        ]
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

impl TopLink {
    /// Absolute elevation of the top of an element on `home` whose base is
    /// at the absolute elevation `base` (ELM-006). Storey links are resolved
    /// against `storeys`, so linked tops follow storey edits (STR-006).
    /// `None` if a storey it refers to is missing.
    pub fn elevation(self, home: StoreyId, base: f64, storeys: &StoreyList) -> Option<f64> {
        match self {
            TopLink::Fixed { height } => Some(base + height),
            TopLink::Storey { storey, offset } => Some(storeys.get(storey)?.elevation + offset),
            TopLink::Relative { storeys: n, offset } => Some(storeys.relative_elevation(home, n)? + offset),
        }
    }
}

impl ElementKind {
    /// Translates the element in plan through [`ElementKind::transform`]
    /// (EDT-001). Every kind supports translations.
    pub fn translate(&mut self, delta: DVec2) {
        let translated = self.transform(&PlanTransform::translation(delta));
        debug_assert!(translated.is_ok(), "every kind supports translations");
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
}
