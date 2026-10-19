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
            .is_some_and(|rest| rest.starts_with([' ', '{', '(', '<']))
    })
}

/// The wall and slab payloads live in their kind modules, and `element.rs`
/// keeps the envelope (Element, ElementKind, KindTag, IfcClass, Position,
/// TopLink).
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
        "pub enum ElementKind",
        "pub enum KindTag",
        "pub enum IfcClass",
        "pub enum Position",
        "pub enum TopLink",
    ] {
        assert!(defines(&element, item), "element.rs must keep `{item}`");
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
