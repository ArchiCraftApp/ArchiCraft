//! Geometry generators per element kind and their registry (SRS 3.2,
//! IFX-016, NFR-MNT-002, NFR-MNT-004).
//!
//! The model declares the kinds and, for compound elements, their parts
//! ([`archicraft_model::kinds::Kind::parts`]); this crate generates the
//! bodies. Each kind's generator is one module here implementing
//! [`GeometryKind`] for the model's payload, listed once in the
//! `generators!` invocation below, which generates [`GENERATORS`] and the
//! dispatch of [`local_extent`] and [`evaluate`]. Adding a kind's geometry
//! is one module plus one line, as in the model's registry, and
//! [`crate::evaluate`] stays generic. The IFCX writer joins the model's part
//! classes and the part bodies by role.
//!
//! A registry of its own, because the model must not depend on geometry
//! (NFR-MNT-002, NFR-MNT-004). Test builds register the test kinds too (cargo
//! feature `test-kinds`), so nothing outside this module iterates the table
//! to produce output that reaches golden files.
//!
//! Neighbour data a generator needs from other elements, such as the
//! junction footprint of a wall, is resolved before the kinds run and handed
//! in through [`EvalContext`], so that evaluation can later become
//! incremental.

use archicraft_model::{Element, ElementKind, KindTag, Project};

use crate::GeometryError;
use crate::evaluate::ElementGeometry;
use crate::wall::Footprint;

/// What a generator evaluates one element from.
#[derive(Clone, Copy, Debug)]
pub struct EvalContext<'a> {
    pub project: &'a Project,
    pub element: &'a Element,
    /// The junction-resolved footprint of a wall, if one was computed
    /// ([`crate::wall::footprints`]); `None` for other kinds. A slot for
    /// other neighbour data arrives with incremental evaluation.
    pub footprint: Option<&'a Footprint>,
}

impl EvalContext<'_> {
    /// The elevation of the element's home storey.
    pub fn home_elevation(&self) -> Result<f64, GeometryError> {
        self.project
            .storeys
            .get(self.element.storey)
            .map(|storey| storey.elevation)
            .ok_or(GeometryError::MissingHomeStorey)
    }

    /// Absolute bottom and top of the element (ELM-006), resolved through
    /// the kind's vertical extent hook.
    pub fn vertical_extent(&self) -> Result<(f64, f64), GeometryError> {
        self.home_elevation()?;
        self.project
            .vertical_extent(self.element)
            .ok_or(GeometryError::MissingLinkedStorey)
    }

    /// Bottom and top of the element relative to its home storey: the
    /// default of [`GeometryKind::local_extent`].
    pub fn storey_relative_extent(&self) -> Result<(f64, f64), GeometryError> {
        let home = self.home_elevation()?;
        let (bottom, top) = self.vertical_extent()?;
        Ok((bottom - home, top - home))
    }
}

/// The geometry of one element kind, implemented for the model's payload.
/// Bodies are in the coordinates of the element's home storey (see
/// [`crate::evaluate`]).
pub trait GeometryKind {
    /// Bottom and top of the element relative to its home storey. An element
    /// lower than the model tolerance gets no body: [`GeometryKind::evaluate`]
    /// is not called for it (NFR-ACC-003).
    fn local_extent(&self, ctx: &EvalContext<'_>) -> Result<(f64, f64), GeometryError> {
        ctx.storey_relative_extent()
    }

    /// The element's bodies: its own parts and axis, and one
    /// [`PartGeometry`](crate::evaluate::PartGeometry) per entry of the
    /// model's `Kind::parts`, in that order and with those roles (IFX-016).
    fn evaluate(&self, ctx: &EvalContext<'_>) -> Result<ElementGeometry, GeometryError>;
}

/// Generates the registry from one list of `module => Payload` entries, each
/// optionally behind attributes such as `#[cfg(…)]`; see the module
/// documentation.
macro_rules! generators {
    ($($(#[$meta:meta])* $module:ident => $Payload:ident),+ $(,)?) => {
        $($(#[$meta])* pub mod $module;)+

        /// The kinds with a generator, in registry order.
        pub static GENERATORS: &[KindTag] = &[$($(#[$meta])* KindTag::$Payload,)+];

        /// See [`GeometryKind::local_extent`]. `ElementKind` is
        /// non-exhaustive, so a kind without a generator is an error here;
        /// `tests/kinds.rs` checks that this build has none.
        pub fn local_extent(ctx: &EvalContext<'_>) -> Result<(f64, f64), GeometryError> {
            match &ctx.element.kind {
                $($(#[$meta])* ElementKind::$Payload(kind) => kind.local_extent(ctx),)+
                _ => Err(GeometryError::UnsupportedKind(ctx.element.tag())),
            }
        }

        /// See [`GeometryKind::evaluate`].
        pub fn evaluate(ctx: &EvalContext<'_>) -> Result<ElementGeometry, GeometryError> {
            match &ctx.element.kind {
                $($(#[$meta])* ElementKind::$Payload(kind) => kind.evaluate(ctx),)+
                _ => Err(GeometryError::UnsupportedKind(ctx.element.tag())),
            }
        }
    };
}

// Every kind's generator, once. Adding a kind is a new module plus one line here.
generators! {
    wall => Wall,
    slab => Slab,
    #[cfg(feature = "test-kinds")] dummy => Dummy,
    #[cfg(feature = "test-kinds")] note => Note,
}
