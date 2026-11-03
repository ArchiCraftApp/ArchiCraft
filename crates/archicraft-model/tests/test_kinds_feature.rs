//! The test element kind (`KindTag::Dummy`, SRS 3.2, ELM-011) exists only in
//! test builds: the feature `test-kinds` is defined in archicraft-model's
//! `[features]` and enabled only through `[dev-dependencies]`, never by a
//! normal dependency, a workspace dependency or another feature, so it cannot
//! reach a release build through feature unification. Another crate may
//! define a `test-kinds` feature of its own that only forwards to
//! `<crate>/test-kinds` (archicraft-geometry, whose generator registry has
//! the test kind behind it), on one line; it is enabled the same way.
//!
//! The scan reads every `Cargo.toml` of the repository (hidden directories,
//! `target` and `node_modules` skipped) line by line, ignoring comments.

use std::fs;
use std::path::{Path, PathBuf};

const FEATURE: &str = "test-kinds";

/// The repository root, two levels above this crate.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate lies in crates/")
        .to_path_buf()
}

/// The `Cargo.toml` files under `dir`, sorted.
fn manifests(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| entry.expect("a directory entry").path())
        .collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if path.is_dir() {
            if !name.starts_with('.') && name != "target" && name != "node_modules" {
                manifests(&path, out);
            }
        } else if name == "Cargo.toml" {
            out.push(path);
        }
    }
}

/// `line` without its comment; a `#` inside a string is kept.
fn strip_comment(line: &str) -> &str {
    let mut quote = None;
    for (i, c) in line.char_indices() {
        match (quote, c) {
            (None, '#') => return &line[..i],
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), _) if c == q => quote = None,
            _ => {}
        }
    }
    line
}

/// The table name of a header line (`[dependencies]`, `[[bin]]`,
/// `[target.'cfg(unix)'.dev-dependencies]`), or `None` for any other line.
/// An array element on a line of its own (`["a", "b"]`) is not a header.
fn table_header(line: &str) -> Option<&str> {
    let inner = line.strip_prefix("[[").and_then(|l| l.strip_suffix("]]"));
    let inner = inner.or_else(|| line.strip_prefix('[').and_then(|l| l.strip_suffix(']')))?;
    inner
        .starts_with(|c: char| c.is_ascii_alphabetic())
        .then(|| inner.trim())
}

/// True for `[dev-dependencies]` and its forms `[dev-dependencies.<dep>]` and
/// `[target.<cfg>.dev-dependencies]`.
fn is_dev_dependencies(table: &str) -> bool {
    table.split('.').any(|part| part.trim() == "dev-dependencies")
}

/// The quoted strings of a line, without their quotes.
fn quoted(line: &str) -> Vec<&str> {
    line.split('"').skip(1).step_by(2).collect()
}

/// The lines (1-based number and text) of `manifest` that name the feature
/// outside `[dev-dependencies]`. In archicraft-model's own manifest
/// (`is_model`), the definition `test-kinds = …` in `[features]` is allowed
/// too; in any manifest, a one-line `test-kinds = ["<crate>/test-kinds", …]`
/// that only forwards the feature is allowed; but not another feature that
/// enables it.
fn leaks(manifest: &str, is_model: bool) -> Vec<(usize, String)> {
    let mut table = "";
    let mut found = Vec::new();
    for (n, raw) in manifest.lines().enumerate() {
        let line = strip_comment(raw).trim();
        if let Some(header) = table_header(line) {
            table = header;
            continue;
        }
        if !line.contains(FEATURE) || is_dev_dependencies(table) {
            continue;
        }
        if table == "features"
            && let Some((key, value)) = line.split_once('=')
            && key.trim() == FEATURE
        {
            let forwarded = quoted(value);
            let forwards_only = !forwarded.is_empty()
                && forwarded
                    .iter()
                    .all(|feature| feature.strip_suffix(FEATURE).is_some_and(|c| c.ends_with('/')));
            if is_model || forwards_only {
                continue;
            }
        }
        found.push((n + 1, raw.to_string()));
    }
    found
}

/// ELM-011: no manifest of the repository enables `test-kinds` outside
/// `[dev-dependencies]`, and archicraft-model's own tests enable it through
/// the dev-dependency on the crate itself.
#[test]
fn test_kinds_is_enabled_only_through_dev_dependencies() {
    let root = repo_root();
    let mut paths = Vec::new();
    manifests(&root, &mut paths);
    let model = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    for expected in [root.join("Cargo.toml"), model.clone()] {
        assert!(paths.contains(&expected), "the scan must read {}", expected.display());
    }
    let mut offending = Vec::new();
    for path in &paths {
        let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for (n, line) in leaks(&text, *path == model) {
            offending.push(format!("{}:{n}: {}", path.display(), line.trim()));
        }
    }
    assert!(
        offending.is_empty(),
        "`{FEATURE}` may be enabled only under [dev-dependencies]:\n{}",
        offending.join("\n")
    );
    let own = fs::read_to_string(&model).expect("the model manifest");
    let mut table = "";
    let self_dev_dependency = own.lines().map(|l| strip_comment(l).trim()).any(|line| {
        if let Some(header) = table_header(line) {
            table = header;
        }
        table == "dev-dependencies" && line.starts_with("archicraft-model ") && line.contains(FEATURE)
    });
    assert!(
        self_dev_dependency,
        "archicraft-model's tests must enable `{FEATURE}` through a dev-dependency on itself"
    );
}

/// The scan flags every way a manifest can enable the feature in a normal
/// build and accepts the dev-dependency forms, comments, the model's
/// definition of the feature and another crate's feature that only forwards
/// it.
#[test]
fn the_scan_flags_test_kinds_outside_dev_dependencies() {
    let leaking = [
        "[dependencies]\narchicraft-model = { workspace = true, features = [\"test-kinds\"] }\n",
        "[dependencies]\narchicraft-model = { workspace = true, features = [\n    \"test-kinds\",\n] }\n",
        "[dependencies.archicraft-model]\nworkspace = true\nfeatures = [\"test-kinds\"]\n",
        "[target.'cfg(unix)'.dependencies]\narchicraft-model = { workspace = true, features = [\"test-kinds\"] }\n",
        "[build-dependencies]\narchicraft-model = { workspace = true, features = [\"test-kinds\"] }\n",
        "[workspace.dependencies]\narchicraft-model = { path = \"m\", features = [\"test-kinds\"] }\n",
        "[features]\nfull = [\"archicraft-model/test-kinds\"]\n",
        "[features]\ntest-kinds = [\"archicraft-model/test-kinds\", \"extra\"]\n",
        "[features]\ntest-kinds = [\"test-kinds\"]\n",
        "[dependencies]\narchicraft-geometry = { workspace = true, features = [\"test-kinds\"] }\n",
        "[dev-dependencies]\nx = \"1\"\n\n[dependencies]\nm = { package = \"archicraft-model\", features = [\"test-kinds\"] }\n",
    ];
    for manifest in leaking {
        assert_eq!(leaks(manifest, false).len(), 1, "must be flagged:\n{manifest}");
    }
    let model_leaking = [
        "[features]\ntest-kinds = []\ndefault = [\"test-kinds\"]\n",
        "[features]\ntest-kinds = []\ndefault = [\n    \"test-kinds\",\n]\n",
    ];
    for manifest in model_leaking {
        assert_eq!(
            leaks(manifest, true).len(),
            1,
            "must be flagged in the model:\n{manifest}"
        );
    }
    assert_eq!(
        leaks("[features]\ntest-kinds = []\n", false).len(),
        1,
        "only the model defines it without forwarding"
    );

    let allowed = [
        "[features]\ntest-kinds = [\"archicraft-model/test-kinds\"]\n",
        "[features]\ntest-kinds = [\"archicraft-model/test-kinds\", \"archicraft-geometry/test-kinds\"]\n",
        "[dev-dependencies]\narchicraft-geometry = { workspace = true, features = [\"test-kinds\"] }\n",
        "[dev-dependencies]\narchicraft-model = { workspace = true, features = [\"test-kinds\"] }\n",
        "[dev-dependencies]\narchicraft-model = { workspace = true, features = [\n    \"test-kinds\",\n] }\n",
        "[dev-dependencies.archicraft-model]\nworkspace = true\nfeatures = [\"test-kinds\"]\n",
        "[target.'cfg(unix)'.dev-dependencies]\narchicraft-model = { workspace = true, features = [\"test-kinds\"] }\n",
        "[dependencies]\n# never enable test-kinds here\narchicraft-model.workspace = true\n",
    ];
    for manifest in allowed {
        assert!(leaks(manifest, false).is_empty(), "must be accepted:\n{manifest}");
    }
    assert!(leaks("[features]\n# test-only\ntest-kinds = []\n", true).is_empty());
}
