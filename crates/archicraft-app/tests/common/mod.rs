//! The test harness shared by the UI test crates: the application without a
//! GPU, its widgets found by their accessible labels (AccessKit).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use archicraft_app::ArchiCraftApp;
use archicraft_app::persistence::DataDirs;
use archicraft_app::persistence::crash::RecentLog;
use archicraft_app::persistence::prefs::Preferences;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

/// The application at 100 % scaling. Its data folder is keyed by `name` and
/// the process id, so tests running in parallel do not share recovery
/// folders.
pub fn harness(name: &str) -> Harness<'static, ArchiCraftApp> {
    harness_at(name, 1.0)
}

/// The application at `pixels_per_point` (NFR-PORT-005: 1.0 to 2.0 is 100 to
/// 200 % scaling).
pub fn harness_at(name: &str, pixels_per_point: f32) -> Harness<'static, ArchiCraftApp> {
    harness_in(&data_dir(name), Preferences::default(), pixels_per_point)
}

/// An empty data folder for the test `name`, keyed by the process id, so
/// tests running in parallel do not share recovery folders.
pub fn data_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("archicraft-ui-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// The application with its data in `dir`, as left by the test, and with
/// `prefs`.
pub fn harness_in(dir: &Path, prefs: Preferences, pixels_per_point: f32) -> Harness<'static, ArchiCraftApp> {
    let app = ArchiCraftApp::new(DataDirs::at(dir), prefs, Arc::new(RecentLog::default()), None);
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1400.0, 900.0))
        .with_pixels_per_point(pixels_per_point)
        .build_eframe(move |_| app);
    harness.run_steps(3);
    harness
}

pub fn assert_present(harness: &Harness<'static, ArchiCraftApp>, label: &str) {
    assert!(
        harness.query_all_by_label(label).next().is_some(),
        "“{label}” is missing"
    );
}
