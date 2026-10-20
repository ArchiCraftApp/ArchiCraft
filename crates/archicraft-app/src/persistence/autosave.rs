//! Autosave and crash recovery (PRJ-004, NFR-REL-002, NFR-PERF-009).
//!
//! Every running instance owns a session folder under the recovery folder and
//! holds an exclusive lock on a file in it. Autosaves go there, never to the
//! project file. A clean exit removes the folder. At the next start, a session
//! folder whose lock can be taken belonged to an instance that ended
//! abnormally, and its autosave is offered for recovery.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;
use std::time::SystemTime;

use archicraft_geometry::ModelGeometry;
use archicraft_model::Project;
use serde::{Deserialize, Serialize};

use super::{save_atomically, utc_timestamp};

const MODEL_FILE: &str = "autosave.ifcx";
const INFO_FILE: &str = "autosave.json";
const LOCK_FILE: &str = "session.lock";

/// What a recovery file is about.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecoveryInfo {
    pub project_name: String,
    /// The project file the autosave belongs to, if it had been saved.
    pub original_path: Option<PathBuf>,
    pub saved_at: String,
}

/// The session of this running instance.
#[derive(Debug)]
pub struct Session {
    dir: PathBuf,
    // Held for the lifetime of the session; the OS releases it on a crash.
    _lock: File,
}

impl Session {
    pub fn start(recovery_root: &Path) -> std::io::Result<Self> {
        let dir = recovery_root.join(format!(
            "session-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis())
        ));
        fs::create_dir_all(&dir)?;
        let lock = File::create(dir.join(LOCK_FILE))?;
        lock.try_lock().map_err(std::io::Error::other)?;
        Ok(Self { dir, _lock: lock })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Removes the autosave, for example after the project was saved.
    pub fn clear(&self) {
        let _ = fs::remove_file(self.dir.join(MODEL_FILE));
        let _ = fs::remove_file(self.dir.join(INFO_FILE));
    }

    /// Ends the session cleanly: nothing is left to recover.
    pub fn end(self) {
        let dir = self.dir.clone();
        drop(self);
        let _ = fs::remove_dir_all(dir);
    }
}

/// Writes an autosave into a session folder. Runs on the autosave thread.
/// Autosaves are recovery data and always compact, whatever the user chose
/// for saved files (IFX-008): smaller and faster to write (NFR-PERF-009).
pub fn write_autosave(
    dir: &Path,
    project: &Project,
    geometry: &ModelGeometry,
    info: &RecoveryInfo,
    author: &str,
) -> std::io::Result<()> {
    let options = archicraft_ifcx::WriteOptions {
        author: author.to_owned(),
        timestamp: info.saved_at.clone(),
        data_version: project.data_version,
    };
    let (bytes, _) = archicraft_ifcx::save(project, geometry, &options, false);
    save_atomically(&dir.join(MODEL_FILE), &bytes, 0)?;
    let info_bytes = serde_json::to_vec_pretty(info).map_err(std::io::Error::other)?;
    save_atomically(&dir.join(INFO_FILE), &info_bytes, 0)
}

/// An autosave left behind by an instance that did not end cleanly.
#[derive(Clone, Debug)]
pub struct Abandoned {
    pub dir: PathBuf,
    pub model: PathBuf,
    pub info: RecoveryInfo,
}

impl Abandoned {
    /// Deletes the abandoned session, after recovery or when declined.
    pub fn discard(&self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Finds autosaves of sessions that ended abnormally. Folders of sessions
/// that are still running are locked and skipped; abandoned folders without
/// an autosave are removed.
pub fn find_abandoned(recovery_root: &Path) -> Vec<Abandoned> {
    let Ok(entries) = fs::read_dir(recovery_root) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Ok(lock) = File::options().write(true).open(dir.join(LOCK_FILE)) else {
            continue;
        };
        if lock.try_lock().is_err() {
            continue; // another running instance
        }
        drop(lock);
        let model = dir.join(MODEL_FILE);
        let info = fs::read(dir.join(INFO_FILE))
            .ok()
            .and_then(|b| serde_json::from_slice::<RecoveryInfo>(&b).ok());
        match info {
            Some(info) if model.exists() => found.push(Abandoned { dir, model, info }),
            _ => {
                let _ = fs::remove_dir_all(&dir);
            }
        }
    }
    found.sort_by(|a, b| b.info.saved_at.cmp(&a.info.saved_at));
    found
}

struct Job {
    dir: PathBuf,
    project: Project,
    geometry: Arc<ModelGeometry>,
    info: RecoveryInfo,
    author: String,
}

/// Writes autosaves on a background thread, so editing is not blocked
/// (NFR-PERF-009).
#[derive(Debug)]
pub struct Autosaver {
    sender: Option<Sender<Job>>,
    thread: Option<JoinHandle<()>>,
}

impl Default for Autosaver {
    fn default() -> Self {
        Self::new()
    }
}

impl Autosaver {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel::<Job>();
        let thread = std::thread::Builder::new()
            .name("autosave".into())
            .spawn(move || {
                while let Ok(mut job) = receiver.recv() {
                    // Only the newest pending snapshot matters.
                    while let Ok(newer) = receiver.try_recv() {
                        job = newer;
                    }
                    match write_autosave(&job.dir, &job.project, &job.geometry, &job.info, &job.author) {
                        Ok(()) => tracing::info!("autosaved to {}", job.dir.display()),
                        Err(e) => tracing::error!("autosave failed: {e}"),
                    }
                }
            })
            .ok();
        Self {
            sender: thread.as_ref().map(|_| sender),
            thread,
        }
    }

    /// Queues an autosave of `project` into `session`.
    pub fn request(
        &self,
        session: &Session,
        project: &Project,
        geometry: Arc<ModelGeometry>,
        original_path: Option<PathBuf>,
        author: &str,
    ) {
        let Some(sender) = &self.sender else {
            return;
        };
        let job = Job {
            dir: session.dir().to_owned(),
            project: project.clone(),
            geometry,
            info: RecoveryInfo {
                project_name: project.info.name.clone(),
                original_path,
                saved_at: utc_timestamp(SystemTime::now()),
            },
            author: author.to_owned(),
        };
        let _ = sender.send(job);
    }

    /// Waits for pending autosaves to finish.
    pub fn shutdown(&mut self) {
        self.sender = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Autosaver {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abandoned_sessions_are_found_and_live_ones_skipped() {
        let root = std::env::temp_dir().join(format!("archicraft-recovery-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let project = archicraft_model::samples::storey_with_walls_and_slab();
        let geometry = Arc::new(ModelGeometry::evaluate(&project));

        // A session that "crashed": its lock file is no longer held.
        let crashed = Session::start(&root).unwrap();
        let info = RecoveryInfo {
            project_name: project.info.name.clone(),
            original_path: Some(PathBuf::from("/projects/house.ifcx")),
            saved_at: "2026-10-16T10:00:00Z".into(),
        };
        write_autosave(crashed.dir(), &project, &geometry, &info, "Test").unwrap();
        let crashed_dir = crashed.dir().to_owned();
        drop(crashed); // releases the lock without removing the folder

        // A running session with an autosave of its own.
        let running = Session::start(&root).unwrap();
        let mut saver = Autosaver::new();
        saver.request(&running, &project, geometry, None, "Test");
        saver.shutdown();
        assert!(running.dir().join(MODEL_FILE).exists());

        let found = find_abandoned(&root);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].dir, crashed_dir);
        assert_eq!(found[0].info, info);
        let recovered = archicraft_ifcx::load(&fs::read(&found[0].model).unwrap()).unwrap();
        assert_eq!(recovered.project.elements.len(), project.elements.len());

        found[0].discard();
        running.end();
        assert!(find_abandoned(&root).is_empty());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        fs::remove_dir_all(&root).unwrap();
    }

    /// IFX-008, NFR-PERF-009: autosaves are recovery data and stay compact
    /// whatever the user chose for saved files.
    #[test]
    fn autosaves_are_compact() {
        let root = std::env::temp_dir().join(format!("archicraft-autosave-compact-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let project = archicraft_model::samples::storey_with_walls_and_slab();
        let geometry = ModelGeometry::evaluate(&project);
        let info = RecoveryInfo {
            project_name: project.info.name.clone(),
            original_path: None,
            saved_at: "2026-10-20T10:00:00Z".into(),
        };
        write_autosave(&root, &project, &geometry, &info, "Test").unwrap();
        let bytes = fs::read(root.join(MODEL_FILE)).unwrap();
        assert!(bytes.starts_with(b"{\"header\":{\""));
        assert!(!bytes.contains(&b'\n'), "an autosave is one line of JSON");
        fs::remove_dir_all(&root).unwrap();
    }
}
