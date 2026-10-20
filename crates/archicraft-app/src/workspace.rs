//! Workspace state shared by views and panels: the open document, the active
//! tool, the selection, and per-view state.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use archicraft_model::{Command, CommandError, ElementId, StoreyId, ViewId};

use crate::document::Document;
use crate::persistence::prefs::Preferences;
use crate::tools::ToolState;
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
    pub selection: HashSet<ElementId>,
    pub hover: Option<ElementId>,
    pub plan_views: HashMap<StoreyId, PlanViewState>,
    pub views_3d: HashMap<ViewId, View3dState>,
    pub renderer: Option<Renderer3d>,
    pub scene: Option<(SceneKey, GpuScene)>,
    /// The storey new elements go to: the storey of the plan used last.
    pub active_storey: StoreyId,
    /// The view used last; view commands apply to it.
    pub active_pane: Option<Pane>,
    pub status: Option<Status>,
    pub requests: Vec<Request>,
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
            selection: HashSet::new(),
            hover: None,
            plan_views: HashMap::new(),
            views_3d: HashMap::new(),
            renderer: None,
            scene: None,
            active_storey,
            active_pane: Some(Pane::Plan(active_storey)),
            status: None,
            requests: Vec::new(),
            #[cfg(feature = "perf")]
            perf: crate::perf::PerfRecorder::from_env(),
        }
    }

    /// Replaces the document, for example after New or Open. View state is
    /// reset; the GPU renderer is kept.
    pub fn replace_document(&mut self, doc: Document) {
        self.doc = doc;
        self.tool.reset();
        self.selection.clear();
        self.hover = None;
        self.plan_views.clear();
        self.views_3d.clear();
        self.scene = None;
        self.active_storey = self.doc.project.storeys.ground();
        self.active_pane = Some(Pane::Plan(self.active_storey));
        self.requests.clear();
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

    /// Executes a command and reports a rejection in the status line
    /// (NFR-REL-005). Returns true on success.
    pub fn execute(&mut self, command: Command) -> bool {
        let label = command.label();
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
                self.error(format!("{label}: {e}"));
                false
            }
        }
    }

    pub fn undo(&mut self) {
        match self.doc.undo() {
            Some(label) => self.info(format!("Undone: {label}")),
            None => self.info("Nothing to undo"),
        }
        self.prune_selection();
    }

    pub fn redo(&mut self) {
        match self.doc.redo() {
            Some(label) => self.info(format!("Redone: {label}")),
            None => self.info("Nothing to redo"),
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
                self.error("The selected elements are locked.");
            }
            return;
        }
        if self.execute(Command::DeleteElements(ids)) {
            self.selection.clear();
            if locked > 0 {
                self.info(format!("{locked} locked element(s) were not deleted."));
            }
        }
    }

    /// Selects every visible element of the active tool's kind on the active
    /// storey, or of every kind with the selection tool (SEL-004).
    pub fn select_all(&mut self) {
        use crate::tools::ToolKind;
        let project = &self.doc.project;
        let kind = match self.tool.kind {
            ToolKind::Select => None,
            ToolKind::Wall => Some(archicraft_model::KindTag::Wall),
            ToolKind::Slab => Some(archicraft_model::KindTag::Slab),
        };
        let storey = match self.active_pane {
            Some(Pane::Plan(s)) => Some(s),
            _ => None,
        };
        self.selection = project
            .elements
            .values()
            .filter(|e| project.is_visible(e))
            .filter(|e| storey.is_none_or(|s| e.storey == s))
            .filter(|e| kind.is_none_or(|k| e.tag() == k))
            .map(|e| e.id)
            .collect();
        self.info(format!("{} element(s) selected", self.selection.len()));
    }
}
