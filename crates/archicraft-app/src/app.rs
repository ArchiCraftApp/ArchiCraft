//! The application window (UI-001, PRJ-003, PRJ-004, PRJ-008).

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use archicraft_ifcx::Problem;
use archicraft_model::template::default_metric_project;
use eframe::egui_wgpu;
use egui::{Id, RichText, ThemePreference, Ui, ViewportCommand, WidgetText};
use egui_tiles::{SimplificationOptions, Tile, TileId, Tiles, Tree, UiResponse};

use crate::document::Document;
use crate::i18n::{self, Localizer};
use crate::persistence::autosave::{Abandoned, Autosaver, Clock, Session, SystemClock, find_abandoned};
use crate::persistence::crash::{RecentLog, reports_since};
use crate::persistence::prefs::{Preferences, ThemeChoice};
use crate::persistence::{DataDirs, save_atomically};
use crate::tools::ToolKind;
use crate::ui::actions::Action;
use crate::ui::dialogs::{self, Dialogs};
use crate::ui::{browser, properties, shell};
use crate::views::plan;
use crate::views::view3d::{self, View3dState, model_bounds};
use crate::workspace::{Pane, Request, Workspace};

/// The extensions the open and save dialogs list (IFX-001). Backups end in
/// `.bak` or `.bakN` and are therefore not listed (PRJ-005).
pub const IFCX_EXTENSIONS: &[&str] = &["ifcx"];

/// The path a save writes to: `path` with `.ifcx` added when the chosen name
/// has no extension (IFX-001).
pub fn with_ifcx_extension(path: PathBuf) -> PathBuf {
    if path.extension().is_none() {
        path.with_extension(IFCX_EXTENSIONS[0])
    } else {
        path
    }
}

/// What to do once unsaved changes are saved or discarded.
#[derive(Clone, Debug, PartialEq)]
enum Then {
    New,
    Open(Option<PathBuf>),
    Close,
    Quit,
    Recover(usize),
}

/// Work to do after a prompt closes, with access to the whole application.
type FollowUp = Box<dyn FnOnce(&mut ArchiCraftApp, &egui::Context)>;

#[derive(Clone, Debug)]
enum Prompt {
    Unsaved(Then),
    Message {
        title: String,
        text: String,
    },
    LoadReport,
    /// IFX-062: the file was written by a newer ArchiCraft.
    OverwriteNewer {
        save_as: bool,
    },
}

impl Prompt {
    fn title(&self) -> &str {
        match self {
            Prompt::Unsaved(_) => "Save changes?",
            Prompt::Message { title, .. } => title,
            Prompt::LoadReport => "The project was opened with problems",
            Prompt::OverwriteNewer { .. } => "Overwrite a newer file?",
        }
    }
}

pub struct ArchiCraftApp {
    dirs: DataDirs,
    ws: Workspace,
    tree: Tree<Pane>,
    dialogs: Dialogs,
    session: Option<Session>,
    autosaver: Autosaver,
    /// The time the autosave interval is measured with (PRJ-004).
    clock: Arc<dyn Clock>,
    /// When the last autosave was queued, by `clock`.
    last_autosave: Duration,
    load_problems: Vec<Problem>,
    prompt: Option<Prompt>,
    recovery: Vec<Abandoned>,
    crash_reports: Vec<PathBuf>,
    quit_confirmed: bool,
    newer_confirmed: bool,
    title: String,
    theme: Option<ThemeChoice>,
    /// The user-interface language, installed at the start of every frame
    /// (NFR-L10N-001).
    localizer: Rc<Localizer>,
    released: Vec<View3dState>,
    /// The largest file opened or recovered (Appendix D.9, NFR-SEC-003);
    /// lowered only by tests.
    file_size_limit: u64,
    _log: Arc<RecentLog>,
}

impl std::fmt::Debug for ArchiCraftApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArchiCraftApp")
            .field("ws", &self.ws)
            .finish_non_exhaustive()
    }
}

fn default_tree(ws: &Workspace) -> Tree<Pane> {
    let mut panes = vec![Pane::Plan(ws.doc.project.storeys.ground())];
    if let Some(view) = ws.doc.project.views.first() {
        panes.push(Pane::View3d(view.id));
    }
    Tree::new_tabs(Id::new("archicraft-views"), panes)
}

impl ArchiCraftApp {
    /// Creates the application. `open` is a project given on the command
    /// line.
    pub fn new(dirs: DataDirs, prefs: Preferences, log: Arc<RecentLog>, open: Option<PathBuf>) -> Self {
        Self::with_clock(dirs, prefs, log, open, Arc::new(SystemClock::new()))
    }

    /// Creates the application with the autosave interval measured by
    /// `clock` (PRJ-004), so that tests can step through it (AS-05).
    pub fn with_clock(
        dirs: DataDirs,
        prefs: Preferences,
        log: Arc<RecentLog>,
        open: Option<PathBuf>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let undo_steps = prefs.undo_steps;
        let ws = Workspace::new(Document::new(default_metric_project(), undo_steps), prefs);
        let tree = default_tree(&ws);
        let session = match Session::start(&dirs.recovery()) {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::error!("autosave is unavailable: {e}");
                None
            }
        };
        let recovery = find_abandoned(&dirs.recovery());
        let crash_reports = reports_since(&dirs.crash_reports(), ws.prefs.last_crash_report.as_deref());
        let mut app = Self {
            dirs,
            ws,
            tree,
            dialogs: Dialogs::default(),
            session,
            autosaver: Autosaver::new(),
            last_autosave: clock.now(),
            clock,
            load_problems: Vec::new(),
            prompt: None,
            recovery,
            crash_reports,
            quit_confirmed: false,
            newer_confirmed: false,
            title: String::new(),
            theme: None,
            localizer: Rc::new(Localizer::english()),
            released: Vec::new(),
            file_size_limit: archicraft_ifcx::validate::MAX_FILE_SIZE as u64,
            _log: log,
        };
        if let Some(path) = open {
            app.open_path(&path);
        }
        app
    }

    /// The workspace, for tests.
    pub fn workspace(&mut self) -> &mut Workspace {
        &mut self.ws
    }

    /// The autosaves written so far, after waiting up to `timeout` for the
    /// queued ones; `None` if some are still pending. For tests (AS-05).
    #[doc(hidden)]
    pub fn autosaves_written(&self, timeout: Duration) -> Option<usize> {
        self.autosaver.wait_idle(timeout).then(|| self.autosaver.written())
    }

    /// The layout of the views, for tests that build a split (UI-007).
    #[doc(hidden)]
    pub fn tree_mut(&mut self) -> &mut Tree<Pane> {
        &mut self.tree
    }

    /// The frame and latency samples so far (NFR-PERF-001).
    #[cfg(feature = "perf")]
    pub fn perf(&self) -> &crate::perf::PerfRecorder {
        &self.ws.perf
    }

    /// The title of the open application prompt, for tests
    /// (NFR-USE-004).
    #[doc(hidden)]
    pub fn prompt_title(&self) -> Option<String> {
        self.prompt.as_ref().map(|p| p.title().to_owned())
    }

    /// Lowers the file size limit, so that tests can open an oversized file
    /// without writing 2 GB.
    pub fn set_file_size_limit(&mut self, limit: u64) {
        self.file_size_limit = limit;
    }

    /// Switches the user-interface language from the next frame on
    /// (NFR-L10N-001), for example to the en-XA pseudo-locale in tests.
    pub fn set_localizer(&mut self, localizer: Rc<Localizer>) {
        self.localizer = localizer;
    }

    fn message(&mut self, title: &str, text: impl Into<String>) {
        self.prompt = Some(Prompt::Message {
            title: title.to_owned(),
            text: text.into(),
        });
    }

    fn replace_document(&mut self, doc: Document) {
        // 3D views hold GPU textures registered with egui; release them on
        // the next frame, when the render state is at hand.
        self.released.extend(self.ws.views_3d.drain().map(|(_, state)| state));
        self.ws.replace_document(doc);
        self.tree = default_tree(&self.ws);
        self.newer_confirmed = false;
    }

    /// Opens a project file (PRJ-003, PRJ-008), checking its size before
    /// reading it (NFR-SEC-003).
    pub fn open_path(&mut self, path: &Path) {
        match archicraft_ifcx::load_path_with_limit(path, self.file_size_limit) {
            Ok(outcome) => {
                let doc = Document::opened(
                    outcome.project,
                    path.to_owned(),
                    outcome.newer_format,
                    self.ws.prefs.undo_steps,
                );
                self.replace_document(doc);
                self.load_problems = outcome.problems;
                if !self.load_problems.is_empty() {
                    self.prompt = Some(Prompt::LoadReport);
                }
                self.ws.prefs.add_recent(path);
                self.save_prefs();
                self.ws.info(format!("Opened {}", path.display()));
            }
            Err(problem) => self.message("Cannot open the project", format!("{}: {problem}", path.display())),
        }
    }

    fn save_prefs(&mut self) {
        if let Err(e) = self.ws.prefs.save(&self.dirs.preferences_file()) {
            tracing::warn!("preferences not saved: {e}");
        }
    }

    /// Saves the project (PRJ-003, PRJ-005, NFR-REL-001…003). Returns true
    /// if it was saved.
    fn save(&mut self, save_as: bool) -> bool {
        if self.ws.doc.newer_format && !self.newer_confirmed {
            self.prompt = Some(Prompt::OverwriteNewer { save_as });
            return false;
        }
        let path = match (&self.ws.doc.path, save_as) {
            (Some(path), false) => path.clone(),
            _ => {
                let mut dialog = rfd::FileDialog::new()
                    .add_filter("IFCX project", IFCX_EXTENSIONS)
                    .set_file_name(format!("{}.{}", self.ws.doc.project.info.name, IFCX_EXTENSIONS[0]));
                if let Some(dir) = self.ws.doc.path.as_ref().and_then(|p| p.parent()) {
                    dialog = dialog.set_directory(dir);
                }
                match dialog.save_file() {
                    Some(p) => with_ifcx_extension(p),
                    None => return false,
                }
            }
        };
        self.save_to(&path)
    }

    /// Writes the project to `path`, indented if the preferences ask for it
    /// (IFX-008). Save, Save as and the confirmed overwrite of a newer file
    /// (IFX-062) all end here.
    pub fn save_to(&mut self, path: &Path) -> bool {
        let (bytes, warnings) = self.ws.doc.to_bytes(&self.ws.prefs.author, self.ws.prefs.indented_ifcx);
        match save_atomically(path, &bytes, self.ws.prefs.backups) {
            Ok(()) => {
                self.ws.doc.saved_to(path.to_owned());
                self.ws.prefs.add_recent(path);
                self.save_prefs();
                if let Some(session) = &self.session {
                    session.clear();
                }
                match warnings.first() {
                    Some(w) => self.ws.error(format!("Saved with warnings: {w}")),
                    None => self.ws.info(format!("Saved {}", path.display())),
                }
                true
            }
            Err(e) => {
                self.message("The project was not saved", format!("{}: {e}", path.display()));
                false
            }
        }
    }

    /// Runs `then`, first asking to save unsaved changes (PRJ-003).
    fn guard(&mut self, ctx: &egui::Context, then: Then) {
        if self.ws.doc.is_dirty() {
            self.prompt = Some(Prompt::Unsaved(then));
        } else {
            self.proceed(ctx, then);
        }
    }

    fn proceed(&mut self, ctx: &egui::Context, then: Then) {
        match then {
            Then::New | Then::Close => {
                let doc = Document::new(default_metric_project(), self.ws.prefs.undo_steps);
                self.replace_document(doc);
                self.load_problems.clear();
                if let Some(session) = &self.session {
                    session.clear();
                }
            }
            Then::Open(path) => {
                let path = path.or_else(|| {
                    rfd::FileDialog::new()
                        .add_filter("IFCX project", IFCX_EXTENSIONS)
                        .pick_file()
                });
                if let Some(path) = path {
                    self.open_path(&path);
                }
            }
            Then::Quit => {
                self.quit_confirmed = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            Then::Recover(index) => {
                if index < self.recovery.len() {
                    let abandoned = self.recovery.remove(index);
                    match archicraft_ifcx::load_path_with_limit(&abandoned.model, self.file_size_limit)
                        .map_err(|p| p.to_string())
                    {
                        Ok(outcome) => {
                            let doc = Document::recovered(
                                outcome.project,
                                abandoned.info.original_path.clone(),
                                self.ws.prefs.undo_steps,
                            );
                            self.replace_document(doc);
                            self.load_problems = outcome.problems;
                            abandoned.discard();
                            self.ws.info("Recovered the autosaved project. Save it to keep it.");
                        }
                        Err(e) => self.message("Recovery failed", e),
                    }
                }
            }
        }
    }

    fn open_pane(&mut self, pane: Pane) {
        if let Pane::Plan(storey) = pane {
            self.ws.active_storey = storey;
        }
        self.ws.active_pane = Some(pane);
        if let Some(tile) = self.tree.tiles.find_pane(&pane) {
            self.tree.make_active(|id, _| id == tile);
            return;
        }
        let new = self.tree.tiles.insert_pane(pane);
        // Add the tab to the tab group that shows the most recent view.
        let target = self.tree.tiles.iter().find_map(|(id, tile)| match tile {
            Tile::Container(egui_tiles::Container::Tabs(_)) => Some(*id),
            _ => None,
        });
        match target {
            Some(tabs) => {
                if let Some(Tile::Container(container)) = self.tree.tiles.get_mut(tabs) {
                    container.add_child(new);
                }
            }
            None => {
                let tabs = self.tree.tiles.insert_tab_tile(vec![new]);
                self.tree.root = Some(tabs);
            }
        }
        self.tree.make_active(|id, _| id == new);
    }

    /// Panes in tab order, for next/previous tab.
    fn panes(&self) -> Vec<Pane> {
        let mut out = Vec::new();
        fn walk(tiles: &Tiles<Pane>, id: TileId, out: &mut Vec<Pane>) {
            match tiles.get(id) {
                Some(Tile::Pane(p)) => out.push(*p),
                Some(Tile::Container(c)) => {
                    for child in c.children() {
                        walk(tiles, *child, out);
                    }
                }
                None => {}
            }
        }
        if let Some(root) = self.tree.root {
            walk(&self.tree.tiles, root, &mut out);
        }
        out
    }

    fn cycle_tab(&mut self, step: isize) {
        let panes = self.panes();
        if panes.is_empty() {
            return;
        }
        let current = self
            .ws
            .active_pane
            .and_then(|p| panes.iter().position(|q| *q == p))
            .unwrap_or(0) as isize;
        let next = (current + step).rem_euclid(panes.len() as isize) as usize;
        self.open_pane(panes[next]);
    }

    fn zoom_active(&mut self, ids: Option<Vec<archicraft_model::ElementId>>) {
        match self.ws.active_pane {
            Some(Pane::Plan(storey)) => {
                let plan = self.ws.doc.plan(storey);
                if let Some(state) = self.ws.plan_views.get_mut(&storey) {
                    let rect = state.last_rect;
                    match ids {
                        Some(ids) => state.zoom_to(rect, &plan, &ids),
                        None => state.fit(rect, &plan),
                    }
                }
            }
            Some(Pane::View3d(view)) => {
                let geometry = self.ws.doc.geometry();
                let bounds = model_bounds(&self.ws.doc.project, &geometry, ids.as_deref());
                if let Some(state) = self.ws.views_3d.get_mut(&view) {
                    state.fit(bounds);
                }
            }
            None => {}
        }
    }

    fn run(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::New => self.guard(ctx, Then::New),
            Action::Open => self.guard(ctx, Then::Open(None)),
            Action::Save => {
                self.save(false);
            }
            Action::SaveAs => {
                self.save(true);
            }
            Action::Close => self.guard(ctx, Then::Close),
            Action::Quit => self.guard(ctx, Then::Quit),
            Action::Undo => self.ws.undo(),
            Action::Redo => self.ws.redo(),
            Action::Delete => self.ws.delete_selection(),
            Action::SelectAll => self.ws.select_all(),
            Action::ToolSelect => self.ws.select_tool(ToolKind::Select),
            Action::ToolWall => self.ws.select_tool(ToolKind::Wall),
            Action::ToolSlab => self.ws.select_tool(ToolKind::Slab),
            Action::Settings => self.dialogs.settings = true,
            Action::FitWindow => self.zoom_active(None),
            Action::ZoomSelection => {
                let ids = self.ws.selected_ids();
                if !ids.is_empty() {
                    self.zoom_active(Some(ids));
                }
            }
            Action::PreviousZoom | Action::NextZoom => {
                if let Some(Pane::Plan(storey)) = self.ws.active_pane
                    && let Some(state) = self.ws.plan_views.get_mut(&storey)
                {
                    if action == Action::PreviousZoom {
                        state.previous_zoom();
                    } else {
                        state.next_zoom();
                    }
                }
            }
            Action::NextTab => self.cycle_tab(1),
            Action::PreviousTab => self.cycle_tab(-1),
            Action::SwitchPlan3d => {
                let target = match self.ws.active_pane {
                    Some(Pane::Plan(_)) => self.ws.doc.project.views.first().map(|v| Pane::View3d(v.id)),
                    _ => Some(Pane::Plan(self.ws.active_storey)),
                };
                if let Some(pane) = target {
                    self.open_pane(pane);
                }
            }
            Action::OrbitMode => {
                let view = match self.ws.active_pane {
                    Some(Pane::View3d(v)) => Some(v),
                    _ => self.ws.doc.project.views.first().map(|v| v.id),
                };
                if let Some(view) = view {
                    self.open_pane(Pane::View3d(view));
                    let projection = self
                        .ws
                        .doc
                        .project
                        .view(view)
                        .map_or(archicraft_model::project::Projection::Perspective, |v| v.projection);
                    let state = self
                        .ws
                        .views_3d
                        .entry(view)
                        .or_insert_with(|| View3dState::new(projection));
                    state.orbit = !state.orbit;
                }
            }
            Action::ToggleReferenceLines => {
                self.ws.prefs.show_reference_lines = !self.ws.prefs.show_reference_lines;
                self.save_prefs();
            }
            Action::Storeys => self.dialogs.storeys = true,
            Action::Layers => self.dialogs.layers = true,
            Action::Materials => self.dialogs.materials = true,
            Action::ProjectSettings => self.dialogs.open_project_settings(&self.ws),
            Action::Preferences => self.dialogs.preferences = Some(self.ws.prefs.clone()),
            Action::Problems => self.dialogs.problems = !self.dialogs.problems,
            Action::Shortcuts => self.dialogs.shortcuts = true,
            Action::About => self.dialogs.about = true,
        }
    }

    /// Keyboard shortcuts (UI-008). Single-key shortcuts are ignored while a
    /// text field has focus or a value is being typed during point input.
    fn shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.text_edit_focused();
        let drawing = self.ws.input.captures_keys(&self.ws.tool);
        let mut triggered = ctx.input_mut(|i| {
            Action::consume_shortcuts(i, |action| {
                (!typing || action.works_while_typing())
                    && !(drawing && (action.is_single_key() || action == Action::NextTab))
            })
        });
        if !typing {
            let redo_y = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Y);
            if ctx.input_mut(|i| i.consume_shortcut(&redo_y)) {
                triggered.push(Action::Redo);
            }
            let backspace = egui::KeyboardShortcut::new(egui::Modifiers::NONE, egui::Key::Backspace);
            if !drawing && ctx.input_mut(|i| i.consume_shortcut(&backspace)) {
                triggered.push(Action::Delete);
            }
        }
        for action in triggered {
            self.run(ctx, action);
        }
    }

    fn autosave(&mut self, ctx: &egui::Context) {
        let interval = Duration::from_secs(u64::from(self.ws.prefs.autosave_minutes) * 60);
        if let Some(session) = &self.session
            && self.ws.doc.needs_autosave()
            && self.clock.now().saturating_sub(self.last_autosave) >= interval
        {
            #[cfg(feature = "perf")]
            let started = std::time::Instant::now();
            let geometry = self.ws.doc.geometry();
            self.autosaver.request(
                session,
                &self.ws.doc.project,
                geometry,
                self.ws.doc.path.clone(),
                &self.ws.prefs.author,
            );
            self.ws.doc.mark_autosaved();
            self.last_autosave = self.clock.now();
            #[cfg(feature = "perf")]
            self.ws
                .perf
                .record(crate::perf::Metric::AutosaveBlocking, started.elapsed());
        }
        ctx.request_repaint_after(Duration::from_secs(30));
    }

    fn update_title_and_theme(&mut self, ctx: &egui::Context) {
        let title = format!(
            "{}{} — ArchiCraft",
            if self.ws.doc.is_dirty() { "● " } else { "" },
            self.ws.doc.display_name()
        );
        if title != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        if self.theme != Some(self.ws.prefs.theme) {
            ctx.set_theme(match self.ws.prefs.theme {
                ThemeChoice::System => ThemePreference::System,
                ThemeChoice::Light => ThemePreference::Light,
                ThemeChoice::Dark => ThemePreference::Dark,
            });
            self.theme = Some(self.ws.prefs.theme);
        }
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.quit_confirmed {
            if self.ws.doc.is_dirty() {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                self.prompt = Some(Prompt::Unsaved(Then::Quit));
            } else {
                self.quit_confirmed = true;
            }
        }
    }

    fn prompts(&mut self, ctx: &egui::Context) {
        let Some(prompt) = self.prompt.clone() else {
            return;
        };
        let mut close = false;
        let mut follow_up: Option<FollowUp> = None;
        let modal = egui::Modal::new(Id::new("prompt")).show(ctx, |ui| {
            ui.set_max_width(480.0);
            ui.heading(prompt.title());
            match &prompt {
                Prompt::Unsaved(then) => {
                    ui.label(format!(
                        "“{}” has unsaved changes. Save them before continuing?",
                        self.ws.doc.display_name()
                    ));
                    ui.separator();
                    ui.horizontal(|ui| {
                        let then = then.clone();
                        if ui.button("Save").clicked() {
                            close = true;
                            follow_up = Some(Box::new(move |app, ctx| {
                                if app.save(false) {
                                    app.proceed(ctx, then);
                                }
                            }));
                        } else if ui.button("Don't save").clicked() {
                            close = true;
                            let then = then.clone();
                            follow_up = Some(Box::new(move |app, ctx| app.proceed(ctx, then)));
                        } else if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                }
                Prompt::Message { text, .. } => {
                    ui.label(text);
                    if ui.button("OK").clicked() {
                        close = true;
                    }
                }
                Prompt::LoadReport => {
                    ui.label("Everything that could be read was loaded. These objects were skipped, repaired or replaced:");
                    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                        for p in &self.load_problems {
                            ui.label(p.to_string());
                        }
                    });
                    if ui.button("OK").clicked() {
                        close = true;
                    }
                }
                Prompt::OverwriteNewer { save_as } => {
                    ui.label(
                        "This file was written by a newer version of ArchiCraft. Saving it with this version may lose data that this version does not know.",
                    );
                    ui.horizontal(|ui| {
                        let save_as = *save_as;
                        if ui.button("Save anyway").clicked() {
                            close = true;
                            follow_up = Some(Box::new(move |app, _| {
                                app.newer_confirmed = true;
                                app.save(save_as);
                            }));
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                }
            }
        });
        if modal.should_close() && !matches!(prompt, Prompt::Unsaved(_)) {
            close = true;
        }
        if close {
            self.prompt = None;
        }
        if let Some(f) = follow_up {
            f(self, ctx);
        }
    }

    fn startup_windows(&mut self, ctx: &egui::Context) {
        if !self.recovery.is_empty() && self.prompt.is_none() {
            let mut action: Option<(usize, bool)> = None;
            let mut later = false;
            egui::Window::new("Recover unsaved work")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(
                        "ArchiCraft did not close properly last time. These autosaves can be recovered (PRJ-004):",
                    );
                    for (i, a) in self.recovery.iter().enumerate() {
                        ui.horizontal(|ui| {
                            let place = a
                                .info
                                .original_path
                                .as_ref()
                                .map_or_else(|| "never saved".to_owned(), |p| p.display().to_string());
                            ui.label(format!(
                                "{} — autosaved {} ({place})",
                                a.info.project_name, a.info.saved_at
                            ));
                            if ui.button("Recover").clicked() {
                                action = Some((i, true));
                            }
                            if ui.button("Discard").clicked() {
                                action = Some((i, false));
                            }
                        });
                    }
                    if ui.button("Decide later").clicked() {
                        later = true;
                    }
                });
            match action {
                Some((i, true)) => self.guard(ctx, Then::Recover(i)),
                Some((i, false)) => {
                    let a = self.recovery.remove(i);
                    a.discard();
                }
                None => {}
            }
            if later {
                self.recovery.clear();
            }
        }
        if !self.crash_reports.is_empty() {
            let mut dismissed = false;
            egui::Window::new("ArchiCraft crashed last time")
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label("A crash report was saved on this computer. Nothing was sent. You can pass it on to the developers if you want to:");
                    for path in &self.crash_reports {
                        ui.horizontal(|ui| {
                            ui.monospace(path.display().to_string());
                            if ui.small_button("Copy path").clicked() {
                                ctx.copy_text(path.display().to_string());
                            }
                        });
                    }
                    if ui.button("OK").clicked() {
                        dismissed = true;
                    }
                });
            if dismissed {
                self.ws.prefs.last_crash_report = self
                    .crash_reports
                    .first()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned());
                self.crash_reports.clear();
                self.save_prefs();
            }
        }
    }

    /// Advances a scripted measurement (`ARCHICRAFT_PERF_SCRIPT`): opens its
    /// view, moves the camera every frame, drops the warm-up frames and quits
    /// at the end; `on_exit` writes the results.
    #[cfg(feature = "perf")]
    fn drive_script(&mut self, ctx: &egui::Context) {
        let Some(run) = self.ws.perf.script_mut() else {
            return;
        };
        let script = run.script;
        match run.advance() {
            Some(frame) => {
                if frame == crate::perf::WARM_UP_FRAMES {
                    self.ws.perf.reset_samples();
                }
                if let Some(pane) = script.pane(&self.ws) {
                    if frame == 0 {
                        self.open_pane(pane);
                    }
                    script.step(&mut self.ws, pane, frame);
                }
                // egui repaints only on input: without this the run would
                // measure idle time.
                ctx.request_repaint();
            }
            None if !self.quit_confirmed => {
                self.quit_confirmed = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            None => {}
        }
    }

    /// Closes a pending edit sample once the geometry and the active plan
    /// show the new revision (NFR-PERF-006).
    #[cfg(feature = "perf")]
    fn settle_edit(&mut self) {
        let storey = match self.ws.active_pane {
            Some(Pane::Plan(storey)) => Some(storey),
            _ => None,
        };
        let current = self.ws.doc.is_current(storey);
        self.ws.perf.edit_settled(self.ws.doc.project.revision, current);
    }

    fn handle_requests(&mut self) {
        for request in std::mem::take(&mut self.ws.requests) {
            match request {
                Request::OpenPane(pane) => self.open_pane(pane),
                Request::ZoomTo(ids) if ids.is_empty() => self.zoom_active(None),
                Request::ZoomTo(ids) => self.zoom_active(Some(ids)),
                Request::Settings => self.dialogs.settings = true,
            }
        }
    }
}

struct Behavior<'a> {
    ws: &'a mut Workspace,
    render_state: Option<&'a egui_wgpu::RenderState>,
    closed: Vec<Pane>,
}

impl egui_tiles::Behavior<Pane> for Behavior<'_> {
    fn pane_ui(&mut self, ui: &mut Ui, _tile_id: TileId, pane: &mut Pane) -> UiResponse {
        match *pane {
            Pane::Plan(storey) => plan::ui(ui, self.ws, storey),
            Pane::View3d(view) => view3d::ui(ui, self.ws, view, self.render_state),
        }
        UiResponse::None
    }

    fn tab_title_for_pane(&mut self, pane: &Pane) -> WidgetText {
        let project = &self.ws.doc.project;
        match pane {
            Pane::Plan(storey) => project
                .storeys
                .get(*storey)
                .map_or_else(|| "Plan".to_owned(), |s| format!("Plan: {}", s.name))
                .into(),
            Pane::View3d(view) => project
                .view(*view)
                .map_or_else(|| "3D".to_owned(), |v| v.name.clone())
                .into(),
        }
    }

    fn on_tab_button(&mut self, tiles: &mut Tiles<Pane>, tile_id: TileId, response: egui::Response) -> egui::Response {
        if (response.clicked() || response.drag_started())
            && let Some(Tile::Pane(pane)) = tiles.get(tile_id)
        {
            self.ws.active_pane = Some(*pane);
            if let Pane::Plan(storey) = pane {
                self.ws.active_storey = *storey;
            }
        }
        response
    }

    fn is_tab_closable(&self, _tiles: &Tiles<Pane>, _tile_id: TileId) -> bool {
        true
    }

    fn on_tab_close(&mut self, tiles: &mut Tiles<Pane>, tile_id: TileId) -> bool {
        if let Some(Tile::Pane(pane)) = tiles.get(tile_id) {
            self.closed.push(*pane);
        }
        true
    }

    fn retain_pane(&mut self, pane: &Pane) -> bool {
        let project = &self.ws.doc.project;
        match pane {
            Pane::Plan(storey) => project.storeys.get(*storey).is_some(),
            Pane::View3d(view) => project.view(*view).is_some(),
        }
    }

    fn simplification_options(&self) -> SimplificationOptions {
        SimplificationOptions {
            all_panes_must_have_tabs: true,
            ..SimplificationOptions::default()
        }
    }
}

impl eframe::App for ArchiCraftApp {
    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        #[cfg(feature = "perf")]
        self.ws.perf.begin_frame();
        i18n::install(Rc::clone(&self.localizer));
        let ctx = ui.ctx().clone();
        #[cfg(feature = "perf")]
        self.drive_script(&ctx);
        for state in self.released.drain(..) {
            state.release(frame.wgpu_render_state());
        }
        self.handle_close_request(&ctx);
        self.update_title_and_theme(&ctx);
        if self.prompt.is_none() {
            self.shortcuts(&ctx);
        }
        self.ws.prune_selection();

        let mut actions: Vec<Action> = Vec::new();
        let mut recent: Option<PathBuf> = None;
        egui::Panel::top("menu").show(ui, |ui| {
            let (action, r) = shell::menu_bar(ui, &self.ws, self.dialogs.problems);
            actions.extend(action);
            recent = r;
        });
        egui::Panel::top("properties").show(ui, |ui| properties::strip(ui, &mut self.ws));
        let problem_count = self.ws.doc.geometry().problems.len() + self.load_problems.len();
        egui::Panel::bottom("status").show(ui, |ui| {
            actions.extend(shell::status_bar(ui, &mut self.ws, problem_count));
        });
        if self.dialogs.problems {
            egui::Panel::bottom("problems").resizable(true).show(ui, |ui| {
                dialogs::problems_panel(ui, &mut self.ws, &self.load_problems);
            });
        }
        egui::Panel::left("toolbox").resizable(false).show(ui, |ui| {
            actions.extend(shell::toolbox(ui, &mut self.ws));
        });
        egui::Panel::right("browser")
            .resizable(true)
            .default_size(240.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| browser::ui(ui, &mut self.ws, &mut self.dialogs));
            });
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            if self.tree.root.is_none() {
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new("Open a plan or 3D view from the project browser.").weak());
                });
                return;
            }
            let mut behavior = Behavior {
                ws: &mut self.ws,
                render_state: frame.wgpu_render_state(),
                closed: Vec::new(),
            };
            self.tree.ui(&mut behavior, ui);
            let closed = std::mem::take(&mut behavior.closed);
            for pane in closed {
                if let Pane::View3d(view) = pane
                    && let Some(state) = self.ws.views_3d.remove(&view)
                {
                    state.release(frame.wgpu_render_state());
                }
            }
        });

        #[cfg(feature = "perf")]
        crate::perf::window(&ctx, &self.ws.perf);
        if let Some(prefs) = self.dialogs.show(&ctx, &mut self.ws) {
            self.ws.prefs = prefs;
            self.save_prefs();
        }
        self.handle_requests();
        self.startup_windows(&ctx);
        self.prompts(&ctx);
        for action in actions {
            self.run(&ctx, action);
        }
        if let Some(path) = recent {
            self.guard(&ctx, Then::Open(Some(path)));
        }
        self.autosave(&ctx);
        #[cfg(feature = "perf")]
        {
            self.settle_edit();
            let interval = ctx.input(|i| i.unstable_dt);
            self.ws.perf.end_frame(Duration::from_secs_f32(interval.max(0.0)));
        }
    }

    fn on_exit(&mut self) {
        self.autosaver.shutdown();
        self.save_prefs();
        if let Some(session) = self.session.take() {
            session.end();
        }
        #[cfg(feature = "perf")]
        match self.ws.perf.write_results(self.dirs.root()) {
            Ok(path) => tracing::info!(path = %path.display(), "{}", self.ws.perf.summary()),
            Err(e) => tracing::warn!("performance results not written: {e}"),
        }
    }
}
