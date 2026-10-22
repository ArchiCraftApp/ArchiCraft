//! User preferences, stored outside projects (SET-001, PRJ-007): opening a
//! project never changes them.

use std::path::{Path, PathBuf};

use archicraft_model::units::NumberFormat;
use serde::{Deserialize, Deserializer, Serialize};

use crate::i18n::os_locale::{self, OsNumberLocale};
use crate::input::InputMode;
use crate::input::InputSettings;

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
    /// Click-click or press-drag-release point input (INP-020). An unknown
    /// value reads as click-click and keeps the rest of the file.
    #[serde(deserialize_with = "lenient_input_mode")]
    pub input_mode: InputMode,
    /// Step of the invisible snap grid in metres (INP-013).
    pub snap_step: f64,
    /// The division mode and scope of hovered edges (INP-010). An unknown
    /// value reads as the defaults and keeps the rest of the file.
    #[serde(deserialize_with = "lenient_input_settings")]
    pub input: InputSettings,
    /// Tracker behaviour (SET-001): show the absolute X, Y and Z beside
    /// the distance and angle (INP-002, default off).
    pub tracker_xyz: bool,
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
            input_mode: InputMode::ClickClick,
            snap_step: 0.05,
            input: InputSettings::default(),
            tracker_xyz: false,
            show_reference_lines: false,
            indented_ifcx: false,
            recent: Vec::new(),
            last_crash_report: None,
        }
    }
}

const MAX_RECENT: usize = 10;

/// Reads an input mode, taking click-click for a value this version does not
/// know (a newer version's mode, a typo), so the other preferences of the
/// file stay (INP-020).
fn lenient_input_mode<'de, D: Deserializer<'de>>(deserializer: D) -> Result<InputMode, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Lenient {
        Known(InputMode),
        Unknown(serde::de::IgnoredAny),
    }
    Ok(match Lenient::deserialize(deserializer)? {
        Lenient::Known(mode) => mode,
        Lenient::Unknown(_) => InputMode::ClickClick,
    })
}

/// Reads input settings, taking the defaults for a value this version does
/// not know (a newer version's mode, a typo), so the other preferences of
/// the file stay (INP-010).
fn lenient_input_settings<'de, D: Deserializer<'de>>(deserializer: D) -> Result<InputSettings, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Lenient {
        Known(InputSettings),
        Unknown(serde::de::IgnoredAny),
    }
    Ok(match Lenient::deserialize(deserializer)? {
        Lenient::Known(settings) => settings,
        Lenient::Unknown(_) => InputSettings::default(),
    })
}

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
        self.input = self.input.sanitized();
        self.recent.truncate(MAX_RECENT);
        self
    }

    /// Moves `path` to the top of the recent projects.
    pub fn add_recent(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_owned());
        self.recent.truncate(MAX_RECENT);
    }

    /// The number format of these preferences in the operating system's
    /// number locale ([`os_locale::detected`]). Cheap enough to call every
    /// frame, so a changed preference applies without a restart (STR-010).
    pub fn number_format(&self) -> NumberFormat {
        self.number_format_in(os_locale::detected())
    }

    /// The number format of these preferences in `locale` (STR-010,
    /// NFR-L10N-004): "Automatic" takes the locale's decimal separator,
    /// "Point" and "Comma" override it, and digit grouping uses the locale's
    /// grouping character. A grouping character equal to the chosen separator
    /// becomes a no-break space, and so does a narrow no-break space, which
    /// egui's default fonts lack (input accepts both).
    pub fn number_format_in(&self, locale: &OsNumberLocale) -> NumberFormat {
        let decimal_separator = match self.decimal_separator {
            SeparatorChoice::Point => '.',
            SeparatorChoice::Comma => ',',
            SeparatorChoice::Automatic => locale.decimal,
        };
        let grouping = self.digit_grouping.then_some(match locale.grouping {
            g if g == decimal_separator || g == '\u{202F}' => '\u{A0}',
            g => g,
        });
        NumberFormat {
            decimal_separator,
            grouping,
        }
    }
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

    /// INP-020: the input mode survives a restart; preference files without
    /// it read as click-click.
    #[test]
    fn input_mode_round_trips() {
        assert_eq!(Preferences::default().input_mode, InputMode::ClickClick);
        let dir = std::env::temp_dir().join(format!("archicraft-prefs-input-mode-{}", std::process::id()));
        let path = dir.join("preferences.json");
        let prefs = Preferences {
            input_mode: InputMode::DragRelease,
            ..Preferences::default()
        };
        prefs.save(&path).unwrap();
        assert_eq!(Preferences::load(&path), prefs);
        std::fs::write(&path, r#"{"author": "Ada"}"#).unwrap();
        assert_eq!(Preferences::load(&path).input_mode, InputMode::ClickClick);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// SET-001, INP-002: the tracker shows X, Y and Z only on demand; the
    /// choice survives a restart and older files read as off.
    #[test]
    fn tracker_xyz_defaults_to_off_and_persists() {
        assert!(!Preferences::default().tracker_xyz);
        let dir = std::env::temp_dir().join(format!("archicraft-prefs-tracker-{}", std::process::id()));
        let path = dir.join("preferences.json");
        let prefs = Preferences {
            tracker_xyz: true,
            ..Preferences::default()
        };
        prefs.save(&path).unwrap();
        assert_eq!(Preferences::load(&path), prefs);
        std::fs::write(&path, r#"{"author": "Ada"}"#).unwrap();
        assert!(!Preferences::load(&path).tracker_xyz);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// INP-020: an input mode this version does not know falls back to
    /// click-click; the other preferences of the same file are kept.
    #[test]
    fn an_unknown_input_mode_falls_back_to_click_click() {
        let dir = std::env::temp_dir().join(format!("archicraft-prefs-unknown-mode-{}", std::process::id()));
        let path = dir.join("preferences.json");
        std::fs::create_dir_all(&dir).unwrap();
        for unknown in [r#""Telepathy""#, "3", r#"{"Gesture": 1}"#, "null"] {
            let file =
                format!(r#"{{"author": "Ada", "input_mode": {unknown}, "snap_radius": 12.0, "indented_ifcx": true}}"#);
            std::fs::write(&path, file).unwrap();
            let prefs = Preferences::load(&path);
            assert_eq!(prefs.input_mode, InputMode::ClickClick, "{unknown}");
            assert_eq!(
                prefs.author, "Ada",
                "{unknown}: the file was read, not replaced by the defaults"
            );
            assert_eq!(prefs.snap_radius, 12.0);
            assert!(prefs.indented_ifcx);
        }
        std::fs::write(&path, r#"{"author": "Ada", "input_mode": "DragRelease"}"#).unwrap();
        assert_eq!(Preferences::load(&path).input_mode, InputMode::DragRelease);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn with(separator: SeparatorChoice, grouping: bool) -> Preferences {
        Preferences {
            decimal_separator: separator,
            digit_grouping: grouping,
            ..Preferences::default()
        }
    }

    /// STR-010: "Automatic" takes the separator and the grouping character of
    /// the detected locale.
    #[test]
    fn automatic_follows_the_detected_locale() {
        let swiss = OsNumberLocale::from_tag("de-CH");
        let nf = with(SeparatorChoice::Automatic, true).number_format_in(&swiss);
        assert_eq!((nf.decimal_separator, nf.grouping), ('.', Some('’')));
        let austrian = OsNumberLocale::from_tag("de-AT");
        let nf = with(SeparatorChoice::Automatic, true).number_format_in(&austrian);
        assert_eq!((nf.decimal_separator, nf.grouping), (',', Some('\u{A0}')));
        let prefs = with(SeparatorChoice::Automatic, true);
        assert_eq!(prefs.number_format(), prefs.number_format_in(os_locale::detected()));
    }

    /// STR-010: "Point" and "Comma" override the locale as soon as the
    /// preference changes, without a restart.
    #[test]
    fn point_and_comma_override_the_locale_without_restart() {
        let german = OsNumberLocale::from_tag("de-DE");
        let mut prefs = with(SeparatorChoice::Automatic, false);
        assert_eq!(prefs.number_format_in(&german).decimal_separator, ',');
        prefs.decimal_separator = SeparatorChoice::Point;
        assert_eq!(prefs.number_format_in(&german).decimal_separator, '.');
        assert_eq!(prefs.number_format().decimal_separator, '.');
        prefs.decimal_separator = SeparatorChoice::Comma;
        assert_eq!(
            prefs
                .number_format_in(&OsNumberLocale::from_tag("en-US"))
                .decimal_separator,
            ','
        );
        assert_eq!(prefs.number_format().decimal_separator, ',');
        assert_eq!(prefs.number_format_in(&german).grouping, None, "grouping is off");
    }

    /// NFR-L10N-004: a grouping character equal to the chosen separator, and
    /// the narrow no-break space egui cannot draw, become a no-break space.
    #[test]
    fn grouping_equal_to_the_separator_becomes_a_no_break_space() {
        let german = OsNumberLocale::from_tag("de-DE");
        let nf = with(SeparatorChoice::Point, true).number_format_in(&german);
        assert_eq!((nf.decimal_separator, nf.grouping), ('.', Some('\u{A0}')));
        let american = OsNumberLocale::from_tag("en-US");
        let nf = with(SeparatorChoice::Comma, true).number_format_in(&american);
        assert_eq!((nf.decimal_separator, nf.grouping), (',', Some('\u{A0}')));
        let nf = with(SeparatorChoice::Point, true).number_format_in(&american);
        assert_eq!((nf.decimal_separator, nf.grouping), ('.', Some(',')));
        let french = OsNumberLocale::from_tag("fr-FR");
        let nf = with(SeparatorChoice::Automatic, true).number_format_in(&french);
        assert_eq!((nf.decimal_separator, nf.grouping), (',', Some('\u{A0}')));
    }

    /// INP-010: the division mode, its value and the scope survive a
    /// restart; files without them read as half between nodes, an unknown
    /// mode reads as the defaults and keeps the other preferences, and
    /// out-of-range values are sanitised.
    #[test]
    fn division_settings_round_trip_and_are_sanitised() {
        use crate::input::{DivisionMode, DivisionScope};
        let defaults = InputSettings::default();
        assert_eq!(
            (defaults.division, defaults.scope),
            (DivisionMode::Half, DivisionScope::BetweenNodes)
        );
        let dir = std::env::temp_dir().join(format!("archicraft-prefs-division-{}", std::process::id()));
        let path = dir.join("preferences.json");
        for (division, scope) in [
            (DivisionMode::Equal(7), DivisionScope::BetweenIntersections),
            (DivisionMode::Percent(12.5), DivisionScope::WholeElement),
            (DivisionMode::Repeated(0.625), DivisionScope::BetweenNodes),
            (DivisionMode::BestEqualNear(1.1), DivisionScope::WholeElement),
        ] {
            let prefs = Preferences {
                input: InputSettings { division, scope },
                ..Preferences::default()
            };
            prefs.save(&path).unwrap();
            assert_eq!(Preferences::load(&path), prefs);
        }
        let read = |json: &str| {
            std::fs::write(&path, json).unwrap();
            Preferences::load(&path)
        };
        assert_eq!(read(r#"{"author": "Ada"}"#).input, defaults);
        let unknown = read(r#"{"author": "Ada", "input": {"division": {"Spiral": 3}, "scope": "WholeElement"}}"#);
        assert_eq!((unknown.author.as_str(), unknown.input), ("Ada", defaults));
        let out_of_range = [
            (r#"{"Equal": 1}"#, DivisionMode::Equal(2)),
            (r#"{"Equal": 200}"#, DivisionMode::Equal(100)),
            (r#"{"Percent": 140.0}"#, DivisionMode::Percent(100.0)),
            (r#"{"Repeated": -1.0}"#, DivisionMode::Repeated(1.0)),
            (r#"{"BestEqualNear": 0.0}"#, DivisionMode::BestEqualNear(1.0)),
        ];
        for (json, expected) in out_of_range {
            let prefs = read(&format!(
                r#"{{"input": {{"division": {json}, "scope": "BetweenIntersections"}}}}"#
            ));
            assert_eq!(prefs.input.division, expected, "{json}");
            assert_eq!(prefs.input.scope, DivisionScope::BetweenIntersections, "{json}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
