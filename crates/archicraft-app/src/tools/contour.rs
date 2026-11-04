//! Contour detection (INP-018, EDT-020): while it is on, a creation tool's
//! click picks a contour from a [`ContourGraph`] instead of taking a point,
//! and the tool creates its element from it.
//!
//! The detection holds no tool: the flag, the pick step and the pick are the
//! same for every tool that plugs it in, which says which contours it
//! [accepts](Accepts) and previews what it creates from the picked one. The
//! view under the cursor supplies the graph of its plane
//! ([`ContourDetection::set_source`]), the cached one of a storey's plan for
//! the wall and slab tools; other tools can supply their own.

use std::sync::Arc;

use archicraft_geometry::contour::{Chain, ContourGraph, Region};
use archicraft_model::StoreyId;
use archicraft_model::glam::DVec2;

use crate::input::{Expect, KeyHint, StepSpec};
use crate::tr;

/// What a click picks (INP-018).
#[derive(Clone, Debug, PartialEq)]
pub enum Contour {
    /// The smallest closed region around the point, with its islands.
    Region(Region),
    /// The clicked edge or the chain of connected edges.
    Chain(Chain),
}

/// The contours a tool creates its elements from (INP-018). The preview and
/// the click pick with the same value, so the preview never shows a contour
/// the click would reject.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accepts {
    /// Every contour: the clicked edge, open and closed chains and regions,
    /// as walls run along any of them.
    All,
    /// Closed contours only, regions and closed chains, as for an outline
    /// such as a slab's: near an open chain a click picks the region around
    /// the point, and Alt picks no single edge.
    Closed,
}

/// The contour-detection state of the creation tools (INP-018): on while
/// toggled by the Contour detection command or while its key is held.
#[derive(Clone, Debug, Default)]
pub struct ContourDetection {
    /// Switched on and off by the Contour detection command (EDT-020).
    pub toggled: bool,
    /// The key that switches to detection is held.
    pub held: bool,
    /// Alt is held: a click picks the clicked edge only (provisional), where
    /// the tool [accepts](Accepts::All) open contours.
    pub edge_only: bool,
    /// The graph picks are made from, the storey it belongs to and the pick
    /// radius in model units.
    source: Option<(StoreyId, Arc<ContourGraph>, f64)>,
}

impl ContourDetection {
    pub fn is_on(&self) -> bool {
        self.toggled || self.held
    }

    pub fn toggle(&mut self) {
        self.toggled = !self.is_on();
        self.held = false;
    }

    /// Turns detection off (Esc, INP-003). Returns `false` when it was off.
    pub fn cancel(&mut self) -> bool {
        let on = self.is_on();
        self.toggled = false;
        self.held = false;
        on
    }

    /// Picks from `graph`, the plane of `storey`, within `radius` of an edge.
    pub fn set_source(&mut self, storey: StoreyId, graph: Arc<ContourGraph>, radius: f64) {
        self.source = Some((storey, graph, radius));
    }

    /// The contour under `p` on `storey` that a tool taking `accepts` creates
    /// its element from: with Alt the clicked edge, otherwise the chain of
    /// connected edges within the pick radius, or else the smallest region
    /// around `p`. `None` on another storey than the source's.
    pub fn pick(&self, storey: StoreyId, p: DVec2, accepts: Accepts) -> Option<Contour> {
        let (on, graph, radius) = self.source.as_ref()?;
        if *on != storey {
            return None;
        }
        let open = accepts == Accepts::All;
        if self.edge_only && open {
            return graph.edge_at(p, *radius).map(Contour::Chain);
        }
        graph
            .chain_at(p, *radius)
            .filter(|chain| open || chain.closed)
            .map(Contour::Chain)
            .or_else(|| graph.region_at(p).map(Contour::Region))
    }

    /// The contour under `p` on the source's storey, for the preview.
    pub fn hovered(&self, p: DVec2, accepts: Accepts) -> Option<Contour> {
        self.pick(self.source.as_ref()?.0, p, accepts)
    }

    /// The pick step of a tool taking `accepts`, with its prompt (INP-001,
    /// NFR-USE-003); Alt is a hint only where it picks the clicked edge.
    pub fn step(&self, prompt: String, accepts: Accepts) -> StepSpec {
        let edge = (accepts == Accepts::All).then(|| KeyHint::new("Alt", tr!("prompt-contour-key-edge")));
        StepSpec {
            hints: edge
                .into_iter()
                .chain([KeyHint::new("Esc", tr!("prompt-contour-key-cancel"))])
                .collect(),
            ..StepSpec::new(prompt, Expect::Pick)
        }
    }
}
