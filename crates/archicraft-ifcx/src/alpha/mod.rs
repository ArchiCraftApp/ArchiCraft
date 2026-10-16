//! The adapter for `ifcx_alpha`, the IFCX version this release reads and
//! writes (CON-2, IFX-090). A future IFCX version gets a sibling module; the
//! domain model and geometry crates do not change (NFR-MNT-004).

pub mod reader;
pub mod schema;
pub mod writer;
