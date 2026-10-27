//! Elements (SRS 3.2, 4.9): the envelope every kind shares. The kinds'
//! parameter payloads, [`ElementKind`], [`KindTag`] and the kind registry
//! live in [`crate::kinds`].

use glam::DVec2;
use serde::{Deserialize, Serialize};

use crate::ids::{ElementId, LayerId, StoreyId};
pub use crate::kinds::{ElementKind, KindTag};
use crate::storey::StoreyList;
use crate::transform::PlanTransform;

/// IFC class of an element or a compound part (ELM-011): the twenty
/// classes of Appendix D.5 followed by the other classes of Table 5-1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IfcClass {
    /// `IfcBuildingElementProxy`, for elements without a more specific
    /// class.
    BuildingElementProxy,
    Wall,
    Slab,
    Roof,
    Column,
    Beam,
    Member,
    Plate,
    Covering,
    Footing,
    Pile,
    Railing,
    Stair,
    Ramp,
    Chimney,
    ShadingDevice,
    CurtainWall,
    Furniture,
    Space,
    GeographicElement,
    // Table 5-1 classes outside Appendix D.5.
    StairFlight,
    OpeningElement,
    Door,
    Window,
    Grid,
    Annotation,
}

/// The class of one kind of part of a compound element (IFX-016, a part row
/// of Table 5-1), for the part specifications of the compound kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompoundPart {
    /// The class of the compound element.
    pub compound: IfcClass,
    /// The part's role within the compound, such as `flight` or `panel`.
    pub part: &'static str,
    /// The class the part is written with.
    pub class: IfcClass,
}

impl IfcClass {
    /// Every class, Appendix D.5 first.
    pub const ALL: [IfcClass; 26] = [
        IfcClass::BuildingElementProxy,
        IfcClass::Wall,
        IfcClass::Slab,
        IfcClass::Roof,
        IfcClass::Column,
        IfcClass::Beam,
        IfcClass::Member,
        IfcClass::Plate,
        IfcClass::Covering,
        IfcClass::Footing,
        IfcClass::Pile,
        IfcClass::Railing,
        IfcClass::Stair,
        IfcClass::Ramp,
        IfcClass::Chimney,
        IfcClass::ShadingDevice,
        IfcClass::CurtainWall,
        IfcClass::Furniture,
        IfcClass::Space,
        IfcClass::GeographicElement,
        IfcClass::StairFlight,
        IfcClass::OpeningElement,
        IfcClass::Door,
        IfcClass::Window,
        IfcClass::Grid,
        IfcClass::Annotation,
    ];

    /// ELM-011, Appendix D.5: the classes selectable for free-form solids,
    /// objects and imported elements, `IfcBuildingElementProxy` (their
    /// default) first.
    pub const SELECTABLE_D5: [IfcClass; 20] = [
        IfcClass::BuildingElementProxy,
        IfcClass::Wall,
        IfcClass::Slab,
        IfcClass::Roof,
        IfcClass::Column,
        IfcClass::Beam,
        IfcClass::Member,
        IfcClass::Plate,
        IfcClass::Covering,
        IfcClass::Footing,
        IfcClass::Pile,
        IfcClass::Railing,
        IfcClass::Stair,
        IfcClass::Ramp,
        IfcClass::Chimney,
        IfcClass::ShadingDevice,
        IfcClass::CurtainWall,
        IfcClass::Furniture,
        IfcClass::Space,
        IfcClass::GeographicElement,
    ];

    /// ELM-011: the classes selectable for walls.
    pub const WALL_CLASSES: &'static [IfcClass] = &[IfcClass::Wall, IfcClass::Footing];

    /// ELM-011: the classes selectable for slabs.
    pub const SLAB_CLASSES: &'static [IfcClass] = &[IfcClass::Slab, IfcClass::Footing, IfcClass::Covering];

    /// ELM-011: the classes selectable for beams.
    pub const BEAM_CLASSES: &'static [IfcClass] = &[IfcClass::Beam, IfcClass::Member, IfcClass::Footing];

    /// ELM-011: the classes selectable for columns.
    pub const COLUMN_CLASSES: &'static [IfcClass] = &[IfcClass::Column, IfcClass::Member];

    /// IFX-016, Table 5-1: the classes of the parts of stairs, railings,
    /// curtain walls and multi-plane roofs.
    pub const COMPOUND_PART_CLASSES: &'static [CompoundPart] = &[
        CompoundPart::new(IfcClass::Stair, "flight", IfcClass::StairFlight),
        CompoundPart::new(IfcClass::Stair, "landing", IfcClass::Slab),
        CompoundPart::new(IfcClass::Stair, "stringer", IfcClass::Member),
        CompoundPart::new(IfcClass::Railing, "post", IfcClass::Member),
        CompoundPart::new(IfcClass::Railing, "rail", IfcClass::Member),
        CompoundPart::new(IfcClass::Railing, "infill-panel", IfcClass::Plate),
        CompoundPart::new(IfcClass::CurtainWall, "frame", IfcClass::Member),
        CompoundPart::new(IfcClass::CurtainWall, "panel", IfcClass::Plate),
        CompoundPart::new(IfcClass::Roof, "plane", IfcClass::Slab),
    ];

    /// The IFC 4.3 class name written as `bsi::ifc::class.code`.
    pub fn code(self) -> &'static str {
        match self {
            IfcClass::BuildingElementProxy => "IfcBuildingElementProxy",
            IfcClass::Wall => "IfcWall",
            IfcClass::Slab => "IfcSlab",
            IfcClass::Roof => "IfcRoof",
            IfcClass::Column => "IfcColumn",
            IfcClass::Beam => "IfcBeam",
            IfcClass::Member => "IfcMember",
            IfcClass::Plate => "IfcPlate",
            IfcClass::Covering => "IfcCovering",
            IfcClass::Footing => "IfcFooting",
            IfcClass::Pile => "IfcPile",
            IfcClass::Railing => "IfcRailing",
            IfcClass::Stair => "IfcStair",
            IfcClass::Ramp => "IfcRamp",
            IfcClass::Chimney => "IfcChimney",
            IfcClass::ShadingDevice => "IfcShadingDevice",
            IfcClass::CurtainWall => "IfcCurtainWall",
            IfcClass::Furniture => "IfcFurniture",
            IfcClass::Space => "IfcSpace",
            IfcClass::GeographicElement => "IfcGeographicElement",
            IfcClass::StairFlight => "IfcStairFlight",
            IfcClass::OpeningElement => "IfcOpeningElement",
            IfcClass::Door => "IfcDoor",
            IfcClass::Window => "IfcWindow",
            IfcClass::Grid => "IfcGrid",
            IfcClass::Annotation => "IfcAnnotation",
        }
    }

    /// Message key of the class's name in choices (NFR-L10N-001).
    pub fn label_key(self) -> &'static str {
        match self {
            IfcClass::BuildingElementProxy => "model-ifc-class-proxy",
            IfcClass::Wall => "model-ifc-class-wall",
            IfcClass::Slab => "model-ifc-class-slab",
            IfcClass::Roof => "model-ifc-class-roof",
            IfcClass::Column => "model-ifc-class-column",
            IfcClass::Beam => "model-ifc-class-beam",
            IfcClass::Member => "model-ifc-class-member",
            IfcClass::Plate => "model-ifc-class-plate",
            IfcClass::Covering => "model-ifc-class-covering",
            IfcClass::Footing => "model-ifc-class-footing",
            IfcClass::Pile => "model-ifc-class-pile",
            IfcClass::Railing => "model-ifc-class-railing",
            IfcClass::Stair => "model-ifc-class-stair",
            IfcClass::Ramp => "model-ifc-class-ramp",
            IfcClass::Chimney => "model-ifc-class-chimney",
            IfcClass::ShadingDevice => "model-ifc-class-shading-device",
            IfcClass::CurtainWall => "model-ifc-class-curtain-wall",
            IfcClass::Furniture => "model-ifc-class-furniture",
            IfcClass::Space => "model-ifc-class-space",
            IfcClass::GeographicElement => "model-ifc-class-geographic-element",
            IfcClass::StairFlight => "model-ifc-class-stair-flight",
            IfcClass::OpeningElement => "model-ifc-class-opening-element",
            IfcClass::Door => "model-ifc-class-door",
            IfcClass::Window => "model-ifc-class-window",
            IfcClass::Grid => "model-ifc-class-grid",
            IfcClass::Annotation => "model-ifc-class-annotation",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.code() == code)
    }
}

impl CompoundPart {
    const fn new(compound: IfcClass, part: &'static str, class: IfcClass) -> Self {
        Self { compound, part, class }
    }
}

/// What IFC 4.x expresses as the predefined type of an element's class
/// (IFX-024), for example a slab that is a floor or a footing that is a
/// strip. The codes are the IFC 4.3 enumeration literals (IfcSlabTypeEnum,
/// IfcFootingTypeEnum), so that a future standard attribute can carry them
/// verbatim (O-6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PredefinedType {
    Floor,
    Roof,
    Landing,
    BaseSlab,
    PadFooting,
    StripFooting,
}

impl PredefinedType {
    pub const ALL: [PredefinedType; 6] = [
        PredefinedType::Floor,
        PredefinedType::Roof,
        PredefinedType::Landing,
        PredefinedType::BaseSlab,
        PredefinedType::PadFooting,
        PredefinedType::StripFooting,
    ];

    /// The IFC 4.3 enumeration literal, written as
    /// `archicraft::element.predefinedType` (IFX-024).
    pub fn code(self) -> &'static str {
        match self {
            PredefinedType::Floor => "FLOOR",
            PredefinedType::Roof => "ROOF",
            PredefinedType::Landing => "LANDING",
            PredefinedType::BaseSlab => "BASESLAB",
            PredefinedType::PadFooting => "PAD_FOOTING",
            PredefinedType::StripFooting => "STRIP_FOOTING",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.code() == code)
    }

    /// Message key of the type's name in choices (NFR-L10N-001).
    pub fn label_key(self) -> &'static str {
        match self {
            PredefinedType::Floor => "model-predefined-type-floor",
            PredefinedType::Roof => "model-predefined-type-roof",
            PredefinedType::Landing => "model-predefined-type-landing",
            PredefinedType::BaseSlab => "model-predefined-type-base-slab",
            PredefinedType::PadFooting => "model-predefined-type-pad-footing",
            PredefinedType::StripFooting => "model-predefined-type-strip-footing",
        }
    }

    /// The types IFC 4.3 defines for `class` that ArchiCraft offers
    /// (IFX-024): floor, roof, landing and base slab for `IfcSlab`, pad and
    /// strip for `IfcFooting`, none for the other classes.
    pub fn for_class(class: IfcClass) -> &'static [PredefinedType] {
        match class {
            IfcClass::Slab => &[
                PredefinedType::Floor,
                PredefinedType::Roof,
                PredefinedType::Landing,
                PredefinedType::BaseSlab,
            ],
            IfcClass::Footing => &[PredefinedType::PadFooting, PredefinedType::StripFooting],
            _ => &[],
        }
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
    /// One of the types the kind offers for `ifc_class`, or none (IFX-024).
    #[serde(default)]
    pub predefined_type: Option<PredefinedType>,
    pub position: Position,
    /// Locked elements reject edits (ELM-013).
    pub locked: bool,
    /// Excluded elements are drawn but provide no snap targets (INP-017).
    #[serde(default)]
    pub snap_excluded: bool,
    pub kind: ElementKind,
}

impl Element {
    pub fn tag(&self) -> KindTag {
        self.kind.tag()
    }
}
