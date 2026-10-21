//! SRS AS-05, crash recovery, end to end (PRJ-004, PRJ-005, NFR-REL-001,
//! NFR-REL-002): the application is force-terminated six minutes after the
//! last save, while editing; on restart it offers recovery, the recovered
//! model contains everything up to the last autosave and the project file is
//! intact.
//!
//! `as05_crash_recovery` re-runs this test binary as a child process that
//! runs only `as05_child`. The child builds the real application in a kittest
//! harness (no window or GPU, which the CI runners lack) with a
//! `ManualClock`, saves, edits and lets the autosave run, then keeps stepping
//! frames until the parent kills it: SIGKILL or TerminateProcess, so no
//! destructor runs and the session folder stays behind as after a crash. The
//! parent then starts a fresh application on the same data folder. The manual
//! procedure with the windowed binary is docs/test-plan/as-05.md.

mod common;

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use archicraft_app::ArchiCraftApp;
use archicraft_app::persistence::autosave::ManualClock;
use archicraft_app::persistence::prefs::Preferences;
use archicraft_app::persistence::{DataDirs, backup_path};
use archicraft_model::glam::DVec2;
use common::{assert_present, harness_in, harness_with_clock};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

type App = Harness<'static, ArchiCraftApp>;

/// Set to `child` in the child process; `as05_child` refuses to run without
/// it.
const ROLE_ENV: &str = "ARCHICRAFT_AS05_ROLE";
/// The test's temporary folder, shared by parent and child.
const ROOT_ENV: &str = "ARCHICRAFT_AS05_ROOT";
/// The data folder the application uses (`DataDirs::locate`).
const DATA_ENV: &str = "ARCHICRAFT_DATA_DIR";

/// Written by the child once the scenario is in place; holds the path of its
/// session folder.
const READY_FILE: &str = "ready";
/// A copy of the project file as the child saved it.
const SAVED_COPY: &str = "saved.ifcx";
/// The child's standard output and error.
const CHILD_LOG: &str = "child.log";

/// How long the parent waits for the child to get ready.
const READY_TIMEOUT: Duration = Duration::from_secs(40);
/// How long the child keeps running before it gives up on being killed, so
/// it never outlives a parent that failed.
const CHILD_LIFETIME: Duration = Duration::from_secs(120);
/// How long the child waits for the autosave thread.
const AUTOSAVE_TIMEOUT: Duration = Duration::from_secs(10);

/// The wall saved in house.ifcx, the wall added at minute 1 (autosaved at
/// minute 5) and the wall added at minute 5.5 (never autosaved), by their
/// start points.
const SAVED_WALL: DVec2 = DVec2::new(0.0, 0.0);
const MINUTE_1_WALL: DVec2 = DVec2::new(0.0, 3.0);
const MINUTE_5_5_WALL: DVec2 = DVec2::new(0.0, 6.0);

fn project_file(root: &Path) -> PathBuf {
    root.join("project").join("house.ifcx")
}

fn add_wall(h: &mut App, start: DVec2) {
    let ws = h.state_mut().workspace();
    let storey = ws.doc.project.storeys.ground();
    let wall = ws.doc.project.new_wall(storey, start, start + DVec2::new(4.0, 0.0), 0);
    assert!(ws.execute(archicraft_model::Command::AddElements(vec![wall])));
}

/// The start points of the walls of the open project.
fn wall_starts(h: &mut App) -> Vec<DVec2> {
    let ws = h.state_mut().workspace();
    ws.doc
        .project
        .elements
        .values()
        .filter_map(|e| e.as_wall())
        .map(|w| w.start)
        .collect()
}

fn has_wall_at(starts: &[DVec2], start: DVec2) -> bool {
    starts.iter().any(|s| s.distance(start) < 1e-9)
}

/// The files below `dir`, recursively.
fn files_below(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot list {}: {e}", dir.display())) {
        let path = entry.expect("a folder entry").path();
        if path.is_dir() {
            out.extend(files_below(&path));
        } else {
            out.push(path);
        }
    }
    out
}

/// The child process of `as05_crash_recovery`: saves house.ifcx at minute 0,
/// edits at minute 1, is autosaved at minute 5, edits again at minute 5.5 and
/// keeps stepping frames at minute 6, with the minute-5.5 edit unsaved, until
/// it is killed.
#[test]
#[ignore = "the child process of as05_crash_recovery, which runs it"]
fn as05_child() {
    assert_eq!(
        std::env::var(ROLE_ENV).as_deref(),
        Ok("child"),
        "as05_child runs only as the child process of as05_crash_recovery"
    );
    let root = PathBuf::from(std::env::var_os(ROOT_ENV).expect("the test folder"));
    let dirs = DataDirs::locate();
    let clock = ManualClock::new();
    let prefs = Preferences::load(&dirs.preferences_file());
    let mut h = harness_with_clock(dirs.root(), prefs, 1.0, Arc::new(clock.clone()));

    // Minute 0: the project is saved twice through the application's save
    // path, so that house.ifcx has a backup (PRJ-005).
    let project = project_file(&root);
    fs::create_dir_all(project.parent().expect("a project folder")).expect("the project folder");
    assert!(h.state_mut().save_to(&project), "the first save");
    add_wall(&mut h, SAVED_WALL);
    assert!(h.state_mut().save_to(&project), "the second save");
    h.run_steps(2);
    fs::copy(&project, root.join(SAVED_COPY)).expect("a copy of the saved file");

    let advance = |h: &mut App, by: Duration| {
        clock.advance(by);
        h.run_steps(2);
        h.state()
            .autosaves_written(AUTOSAVE_TIMEOUT)
            .expect("the queued autosaves are written in time")
    };
    assert_eq!(advance(&mut h, Duration::from_secs(60)), 0, "autosaved by minute 1");
    add_wall(&mut h, MINUTE_1_WALL);
    assert_eq!(
        advance(&mut h, Duration::from_secs(4 * 60)),
        1,
        "no autosave at minute 5"
    );
    assert_eq!(
        advance(&mut h, Duration::from_secs(30)),
        1,
        "autosaved again by minute 5.5"
    );
    add_wall(&mut h, MINUTE_5_5_WALL);
    assert_eq!(
        advance(&mut h, Duration::from_secs(30)),
        1,
        "autosaved again by minute 6"
    );

    let sessions: Vec<PathBuf> = fs::read_dir(dirs.recovery())
        .expect("the recovery folder")
        .map(|e| e.expect("a folder entry").path())
        .collect();
    let [session] = sessions.as_slice() else {
        panic!("one session folder expected: {sessions:?}");
    };
    // Written under another name and renamed, so the parent never reads a
    // half-written marker.
    let ready = root.join(READY_FILE);
    let partial = ready.with_extension("partial");
    fs::write(&partial, session.to_string_lossy().as_bytes()).expect("the ready marker");
    fs::rename(&partial, &ready).expect("the ready marker");

    // Minute 6, with the minute-5.5 edit unsaved: frames keep running until
    // the parent kills the process.
    let deadline = Instant::now() + CHILD_LIFETIME;
    while Instant::now() < deadline {
        h.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("not killed within {CHILD_LIFETIME:?}");
}

/// How long the parent waits for the operating system to release the killed
/// child's session lock.
const LOCK_RELEASE_TIMEOUT: Duration = Duration::from_secs(10);

/// Waits until the lock on `lock_file` can be taken. Windows releases the
/// locks of a terminated process after a delay that depends on the system's
/// load; until then the restarted application would take the killed session
/// for a running one.
fn wait_for_lock_release(lock_file: &Path) {
    let started = Instant::now();
    loop {
        let file = File::options()
            .write(true)
            .open(lock_file)
            .unwrap_or_else(|e| panic!("cannot open {}: {e}", lock_file.display()));
        if file.try_lock().is_ok() {
            return;
        }
        assert!(
            started.elapsed() < LOCK_RELEASE_TIMEOUT,
            "the killed session is still locked after {LOCK_RELEASE_TIMEOUT:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Removes the test folder when the test ends, passed or failed.
struct TempFolder(PathBuf);

impl Drop for TempFolder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Kills the child process when the test ends early, so that no process is
/// left behind.
struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// AS-05 (PRJ-004, PRJ-005, NFR-REL-001, NFR-REL-002): kill the application
/// six minutes after the last save, while editing; on restart recovery is
/// offered, the recovered model holds everything up to the last autosave,
/// and the original file is intact.
#[test]
fn as05_crash_recovery() {
    let started = Instant::now();
    let root = std::env::temp_dir().join(format!("archicraft-as05-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("the test folder");
    let folder = TempFolder(root.clone());
    let data = root.join("data");
    let log_path = root.join(CHILD_LOG);
    let child_log = || fs::read_to_string(&log_path).unwrap_or_default();

    let log = File::create(&log_path).expect("the child log");
    let mut child = ChildGuard(
        Command::new(std::env::current_exe().expect("the test binary"))
            .args(["--exact", "as05_child", "--ignored", "--nocapture", "--test-threads=1"])
            .env(ROLE_ENV, "child")
            .env(ROOT_ENV, &root)
            .env(DATA_ENV, &data)
            .stdin(Stdio::null())
            .stdout(log.try_clone().expect("the child log"))
            .stderr(log)
            .spawn()
            .expect("the child process"),
    );
    let ready = root.join(READY_FILE);
    while !ready.exists() {
        if let Some(status) = child.0.try_wait().expect("the child's status") {
            panic!("the child ended before it was killed ({status}):\n{}", child_log());
        }
        assert!(
            started.elapsed() < READY_TIMEOUT,
            "the child was not ready within {READY_TIMEOUT:?}:\n{}",
            child_log()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let killed_session = PathBuf::from(fs::read_to_string(&ready).expect("the ready marker"));

    // Force-terminate (SIGKILL or TerminateProcess) and wait until the
    // process is gone, so that its lock on the session is released.
    child.0.kill().expect("the child is killed");
    let status = child.0.wait().expect("the child ends");
    assert!(!status.success(), "the child was killed, it did not finish");
    wait_for_lock_release(&killed_session.join("session.lock"));

    // PRJ-004: the autosave went to the recovery folder, never next to the
    // project; the project folder holds the file and its backup only.
    let recovery = DataDirs::at(&data).recovery();
    assert!(
        killed_session.join("autosave.ifcx").exists(),
        "no autosave in {}",
        killed_session.display()
    );
    for file in files_below(&root) {
        let name = file.file_name().unwrap_or_default().to_string_lossy();
        if name.starts_with("autosave") {
            assert!(
                file.starts_with(&recovery),
                "an autosave outside the recovery folder: {}",
                file.display()
            );
        }
    }
    let project = project_file(&root);
    let mut project_folder: Vec<PathBuf> = fs::read_dir(project.parent().expect("a project folder"))
        .expect("the project folder")
        .map(|e| e.expect("a folder entry").path())
        .collect();
    project_folder.sort();
    assert_eq!(project_folder, vec![project.clone(), backup_path(&project, 1)]);

    // The restart, as main.rs starts the application.
    let prefs = Preferences::load(&DataDirs::at(&data).preferences_file());
    let mut h = harness_in(&data, prefs, 1.0);
    assert_present(&h, "Recover unsaved work");
    h.get_by_label("Recover").click();
    h.run_steps(2);
    assert!(h.query_all_by_label("Recover unsaved work").next().is_none());

    // NFR-REL-002: everything up to the last autosave, nothing after it.
    let starts = wall_starts(&mut h);
    assert!(
        has_wall_at(&starts, SAVED_WALL),
        "the saved wall is missing: {starts:?}"
    );
    assert!(
        has_wall_at(&starts, MINUTE_1_WALL),
        "the minute-1 edit is missing: {starts:?}"
    );
    assert!(
        !has_wall_at(&starts, MINUTE_5_5_WALL),
        "the minute-5.5 edit was never autosaved: {starts:?}"
    );
    assert_eq!(starts.len(), 2, "{starts:?}");
    let ws = h.state_mut().workspace();
    assert!(ws.doc.is_dirty(), "the recovered project is unsaved");
    assert_eq!(ws.doc.path.as_deref(), Some(project.as_path()));

    // PRJ-004, NFR-REL-001: the project file was not touched.
    assert_eq!(
        fs::read(&project).expect("the project file"),
        fs::read(root.join(SAVED_COPY)).expect("the saved copy"),
        "house.ifcx changed after the save"
    );
    assert!(!killed_session.exists(), "the recovered session folder is left behind");

    drop(h);
    drop(child);
    drop(folder);
    assert!(!root.exists(), "the test folder is left behind");
    assert!(
        started.elapsed() < Duration::from_secs(60),
        "AS-05 took {:?}",
        started.elapsed()
    );
}
