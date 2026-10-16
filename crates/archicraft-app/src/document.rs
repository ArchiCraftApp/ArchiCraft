//! An open project: the model, its undo history, where it is saved, and the
//! evaluated geometry and plans derived from it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use archicraft_geometry::ModelGeometry;
use archicraft_geometry::plan::{PlanDrawing, ViewRange, plan_drawing};
use archicraft_ifcx::{Problem, WriteOptions};
use archicraft_model::{Command, CommandError, History, Project, StoreyId};

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

    pub fn undo(&mut self) -> Option<String> {
        self.history.undo(&mut self.project)
    }

    pub fn redo(&mut self) -> Option<String> {
        self.history.redo(&mut self.project)
    }

    /// The evaluated geometry for the current model, re-evaluated after
    /// changes.
    pub fn geometry(&mut self) -> Arc<ModelGeometry> {
        if self.geometry.revision != self.project.revision {
            self.geometry = Arc::new(ModelGeometry::evaluate(&self.project));
            self.plans.clear();
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

    /// The name shown in the window title: the file name, or the project
    /// name for an unsaved project.
    pub fn display_name(&self) -> String {
        self.path
            .as_deref()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.project.info.name.clone())
    }

    /// Serialises the project for saving as the next data version.
    pub fn to_bytes(&mut self, author: &str) -> (Vec<u8>, Vec<Problem>) {
        let geometry = self.geometry();
        let options = WriteOptions {
            author: author.to_owned(),
            timestamp: utc_timestamp(SystemTime::now()),
            data_version: self.project.data_version + 1,
        };
        archicraft_ifcx::save(&self.project, &geometry, &options, false)
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

    #[test]
    fn recovered_documents_are_unsaved() {
        let doc = Document::recovered(default_metric_project(), None, 100);
        assert!(doc.is_dirty());
    }
}
