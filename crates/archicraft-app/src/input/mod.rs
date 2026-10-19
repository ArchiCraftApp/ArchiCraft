//! The headless point-input service (INP-001…INP-009).
//!
//! Every point the user defines passes through this module: typed values,
//! the angle constraint and snapping. It knows nothing of the windowing
//! toolkit, so plan views, 3D views and every tool share it.

mod constrain;
mod keys;
mod snap;
mod typed;

pub use constrain::constrain;
pub use keys::{KeyInput, Modifiers};
pub use snap::{Snap, SnapKind, SnapTargets, snap};
pub use typed::{Field, TypedInput};
