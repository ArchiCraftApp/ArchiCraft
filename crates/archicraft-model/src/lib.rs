//! ArchiCraft domain model (SRS Section 3).
//!
//! The model is plain Rust data, independent of the user interface, geometry
//! evaluation and file format (NFR-MNT-002, NFR-MNT-004). Every change goes
//! through a [`command::Command`] executed by a [`command::History`]
//! (NFR-MNT-001).

pub mod command;
pub mod element;
pub mod ids;
pub mod layer;
pub mod material;
pub mod project;
pub mod samples;
pub mod storey;
pub mod template;
pub mod units;
pub mod validate;

pub use command::{Command, CommandError, History};
pub use element::{
    Element, ElementKind, IfcClass, KindTag, Position, ReferenceLine, Slab, SlabReference, TopLink, Wall,
};
pub use ids::{BuildingId, ElementId, LayerId, MaterialId, ProjectId, SiteId, StoreyId, ViewId};
pub use layer::Layer;
pub use material::{CutFill, FillPattern, Material, MaterialCategory, Rgb};
pub use project::{Project, ProjectInfo, ProjectSettings};
pub use storey::{Storey, StoreyList};

/// Re-exported so that dependants use the same vector types.
pub use glam;
