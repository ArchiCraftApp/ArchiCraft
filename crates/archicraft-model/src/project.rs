//! The project document (SRS 3.1, 3.4, 4.1, 4.2).

use std::collections::BTreeMap;

use glam::DVec2;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::element::{Element, ElementKind, IfcClass, KindTag, Position, PredefinedType, StructuralFunction};
use crate::group::Group;
use crate::ids::{
    BuildingId, ElementId, GroupId, LayerId, MaterialId, ProjectId, RelationId, SiteId, StoreyId, TextStyleId, ViewId,
};
use crate::layer::Layer;
use crate::material::Material;
use crate::message::Message;
use crate::relations::RelationStore;
use crate::storey::StoreyList;
use crate::template::TemplateContext;
use crate::text::TextStyle;
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
    /// Whether points snap to the snap grid.
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
    pub fn check(&self) -> Result<(), Message> {
        use crate::validate::{MAX_COORDINATE, MAX_GRID_SPACING, valid_grid_origin, valid_grid_spacing};
        if !valid_grid_spacing(self.snap_step) {
            return Err(Message::new("model-error-snap-grid-range").length("max", MAX_GRID_SPACING));
        }
        if !valid_grid_spacing(self.main_spacing) || !valid_grid_spacing(self.auxiliary_spacing) {
            return Err(Message::new("model-error-construction-grid-range").length("max", MAX_GRID_SPACING));
        }
        if self.auxiliary_spacing.cmpgt(self.main_spacing).any() {
            return Err(Message::new("model-error-auxiliary-spacing"));
        }
        if !self.rotation.is_finite() {
            return Err(Message::new("model-error-grid-rotation"));
        }
        if !valid_grid_origin(self.origin) {
            return Err(Message::new("model-error-grid-origin").length("max", MAX_COORDINATE));
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

/// The defaults of one tool (ELM-001): the layer, position, structural
/// function, IFC class and predefined type a new element gets, and a
/// prototype payload whose parameters it copies.
/// The payload's geometry is a placeholder that the tool replaces (a wall
/// from the origin to X, a slab over the unit square).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DefaultsSlot {
    pub layer: LayerId,
    pub position: Position,
    /// ELM-020; defaulted, so journals of older sessions replay.
    #[serde(default)]
    pub structural: StructuralFunction,
    /// One of the kind's selectable classes (ELM-011).
    pub ifc_class: IfcClass,
    /// One of the types the kind offers for `ifc_class`, or none (IFX-024).
    #[serde(default)]
    pub predefined_type: Option<PredefinedType>,
    pub kind: ElementKind,
}

impl DefaultsSlot {
    pub fn tag(&self) -> KindTag {
        self.kind.tag()
    }
}

/// The tool defaults of a project, one [`DefaultsSlot`] per kind with a tool
/// (ELM-001). Slots keep their insertion order, which is the registry order
/// for projects from the template or a file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct ToolDefaults(IndexMap<KindTag, DefaultsSlot>);

impl ToolDefaults {
    pub fn get(&self, kind: KindTag) -> Option<&DefaultsSlot> {
        self.0.get(&kind)
    }

    pub fn get_mut(&mut self, kind: KindTag) -> Option<&mut DefaultsSlot> {
        self.0.get_mut(&kind)
    }

    /// Sets the slot of the payload's kind, in place if the kind has one,
    /// and returns the slot it replaces.
    pub fn insert(&mut self, slot: DefaultsSlot) -> Option<DefaultsSlot> {
        self.0.insert(slot.tag(), slot)
    }

    /// The slots with the kinds they are stored under, in order.
    pub fn iter(&self) -> impl Iterator<Item = (KindTag, &DefaultsSlot)> {
        self.0.iter().map(|(kind, slot)| (*kind, slot))
    }

    pub fn slots_mut(&mut self) -> impl Iterator<Item = &mut DefaultsSlot> {
        self.0.values_mut()
    }
}

impl FromIterator<DefaultsSlot> for ToolDefaults {
    fn from_iter<I: IntoIterator<Item = DefaultsSlot>>(slots: I) -> Self {
        let mut defaults = Self::default();
        for slot in slots {
            defaults.insert(slot);
        }
        defaults
    }
}

/// Counters for automatic element names, one per kind (ELM-019).
///
/// A counter is the number of elements of its kind named so far; kinds
/// without a counter count zero. Zero counters are not stored, so two
/// counter sets with the same values compare equal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(from = "BTreeMap<KindTag, u32>")]
pub struct NameCounters(BTreeMap<KindTag, u32>);

impl From<BTreeMap<KindTag, u32>> for NameCounters {
    fn from(mut counters: BTreeMap<KindTag, u32>) -> Self {
        counters.retain(|_, n| *n != 0);
        Self(counters)
    }
}

impl NameCounters {
    /// The number of elements of `kind` named so far.
    pub fn get(&self, kind: KindTag) -> u32 {
        self.0.get(&kind).copied().unwrap_or(0)
    }

    /// The number the next element of `kind` gets.
    pub fn next(&self, kind: KindTag) -> u32 {
        self.get(kind).saturating_add(1)
    }

    /// Advances the counter of `kind` by one, as adding an element does.
    pub fn bump(&mut self, kind: KindTag) {
        let next = self.next(kind);
        self.set(kind, next);
    }

    pub fn set(&mut self, kind: KindTag, value: u32) {
        if value == 0 {
            self.0.remove(&kind);
        } else {
            self.0.insert(kind, value);
        }
    }
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
    /// The text styles the project's texts refer to, in list order
    /// (ANN-007); valid together by [`crate::text::check_text_styles`]. A
    /// project from the template has its presets.
    #[serde(default = "crate::template::template_text_styles")]
    pub text_styles: Vec<TextStyle>,
    pub elements: IndexMap<ElementId, Element>,
    /// Element groups, outermost and nested, in creation order (ELM-015).
    #[serde(default)]
    pub groups: IndexMap<GroupId, Group>,
    /// The explicit relations between elements (SRS 3.3). Derived storey
    /// links are not stored; they follow from the elements.
    #[serde(default)]
    pub relations: RelationStore,
    pub tool_defaults: ToolDefaults,
    pub views: Vec<View3d>,
    pub counters: NameCounters,
    /// The number of groups named so far: the next group is
    /// `Group <group_counter + 1>` (ELM-015). Like [`NameCounters`], it
    /// never goes back, so names do not repeat after a group is removed.
    #[serde(default)]
    pub group_counter: u32,
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

    /// The text style `id` (ANN-007).
    pub fn text_style(&self, id: TextStyleId) -> Option<&TextStyle> {
        self.text_styles.iter().find(|s| s.id == id)
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

    /// True if the element provides snap targets: it is visible and neither
    /// it nor its layer is excluded from snapping. Locking does not matter
    /// (INP-017).
    pub fn provides_snaps(&self, element: &Element) -> bool {
        !element.snap_excluded && self.layer(element.layer).is_none_or(|l| l.visible && !l.snap_excluded)
    }

    /// Why an element cannot be edited, if it cannot (ELM-013, LAY-001), as
    /// a message key (NFR-L10N-001).
    pub fn edit_lock_reason(&self, element: &Element) -> Option<Message> {
        if element.locked {
            return Some(Message::new("model-error-element-locked").text("name", &element.name));
        }
        match self.layer(element.layer) {
            Some(layer) if layer.locked => Some(
                Message::new("model-error-layer-locked")
                    .text("name", &element.name)
                    .text("layer", &layer.name),
            ),
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

    /// Absolute bottom and top elevations of an element, resolved through
    /// its kind's hook against the current storeys (ELM-006, STR-006).
    pub fn vertical_extent(&self, element: &Element) -> Option<(f64, f64)> {
        element.kind.vertical_extent(element.storey, &self.storeys)
    }

    /// Number of elements per layer, for the layer manager (LAY-002).
    pub fn element_count_on_layer(&self, layer: LayerId) -> usize {
        self.elements.values().filter(|e| e.layer == layer).count()
    }

    /// Automatic name of the `nth` (0-based) next element of a kind, for
    /// example `W-004` (ELM-019). The counter itself advances when elements
    /// are added.
    pub fn next_element_name(&self, kind: KindTag, nth: u32) -> String {
        let n = self.counters.next(kind).saturating_add(nth);
        format!("{}-{n:03}", kind.name_prefix())
    }

    /// The `nth` (0-based) next element of `kind` on `storey`, with the
    /// tool's defaults (the template's if the slot is missing) and their
    /// placeholder geometry, placed on `storey` ([`crate::kinds::Kind::place_on`]):
    /// with `nth` 0, the prototype the properties panel edits (ELM-001,
    /// ELM-019). `None` for a kind without a tool.
    pub fn new_element(&self, kind: KindTag, storey: StoreyId, nth: u32) -> Option<Element> {
        let slot = match self.tool_defaults.get(kind) {
            Some(slot) if slot.tag() == kind => slot.clone(),
            _ => (kind.descriptor().template_defaults?)(&TemplateContext::new(&self.materials, &self.layers)),
        };
        let mut payload = slot.kind;
        payload.place_on(storey);
        Some(Element {
            id: ElementId::new(),
            name: self.next_element_name(kind, nth),
            storey,
            layer: slot.layer,
            ifc_class: slot.ifc_class,
            predefined_type: slot.predefined_type,
            position: slot.position,
            structural: slot.structural,
            locked: false,
            snap_excluded: false,
            group: None,
            kind: payload,
        })
    }

    /// The dangling references of the project (NFR-REL-004): an element's
    /// storey, layer, material or text style (ANN-007) that does not exist,
    /// and an explicit relation whose end names no element. Each problem
    /// carries what a problems panel needs to select it (UI-012) and its
    /// message (NFR-L10N-001).
    pub fn integrity_problems(&self) -> Vec<IntegrityProblem> {
        let mut problems = Vec::new();
        for e in self.elements.values() {
            let mut problem = |key| {
                problems.push(IntegrityProblem {
                    element: Some(e.id),
                    relation: None,
                    message: Message::new(key).text("name", &e.name),
                });
            };
            if self.storeys.get(e.storey).is_none() {
                problem("model-problem-home-storey-missing");
            }
            if self.layer(e.layer).is_none() {
                problem("model-problem-layer-missing");
            }
            if e.kind.material().is_some_and(|m| self.material(m).is_none()) {
                problem("model-problem-material-missing");
            }
            if e.kind.text_style().is_some_and(|s| self.text_style(s).is_none()) {
                problem("model-problem-text-style-missing");
            }
        }
        for (id, relation) in self.relations.iter() {
            let Some((first, second)) = relation.ends() else {
                continue;
            };
            let ends = [first, second].map(|end| self.element(end));
            if ends.iter().all(Option::is_some) {
                continue;
            }
            let kind = Message::new(relation.label_key());
            let (element, message) = match ends.into_iter().flatten().next() {
                Some(e) => (
                    Some(e.id),
                    Message::new("model-problem-relation-end-missing")
                        .text("name", &e.name)
                        .nested("relation", kind),
                ),
                None => (
                    None,
                    Message::new("model-problem-relation-ends-missing").nested("relation", kind),
                ),
            };
            problems.push(IntegrityProblem {
                element,
                relation: Some(id),
                message,
            });
        }
        problems
    }
}

/// A dangling reference found by [`Project::integrity_problems`]
/// (NFR-REL-004): the element to select, if one exists, the relation when a
/// relation's end is missing, and the message to show (UI-012).
#[derive(Clone, Debug, PartialEq)]
pub struct IntegrityProblem {
    pub element: Option<ElementId>,
    pub relation: Option<RelationId>,
    pub message: Message,
}
