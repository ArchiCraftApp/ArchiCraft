//! The space tool (SPC-001): its geometry methods, the detection of a room
//! bounded by walls (INP-018), and the default name of a new space.
//!
//! Detection is the space tool's first method: a click inside a room picks
//! the room's region from the [`SpaceRegions`] of the storey, the wall
//! footprints, and not from the plan's contour graph, so door openings do
//! not open a room. The polygon and rectangle methods are the slab tool's
//! (SLB-001), on the same point input, so snapping and typed lengths work
//! as for slabs (NFR-USE-002).

use std::sync::Arc;

use archicraft_geometry::contour::Region;
use archicraft_geometry::space::{RegionError, SpaceRegions};
use archicraft_model::glam::DVec2;
use archicraft_model::{BoundarySource, Element, KindTag, Project, StoreyId};

use super::SlabMode;
use crate::input::{Expect, KeyHint, StepSpec};
use crate::tr;

/// Geometry methods of the space tool (SPC-001).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SpaceMode {
    /// A click inside a room bounded by walls (INP-018).
    #[default]
    Detect,
    Polygon,
    Rectangle,
}

impl SpaceMode {
    pub const ALL: [SpaceMode; 3] = [SpaceMode::Detect, SpaceMode::Polygon, SpaceMode::Rectangle];

    /// The stable code in the id of the mode's command,
    /// `tool.space.mode.<code>` (UI-017).
    pub fn code(self) -> &'static str {
        match self {
            SpaceMode::Detect => "detect",
            SpaceMode::Polygon => "polygon",
            SpaceMode::Rectangle => "rectangle",
        }
    }

    /// The slab tool's method this method draws with (SLB-001), `None` for
    /// detection.
    pub fn outline(self) -> Option<SlabMode> {
        match self {
            SpaceMode::Detect => None,
            SpaceMode::Polygon => Some(SlabMode::Polygon),
            SpaceMode::Rectangle => Some(SlabMode::Rectangle),
        }
    }
}

/// The regions the space tool detects boundaries from (SPC-001, INP-018):
/// the view under the cursor supplies the [`SpaceRegions`] of its storey
/// and the cursor, so the prompt can say why no room lies under it.
#[derive(Clone, Debug, Default)]
pub struct SpaceDetection {
    source: Option<Arc<SpaceRegions>>,
    /// The cursor as it is, not snapped: a point snapped onto a wall face
    /// would lie on the boundary of the footprint.
    hovered: Option<DVec2>,
}

impl SpaceDetection {
    /// Picks from `regions`, with the cursor at `cursor` on its storey.
    pub fn set_source(&mut self, regions: Arc<SpaceRegions>, cursor: DVec2) {
        self.source = Some(regions);
        self.hovered = Some(cursor);
    }

    /// The storey of the regions picked from.
    pub fn storey(&self) -> Option<StoreyId> {
        self.source.as_ref().map(|regions| regions.storey())
    }

    /// The room around `p` on `storey`, or why there is none; `None` without
    /// regions of that storey.
    pub fn pick(&self, storey: StoreyId, p: DVec2) -> Option<Result<Region, RegionError>> {
        let regions = self.source.as_ref().filter(|regions| regions.storey() == storey)?;
        Some(regions.region_at(p))
    }

    /// The pick step (NFR-USE-003): the prompt says why the cursor lies in
    /// no room, if it does; Esc is a hint while the shared contour detection
    /// is toggled or held (`detection_on`), in any method, since Esc then
    /// turns it off.
    pub fn step(&self, detection_on: bool) -> StepSpec {
        let hovered = self
            .hovered
            .zip(self.storey())
            .and_then(|(p, storey)| self.pick(storey, p));
        let prompt = match hovered {
            Some(Err(RegionError::Unbounded)) => tr!("prompt-space-region-open"),
            Some(Err(RegionError::SeedInFootprint)) => tr!("prompt-space-in-wall"),
            _ => tr!("prompt-space-detect"),
        };
        StepSpec {
            hints: detection_on
                .then(|| KeyHint::new("Esc", tr!("prompt-contour-key-cancel")))
                .into_iter()
                .collect(),
            ..StepSpec::new(prompt, Expect::Pick)
        }
    }
}

/// A new space on `storey` with the boundary `outer` and `holes` and the
/// space tool's defaults (SPC-001): its long name is the default name with
/// `{n}` replaced by the space's number in its kind (ELM-019, `SP-001` gives
/// 1), the long name of the defaults slot when set, for example `Raum {n}`,
/// else the localised `Room {n}`. The room number stays as the defaults
/// give it, empty in the template.
pub fn new_space(
    project: &Project,
    storey: StoreyId,
    outer: Vec<DVec2>,
    holes: Vec<Vec<DVec2>>,
    source: BoundarySource,
) -> Element {
    let mut element = project.new_space(storey, outer, holes, 0);
    let n = project.counters.next(KindTag::Space).to_string();
    if let Some(space) = element.as_space_mut() {
        space.long_name = if space.long_name.is_empty() {
            tr!("tool-space-default-name", n = n)
        } else {
            space.long_name.replace("{n}", &n)
        };
        space.source = source;
    }
    element
}
