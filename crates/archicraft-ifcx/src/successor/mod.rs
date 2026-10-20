//! A mock successor of `ifcx_alpha`, compiled only with the cargo feature
//! `successor-stub` (IFX-091, NFR-MNT-004).
//!
//! No successor of `ifcx_alpha` is published yet. This adapter stands in for
//! one: its files are `ifcx_alpha` files under another `header.ifcxVersion`,
//! so its reader maps the header and hands the file to the alpha reader. It
//! proves that a second IFCX version plugs into [`crate::ADAPTERS`] next to
//! [`crate::alpha`] without any change to the domain model or geometry
//! crates. A real successor copies `alpha/` instead and follows the runbook
//! docs/ifcx-evolution.md.

pub mod reader;

/// The `header.ifcxVersion` of the mock successor.
pub const IFCX_VERSION: &str = "ifcx_successor_mock";
