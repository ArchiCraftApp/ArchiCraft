//! Module layout rules of the application crate.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

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

/// The point-input service is headless (INP-001): it serves plan views, 3D
/// views and every tool through plain types, so no file under `src/input/`
/// may mention egui.
#[test]
fn input_module_does_not_mention_egui() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/input");
    let mut files = Vec::new();
    rust_files(&dir, &mut files);
    assert!(!files.is_empty(), "no sources under {}", dir.display());
    let offenders: Vec<_> = files
        .iter()
        .filter(|f| std::fs::read_to_string(f).unwrap().contains("egui"))
        .collect();
    assert!(
        offenders.is_empty(),
        "files under src/input/ mention egui: {offenders:?}"
    );
}

/// Whether `src` calls the free function `name`: `name(` not preceded by a
/// character of an identifier, a `.` or a `::`.
fn calls(src: &str, name: &str) -> bool {
    let call = format!("{name}(");
    src.match_indices(&call).any(|(i, _)| {
        !src[..i]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == ':')
    })
}

/// INP-001: the plan view hands every point to the point-input service
/// instead of running typed values, the constraint and snapping itself.
#[test]
fn plan_view_resolves_points_through_point_input() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/views/plan.rs");
    let src = std::fs::read_to_string(&path).unwrap();
    for name in ["snap", "constrain"] {
        assert!(!calls(&src, name), "views/plan.rs calls {name}()");
    }
    assert!(!src.contains("apply_typed("), "views/plan.rs calls apply_typed()");
    assert!(calls("let s = snap(&t);", "snap") && !calls("draw_snap(x)", "snap"));
}
