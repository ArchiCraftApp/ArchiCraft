//! User preferences, stored outside projects (SET-001, PRJ-007): opening a
//! project never changes them.

use std::path::{Path, PathBuf};

use archicraft_model::units::NumberFormat;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

/// Decimal separator (STR-010): from the operating-system locale unless
/// overridden.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SeparatorChoice {
    #[default]
    Automatic,
    Point,
    Comma,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// Written to the IFCX header (IFX-002).
    pub author: String,
    /// Autosave interval in minutes (PRJ-004, default 5).
    pub autosave_minutes: u32,
    /// Backup versions kept on save (NFR-REL-003, default 1).
    pub backups: usize,
    /// Undo steps (UND-001, at least 100).
    pub undo_steps: usize,
    pub theme: ThemeChoice,
    pub decimal_separator: SeparatorChoice,
    pub digit_grouping: bool,
    /// Snap radius in screen points (Appendix D.1: 8 px).
    pub snap_radius: f32,
    /// Angle increment of the constraint modifier, in degrees (INP-006).
    pub angle_increment_deg: f64,
    /// Step of the invisible snap grid in metres (INP-013).
    pub snap_step: f64,
    /// Show the reference lines of all walls in plan views (WAL-007).
    pub show_reference_lines: bool,
    /// Save projects as indented, human-readable IFCX instead of compact JSON
    /// (IFX-008, default off). Autosaves stay compact either way.
    pub indented_ifcx: bool,
    /// Recently opened projects, newest first (SW-001).
    pub recent: Vec<PathBuf>,
    /// File name of the newest crash report the user has been told about.
    pub last_crash_report: Option<String>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            author: "ArchiCraft user".into(),
            autosave_minutes: 5,
            backups: 1,
            undo_steps: archicraft_model::command::DEFAULT_UNDO_LIMIT,
            theme: ThemeChoice::System,
            decimal_separator: SeparatorChoice::Automatic,
            digit_grouping: false,
            snap_radius: 8.0,
            angle_increment_deg: 45.0,
            snap_step: 0.05,
            show_reference_lines: false,
            indented_ifcx: false,
            recent: Vec::new(),
            last_crash_report: None,
        }
    }
}

const MAX_RECENT: usize = 10;

impl Preferences {
    /// Loads preferences; a missing or unreadable file gives the defaults.
    pub fn load(path: &Path) -> Self {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice::<Preferences>(&bytes)
                .map(Preferences::sanitized)
                .unwrap_or_else(|e| {
                    tracing::warn!("preferences ignored: {e}");
                    Preferences::default()
                }),
            Err(_) => Preferences::default(),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let bytes = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        super::save_atomically(path, &bytes, 0)
    }

    /// Clamps values into their allowed ranges.
    fn sanitized(mut self) -> Self {
        self.autosave_minutes = self.autosave_minutes.clamp(1, 120);
        self.backups = self.backups.min(20);
        self.undo_steps = self.undo_steps.clamp(100, 10_000);
        self.snap_radius = self.snap_radius.clamp(2.0, 40.0);
        if !(1.0..=90.0).contains(&self.angle_increment_deg) {
            self.angle_increment_deg = 45.0;
        }
        if !(self.snap_step > 0.0 && self.snap_step <= 10.0) {
            self.snap_step = 0.05;
        }
        self.recent.truncate(MAX_RECENT);
        self
    }

    /// Moves `path` to the top of the recent projects.
    pub fn add_recent(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_owned());
        self.recent.truncate(MAX_RECENT);
    }

    pub fn number_format(&self) -> NumberFormat {
        let comma = match self.decimal_separator {
            SeparatorChoice::Point => false,
            SeparatorChoice::Comma => true,
            SeparatorChoice::Automatic => locale_uses_decimal_comma(),
        };
        let decimal_separator = if comma { ',' } else { '.' };
        let grouping = self.digit_grouping.then_some(if comma { '.' } else { ',' });
        NumberFormat {
            decimal_separator,
            grouping,
        }
    }
}

/// Whether the operating-system locale writes decimals with a comma, from
/// the POSIX locale variables. Other platforms default to a point; the
/// preference overrides it (STR-010).
fn locale_uses_decimal_comma() -> bool {
    let locale = ["LC_ALL", "LC_NUMERIC", "LANG"]
        .iter()
        .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))
        .unwrap_or_default();
    let language = locale.split(['_', '.', '-']).next().unwrap_or_default();
    matches!(
        language,
        "de" | "fr"
            | "es"
            | "it"
            | "nl"
            | "pt"
            | "ru"
            | "pl"
            | "cs"
            | "sk"
            | "sv"
            | "da"
            | "fi"
            | "nb"
            | "nn"
            | "tr"
            | "hu"
            | "ro"
            | "sl"
            | "hr"
            | "el"
            | "uk"
            | "id"
            | "vi"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_recent_list() {
        let dir = std::env::temp_dir().join(format!("archicraft-prefs-{}", std::process::id()));
        let path = dir.join("preferences.json");
        let mut prefs = Preferences::default();
        for i in 0..15 {
            prefs.add_recent(Path::new(&format!("/p/{i}.ifcx")));
        }
        prefs.add_recent(Path::new("/p/3.ifcx"));
        assert_eq!(prefs.recent.len(), MAX_RECENT);
        assert_eq!(prefs.recent[0], Path::new("/p/3.ifcx"));
        prefs.save(&path).unwrap();
        assert_eq!(Preferences::load(&path), prefs);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn invalid_values_are_clamped() {
        let prefs: Preferences = serde_json::from_str(r#"{"autosave_minutes": 0, "undo_steps": 3}"#).unwrap();
        let prefs = prefs.sanitized();
        assert_eq!(prefs.autosave_minutes, 1);
        assert_eq!(prefs.undo_steps, 100);
        assert_eq!(prefs.author, "ArchiCraft user");
    }

    /// IFX-008: compact IFCX is the default, also for preference files
    /// written before the option existed; the choice survives a restart.
    #[test]
    fn indented_ifcx_defaults_to_compact_and_persists() {
        assert!(!Preferences::default().indented_ifcx);
        let dir = std::env::temp_dir().join(format!("archicraft-prefs-indented-{}", std::process::id()));
        let path = dir.join("preferences.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, r#"{"author": "Ada", "digit_grouping": true}"#).unwrap();
        let old = Preferences::load(&path);
        assert_eq!(old.author, "Ada", "the file was read, not replaced by the defaults");
        assert!(!old.indented_ifcx);
        let prefs = Preferences {
            indented_ifcx: true,
            ..Preferences::default()
        };
        prefs.save(&path).unwrap();
        assert!(Preferences::load(&path).indented_ifcx);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
