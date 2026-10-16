//! Files on disk: atomic project saves with backups, autosave and recovery,
//! crash reports and user preferences (PRJ-004, PRJ-005, PRJ-007,
//! NFR-REL-001…003, NFR-SEC-002, SET-001).

pub mod autosave;
pub mod crash;
pub mod prefs;

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Where ArchiCraft keeps data outside projects.
#[derive(Clone, Debug)]
pub struct DataDirs {
    root: PathBuf,
}

impl DataDirs {
    /// The platform's application data folder, or `ARCHICRAFT_DATA_DIR` if set
    /// (for tests and portable use).
    pub fn locate() -> Self {
        let root = std::env::var_os("ARCHICRAFT_DATA_DIR")
            .map(PathBuf::from)
            .or_else(|| eframe::storage_dir("ArchiCraft"))
            .unwrap_or_else(|| std::env::temp_dir().join("ArchiCraft"));
        Self { root }
    }

    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn recovery(&self) -> PathBuf {
        self.root.join("recovery")
    }

    pub fn crash_reports(&self) -> PathBuf {
        self.root.join("crash-reports")
    }

    pub fn preferences_file(&self) -> PathBuf {
        self.root.join("preferences.json")
    }
}

/// Path of the `n`th backup of `path` (1 = newest): `house.ifcx.bak`,
/// `house.ifcx.bak2`, … The extension keeps backups out of the default
/// listing of the open dialog (PRJ-005).
pub fn backup_path(path: &Path, n: usize) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    if n <= 1 {
        name.push(".bak");
    } else {
        name.push(format!(".bak{n}"));
    }
    path.with_file_name(name)
}

/// Writes `bytes` to `path` atomically (NFR-REL-001): the data goes to a
/// temporary file in the same folder, is flushed to disk, and then replaces
/// the target. The previous version is kept as backup number 1, older backups
/// move up, and at most `backups` are kept (NFR-REL-003). A crash or a full
/// disk can therefore never leave a half-written project.
pub fn save_atomically(path: &Path, bytes: &[u8], backups: usize) -> io::Result<()> {
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the path has no file name"))?;
    let mut temp_name = std::ffi::OsString::from(".");
    temp_name.push(file_name);
    temp_name.push(format!(".{}.tmp", std::process::id()));
    let temp = dir.join(temp_name);

    let result = (|| {
        let mut file = File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if path.exists() && backups > 0 {
            for n in (1..backups).rev() {
                let older = backup_path(path, n);
                if older.exists() {
                    fs::rename(&older, backup_path(path, n + 1))?;
                }
            }
            fs::copy(path, backup_path(path, 1))?;
        }
        fs::rename(&temp, path)?;
        // Make the rename itself durable where the platform allows it.
        if let Ok(d) = File::open(dir) {
            let _ = d.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// An ISO 8601 UTC timestamp, for example `2026-10-16T14:05:00Z`.
pub fn utc_timestamp(time: SystemTime) -> String {
    let secs = time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem / 60) % 60,
        rem % 60
    )
}

/// Days since 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's
/// algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn timestamps() {
        assert_eq!(utc_timestamp(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        let t = UNIX_EPOCH + Duration::from_secs(1_792_159_500);
        assert_eq!(utc_timestamp(t), "2026-10-16T14:05:00Z");
        let leap = UNIX_EPOCH + Duration::from_secs(951_782_400);
        assert_eq!(utc_timestamp(leap), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn atomic_save_keeps_backups() {
        let dir = std::env::temp_dir().join(format!("archicraft-save-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("house.ifcx");
        for version in 1..=4 {
            save_atomically(&path, format!("v{version}").as_bytes(), 2).unwrap();
        }
        assert_eq!(fs::read_to_string(&path).unwrap(), "v4");
        assert_eq!(fs::read_to_string(backup_path(&path, 1)).unwrap(), "v3");
        assert_eq!(fs::read_to_string(backup_path(&path, 2)).unwrap(), "v2");
        assert!(!backup_path(&path, 3).exists());
        let names: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().all(|n| !n.ends_with(".tmp")), "{names:?}");
        assert_eq!(names.iter().filter(|n| n.ends_with(".ifcx")).count(), 1);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn failed_save_leaves_the_original() {
        let dir = std::env::temp_dir().join(format!("archicraft-fail-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("house.ifcx");
        save_atomically(&path, b"original", 1).unwrap();
        // A directory where the temporary file should go makes the save fail.
        let blocker = dir.join(format!(".house.ifcx.{}.tmp", std::process::id()));
        fs::create_dir_all(&blocker).unwrap();
        assert!(save_atomically(&path, b"new", 1).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "original");
        fs::remove_dir_all(&dir).unwrap();
    }
}
