//! Local crash reports (NFR-SEC-002, D-12).
//!
//! A panic hook writes the panic message, a backtrace, the version, the
//! operating system and the most recent log lines to a file in the
//! crash-report folder. Nothing is sent anywhere; at the next start the
//! application shows where the report is, and the user decides whether to
//! pass it on.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use super::utc_timestamp;

/// The most recent log lines, kept in memory for crash reports.
#[derive(Debug, Default)]
pub struct RecentLog {
    lines: Mutex<VecDeque<String>>,
}

const KEPT_LINES: usize = 200;

impl RecentLog {
    pub fn push(&self, line: String) {
        if let Ok(mut lines) = self.lines.lock() {
            lines.push_back(line);
            while lines.len() > KEPT_LINES {
                lines.pop_front();
            }
        }
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.lines
            .lock()
            .map(|l| l.iter().cloned().collect())
            .unwrap_or_default()
    }
}

/// Builds the text of a crash report.
pub fn report_text(message: &str, location: &str, backtrace: &str, log: &[String]) -> String {
    let mut text = String::new();
    let _ = writeln!(text, "ArchiCraft crash report");
    let _ = writeln!(text, "Time: {}", utc_timestamp(SystemTime::now()));
    let _ = writeln!(text, "Version: {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(
        text,
        "System: {} {} ({})",
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::env::consts::FAMILY
    );
    let _ = writeln!(text, "\nPanic: {message}\nAt: {location}\n\nBacktrace:\n{backtrace}");
    let _ = writeln!(text, "\nRecent log ({} lines):", log.len());
    for line in log {
        let _ = writeln!(text, "{line}");
    }
    text
}

/// Installs the panic hook. Reports are written to `dir`.
pub fn install(dir: PathBuf, log: Arc<RecentLog>) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".to_owned());
        let location = info
            .location()
            .map_or_else(|| "unknown location".to_owned(), |l| l.to_string());
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        let text = report_text(&message, &location, &backtrace, &log.snapshot());
        let stamp = utc_timestamp(SystemTime::now()).replace([':', '-'], "");
        if fs::create_dir_all(&dir).is_ok() {
            let path = dir.join(format!("crash-{stamp}-{}.txt", std::process::id()));
            if fs::write(&path, text).is_ok() {
                eprintln!("ArchiCraft crashed. A crash report was saved to {}", path.display());
            }
        }
        previous(info);
    }));
}

/// Crash reports in `dir` newer than `seen`, newest first.
pub fn reports_since(dir: &Path, seen: Option<&str>) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut reports: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("crash-") && n.ends_with(".txt") && seen.is_none_or(|s| n > s))
        })
        .collect();
    reports.sort();
    reports.reverse();
    reports
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_contains_the_essentials() {
        let text = report_text("boom", "src/main.rs:1:1", "frame 0", &["line one".into()]);
        assert!(text.contains("Panic: boom"));
        assert!(text.contains("Version:"));
        assert!(text.contains("line one"));
    }

    #[test]
    fn recent_log_is_bounded() {
        let log = RecentLog::default();
        for i in 0..500 {
            log.push(format!("{i}"));
        }
        let lines = log.snapshot();
        assert_eq!(lines.len(), KEPT_LINES);
        assert_eq!(lines.last().map(String::as_str), Some("499"));
    }

    #[test]
    fn finds_new_reports() {
        let dir = std::env::temp_dir().join(format!("archicraft-crash-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("crash-20261016T100000Z-1.txt"), "a").unwrap();
        fs::write(dir.join("crash-20261016T110000Z-1.txt"), "b").unwrap();
        fs::write(dir.join("notes.txt"), "c").unwrap();
        assert_eq!(reports_since(&dir, None).len(), 2);
        let newer = reports_since(&dir, Some("crash-20261016T100000Z-1.txt"));
        assert_eq!(newer.len(), 1);
        assert!(newer[0].ends_with("crash-20261016T110000Z-1.txt"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
