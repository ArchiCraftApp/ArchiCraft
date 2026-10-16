//! ArchiCraft geometry evaluation (NFR-MNT-002).
//!
//! Turns element parameters into closed triangle meshes, resolves wall
//! junctions, and derives plan drawings from the same 3D bodies, so that plan,
//! 3D view and IFCX agree (WAL-027).

pub mod evaluate;
pub mod extrude;
pub mod mesh;
pub mod plan;
pub mod slice;
pub mod triangulate;
pub mod wall;

pub use evaluate::{BodyPart, ElementGeometry, ModelGeometry};
pub use mesh::{MeshIssue, TriMesh};
pub use plan::{PlanDrawing, ViewRange, plan_drawing};

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum GeometryError {
    #[error("invalid polygon: {0}")]
    InvalidPolygon(String),
    #[error("the element has no height")]
    InvalidExtent,
    #[error("the {0} is missing")]
    MissingReference(&'static str),
}

/// Re-exported for dependants that work with plan polygons.
pub use geo;
