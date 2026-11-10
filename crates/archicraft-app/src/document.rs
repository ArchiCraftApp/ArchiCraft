//! An open project: the model, its undo history, where it is saved, and the
//! evaluated geometry and plans derived from it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use archicraft_geometry::ModelGeometry;
use archicraft_geometry::contour::{ContourGraph, ContourOptions};
use archicraft_geometry::plan::{PlanDrawing, ViewRange, plan_drawing};
use archicraft_geometry::space::SpaceRegions;
use archicraft_ifcx::{Problem, WriteOptions};
use archicraft_model::{Command, CommandError, History, Message, Project, StoreyId};

use crate::persistence::utc_timestamp;

/// A state ID that no history produces: marks a document that has never been
/// saved to its path, for example one restored from an autosave.
const NEVER_SAVED: u64 = u64::MAX;

#[derive(Debug)]
pub struct Document {
    pub project: Project,
    pub history: History,
    pub path: Option<PathBuf>,
    /// The file was written by a newer ArchiCraft (IFX-062).
    pub newer_format: bool,
    saved_state: u64,
    autosaved_state: u64,
    geometry: Arc<ModelGeometry>,
    plans: HashMap<StoreyId, (u64, Arc<PlanDrawing>)>,
    /// The contour graph of each storey's plan with the revision it was
    /// built for (INP-018).
    contours: HashMap<StoreyId, (u64, Arc<ContourGraph>)>,
    /// How many contour graphs were built, for tests of the cache.
    contour_builds: usize,
    /// The regions the space tool detects rooms in, per storey, with the
    /// revision they were built for (SPC-001).
    space_regions: HashMap<StoreyId, (u64, Arc<SpaceRegions>)>,
    /// How many space regions were built, for tests of the cache.
    space_region_builds: usize,
}

impl Document {
    pub fn new(project: Project, undo_steps: usize) -> Self {
        let geometry = Arc::new(ModelGeometry::evaluate(&project));
        Self {
            project,
            history: History::new(undo_steps),
            path: None,
            newer_format: false,
            saved_state: 0,
            autosaved_state: 0,
            geometry,
            plans: HashMap::new(),
            contours: HashMap::new(),
            contour_builds: 0,
            space_regions: HashMap::new(),
            space_region_builds: 0,
        }
    }

    /// A document opened from `path`.
    pub fn opened(project: Project, path: PathBuf, newer_format: bool, undo_steps: usize) -> Self {
        Self {
            path: Some(path),
            newer_format,
            ..Self::new(project, undo_steps)
        }
    }

    /// A document restored from an autosave: it has unsaved changes until the
    /// user saves it (PRJ-004).
    pub fn recovered(project: Project, original_path: Option<PathBuf>, undo_steps: usize) -> Self {
        Self {
            path: original_path,
            saved_state: NEVER_SAVED,
            autosaved_state: NEVER_SAVED,
            ..Self::new(project, undo_steps)
        }
    }

    /// True if there are changes since the last save (PRJ-003).
    pub fn is_dirty(&self) -> bool {
        self.history.state_id() != self.saved_state
    }

    /// True if there are changes the autosave has not captured.
    pub fn needs_autosave(&self) -> bool {
        self.history.state_id() != self.autosaved_state && self.is_dirty()
    }

    pub fn mark_autosaved(&mut self) {
        self.autosaved_state = self.history.state_id();
    }

    pub fn execute(&mut self, command: Command) -> Result<(), CommandError> {
        tracing::info!(command = %command.label(), "execute");
        self.history.execute(&mut self.project, command)
    }

    pub fn undo(&mut self) -> Option<Message> {
        self.history.undo(&mut self.project)
    }

    pub fn redo(&mut self) -> Option<Message> {
        self.history.redo(&mut self.project)
    }

    /// The evaluated geometry for the current model, re-evaluated after
    /// changes.
    pub fn geometry(&mut self) -> Arc<ModelGeometry> {
        if self.geometry.revision != self.project.revision {
            self.geometry = Arc::new(ModelGeometry::evaluate(&self.project));
            self.plans.clear();
            self.contours.clear();
            self.space_regions.clear();
        }
        self.geometry.clone()
    }

    /// The plan of a storey, generated from the current geometry.
    pub fn plan(&mut self, storey: StoreyId) -> Arc<PlanDrawing> {
        let geometry = self.geometry();
        let revision = self.project.revision;
        if let Some((rev, plan)) = self.plans.get(&storey)
            && *rev == revision
        {
            return plan.clone();
        }
        let project = &self.project;
        let range = ViewRange::for_storey(project, storey);
        let plan = Arc::new(plan_drawing(project, &geometry, storey, range, |e| {
            project.is_visible(e)
        }));
        self.plans.insert(storey, (revision, plan.clone()));
        plan
    }

    /// The contour graph of a storey's plan (INP-018), built from its
    /// [`PlanDrawing`] on first use and kept until the plan's revision
    /// changes, so hovering never builds it per frame.
    pub fn contour_graph(&mut self, storey: StoreyId) -> Arc<ContourGraph> {
        let plan = self.plan(storey);
        let revision = self.project.revision;
        if let Some((rev, graph)) = self.contours.get(&storey)
            && *rev == revision
        {
            return graph.clone();
        }
        let graph = Arc::new(ContourGraph::build(plan.as_ref(), &ContourOptions::default()));
        self.contour_builds += 1;
        self.contours.insert(storey, (revision, graph.clone()));
        graph
    }

    /// How many contour graphs [`Document::contour_graph`] has built.
    pub fn contour_builds(&self) -> usize {
        self.contour_builds
    }

    /// The regions of a storey that the space tool detects rooms in
    /// (SPC-001, INP-018), built from the footprints of the storey's walls on
    /// first use and kept until the model's revision changes, so hovering
    /// never builds them per frame.
    pub fn space_regions(&mut self, storey: StoreyId) -> Arc<SpaceRegions> {
        let revision = self.project.revision;
        if let Some((rev, regions)) = self.space_regions.get(&storey)
            && *rev == revision
        {
            return regions.clone();
        }
        let regions = Arc::new(SpaceRegions::build(&self.project, storey));
        self.space_region_builds += 1;
        self.space_regions.insert(storey, (revision, regions.clone()));
        regions
    }

    /// How many space regions [`Document::space_regions`] has built.
    pub fn space_region_builds(&self) -> usize {
        self.space_region_builds
    }

    /// True if the geometry, and the plan of `storey` if given, are
    /// evaluated for the current revision: the end of an edit's latency
    /// (NFR-PERF-006).
    #[cfg(feature = "perf")]
    pub fn is_current(&self, storey: Option<StoreyId>) -> bool {
        let revision = self.project.revision;
        self.geometry.revision == revision
            && storey.is_none_or(|s| self.plans.get(&s).is_some_and(|(rev, _)| *rev == revision))
    }

    /// The file name of the saved project, if it was saved.
    pub fn file_name(&self) -> Option<String> {
        self.path
            .as_deref()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
    }

    /// Serialises the project for saving as the next data version, as compact
    /// JSON or, with `indented`, indented and human-readable (IFX-008).
    pub fn to_bytes(&mut self, author: &str, indented: bool) -> (Vec<u8>, Vec<Problem>) {
        let geometry = self.geometry();
        let options = WriteOptions {
            author: author.to_owned(),
            timestamp: utc_timestamp(SystemTime::now()),
            data_version: self.project.data_version + 1,
        };
        archicraft_ifcx::save(&self.project, &geometry, &options, indented)
    }

    /// Records a successful save to `path`.
    pub fn saved_to(&mut self, path: PathBuf) {
        self.project.data_version += 1;
        self.path = Some(path);
        self.saved_state = self.history.state_id();
        self.autosaved_state = self.saved_state;
        self.newer_format = false;
    }
}

/// The window title (PRJ-003): the project's name with the marker of unsaved
/// changes while the document is dirty. The file name stays out, so renaming
/// the project in Project settings renames the window.
pub fn window_title(doc: &Document) -> String {
    let name = doc.project.info.name.as_str();
    if doc.is_dirty() {
        crate::tr!("window-title-unsaved", name = name)
    } else {
        crate::tr!("window-title", name = name)
    }
}

/// The question before unsaved changes are lost (PRJ-003), naming the
/// project and, once it was saved, its file.
pub fn unsaved_question(doc: &Document) -> String {
    let name = doc.project.info.name.as_str();
    match doc.file_name() {
        Some(file) => crate::tr!("modal-unsaved-text-file", name = name, file = file),
        None => crate::tr!("modal-unsaved-text", name = name),
    }
}

#[cfg(test)]
mod tests {
    use archicraft_model::glam::DVec2;
    use archicraft_model::template::default_metric_project;

    use super::*;

    #[test]
    fn dirty_tracking_follows_undo() {
        let mut doc = Document::new(default_metric_project(), 100);
        assert!(!doc.is_dirty());
        let storey = doc.project.storeys.ground();
        let wall = doc.project.new_wall(storey, DVec2::ZERO, DVec2::X * 3.0, 0);
        doc.execute(Command::AddElements(vec![wall])).unwrap();
        assert!(doc.is_dirty());
        assert!(doc.needs_autosave());
        doc.saved_to(PathBuf::from("/tmp/house.ifcx"));
        assert!(!doc.is_dirty());
        assert_eq!(doc.project.data_version, 1);
        doc.undo();
        assert!(doc.is_dirty(), "undoing past the save is a change");
        doc.redo();
        assert!(!doc.is_dirty());
    }

    #[test]
    fn geometry_and_plans_follow_the_model() {
        let mut doc = Document::new(default_metric_project(), 100);
        let storey = doc.project.storeys.ground();
        assert!(doc.plan(storey).elements.is_empty());
        let wall = doc.project.new_wall(storey, DVec2::ZERO, DVec2::X * 3.0, 0);
        doc.execute(Command::AddElements(vec![wall])).unwrap();
        assert_eq!(doc.geometry().elements.len(), 1);
        assert_eq!(doc.plan(storey).elements.len(), 1);
    }

    /// The file bytes after the header object, which holds the save time and
    /// the data version.
    fn without_header(bytes: &[u8]) -> &[u8] {
        let end = bytes.iter().position(|&b| b == b'}').unwrap();
        &bytes[end..]
    }

    /// IFX-008: indented output holds the same project as compact output, and
    /// choosing it once leaves later compact saves unchanged.
    #[test]
    fn indented_save_reads_back_and_leaves_compact_saves_unchanged() {
        let original = archicraft_model::samples::storey_with_walls_and_slab();
        let (indented, _) = Document::new(original.clone(), 100).to_bytes("Test", true);
        let text = std::str::from_utf8(&indented).unwrap();
        assert!(text.starts_with("{\n  \"header\": {\n    \""), "{}", &text[..40]);
        assert!(text.lines().count() > 20, "indented output spans many lines");
        for line in text.lines() {
            let indent = line.len() - line.trim_start_matches(' ').len();
            assert_eq!(indent % 2, 0, "two-space indentation: {line:?}");
            assert!(!line.starts_with('\t'));
        }

        let loaded = archicraft_ifcx::load(&indented).unwrap();
        assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
        let mut expected = original.clone();
        expected.revision = 0;
        expected.data_version = original.data_version + 1;
        assert_eq!(loaded.project, expected);

        let (compact, _) = Document::new(original, 100).to_bytes("Test", false);
        assert!(!compact.contains(&b'\n'), "compact output is one line");
        let (again, _) = Document::new(loaded.project, 100).to_bytes("Test", false);
        assert!(
            without_header(&again) == without_header(&compact),
            "a compact save after an indented one is the same file"
        );
    }

    /// PRJ-003: the title is the project's name, with the marker while there
    /// are unsaved changes, for a project never saved and one saved under
    /// another file name; renaming the project with `SetInfo` renames the
    /// window and marks it unsaved.
    #[test]
    fn the_window_title_shows_the_project_name_and_the_unsaved_marker() {
        let mut doc = Document::new(default_metric_project(), 100);
        let name = doc.project.info.name.clone();
        assert_eq!(window_title(&doc), format!("{name} — ArchiCraft"), "clean, never saved");
        let storey = doc.project.storeys.ground();
        let wall = doc.project.new_wall(storey, DVec2::ZERO, DVec2::X * 3.0, 0);
        doc.execute(Command::AddElements(vec![wall])).unwrap();
        assert_eq!(
            window_title(&doc),
            format!("● {name} — ArchiCraft"),
            "dirty, never saved"
        );
        doc.saved_to(PathBuf::from("/tmp/other.ifcx"));
        assert_eq!(window_title(&doc), format!("{name} — ArchiCraft"), "clean, saved");
        let mut info = doc.project.info.clone();
        info.name = "Villa Brandstötter".into();
        doc.execute(Command::SetInfo(info)).unwrap();
        assert_eq!(window_title(&doc), "● Villa Brandstötter — ArchiCraft", "renamed");
        assert!(!window_title(&doc).contains("other.ifcx"));
    }

    /// PRJ-003: the unsaved-changes question names the project and, once it
    /// was saved, the file.
    #[test]
    fn the_unsaved_question_names_the_project_and_the_file() {
        let mut doc = Document::new(default_metric_project(), 100);
        doc.project.info.name = "House".into();
        assert_eq!(
            unsaved_question(&doc),
            "“House” has unsaved changes. Save them before continuing?"
        );
        doc.saved_to(PathBuf::from("/tmp/other.ifcx"));
        assert_eq!(
            unsaved_question(&doc),
            "“House” (other.ifcx) has unsaved changes. Save them before continuing?"
        );
    }

    #[test]
    fn recovered_documents_are_unsaved() {
        let doc = Document::recovered(default_metric_project(), None, 100);
        assert!(doc.is_dirty());
    }
}
