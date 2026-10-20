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
    for (module, payload) in [("wall", "Wall"), ("slab", "Slab")] {
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

/// The crate-root re-export paths stay as they were, so dependants do not
/// change.
#[test]
fn payload_reexport_paths_are_unchanged() {
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
