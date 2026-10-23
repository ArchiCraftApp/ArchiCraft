//! ArchiCraft geometry evaluation (NFR-MNT-002).
//!
//! Turns element parameters into closed triangle meshes, resolves wall
//! junctions, and derives plan drawings from the same 3D bodies, so that plan,
//! 3D view and IFCX agree (WAL-027).

pub mod evaluate;
pub mod extrude;
pub mod mesh;
pub mod message;
pub mod plan;
pub mod slice;
pub mod triangulate;
pub mod wall;

use archicraft_model::KindTag;

pub use evaluate::{BodyPart, ElementGeometry, ModelGeometry};
pub use mesh::{MeshIssue, TriMesh};
pub use message::Message;
pub use plan::{PlanDrawing, ViewRange, plan_drawing};

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
    /// The element is lower than the model tolerance.
    InvalidExtent,
    /// The element's home storey is not in the storey list.
    MissingHomeStorey,
    /// A storey the element's top or bottom is linked to is not in the
    /// storey list.
    MissingLinkedStorey,
    /// No geometry generator exists for the element's kind yet.
    UnsupportedKind(KindTag),
}

impl GeometryError {
    /// The problem as a `geometry-` message (NFR-L10N-001).
    pub fn message(&self) -> Message {
        match self {
            Self::InvalidPolygon(reason) => Message::new("geometry-invalid-polygon").nested("reason", reason.clone()),
            Self::InvalidExtent => Message::new("geometry-no-height"),
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
