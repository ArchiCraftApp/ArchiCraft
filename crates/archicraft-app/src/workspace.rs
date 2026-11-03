//! Workspace state shared by views and panels: the open document, the active
//! tool, the selection, and per-view state.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use archicraft_model::{Command, CommandError, ElementId, ElementKind, GroupId, KindTag, Message, StoreyId, ViewId};

use crate::commands::{CommandId, Keymap, Origin, Target};
use crate::document::Document;
use crate::input::{Frame2d, InputConsumer, PointInput, SnapIndex};
use crate::persistence::prefs::Preferences;
use crate::tools::{ToolKind, ToolState, UserOriginTool};
use crate::tr;
use crate::views::plan::PlanViewState;
use crate::views::render3d::{GpuScene, Renderer3d};
use crate::views::view3d::View3dState;

/// A view shown in a tab (UI-006, VIEW-002).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pane {
    Plan(StoreyId),
    View3d(ViewId),
}

/// A message in the status line.
#[derive(Clone, Debug)]
pub struct Status {
    pub text: String,
    pub error: bool,
    pub since: Instant,
}

/// Requests from panes and panels that the application handles after the
/// frame's UI is laid out.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    OpenPane(Pane),
    ZoomTo(Vec<ElementId>),
    /// Open the full settings dialog of the active tool or the selection
    /// (UI-003).
    Settings,
    /// Write the preferences, changed outside the Preferences dialog, to
    /// their file.
    SavePreferences,
    /// Run a command from a panel or dialog, which has no access to the
    /// application (UI-017).
    Invoke(CommandId, Target, Origin),
}

/// The frames point input measures in, shared by all plan views: session
/// state, not written to IFCX and reset when the document is replaced
/// (INP-014).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InputFrames {
    /// The user origin; the default frame is the project origin.
    pub user_origin: Frame2d,
}

/// The consumer taking the points of the plan views (INP-001): the active
/// tool, or a transient consumer that suspends it until it is done.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Consumer {
    #[default]
    Tool,
    /// Placing the user origin (INP-014).
    UserOrigin(UserOriginTool),
}

impl Consumer {
    /// The consumer itself, `tool` while no transient consumer is active.
    pub fn get<'a>(&'a self, tool: &'a ToolState) -> &'a dyn InputConsumer {
        match self {
            Consumer::Tool => tool,
            Consumer::UserOrigin(origin) => origin,
        }
    }

    /// The consumer itself to feed points and keys to.
    pub fn get_mut<'a>(&'a mut self, tool: &'a mut ToolState) -> &'a mut dyn InputConsumer {
        match self {
            Consumer::Tool => tool,
            Consumer::UserOrigin(origin) => origin,
        }
    }

    /// The storey the input in progress started on.
    pub fn storey(&self, tool: &ToolState) -> Option<StoreyId> {
        match self {
            Consumer::Tool => tool.input_storey(),
            Consumer::UserOrigin(origin) => origin.storey,
        }
    }
}

/// The key of the uploaded 3D scene: model revision and highlighted elements.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SceneKey {
    pub revision: u64,
    pub selection: Vec<ElementId>,
}

impl SceneKey {
    /// The scene every 3D view shows: the selection is the one of the plans,
    /// so it stays synchronised between views (UI-007).
    pub fn of(ws: &Workspace) -> Self {
        let mut selection: Vec<ElementId> = ws.selection.iter().copied().collect();
        selection.sort();
        Self {
            revision: ws.doc.project.revision,
            selection,
        }
    }
}

#[derive(Debug)]
pub struct Workspace {
    pub doc: Document,
    pub prefs: Preferences,
    pub tool: ToolState,
    /// The consumer taking points: the tool or a transient one (INP-001).
    pub active_consumer: Consumer,
    /// The user origin and later frames of point input (INP-014).
    pub frames: InputFrames,
    /// Point input for the active tool: typed values and the last cursor
    /// position (INP-001).
    pub input: PointInput,
    pub selection: HashSet<ElementId>,
    /// Groups are suspended: a click selects the member alone (ELM-015).
    /// Session state, not saved with the project.
    pub groups_suspended: bool,
    pub hover: Option<ElementId>,
    pub plan_views: HashMap<StoreyId, PlanViewState>,
    /// The snap targets of each storey, following the model's revisions
    /// (NFR-PERF-005).
    snap_indexes: HashMap<StoreyId, Arc<SnapIndex>>,
    pub views_3d: HashMap<ViewId, View3dState>,
    pub renderer: Option<Renderer3d>,
    pub scene: Option<(SceneKey, GpuScene)>,
    /// The storey new elements go to: the storey of the plan used last.
    pub active_storey: StoreyId,
    /// The view used last; view commands apply to it.
    pub active_pane: Option<Pane>,
    pub status: Option<Status>,
    pub requests: Vec<Request>,
    /// The keys of the commands; the plan view matches its local keys
    /// against it (UI-009).
    pub keymap: Keymap,
    /// Local commands of the plan view invoked other than by their key (the
    /// palette), for the active plan view to run; the app drops the ones
    /// no plan view ran by the end of the frame (HW-003).
    pub view_commands: Vec<CommandId>,
    /// Frame and latency samples (NFR-PERF-001).
    #[cfg(feature = "perf")]
    pub perf: crate::perf::PerfRecorder,
}

impl Workspace {
    pub fn new(doc: Document, prefs: Preferences) -> Self {
        let active_storey = doc.project.storeys.ground();
        Self {
            doc,
            prefs,
            tool: ToolState::default(),
            active_consumer: Consumer::Tool,
            frames: InputFrames::default(),
            input: PointInput::default(),
            selection: HashSet::new(),
            groups_suspended: false,
            hover: None,
            plan_views: HashMap::new(),
            snap_indexes: HashMap::new(),
            views_3d: HashMap::new(),
            renderer: None,
            scene: None,
            active_storey,
            active_pane: Some(Pane::Plan(active_storey)),
            status: None,
            requests: Vec::new(),
            keymap: Keymap::default(),
            view_commands: Vec::new(),
            #[cfg(feature = "perf")]
            perf: crate::perf::PerfRecorder::from_env(),
        }
    }

    /// Replaces the document, for example after New or Open. View state is
    /// reset; the GPU renderer is kept.
    pub fn replace_document(&mut self, doc: Document) {
        self.doc = doc;
        self.reset_input();
        self.frames = InputFrames::default();
        self.selection.clear();
        self.hover = None;
        self.plan_views.clear();
        self.snap_indexes.clear();
        self.views_3d.clear();
        self.scene = None;
        self.active_storey = self.doc.project.storeys.ground();
        self.active_pane = Some(Pane::Plan(self.active_storey));
        self.requests.clear();
    }

    /// The snap targets of `storey` at the current revision (INP-008). The
    /// index is built once and then updated per revision, only for the
    /// elements whose targets changed (NFR-PERF-005).
    pub fn snap_index(&mut self, storey: StoreyId) -> Arc<SnapIndex> {
        let revision = self.doc.project.revision;
        if let Some(index) = self.snap_indexes.get(&storey)
            && index.revision() == Some(revision)
        {
            return index.clone();
        }
        let plan = self.doc.plan(storey);
        let project = &self.doc.project;
        self.snap_indexes.retain(|s, _| project.storeys.get(*s).is_some());
        let index = self.snap_indexes.entry(storey).or_default();
        Arc::make_mut(index).update(project, storey, &plan);
        index.clone()
    }

    /// Switches to the tool `kind`, discarding the input in progress. An open
    /// wall chain keeps its walls (UND-003); while the project rejects them,
    /// the tool stays with the chain pending.
    pub fn select_tool(&mut self, kind: ToolKind) {
        self.active_consumer = Consumer::Tool;
        if !self.end_chain() {
            return;
        }
        self.tool.select(kind);
        self.input = PointInput::default();
    }

    /// Ends an open wall chain, creating its walls as one undo step
    /// (UND-003). When the project rejects the walls, the chain stays
    /// pending with its points and the status line names the error; returns
    /// whether no chain is pending any more.
    pub fn end_chain(&mut self) -> bool {
        let (points, storey) = (self.tool.points.clone(), self.tool.storey);
        let Some(command) = self.tool.end_chain(&self.doc.project) else {
            return true;
        };
        if self.execute(command) {
            return true;
        }
        self.tool.points = points;
        self.tool.storey = storey;
        false
    }

    /// Discards the points and typed values of the input in progress,
    /// also of a transient consumer.
    pub fn reset_input(&mut self) {
        self.active_consumer = Consumer::Tool;
        self.tool.reset();
        self.input = PointInput::default();
    }

    /// The consumer taking points now (INP-001).
    pub fn consumer(&self) -> &dyn InputConsumer {
        self.active_consumer.get(&self.tool)
    }

    /// Whether the plan views take points rather than select: a drawing
    /// tool or a transient consumer is active.
    pub fn takes_points(&self) -> bool {
        self.active_consumer != Consumer::Tool || self.tool.kind != ToolKind::Select
    }

    /// Starts placing the user origin (INP-014). The tool and its points
    /// wait until the origin is placed or the placing is cancelled.
    pub fn place_user_origin(&mut self) {
        self.active_consumer = Consumer::UserOrigin(UserOriginTool::default());
        self.input = PointInput::default();
    }

    /// Resets the user origin to the project origin (INP-014). Nothing in
    /// the model changes, so there is no undo step.
    pub fn reset_user_origin(&mut self) {
        self.frames.user_origin = Frame2d::default();
        self.info(tr!("status-user-origin-reset"));
    }

    /// Ends a transient consumer and goes back to the tool.
    pub fn end_consumer(&mut self) {
        if self.active_consumer != Consumer::Tool {
            self.active_consumer = Consumer::Tool;
            self.input = PointInput::default();
        }
    }

    /// Takes the result of a transient consumer that is done: the placed
    /// user origin becomes the frame of absolute input and display
    /// (INP-014).
    pub fn settle_consumer(&mut self) {
        if let Consumer::UserOrigin(tool) = &self.active_consumer
            && let Some(frame) = tool.placed()
        {
            self.frames.user_origin = frame;
            self.end_consumer();
            self.info(tr!("status-user-origin-placed"));
        }
    }

    /// Moves the wall tool's reference line to its next location (WAL-007). The tool defaults are part of the
    /// project, so the change is an undoable command.
    pub fn cycle_wall_reference_line(&mut self) {
        let mut defaults = self.doc.project.tool_defaults.clone();
        let Some(ElementKind::Wall(wall)) = defaults.get_mut(KindTag::Wall).map(|slot| &mut slot.kind) else {
            return;
        };
        wall.reference_line = wall.reference_line.next();
        let label = tr!(wall.reference_line.label_key());
        if self.execute(Command::SetToolDefaults(defaults)) {
            self.info(tr!("status-wall-reference-line", location = label));
        }
    }

    /// A count for the status line in the user's number format; the
    /// catalogue gets it as text beside the number that selects the plural
    /// (NFR-L10N-001, NFR-L10N-004).
    pub(crate) fn count_text(&self, count: usize) -> String {
        self.prefs.number_format().format_decimal(count as f64, 0)
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.status = Some(Status {
            text: text.into(),
            error: false,
            since: Instant::now(),
        });
    }

    pub fn error(&mut self, text: impl Into<String>) {
        let text = text.into();
        tracing::warn!("{text}");
        self.status = Some(Status {
            text,
            error: true,
            since: Instant::now(),
        });
    }

    /// Renders a model or IFCX message with the project units and the user's number
    /// format (NFR-L10N-001).
    pub fn render(&self, message: &Message) -> String {
        crate::i18n::render(message, &self.doc.project.settings.units, &self.prefs.number_format())
    }

    /// Executes a command and reports a rejection in the status line
    /// (NFR-REL-005), rendered from the error's message key with the limits
    /// in the project units (NFR-L10N-001). Returns true on success.
    pub fn execute(&mut self, command: Command) -> bool {
        let label = self.render(&command.label());
        #[cfg(feature = "perf")]
        let started = Instant::now();
        match self.doc.execute(command) {
            Ok(()) => {
                #[cfg(feature = "perf")]
                self.perf.edit_executed(self.doc.project.revision, started);
                self.info(label);
                true
            }
            Err(CommandError::NoChange) => false,
            Err(e) => {
                let reason = self.render(&e.message());
                self.error(tr!("status-rejected", command = label, reason = reason));
                false
            }
        }
    }

    pub fn undo(&mut self) {
        match self.doc.undo() {
            Some(label) => self.info(tr!("status-undone", command = self.render(&label))),
            None => self.info(tr!("status-nothing-to-undo")),
        }
        self.prune_selection();
    }

    pub fn redo(&mut self) {
        match self.doc.redo() {
            Some(label) => self.info(tr!("status-redone", command = self.render(&label))),
            None => self.info(tr!("status-nothing-to-redo")),
        }
        self.prune_selection();
    }

    /// Drops selected elements that no longer exist or are hidden
    /// (SEL-007).
    pub fn prune_selection(&mut self) {
        let project = &self.doc.project;
        self.selection
            .retain(|id| project.element(*id).is_some_and(|e| project.is_visible(e)));
        if self.hover.is_some_and(|id| project.element(id).is_none()) {
            self.hover = None;
        }
        if project.storeys.get(self.active_storey).is_none() {
            self.active_storey = project.storeys.ground();
        }
    }

    /// The selection in model order, for stable display.
    pub fn selected_ids(&self) -> Vec<ElementId> {
        self.doc
            .project
            .elements
            .keys()
            .filter(|id| self.selection.contains(id))
            .copied()
            .collect()
    }

    /// What a selection of `ids` covers (ELM-015): each grouped element
    /// stands for the members of its outermost group, unless groups are
    /// suspended ([`Project::expand_groups`]). Members hidden by their layer
    /// are left out, so that a group does not select what the views do not
    /// show (SEL-007); locked members are selected like the others, since
    /// locking rejects edits, not selection (ELM-013). Every selection path
    /// goes through here, so all of them treat groups alike.
    ///
    /// [`Project::expand_groups`]: archicraft_model::Project::expand_groups
    pub fn expand_groups(&self, ids: &[ElementId]) -> Vec<ElementId> {
        if self.groups_suspended {
            return ids.to_vec();
        }
        let project = &self.doc.project;
        project
            .expand_groups(ids)
            .into_iter()
            .filter(|id| project.element(*id).is_some_and(|e| project.is_visible(e)))
            .collect()
    }

    /// Selects what a click on `hit` selects (SEL-001, ELM-015): the element
    /// with its group alone, or with `toggle` (Shift) the element and its
    /// group added to the selection, or removed from it if the element was
    /// selected. A click on nothing clears the selection unless it toggles.
    pub fn click_select(&mut self, hit: Option<ElementId>, toggle: bool) {
        let Some(id) = hit else {
            if !toggle {
                self.selection.clear();
            }
            return;
        };
        let covered = self.expand_groups(&[id]);
        if !toggle {
            self.selection.clear();
        }
        if toggle && self.selection.contains(&id) {
            for id in &covered {
                self.selection.remove(id);
            }
        } else {
            self.selection.extend(covered);
        }
    }

    /// Selects the elements an area selection caught (SEL-003), added to
    /// the selection with `add` (Shift). A group is selected whole once one
    /// of its members is caught, also when the area selects only what lies
    /// fully inside it (ELM-015).
    pub fn select_area(&mut self, caught: &[ElementId], add: bool) {
        let covered = self.expand_groups(caught);
        if !add {
            self.selection.clear();
        }
        self.selection.extend(covered);
    }

    /// Selects `id` with its group before a context menu opens on it,
    /// unless it is selected already (UI-005, ELM-015).
    pub fn select_for_menu(&mut self, id: ElementId) {
        if !self.selection.contains(&id) {
            self.click_select(Some(id), false);
        }
    }

    /// The groups and ungrouped elements the selection consists of
    /// (ELM-015): the outermost group of each grouped element, then the
    /// ungrouped elements, each once. Group needs two of them.
    pub fn selection_items(&self) -> (Vec<GroupId>, Vec<ElementId>) {
        let project = &self.doc.project;
        let (mut groups, mut loose) = (Vec::new(), Vec::new());
        for id in self.selected_ids() {
            match project.outermost_group(id) {
                Some(group) if !groups.contains(&group) => groups.push(group),
                Some(_) => {}
                None => loose.push(id),
            }
        }
        (groups, loose)
    }

    /// Groups the selection (ELM-015), one undo step, and selects the new
    /// group.
    pub fn group_selection(&mut self) {
        let group = GroupId::new();
        if self.execute(Command::Group {
            group,
            elements: self.selected_ids(),
        }) {
            let project = &self.doc.project;
            self.selection = project
                .group_members(group)
                .into_iter()
                .filter(|id| project.element(*id).is_some_and(|e| project.is_visible(e)))
                .collect();
        }
    }

    /// Removes one level of grouping from every outermost group of the
    /// selection (ELM-015), one undo step per group. The selection stays.
    pub fn ungroup_selection(&mut self) {
        for group in self.selection_items().0 {
            self.execute(Command::Ungroup { group });
        }
    }

    /// Turns the suspension of groups on or off (ELM-015). It is session
    /// state: the project neither changes nor becomes dirty.
    pub fn toggle_groups_suspended(&mut self) {
        self.groups_suspended = !self.groups_suspended;
    }

    /// Deletes the selected elements that are editable (ELM-013).
    pub fn delete_selection(&mut self) {
        let project = &self.doc.project;
        let ids: Vec<ElementId> = self
            .selected_ids()
            .into_iter()
            .filter(|id| project.element(*id).is_some_and(|e| project.is_editable(e)))
            .collect();
        let locked = self.selection.len() - ids.len();
        if ids.is_empty() {
            if locked > 0 {
                self.error(tr!("status-selection-locked"));
            }
            return;
        }
        if self.execute(Command::DeleteElements(ids)) {
            self.selection.clear();
            if locked > 0 {
                self.info(tr!(
                    "status-locked-not-deleted",
                    count = locked,
                    count_text = self.count_text(locked),
                ));
            }
        }
    }

    /// Selects every visible element of the active tool's kind on the active
    /// storey, or of every kind with the selection and the measure tool
    /// (SEL-004), with the groups they belong to (ELM-015).
    pub fn select_all(&mut self) {
        let project = &self.doc.project;
        let kind = match self.tool.kind {
            ToolKind::Select | ToolKind::Measure => None,
            ToolKind::Wall => Some(archicraft_model::KindTag::Wall),
            ToolKind::Slab => Some(archicraft_model::KindTag::Slab),
        };
        let storey = match self.active_pane {
            Some(Pane::Plan(s)) => Some(s),
            _ => None,
        };
        let ids: Vec<ElementId> = project
            .elements
            .values()
            .filter(|e| project.is_visible(e))
            .filter(|e| storey.is_none_or(|s| e.storey == s))
            .filter(|e| kind.is_none_or(|k| e.tag() == k))
            .map(|e| e.id)
            .collect();
        self.selection = self.expand_groups(&ids).into_iter().collect();
        let count = self.selection.len();
        self.info(tr!(
            "status-selected",
            count = count,
            count_text = self.count_text(count)
        ));
    }
}

#[cfg(test)]
mod tests {
    use archicraft_model::glam::DVec2;
    use archicraft_model::template::default_metric_project;

    use super::*;
    use crate::input::{Effect, InputContext, KeyInput, Modifiers, SnapIndex, SurfaceFrame};

    /// UND-003: switching to the slab tool with three pending chain points
    /// keeps the chain: its two walls are created as one undo step.
    #[test]
    fn switching_tool_commits_an_open_wall_chain() {
        let mut ws = Workspace::new(Document::new(default_metric_project(), 100), Preferences::default());
        ws.select_tool(ToolKind::Wall);
        let storey = ws.active_storey;
        for p in [(0.0, 0.0), (5.0, 0.0), (5.0, 4.0)] {
            let outcome = ws.tool.commit(&ws.doc.project, storey, DVec2::new(p.0, p.1));
            assert_eq!(outcome, crate::input::Outcome::Continue);
        }
        ws.select_tool(ToolKind::Slab);
        assert_eq!(ws.tool.kind, ToolKind::Slab);
        assert!(!ws.tool.is_drawing());
        let walls = ws
            .doc
            .project
            .elements
            .values()
            .filter(|e| e.as_wall().is_some())
            .count();
        assert_eq!(walls, 2);
        ws.undo();
        assert!(ws.doc.project.elements.is_empty(), "the chain is one undo step");
    }

    /// Adds a wall per `y`, locked or not, and returns their ids.
    fn add_walls(ws: &mut Workspace, ys: impl IntoIterator<Item = u32>, locked: bool) -> Vec<ElementId> {
        let storey = ws.active_storey;
        let walls: Vec<_> = ys
            .into_iter()
            .map(|nth| {
                let y = f64::from(nth);
                let mut wall = ws
                    .doc
                    .project
                    .new_wall(storey, DVec2::new(0.0, y), DVec2::new(4.0, y), nth);
                wall.locked = locked;
                wall
            })
            .collect();
        let ids = walls.iter().map(|w| w.id).collect();
        assert!(ws.execute(Command::AddElements(walls)));
        ids
    }

    fn status_text(ws: &Workspace) -> &str {
        &ws.status.as_ref().unwrap().text
    }

    /// NFR-L10N-001, NFR-L10N-004: Select all reports its count in the
    /// status line as a Fluent plural, not as “element(s)”, with the number
    /// in the user's number format (here with digit grouping).
    #[test]
    fn select_all_reports_the_count_as_a_plural() {
        let mut ws = Workspace::new(Document::new(default_metric_project(), 100), Preferences::default());
        add_walls(&mut ws, [0], false);
        ws.select_all();
        assert_eq!(status_text(&ws), "1 element selected");
        add_walls(&mut ws, 1..3, false);
        ws.select_all();
        assert_eq!(status_text(&ws), "3 elements selected");
        add_walls(&mut ws, 3..1500, false);
        ws.prefs.digit_grouping = true;
        ws.select_all();
        let count = ws.prefs.number_format().format_decimal(1500.0, 0);
        assert_ne!(count, "1500", "the number format groups digits");
        assert_eq!(status_text(&ws), format!("{count} elements selected"));
    }

    /// NFR-L10N-001: deleting a selection with locked elements reports the
    /// ones left as a Fluent plural, and a selection of locked elements only
    /// says so.
    #[test]
    fn deleting_reports_the_locked_elements_left_as_a_plural() {
        let mut ws = Workspace::new(Document::new(default_metric_project(), 100), Preferences::default());
        for (base, locked) in [(0, 1), (10, 3)] {
            let mut ids = add_walls(&mut ws, [base], false);
            ids.extend(add_walls(&mut ws, base + 1..=base + locked, true));
            ws.selection = ids.into_iter().collect();
            ws.delete_selection();
            let expected = if locked == 1 {
                "1 locked element was not deleted."
            } else {
                "3 locked elements were not deleted."
            };
            assert_eq!(status_text(&ws), expected);
        }
        ws.selection = ws.doc.project.elements.keys().copied().collect();
        ws.delete_selection();
        assert_eq!(status_text(&ws), "The selected elements are locked.");
    }

    /// UND-003: a chain whose command the project rejects keeps its pending
    /// points: here its storey was deleted meanwhile, so the walls have no
    /// home storey. Neither ending the chain nor switching the tool loses
    /// the points; the status line names the error.
    #[test]
    fn a_rejected_chain_keeps_its_pending_points() {
        let mut ws = Workspace::new(Document::new(default_metric_project(), 100), Preferences::default());
        ws.select_tool(ToolKind::Wall);
        let ground = ws.doc.project.storeys.ground();
        let upper = ws
            .doc
            .project
            .storeys
            .iter()
            .map(|s| s.id)
            .find(|&s| s != ground)
            .unwrap();
        let points = [DVec2::ZERO, DVec2::new(5.0, 0.0), DVec2::new(5.0, 4.0)];
        for p in points {
            assert_eq!(
                ws.tool.commit(&ws.doc.project, upper, p),
                crate::input::Outcome::Continue
            );
        }
        assert!(ws.execute(Command::DeleteStorey {
            storey: upper,
            elements: archicraft_model::command::StoreyElements::Delete,
        }));
        let revision = ws.doc.project.revision;

        assert!(!ws.end_chain(), "the chain did not end");
        assert_eq!(ws.tool.points, points);
        assert_eq!(ws.tool.storey, Some(upper));
        assert!(matches!(ws.status, Some(Status { error: true, .. })), "{:?}", ws.status);

        ws.select_tool(ToolKind::Slab);
        assert_eq!(ws.tool.kind, ToolKind::Wall, "the tool waits for the chain");
        assert_eq!(ws.tool.points, points);
        assert_eq!(ws.doc.project.revision, revision);
    }

    /// Feeds `key` to the workspace's consumer, as a plan view does, with
    /// the cursor at (20, 0).
    fn key(ws: &mut Workspace, key: KeyInput) -> Effect {
        key_at(ws, key, DVec2::new(20.0, 0.0))
    }

    /// Feeds `key` with the cursor at `cursor`.
    fn key_at(ws: &mut Workspace, key: KeyInput, cursor: DVec2) -> Effect {
        feed(ws, Feed::Key(key), cursor, &SnapIndex::default())
    }

    /// What [`feed`] gives point input.
    enum Feed {
        Key(KeyInput),
        Click,
    }

    /// Feeds a key or a click to the workspace's consumer with the cursor at
    /// `cursor` and the snap targets `targets`, as a plan view does.
    fn feed(ws: &mut Workspace, event: Feed, cursor: DVec2, targets: &SnapIndex) -> Effect {
        let units = ws.doc.project.settings.units.clone();
        let nf = ws.prefs.number_format();
        let consumer = ws.active_consumer.get(&ws.tool);
        let step = consumer.step();
        let ctx = InputContext {
            project: &ws.doc.project,
            storey: ws.active_storey,
            targets,
            pending: &[],
            previous: consumer.reference_point(),
            guides: &[],
            selection: &[],
            input: crate::input::InputSettings::default(),
            expects: step.as_ref().map_or(crate::input::Expect::Point, |s| s.expects),
            tracker: step.map_or(crate::input::TrackerMode::Relative, |s| s.tracker),
            tracker_xyz: false,
            radius: 0.1,
            drag_threshold: 0.06,
            units: &units,
            number_format: &nf,
            angle_increment_deg: 45.0,
            constraint_reference: crate::input::ConstraintReference::XAxis,
            frame_rotation: 0.0,
            snap_step: 0.0,
            frame: SurfaceFrame::plan(0.0),
            user_origin: ws.frames.user_origin,
            marked: None,
        };
        ws.input.resolve(cursor, Modifiers::default(), &ctx);
        let consumer = ws.active_consumer.get_mut(&mut ws.tool);
        let effect = match event {
            Feed::Key(key) => ws.input.key(key, consumer, &ctx),
            Feed::Click => ws.input.click(cursor, Modifiers::default(), consumer, &ctx),
        };
        ws.settle_consumer();
        effect
    }

    /// INP-014: placing the user origin through the workspace suspends the
    /// wall tool, takes the origin (10, 10) and the typed angle 90, sets the
    /// frame and goes back to the tool; the project revision does not change
    /// and there is no undo step. A second origin is typed from the first;
    /// Enter on its second step keeps rotation 0. Reset restores the default
    /// frame, the project origin.
    #[test]
    fn placing_the_user_origin_changes_no_model() {
        let mut ws = Workspace::new(Document::new(default_metric_project(), 100), Preferences::default());
        ws.select_tool(ToolKind::Wall);
        let revision = ws.doc.project.revision;
        ws.place_user_origin();
        assert!(ws.takes_points());
        assert!(matches!(ws.active_consumer, Consumer::UserOrigin(_)));
        for k in ["10", "\t", "10", "\n", "90", "\n"] {
            let k = match k {
                "\t" => KeyInput::Tab,
                "\n" => KeyInput::Enter,
                text => KeyInput::Text(text.into()),
            };
            assert_eq!(key(&mut ws, k), Effect::None);
        }
        assert_eq!(ws.active_consumer, Consumer::Tool);
        assert_eq!(ws.tool.kind, ToolKind::Wall);
        let frame = ws.frames.user_origin;
        assert_eq!(frame.origin, DVec2::new(10.0, 10.0));
        assert!((frame.rotation - 90f64.to_radians()).abs() < 1e-12);
        assert_eq!(ws.doc.project.revision, revision);
        assert!(!ws.doc.history.can_undo());

        ws.place_user_origin();
        for k in [KeyInput::Text("3".into()), KeyInput::Tab, KeyInput::Text("4".into())] {
            key(&mut ws, k);
        }
        key(&mut ws, KeyInput::Enter);
        assert_eq!(key(&mut ws, KeyInput::Enter), Effect::None, "Enter keeps rotation 0");
        // The new origin was typed from the user origin placed before: (3, 4)
        // along its axes rotated by 90° from (10, 10).
        let frame = ws.frames.user_origin;
        assert!(frame.origin.distance(DVec2::new(6.0, 13.0)) < 1e-12, "{frame:?}");
        assert_eq!(frame.rotation, 0.0);

        ws.reset_user_origin();
        assert_eq!(ws.frames.user_origin, Frame2d::default());
        assert_eq!(ws.doc.project.revision, revision);

        ws.frames.user_origin.origin = DVec2::ONE;
        ws.replace_document(Document::new(default_metric_project(), 100));
        assert_eq!(ws.frames, InputFrames::default(), "session state");
    }

    /// INP-014: a typed angle on the second step sets the rotation also with
    /// the cursor on the origin, where the cursor gives no distance.
    #[test]
    fn a_typed_angle_with_the_cursor_on_the_origin() {
        let mut ws = Workspace::new(Document::new(default_metric_project(), 100), Preferences::default());
        ws.place_user_origin();
        let origin = DVec2::new(2.0, 3.0);
        for k in [
            KeyInput::Text("2".into()),
            KeyInput::Tab,
            KeyInput::Text("3".into()),
            KeyInput::Enter,
        ] {
            key_at(&mut ws, k, origin);
        }
        assert!(matches!(ws.active_consumer, Consumer::UserOrigin(_)));
        for k in [KeyInput::Text("30".into()), KeyInput::Enter] {
            assert_eq!(key_at(&mut ws, k, origin), Effect::None);
        }
        assert_eq!(ws.active_consumer, Consumer::Tool);
        let frame = ws.frames.user_origin;
        assert_eq!(frame.origin, origin);
        assert!((frame.rotation - 30f64.to_radians()).abs() < 1e-12, "{frame:?}");
    }

    /// INP-003, INP-014: Esc with nothing to cancel ends placing the user
    /// origin, as the plan's Esc ladder does; switching tools and cancelling
    /// the input end it too.
    #[test]
    fn placing_the_user_origin_can_be_left() {
        let mut ws = Workspace::new(Document::new(default_metric_project(), 100), Preferences::default());
        ws.place_user_origin();
        assert_eq!(key(&mut ws, KeyInput::Escape), Effect::NothingToCancel);
        ws.end_consumer();
        assert_eq!(ws.active_consumer, Consumer::Tool);
        ws.place_user_origin();
        ws.select_tool(ToolKind::Slab);
        assert_eq!(ws.active_consumer, Consumer::Tool);
        assert_eq!(ws.tool.kind, ToolKind::Slab);
        ws.place_user_origin();
        ws.reset_input();
        assert_eq!(ws.active_consumer, Consumer::Tool, "Cancel input ends it");
    }

    /// MEA-001: the measure tool creates no model element. After 50
    /// measurements of each mode, finished by Enter, by the angle's third
    /// point and cleared by Esc, the project revision and the undo count are
    /// unchanged and no effect asked to run a command.
    #[test]
    fn measuring_changes_no_model() {
        let mut ws = Workspace::new(Document::new(default_metric_project(), 100), Preferences::default());
        add_walls(&mut ws, [0], false);
        let (revision, undo) = (ws.doc.project.revision, ws.doc.history.undo_len());
        ws.select_tool(ToolKind::Measure);
        let targets = ws.snap_index(ws.active_storey);
        for i in 0..50 {
            let x = f64::from(i);
            for mode in crate::tools::MeasureMode::ALL {
                ws.tool.measure.set_mode(mode);
                let mut effects = Vec::new();
                for p in [(x, 10.0), (x + 3.0, 14.0), (x + 3.0, 20.0)] {
                    effects.push(feed(&mut ws, Feed::Click, DVec2::new(p.0, p.1), &targets));
                }
                for key in [KeyInput::Enter, KeyInput::Escape] {
                    effects.push(feed(&mut ws, Feed::Key(key), DVec2::new(x, 30.0), &targets));
                }
                assert!(effects.iter().all(|e| *e == Effect::None), "{effects:?}");
            }
        }
        assert_eq!(ws.tool.kind, ToolKind::Measure);
        assert_eq!(ws.doc.project.revision, revision);
        assert_eq!(ws.doc.history.undo_len(), undo);
    }

    /// MEA-001, INP-001: measure points snap to the nodes of a wall typed
    /// 4.25 m long through the shared point input: clicks near its ends
    /// measure 4.25 m within 1e-9.
    #[test]
    fn measure_points_snap_to_wall_nodes() {
        let mut ws = Workspace::new(Document::new(default_metric_project(), 100), Preferences::default());
        ws.select_tool(ToolKind::Wall);
        let none = SnapIndex::default();
        let start = DVec2::new(1.0, 2.0);
        assert_eq!(feed(&mut ws, Feed::Click, start, &none), Effect::None);
        // The decimal separator of the number format in effect.
        let typed = format!("4{}25", ws.prefs.number_format().decimal_separator);
        for key in [KeyInput::Text(typed), KeyInput::Enter] {
            assert_eq!(key_at(&mut ws, key, DVec2::new(20.0, 2.0)), Effect::None);
        }
        let Effect::Execute(command) = key_at(&mut ws, KeyInput::Enter, DVec2::new(20.0, 2.0)) else {
            panic!("Enter ends the chain")
        };
        assert!(ws.execute(command));
        let wall = ws
            .doc
            .project
            .elements
            .values()
            .next()
            .unwrap()
            .as_wall()
            .unwrap()
            .clone();
        assert!((wall.start.distance(wall.end) - 4.25).abs() < 1e-12, "{wall:?}");

        ws.select_tool(ToolKind::Measure);
        let targets = ws.snap_index(ws.active_storey);
        let near = DVec2::new(0.03, -0.02);
        for p in [wall.start + near, wall.end - near] {
            assert_eq!(feed(&mut ws, Feed::Click, p, &targets), Effect::None);
        }
        assert_eq!(ws.tool.measure.points(), [wall.start, wall.end], "snapped to the nodes");
        let Some(crate::tools::Measurement::Distance { total, .. }) = ws.tool.measure.measurement() else {
            panic!("two points measure a distance")
        };
        assert!((total - 4.25).abs() < 1e-9, "{total}");
    }
}
