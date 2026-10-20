//! Element kinds (SRS 3.2).
//!
//! Every element has a kind: the tool that created it. Each kind is one
//! module holding its parameter payload; [`crate::element`] keeps the
//! kind-independent element envelope.

mod slab;
mod wall;

pub use self::slab::{Slab, SlabReference};
pub use self::wall::{ReferenceLine, Wall};
