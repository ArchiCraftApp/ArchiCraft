//! ArchiCraft: a desktop modeller for buildings that saves IFCX (IFC5).

// Hide the console window on Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::path::PathBuf;

use archicraft_app::ArchiCraftApp;
use archicraft_app::persistence::DataDirs;
use archicraft_app::persistence::crash;
use archicraft_app::persistence::prefs::Preferences;

fn main() -> eframe::Result {
    let log = archicraft_app::logging::init();
    let dirs = DataDirs::locate();
    crash::install(dirs.crash_reports(), log.clone());
    let prefs = Preferences::load(&dirs.preferences_file());
    let open = std::env::args_os().nth(1).map(PathBuf::from);
    tracing::info!(data = %dirs.root().display(), "starting ArchiCraft {}", env!("CARGO_PKG_VERSION"));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ArchiCraft")
            .with_app_id("archicraft")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([800.0, 500.0]),
        renderer: eframe::Renderer::Wgpu,
        ..eframe::NativeOptions::default()
    };
    eframe::run_native(
        "ArchiCraft",
        options,
        Box::new(move |_cc| Ok(Box::new(ArchiCraftApp::new(dirs, prefs, log, open)))),
    )
}
