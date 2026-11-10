//! ArchiCraft geometry evaluation (NFR-MNT-002).
//!
//! Turns element parameters into closed triangle meshes, resolves wall
//! junctions, and derives plan drawings from the same 3D bodies, so that plan,
//! 3D view and IFCX agree (WAL-027).

pub mod contact;
pub mod contour;
pub mod dirty;
pub mod evaluate;
pub mod extrude;
pub mod kinds;
pub mod mesh;
pub mod message;
pub mod plan;
pub mod section;
pub mod slice;
pub mod space;
pub mod sweep;
pub mod triangulate;
pub mod wall;

use archicraft_model::KindTag;
use message::Message;

pub use contour::{Chain, ContourGraph, ContourOptions, PlanSegmentSource, Region};
pub use dirty::{DirtySet, dirty_set};
pub use evaluate::{
    BodyPart, ElementGeometry, MATERIAL_LESS_COLOR, MATERIAL_LESS_OPACITY, ModelGeometry, PartGeometry,
};
pub use kinds::{EvalContext, GeometryKind};
pub use mesh::{MeshIssue, TriMesh};
pub use plan::{PlanDrawing, PlanStamp, ViewRange, plan_drawing};
pub use space::{RegionError, SpaceRegions, space_region};

/// Why an element has no geometry (UI-012). [`GeometryError::message`]
/// gives the user-visible text as a message key (NFR-L10N-001); `Display`
/// is that message's key form, for logs.
#[derive(Clone, Debug, PartialEq)]
pub enum GeometryError {
    /// The outline cannot be made into a body. The value says why: an error
    /// of the model's validation as it is (for example
    /// `model-error-polygon-self-intersection`), or a `geometry-polygon-`
    /// message of the triangulation.
    InvalidPolygon(Message),
    /// The element has no extent to make a body of. The value is the
    /// message that says why: `geometry-no-height`
    /// ([`GeometryError::no_height`]) for an element lower than the model
    /// tolerance, or one of the [`sweep`](sweep::sweep)'s for a member whose
    /// end cuts leave no length or whose axis is undefined.
    InvalidExtent(Message),
    /// The element's home storey is not in the storey list.
    MissingHomeStorey,
    /// A storey the element's top or bottom is linked to is not in the
    /// storey list.
    MissingLinkedStorey,
    /// No geometry generator exists for the element's kind yet.
    UnsupportedKind(KindTag),
}

impl GeometryError {
    /// The element is lower than the model tolerance.
    pub fn no_height() -> Self {
        Self::InvalidExtent(Message::new("geometry-no-height"))
    }

    /// The problem as a `geometry-` message (NFR-L10N-001).
    pub fn message(&self) -> Message {
        match self {
            Self::InvalidPolygon(reason) => Message::new("geometry-invalid-polygon").nested("reason", reason.clone()),
            Self::InvalidExtent(reason) => reason.clone(),
            Self::MissingHomeStorey => Message::new("geometry-home-storey-missing"),
            Self::MissingLinkedStorey => Message::new("geometry-linked-storey-missing"),
            Self::UnsupportedKind(kind) => Message::new("geometry-kind-unsupported").kind("kind", *kind),
        }
    }
}

/// The key form of [`GeometryError::message`], for logs.
impl std::fmt::Display for GeometryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message().fmt(f)
    }
}

impl std::error::Error for GeometryError {}

/// Re-exported for dependants that work with plan polygons.
pub use geo;
