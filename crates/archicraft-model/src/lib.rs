//! ArchiCraft domain model (SRS Section 3).
//!
//! The model is plain Rust data, independent of the user interface, geometry
//! evaluation and file format (NFR-MNT-002, NFR-MNT-004). Every change goes
//! through a [`command::Command`] executed by a [`command::History`]
//! (NFR-MNT-001).

pub mod command;
pub mod element;
pub mod fixtures;
pub mod ids;
pub mod kinds;
pub mod layer;
pub mod material;
pub mod message;
pub mod project;
pub mod samples;
pub mod storey;
pub mod template;
pub mod tolerance;
pub mod transform;
pub mod units;
pub mod validate;

pub use command::{Command, CommandError, History};
pub use element::{Element, ElementKind, IfcClass, KindTag, Position, TopLink};
pub use ids::{BuildingId, ElementId, LayerId, MaterialId, ProjectId, SiteId, StoreyId, ViewId};
pub use kinds::{ReferenceLine, Slab, SlabReference, Wall};
pub use layer::Layer;
pub use material::{CutFill, FillPattern, Material, MaterialCategory, Rgb};
pub use message::{Arg, Message};
pub use project::{PlanGrid, Project, ProjectInfo, ProjectSettings};
pub use storey::{Storey, StoreyList};
pub use tolerance::{ANGLE_TOLERANCE, JOIN_TOLERANCE, MODEL_TOLERANCE};

/// Re-exported so that dependants use the same vector types.
pub use glam;
