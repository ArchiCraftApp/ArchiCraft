//! The Appendix D.1 preference defaults (PRJ-004, NFR-REL-002, NFR-REL-003,
//! UND-001), headless: no egui context and no ArchiCraftApp. The other
//! Appendix D values are pinned by crates/archicraft-model/tests/defaults.rs,
//! whose table names this file and each reference below.

use std::path::PathBuf;

use archicraft_app::persistence::prefs::Preferences;

/// Writes `json` to a preferences file of its own and loads it, which runs
/// the private `Preferences::sanitized`.
fn load(name: &str, json: &str) -> Preferences {
    let dir: PathBuf = std::env::temp_dir().join(format!("archicraft-defaults-{name}-{}", std::process::id()));
    let path = dir.join("preferences.json");
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("cannot create {}: {e}", dir.display()));
    std::fs::write(&path, json).unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
    let prefs = Preferences::load(&path);
    std::fs::remove_dir_all(&dir).unwrap_or_else(|e| panic!("cannot remove {}: {e}", dir.display()));
    prefs
}

/// Appendix D.1 (PRJ-004, NFR-REL-002): autosave every 5 minutes.
#[test]
fn autosave_defaults_to_5_minutes() {
    assert_eq!(
        Preferences::default().autosave_minutes,
        5,
        "D.1 Autosave; backups; undo: autosave every 5 minutes (PRJ-004, NFR-REL-002)"
    );
}

/// Appendix D.1 (NFR-REL-003): one backup version.
#[test]
fn one_backup_version_by_default() {
    assert_eq!(
        Preferences::default().backups,
        1,
        "D.1 Autosave; backups; undo: 1 backup version (NFR-REL-003)"
    );
}

/// Appendix D.1 (UND-001): at least 100 undo steps by default, and a
/// preferences file cannot lower the limit below 100.
#[test]
fn undo_keeps_at_least_100_steps_and_is_clamped() {
    const REFERENCE: &str = "D.1 Autosave; backups; undo: at least 100 undo steps in the preferences (UND-001)";
    let default = Preferences::default().undo_steps;
    assert!(default >= 100, "{REFERENCE}: the default is {default}");
    assert_eq!(
        load("undo", r#"{"undo_steps": 5}"#).undo_steps,
        100,
        "{REFERENCE}: 5 is clamped to 100"
    );
}

/// Appendix D.1: snap radius 8 px.
#[test]
fn snap_radius_defaults_to_8_px() {
    assert_eq!(Preferences::default().snap_radius, 8.0, "D.1 Snapping: radius 8 px");
}

/// Appendix D.1 (INP-006): angle increment 45 degrees, 15 degrees selectable.
#[test]
fn angle_increment_defaults_to_45_degrees_and_15_is_selectable() {
    assert_eq!(
        Preferences::default().angle_increment_deg,
        45.0,
        "D.1 Snapping: angle increment 45°"
    );
    assert_eq!(
        load("angle", r#"{"angle_increment_deg": 15.0}"#).angle_increment_deg,
        15.0,
        "D.1 Snapping: 15° selectable"
    );
}
