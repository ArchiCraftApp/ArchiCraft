//! Module layout of the element kinds (SRS 3.2, ELM-011, ELM-006): each kind
//! is one module under `src/kinds/` holding its parameter payload, while
//! `element.rs` keeps only the kind-independent element envelope.

use std::fs;
use std::path::Path;

use archicraft_model::{ReferenceLine, Slab, SlabReference, Wall};

fn source(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(relative);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// True if `source` defines the item, for example `pub struct Wall`.
fn defines(source: &str, item: &str) -> bool {
    source.lines().any(|line| {
        line.trim_start()
            .strip_prefix(item)
            .is_some_and(|rest| rest.starts_with([' ', '{', '(', '<', ':']))
    })
}

/// The wall and slab payloads live in their kind modules, and `element.rs`
/// keeps the envelope (Element, IfcClass, Position, TopLink).
#[test]
fn kind_payloads_live_in_their_kind_modules() {
    let element = source("element.rs");
    let wall = source("kinds/wall.rs");
    let slab = source("kinds/slab.rs");
    for item in ["pub struct Wall", "pub enum ReferenceLine"] {
        assert!(defines(&wall, item), "kinds/wall.rs must define `{item}`");
        assert!(!defines(&element, item), "element.rs must not define `{item}`");
        assert!(!defines(&slab, item), "kinds/slab.rs must not define `{item}`");
    }
    for item in ["pub struct Slab", "pub enum SlabReference"] {
        assert!(defines(&slab, item), "kinds/slab.rs must define `{item}`");
        assert!(!defines(&element, item), "element.rs must not define `{item}`");
        assert!(!defines(&wall, item), "kinds/wall.rs must not define `{item}`");
    }
    for item in [
        "pub struct Element",
        "pub enum IfcClass",
        "pub enum Position",
        "pub enum TopLink",
    ] {
        assert!(defines(&element, item), "element.rs must keep `{item}`");
    }
}

/// `ElementKind`, `KindTag` and the registry are generated in `kinds/mod.rs`
/// from one `kinds!` invocation that lists every kind once (SRS 3.2,
/// ELM-011), so adding a kind is one module plus one line.
#[test]
fn element_kinds_are_generated_from_one_list() {
    let element = source("element.rs");
    let kinds = source("kinds/mod.rs");
    for item in ["pub enum ElementKind", "pub enum KindTag", "pub static KINDS"] {
        assert!(defines(&kinds, item), "kinds/mod.rs must generate `{item}`");
        assert!(!defines(&element, item), "element.rs must not define `{item}`");
    }
    let invocation = kinds
        .split_once("\nkinds! {")
        .and_then(|(_, rest)| rest.split_once("\n}"))
        .map(|(list, _)| list)
        .expect("kinds/mod.rs must invoke `kinds! { … }`");
    for (module, payload) in [("wall", "Wall"), ("slab", "Slab"), ("dummy", "Dummy")] {
        let entry = format!("{module} => {payload}");
        assert_eq!(invocation.matches(&entry).count(), 1, "`{entry}` must be listed once");
    }
}

/// The project, validation and command code ask the registry instead of
/// matching on kinds: no `KindTag::<kind> =>` arm and no per-kind counter
/// field (ELM-011, ELM-019).
#[test]
fn core_model_code_does_not_match_on_kind_tags() {
    for file in ["project.rs", "validate.rs", "command.rs"] {
        let text = source(file);
        for (n, line) in text.lines().enumerate() {
            for pattern in ["KindTag::Wall =>", "KindTag::Slab =>", "counters.wall", "counters.slab"] {
                assert!(!line.contains(pattern), "{file}:{}: `{pattern}` in {line:?}", n + 1);
            }
        }
    }
}

/// The payload types are defined in the kind modules; the crate-root
/// re-exports they are named through here (`archicraft_model::Wall`, …)
/// still compile, so dependants do not change.
#[test]
fn payload_types_are_defined_in_the_kinds_module() {
    let names = [
        std::any::type_name::<Wall>(),
        std::any::type_name::<ReferenceLine>(),
        std::any::type_name::<Slab>(),
        std::any::type_name::<SlabReference>(),
    ];
    for name in names {
        assert!(name.starts_with("archicraft_model::kinds::"), "{name}");
    }
}

/// Blanks every item annotated with `#[cfg(test)]`, up to its `;` or to the
/// end of its block. Line breaks are kept, so line numbers stay valid.
fn without_test_items(source: &str) -> String {
    const MARKER: &str = "#[cfg(test)]";
    let mut chars: Vec<char> = source.chars().collect();
    let marker: Vec<char> = MARKER.chars().collect();
    let mut i = 0;
    while i + marker.len() <= chars.len() {
        if chars[i..i + marker.len()] != marker[..] {
            i += 1;
            continue;
        }
        let mut k = i + marker.len();
        let mut depth = 0;
        while k < chars.len() {
            match chars[k] {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' => depth -= 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                ';' if depth == 0 => break,
                _ => {}
            }
            k += 1;
        }
        let end = (k + 1).min(chars.len());
        for c in chars[i..end].iter_mut().filter(|c| **c != '\n') {
            *c = ' ';
        }
        i = end;
    }
    chars.into_iter().collect()
}

/// The `.rs` files under `dir`, relative to `root`, sorted.
fn rust_files(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let mut paths: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.expect("a directory entry").path())
        .collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            rust_files(root, &path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let relative = path.strip_prefix(root).expect("below the root");
            out.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// Behaviour that differs by kind, such as storey links, plan transforms and
/// construction, sits behind the kind hooks (ELM-006, STR-006): outside
/// `src/kinds/` and `#[cfg(test)]` code, no model source names a kind's
/// payload variant.
#[test]
fn only_kind_modules_match_on_kind_variants() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&root, &root, &mut files);
    assert!(files.iter().any(|f| f == "project.rs"), "{files:?}");
    for file in files.iter().filter(|f| !f.starts_with("kinds/")) {
        let code = without_test_items(&source(file));
        for (n, line) in code.lines().enumerate() {
            for pattern in ["ElementKind::Wall(", "ElementKind::Slab("] {
                assert!(!line.contains(pattern), "{file}:{}: `{pattern}` in {line:?}", n + 1);
            }
        }
    }
}

/// A file below a crate's `src/` that may name the test kind, and on how
/// many lines (`None`: any number).
type Allowed = (&'static str, Option<usize>);

/// The crates whose sources [`the_test_kind_is_named_only_in_its_module`]
/// scans, with the files of each that may name the test kind.
const TEST_KIND_SCOPE: &[(&str, &[Allowed])] = &[
    (
        "archicraft-model",
        &[("kinds/dummy.rs", None), ("kinds/mod.rs", Some(1))],
    ),
    ("archicraft-app", &[]),
];

/// SRS 3.2, ELM-011: a kind is one module plus one line in the `kinds!`
/// list. The test kind's identifiers `Dummy` and `dummy` occur in the model
/// sources only in `kinds/dummy.rs` and on one line of `kinds/mod.rs`, and
/// in no other scanned crate, so everything else reaches it through the
/// registry and the hooks.
#[test]
fn the_test_kind_is_named_only_in_its_module() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crates folder");
    for (krate, allowed) in TEST_KIND_SCOPE {
        let root = crates.join(krate).join("src");
        let mut files = Vec::new();
        rust_files(&root, &root, &mut files);
        assert!(!files.is_empty(), "{}", root.display());
        for (name, _) in *allowed {
            assert!(files.iter().any(|f| f == name), "{krate}: {name} is missing");
        }
        for file in &files {
            let text = fs::read_to_string(root.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
            let lines: Vec<(usize, &str)> = text
                .lines()
                .enumerate()
                .filter(|(_, line)| line.contains("Dummy") || line.contains("dummy"))
                .map(|(n, line)| (n + 1, line))
                .collect();
            match allowed.iter().find(|(name, _)| name == file) {
                Some((_, None)) => {}
                Some((_, Some(limit))) => assert_eq!(lines.len(), *limit, "{krate}/src/{file}: {lines:?}"),
                None => assert!(lines.is_empty(), "{krate}/src/{file}: {lines:?}"),
            }
        }
    }
}
