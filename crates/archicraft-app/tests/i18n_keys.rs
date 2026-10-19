//! The `tr!` keys in the sources and the English catalogue match
//! (NFR-L10N-001): every literal key exists, and every catalogue key is used
//! or belongs to a key family built at runtime.

#![allow(clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use archicraft_app::i18n::{DYNAMIC_PREFIXES, catalogue};

/// The literal keys of the `tr!` calls in `source`, with their 1-based lines.
/// Comment lines and the trailing `#[cfg(test)] mod tests` block are skipped:
/// tests use keys of their own. A key that is not a string literal is built
/// at runtime and must start with one of [`DYNAMIC_PREFIXES`].
fn literal_keys(source: &str) -> Vec<(String, usize)> {
    let mut keys = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if *line == "#[cfg(test)]" && lines.get(i + 1).is_some_and(|next| next.starts_with("mod tests")) {
            break;
        }
        if line.trim_start().starts_with("//") {
            continue;
        }
        for (at, _) in line.match_indices("tr!(") {
            // `include_str!(` and other macros ending in `tr` are not `tr!`.
            if line[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
            {
                continue;
            }
            // The key may be on the next line after a line break.
            let mut call = line[at + 4..].trim_start();
            if call.is_empty() {
                call = lines.get(i + 1).map_or("", |next| next.trim_start());
            }
            if let Some(literal) = call.strip_prefix('"')
                && let Some(end) = literal.find('"')
            {
                keys.push((literal[..end].to_owned(), i + 1));
            }
        }
    }
    keys
}

fn rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, files);
        } else if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
}

#[test]
fn the_scanner_finds_literal_keys_outside_comments_and_tests() {
    let source = r#"
//! ui.label(tr!("doc-comment-key"));
fn f(ui: &mut Ui) {
    ui.label(tr!("dlg-about-title"));
    ui.heading(tr!("dlg-about-heading", version = "1"));
    ui.label(tr!(
        "menu-help"
    ));
    ui.label(tr!(&format!("cmd-{id}-label")));
    let shader = include_str!("shader.wgsl");
    ui.label(crate::tr!("dlg-about-summary"));
}

#[cfg(test)]
mod tests {
    fn t() { tr!("test-only"); }
}
"#;
    let keys: Vec<String> = literal_keys(source).into_iter().map(|(k, _)| k).collect();
    assert_eq!(
        keys,
        ["dlg-about-title", "dlg-about-heading", "menu-help", "dlg-about-summary"]
    );
}

#[test]
fn tr_keys_and_the_english_catalogue_match() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    let mut used: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for file in &files {
        let source = std::fs::read_to_string(file).unwrap();
        let name = file.strip_prefix(&src).unwrap().display().to_string();
        for (key, line) in literal_keys(&source) {
            used.entry(key).or_default().push(format!("{name}:{line}"));
        }
    }
    let defined: BTreeSet<String> = catalogue::ENGLISH.iter().flat_map(catalogue::keys).collect();

    let missing: Vec<String> = used
        .iter()
        .filter(|(key, _)| !defined.contains(*key))
        .map(|(key, places)| format!("{key} (used in {})", places.join(", ")))
        .collect();
    assert!(
        missing.is_empty(),
        "tr! keys missing from the English catalogue: {missing:?}"
    );

    let unused: Vec<&String> = defined
        .iter()
        .filter(|key| !used.contains_key(*key) && !DYNAMIC_PREFIXES.iter().any(|p| key.starts_with(p)))
        .collect();
    assert!(
        unused.is_empty(),
        "catalogue keys without a tr! call or a dynamic prefix {DYNAMIC_PREFIXES:?}: {unused:?}"
    );

    // The pilot keys are found, so the scan reached the migrated files.
    assert!(used.contains_key("menu-help") && used.contains_key("dlg-about-heading"));
}
