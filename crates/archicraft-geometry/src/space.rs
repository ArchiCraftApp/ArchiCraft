//! Space boundaries detected from walls (SPC-001, INP-018).
//!
//! A click inside a room bounded by walls gives the room's boundary: the
//! smallest region around the click in the [`ContourGraph`] of the wall
//! footprints of the click's storey, as [`crate::wall::footprints`] resolves
//! them, so L and T junctions close the corners. The graph is built from the
//! footprints and not from the plan's cut polygons, so that door and window
//! openings cut at the cut plane do not open a room, walls standing free in
//! a room become its holes, and the openings of slabs, such as a stair hole,
//! are none.
//!
//! Every footprint edge is an edge of the graph, so each face of the graph
//! lies either inside a footprint or outside all of them: a seed outside
//! every footprint picks a room. As in the rest of contour detection,
//! footprints closer than [`MODEL_TOLERANCE`](archicraft_model::MODEL_TOLERANCE)
//! touch and wider gaps leak (NFR-ACC-003), which leaves the seed in no
//! region ([`RegionError::Unbounded`]).
//!
//! The walls are those whose home storey is the storey, visible or not, so
//! that the boundary depends on the model only. Columns, curtain walls and
//! free-form solids join as sources once their kinds exist.

use archicraft_model::outline::Outline;
use archicraft_model::{Project, StoreyId};
use glam::DVec2;

use crate::contour::{ContourGraph, ContourOptions, Region, Segment};
use crate::kinds::wall::wall_inputs;
use crate::plan::ring_contains;
use crate::wall::{Footprint, WallInput, footprints};

/// Why no space boundary surrounds a seed (SPC-001). The space tool turns
/// it into its prompt; it carries no message of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionError {
    /// No closed region of the walls surrounds the seed: it lies outside the
    /// building, or the walls around it leave a gap wider than the model
    /// tolerance.
    Unbounded,
    /// The seed lies inside a wall's footprint.
    SeedInFootprint,
}

/// The regions of one storey that space boundaries are detected from
/// (SPC-001, INP-018): the contour graph of the footprints of the walls on
/// the storey, and the footprints. Built once per storey and model
/// revision, it answers any number of seeds, such as the hovered and the
/// clicked point of the space tool.
#[derive(Clone, Debug)]
pub struct SpaceRegions {
    storey: StoreyId,
    footprints: Vec<Footprint>,
    graph: ContourGraph,
}

impl SpaceRegions {
    /// Resolves the junctions of the walls whose home storey is `storey` and
    /// builds the contour graph of their footprints.
    pub fn build(project: &Project, storey: StoreyId) -> Self {
        let walls: Vec<WallInput> = wall_inputs(project)
            .into_iter()
            .filter(|wall| wall.storey == storey)
            .collect();
        let footprints: Vec<Footprint> = footprints(&walls).into_values().collect();
        // The graph puts the segments into a canonical order, so the order
        // of the walls and of the map does not matter.
        let segments: Vec<Segment> = footprints
            .iter()
            .flat_map(|corners| (0..4).map(|i| Segment::new(corners[i], corners[(i + 1) % 4])))
            .collect();
        let graph = ContourGraph::build(segments.as_slice(), &ContourOptions::default());
        Self {
            storey,
            footprints,
            graph,
        }
    }

    /// The storey the regions belong to.
    pub fn storey(&self) -> StoreyId {
        self.storey
    }

    /// The smallest region around `seed`, counter-clockwise, with the outer
    /// boundaries of the walls standing free in it as islands, clockwise
    /// like the holes of a slab (SPC-001, INP-018).
    ///
    /// A seed exactly on a wall face is decided by the even-odd test of the
    /// footprint, so it gives the room on some faces and
    /// [`RegionError::SeedInFootprint`] on others: pick with the cursor as it
    /// is, not with a point snapped onto a wall face, as the space tool does.
    pub fn region_at(&self, seed: DVec2) -> Result<Region, RegionError> {
        if self.footprints.iter().any(|corners| ring_contains(corners, seed)) {
            return Err(RegionError::SeedInFootprint);
        }
        self.graph.region_at(seed).ok_or(RegionError::Unbounded)
    }
}

/// The boundary of a space around `seed` on `storey` (SPC-001): the
/// smallest region around it bounded by the storey's walls, the outer ring
/// counter-clockwise and the walls standing free in it as holes, clockwise.
/// It builds the storey's [`SpaceRegions`] for this one seed; a tool that
/// asks for many keeps the [`SpaceRegions`] instead.
pub fn space_region(project: &Project, storey: StoreyId, seed: DVec2) -> Result<Outline, RegionError> {
    SpaceRegions::build(project, storey)
        .region_at(seed)
        .map(|region| Outline::from_point_rings(std::iter::once(region.outer).chain(region.islands)))
}
