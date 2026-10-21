//! STR-010: the number locale comes from the operating system's own API, not
//! from the POSIX environment variables. Each test compares the detection
//! with the platform's own tool; both run in the CI matrix (macos.yml,
//! windows.yml).

use archicraft_app::i18n::os_locale;

/// Set in the child process a test re-runs itself in.
#[cfg(target_os = "macos")]
const CHILD_TEST_ENV: &str = "ARCHICRAFT_OS_LOCALE_CHILD";

/// STR-010: on macOS the tag is the system locale (`AppleLocale`, set in
/// Language & Region) even when no POSIX locale variable is set, as for an
/// application started from the Finder: the test re-runs itself without
/// LANG, LC_ALL and LC_NUMERIC.
#[cfg(target_os = "macos")]
#[test]
fn macos_tag_is_the_system_locale_without_posix_variables() {
    use std::process::Command;

    const TEST: &str = "macos_tag_is_the_system_locale_without_posix_variables";
    if std::env::var_os(CHILD_TEST_ENV).is_none() {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([TEST, "--exact", "--test-threads=1"])
            .env(CHILD_TEST_ENV, "1")
            .env_remove("LANG")
            .env_remove("LC_ALL")
            .env_remove("LC_NUMERIC")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("1 passed"),
            "{TEST} in a child process:\n{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    for var in ["LANG", "LC_ALL", "LC_NUMERIC"] {
        assert!(std::env::var_os(var).is_none(), "{var} is removed in the child");
    }
    let defaults = Command::new("defaults")
        .args(["read", "-g", "AppleLocale"])
        .output()
        .unwrap();
    assert!(defaults.status.success(), "defaults read -g AppleLocale failed");
    let apple_locale = String::from_utf8_lossy(&defaults.stdout);
    let locale = os_locale::detect();
    assert_eq!(locale.tag, os_locale::normalize_tag(&apple_locale));
    assert!(matches!(locale.decimal, '.' | ','));
}

/// STR-010: on Windows the decimal separator is the one of the user's
/// culture, including changes made in the Region settings.
#[cfg(windows)]
#[test]
fn windows_decimal_is_the_user_culture_separator() {
    use std::process::Command;

    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-Culture).NumberFormat.NumberDecimalSeparator",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "PowerShell failed");
    let expected = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    assert_eq!(os_locale::detect().decimal.to_string(), expected);
}

/// STR-010: the cached detection is the detection.
#[test]
fn detected_locale_is_cached_detection() {
    assert_eq!(*os_locale::detected(), os_locale::detect());
    assert!(std::ptr::eq(os_locale::detected(), os_locale::detected()));
}
