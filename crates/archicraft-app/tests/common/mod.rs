//! The test harness shared by the UI test crates: the application without a
//! GPU, its widgets found by their accessible labels (AccessKit).

// Each test crate that includes this module uses only some of the helpers.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use archicraft_app::ArchiCraftApp;
use archicraft_app::persistence::DataDirs;
use archicraft_app::persistence::autosave::{Clock, SystemClock};
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
    harness_with_clock(dir, prefs, pixels_per_point, Arc::new(SystemClock::new()))
}

/// The application of [`harness_in`] with its autosave interval measured by
/// `clock`, for example a [`ManualClock`](archicraft_app::persistence::autosave::ManualClock)
/// (PRJ-004, AS-05).
pub fn harness_with_clock(
    dir: &Path,
    prefs: Preferences,
    pixels_per_point: f32,
    clock: Arc<dyn Clock>,
) -> Harness<'static, ArchiCraftApp> {
    let app = ArchiCraftApp::with_clock(DataDirs::at(dir), prefs, Arc::new(RecentLog::default()), None, clock);
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

/// Opens the overflow popup of the properties strip, if the strip hides
/// units and the popup is closed, so that every unit of the strip is shown
/// (UI-002): in the row or in the popup.
pub fn open_strip_overflow(harness: &mut Harness<'static, ArchiCraftApp>) {
    toggle_strip_overflow(harness, true);
}

/// Closes the overflow popup of the properties strip with its chevron, if
/// it is open.
pub fn close_strip_overflow(harness: &mut Harness<'static, ArchiCraftApp>) {
    toggle_strip_overflow(harness, false);
}

/// Clicks the chevron of the properties strip unless its overflow popup is
/// already `open` or closed as asked.
fn toggle_strip_overflow(harness: &mut Harness<'static, ArchiCraftApp>, open: bool) {
    use archicraft_app::tr;
    use archicraft_app::ui::{properties, strip};
    use egui::accesskit::Role;
    use egui_kittest::kittest::By;

    let id = strip::overflow_id(properties::strip_id());
    if harness.ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false) == open {
        return;
    }
    let label = tr!("dlg-properties-more");
    let Some(button) = harness.query(By::new().role(Role::Button).label(&label)) else {
        return;
    };
    button.click();
    harness.run_steps(3);
}

/// The rectangle of the properties strip's overflow popup, while it is
/// open.
pub fn strip_overflow_rect(harness: &Harness<'static, ArchiCraftApp>) -> Option<egui::Rect> {
    use archicraft_app::ui::{properties, strip};

    let id = strip::overflow_id(properties::strip_id());
    let open = harness.ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    harness.ctx.memory(|m| m.area_rect(id)).filter(|_| open)
}

/// Whether the centre of `rect` lies in the properties strip: in its row
/// or in its open overflow popup (UI-002).
pub fn in_strip(harness: &Harness<'static, ArchiCraftApp>, rect: egui::Rect) -> bool {
    let row = egui::containers::panel::PanelState::load(&harness.ctx, egui::Id::new("properties"))
        .map_or(egui::Rect::NOTHING, |p| p.outer_rect);
    let overflow = strip_overflow_rect(harness).unwrap_or(egui::Rect::NOTHING);
    row.contains(rect.center()) || overflow.contains(rect.center())
}
