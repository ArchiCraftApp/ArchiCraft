//! The project document (SRS 3.1, 3.4, 4.1, 4.2).

use glam::DVec2;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::element::{Element, ElementKind, IfcClass, KindTag, Position, TopLink};
use crate::ids::{BuildingId, ElementId, LayerId, MaterialId, ProjectId, SiteId, StoreyId, ViewId};
use crate::kinds::{ReferenceLine, Slab, SlabReference, Wall};
use crate::layer::Layer;
use crate::material::Material;
use crate::storey::StoreyList;
use crate::units::UnitSettings;

/// Project information (PRJ-010).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProjectInfo {
    pub name: String,
    pub number: String,
    pub description: String,
    pub client: String,
    pub address: String,
    pub author: String,
}

/// Curve tessellation tolerances (IFX-034).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tessellation {
    /// Maximum chordal deviation in metres.
    pub chord_deviation: f64,
    /// Maximum angle per segment in degrees.
    pub max_angle_deg: f64,
}

impl Default for Tessellation {
    fn default() -> Self {
        Self {
            chord_deviation: 0.002,
            max_angle_deg: 10.0,
        }
    }
}

/// The drafting grid of the plan views, one for all storeys (INP-013). It is a
/// project setting, so everyone who opens the file sees the same grid
/// (PRJ-007).
///
/// Values are kept as entered, without rounding: [`Command::SetSettings`]
/// compares settings exactly to detect a change.
///
/// [`Command::SetSettings`]: crate::Command::SetSettings
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanGrid {
    /// Steps of the invisible snap grid along its X and Y axes, in metres.
    pub snap_step: DVec2,
    pub snap_enabled: bool,
    /// Spacing of the main lines of the construction grid, in metres.
    pub main_spacing: DVec2,
    /// Spacing of the auxiliary lines between the main lines, in metres.
    pub auxiliary_spacing: DVec2,
    /// Rotation of both grids about `origin`, in radians.
    pub rotation: f64,
    /// Local origin of both grids in project coordinates.
    pub origin: DVec2,
    /// Whether the construction grid is shown.
    pub visible: bool,
}

impl Default for PlanGrid {
    fn default() -> Self {
        Self {
            snap_step: DVec2::splat(0.05),
            snap_enabled: true,
            main_spacing: DVec2::splat(1.0),
            auxiliary_spacing: DVec2::splat(0.25),
            rotation: 0.0,
            origin: DVec2::ZERO,
            visible: false,
        }
    }
}

impl PlanGrid {
    /// Checks the ranges of INP-013: steps and spacings are finite, positive
    /// and at most [`validate::MAX_GRID_SPACING`], the auxiliary spacing is
    /// not larger than the main spacing, the rotation is finite and the
    /// origin lies within [`validate::MAX_COORDINATE`].
    ///
    /// [`validate::MAX_GRID_SPACING`]: crate::validate::MAX_GRID_SPACING
    /// [`validate::MAX_COORDINATE`]: crate::validate::MAX_COORDINATE
    pub fn check(&self) -> Result<(), String> {
        use crate::validate::{MAX_GRID_SPACING, valid_grid_origin, valid_grid_spacing};
        let range = format!("must be greater than 0 and at most {MAX_GRID_SPACING} m");
        if !valid_grid_spacing(self.snap_step) {
            return Err(format!("The snap grid steps {range}."));
        }
        if !valid_grid_spacing(self.main_spacing) || !valid_grid_spacing(self.auxiliary_spacing) {
            return Err(format!("The construction grid spacings {range}."));
        }
        if self.auxiliary_spacing.cmpgt(self.main_spacing).any() {
            return Err("The auxiliary grid spacing must not be larger than the main spacing.".into());
        }
        if !self.rotation.is_finite() {
            return Err("The grid rotation must be a finite number.".into());
        }
        if !valid_grid_origin(self.origin) {
            return Err("The grid origin is too far from the project origin.".into());
        }
        Ok(())
    }
}

/// Project-level settings (PRJ-007, STR-008, STR-009, INP-013).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectSettings {
    pub units: UnitSettings,
    /// Default plan cut-plane height above the storey elevation (STR-008).
    pub cut_plane_height: f64,
    pub tessellation: Tessellation,
    /// The drafting grid shared by all plan views (INP-013).
    pub plan_grid: PlanGrid,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            units: UnitSettings::default(),
            cut_plane_height: 1.10,
            tessellation: Tessellation::default(),
            plan_grid: PlanGrid::default(),
        }
    }
}

/// Project location (STR-011, STR-012).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Location {
    /// Altitude of project zero above sea level, in metres.
    pub altitude: f64,
    /// Projected coordinates of the project origin (eastings, northings).
    pub survey_point: [f64; 2],
    /// Angle from project north to true north, in radians.
    pub true_north: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Site {
    pub id: SiteId,
    pub name: String,
    pub location: Location,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Building {
    pub id: BuildingId,
    pub name: String,
}

/// Projection of a 3D view (VIEW-020).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Projection {
    Perspective,
    Parallel,
}

impl Projection {
    pub fn code(self) -> &'static str {
        match self {
            Projection::Perspective => "perspective",
            Projection::Parallel => "parallel",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "perspective" => Some(Projection::Perspective),
            "parallel" => Some(Projection::Parallel),
            _ => None,
        }
    }
}

/// A 3D view definition (VIEW-002). Plan views are implicit: one per storey.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct View3d {
    pub id: ViewId,
    pub name: String,
    pub projection: Projection,
}

/// Defaults of the wall tool (ELM-001, Appendix D.2).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WallDefaults {
    pub thickness: f64,
    pub reference_line: ReferenceLine,
    pub base_offset: f64,
    pub top: TopLink,
    pub material: MaterialId,
    pub layer: LayerId,
    pub position: Position,
    pub ifc_class: IfcClass,
}

/// Defaults of the slab tool (ELM-001, Appendix D.2).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SlabDefaults {
    pub thickness: f64,
    pub reference: SlabReference,
    pub offset: f64,
    pub material: MaterialId,
    pub layer: LayerId,
    pub position: Position,
    pub ifc_class: IfcClass,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolDefaults {
    pub wall: WallDefaults,
    pub slab: SlabDefaults,
}

/// Counters for automatic element names (ELM-019).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NameCounters {
    pub wall: u32,
    pub slab: u32,
}

/// The project: settings, spatial structure, resources and elements.
///
/// Mutate a project only through [`crate::command::History`] (NFR-MNT-001);
/// the fields are public for reading and for building projects in tests and
/// readers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub info: ProjectInfo,
    pub settings: ProjectSettings,
    pub site: Site,
    pub building: Building,
    pub storeys: StoreyList,
    pub layers: Vec<Layer>,
    pub materials: Vec<Material>,
    pub elements: IndexMap<ElementId, Element>,
    pub tool_defaults: ToolDefaults,
    pub views: Vec<View3d>,
    pub counters: NameCounters,
    /// IFCX `header.dataVersion`: incremented on every save (IFX-002, O-16).
    pub data_version: u64,
    /// Incremented on every model change; used to invalidate caches. Not saved.
    #[serde(skip)]
    pub revision: u64,
}

impl Project {
    pub fn layer(&self, id: LayerId) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    pub fn material(&self, id: MaterialId) -> Option<&Material> {
        self.materials.iter().find(|m| m.id == id)
    }

    pub fn element(&self, id: ElementId) -> Option<&Element> {
        self.elements.get(&id)
    }

    pub fn view(&self, id: ViewId) -> Option<&View3d> {
        self.views.iter().find(|v| v.id == id)
    }

    /// Elements whose home storey is `storey`, in model order.
    pub fn elements_on(&self, storey: StoreyId) -> impl Iterator<Item = &Element> {
        self.elements.values().filter(move |e| e.storey == storey)
    }

    /// True if the element is on a visible layer.
    pub fn is_visible(&self, element: &Element) -> bool {
        self.layer(element.layer).is_none_or(|l| l.visible)
    }

    /// Why an element cannot be edited, if it cannot (ELM-013, LAY-001).
    pub fn edit_lock_reason(&self, element: &Element) -> Option<String> {
        if element.locked {
            return Some(format!("{} is locked.", element.name));
        }
        match self.layer(element.layer) {
            Some(layer) if layer.locked => Some(format!("{} is on the locked layer “{}”.", element.name, layer.name)),
            _ => None,
        }
    }

    pub fn is_editable(&self, element: &Element) -> bool {
        self.edit_lock_reason(element).is_none()
    }

    /// Absolute elevation of the plan cut plane of a storey (STR-008).
    pub fn cut_plane_height(&self, storey: StoreyId) -> f64 {
        self.storeys
            .get(storey)
            .and_then(|s| s.cut_plane_height)
            .unwrap_or(self.settings.cut_plane_height)
    }

    /// Absolute elevation of a top link for an element on `home` with its
    /// base at `base` (ELM-006).
    pub fn top_elevation(&self, home: StoreyId, base: f64, top: TopLink) -> Option<f64> {
        match top {
            TopLink::Fixed { height } => Some(base + height),
            TopLink::Storey { storey, offset } => Some(self.storeys.get(storey)?.elevation + offset),
            TopLink::Relative { storeys, offset } => Some(self.storeys.relative_elevation(home, storeys)? + offset),
        }
    }

    /// Absolute bottom and top elevations of an element.
    pub fn vertical_extent(&self, element: &Element) -> Option<(f64, f64)> {
        let home = self.storeys.get(element.storey)?.elevation;
        match &element.kind {
            ElementKind::Wall(wall) => {
                let bottom = home + wall.base_offset;
                Some((bottom, self.top_elevation(element.storey, bottom, wall.top)?))
            }
            ElementKind::Slab(slab) => {
                let reference = home + slab.offset;
                Some(match slab.reference {
                    SlabReference::Top => (reference - slab.thickness, reference),
                    SlabReference::Bottom => (reference, reference + slab.thickness),
                })
            }
        }
    }

    /// Number of elements per layer, for the layer manager (LAY-002).
    pub fn element_count_on_layer(&self, layer: LayerId) -> usize {
        self.elements.values().filter(|e| e.layer == layer).count()
    }

    /// Automatic name of the `nth` (0-based) next element of a kind, for
    /// example `W-004` (ELM-019). The counter itself advances when elements
    /// are added.
    pub fn next_element_name(&self, kind: KindTag, nth: u32) -> String {
        let n = match kind {
            KindTag::Wall => self.counters.wall,
            KindTag::Slab => self.counters.slab,
        } + 1
            + nth;
        format!("{}-{n:03}", kind.name_prefix())
    }

    /// A new wall on `storey` with the wall tool's defaults (ELM-001). `nth`
    /// numbers walls created together.
    pub fn new_wall(&self, storey: StoreyId, start: DVec2, end: DVec2, nth: u32) -> Element {
        let d = &self.tool_defaults.wall;
        Element {
            id: ElementId::new(),
            name: self.next_element_name(KindTag::Wall, nth),
            storey,
            layer: d.layer,
            ifc_class: d.ifc_class,
            position: d.position,
            locked: false,
            kind: ElementKind::Wall(Wall {
                start,
                end,
                thickness: d.thickness,
                reference_line: d.reference_line,
                base_offset: d.base_offset,
                top: d.top,
                material: d.material,
            }),
        }
    }

    /// A new slab on `storey` with the slab tool's defaults (ELM-001).
    pub fn new_slab(&self, storey: StoreyId, outline: Vec<DVec2>, holes: Vec<Vec<DVec2>>, nth: u32) -> Element {
        let d = &self.tool_defaults.slab;
        Element {
            id: ElementId::new(),
            name: self.next_element_name(KindTag::Slab, nth),
            storey,
            layer: d.layer,
            ifc_class: d.ifc_class,
            position: d.position,
            locked: false,
            kind: ElementKind::Slab(Slab {
                outline,
                holes,
                thickness: d.thickness,
                reference: d.reference,
                offset: d.offset,
                material: d.material,
            }),
        }
    }

    /// The storey, layer and material references that are dangling, as a
    /// list of human-readable problems (NFR-REL-004).
    pub fn integrity_problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        for e in self.elements.values() {
            if self.storeys.get(e.storey).is_none() {
                problems.push(format!("{}: home storey is missing", e.name));
            }
            if self.layer(e.layer).is_none() {
                problems.push(format!("{}: layer is missing", e.name));
            }
            if self.material(e.kind.material()).is_none() {
                problems.push(format!("{}: material is missing", e.name));
            }
        }
        problems
    }
}
