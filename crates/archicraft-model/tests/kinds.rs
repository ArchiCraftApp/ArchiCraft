//! The element kind registry (SRS 3.2, ELM-011, ELM-019): one
//! `KindDescriptor` per kind, generated with `ElementKind` and `KindTag` from
//! one list in `kinds/mod.rs`.

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::HashSet;

use archicraft_model::glam::DVec2;
use archicraft_model::kinds::KINDS;
use archicraft_model::project::NameCounters;
use archicraft_model::template::{TemplateContext, default_metric_project};
use archicraft_model::{Arg, Command, CommandError, History, IfcClass, KindTag};
use common::{ground, wall};

/// The registry lists wall and slab, plus the test kind (tests always build
/// with the feature `test-kinds`, tests/test_kinds_feature.rs), in the order
/// of `KindTag::ALL`, and each tag's descriptor is its registry entry.
#[test]
fn the_registry_has_wall_and_slab() {
    let codes: Vec<&str> = KINDS.iter().map(|d| d.code).collect();
    assert_eq!(codes, ["wall", "slab", "dummy"]);
    assert_eq!(KINDS.len(), KindTag::ALL.len());
    for (tag, descriptor) in KindTag::ALL.iter().zip(KINDS) {
        // Codes are unique (below), so equal codes mean the same entry.
        assert_eq!(tag.descriptor().code, descriptor.code, "{tag:?}");
    }
    assert!(KindTag::Wall.descriptor().has_tool && KindTag::Slab.descriptor().has_tool);
}

/// Codes, message keys, IFCX node prefixes and name prefixes identify a
/// kind, so they are unique (ELM-019: automatic names must not collide
/// between kinds).
#[test]
fn codes_labels_and_name_prefixes_are_unique() {
    for field in [
        |d: &&archicraft_model::kinds::KindDescriptor| d.code,
        |d: &&archicraft_model::kinds::KindDescriptor| d.label_key,
        |d: &&archicraft_model::kinds::KindDescriptor| d.noun_key,
        |d: &&archicraft_model::kinds::KindDescriptor| d.node_prefix,
        |d: &&archicraft_model::kinds::KindDescriptor| d.name_prefix,
    ] {
        let values: Vec<&str> = KINDS.iter().map(field).collect();
        let unique: HashSet<&str> = values.iter().copied().collect();
        assert_eq!(unique.len(), values.len(), "{values:?}");
    }
}

/// `KindTag::from_code` finds every kind by its descriptor's code, and the
/// tag's accessors delegate to the descriptor.
#[test]
fn from_code_round_trips() {
    for &tag in KindTag::ALL {
        let d = tag.descriptor();
        assert_eq!(KindTag::from_code(d.code), Some(tag));
        assert_eq!(
            (
                tag.code(),
                tag.label_key(),
                tag.noun_key(),
                tag.name_prefix(),
                tag.ifc_classes()
            ),
            (d.code, d.label_key, d.noun_key, d.name_prefix, d.ifc_classes)
        );
    }
    assert_eq!(KindTag::from_code("curtain-wall"), None);
}

/// ELM-011: every kind's default IFC class is one of its selectable classes,
/// and the defaults are those of Table 5-1.
#[test]
fn every_default_class_is_selectable() {
    for d in KINDS {
        assert!(d.ifc_classes.contains(&d.default_class), "{}", d.code);
    }
    assert_eq!(KindTag::Wall.descriptor().default_class, IfcClass::Wall);
    assert_eq!(KindTag::Slab.descriptor().default_class, IfcClass::Slab);
}

/// ELM-019: counters are kept per kind; `next` is the number of the next
/// element and `bump` advances only its own kind.
#[test]
fn name_counters_are_keyed_by_kind() {
    let mut counters = NameCounters::default();
    assert_eq!(counters.next(KindTag::Wall), 1);
    counters.bump(KindTag::Wall);
    counters.bump(KindTag::Wall);
    assert_eq!((counters.get(KindTag::Wall), counters.get(KindTag::Slab)), (2, 0));
    assert_eq!(counters.next(KindTag::Wall), 3);
    let mut zero = counters.clone();
    zero.set(KindTag::Slab, 0);
    assert_eq!(zero, counters, "a zero counter equals an absent one");
}

/// ELM-019: adding a slab bumps only the slab counter, and undo restores it.
#[test]
fn adding_elements_bumps_only_their_kinds_counter() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    let before = p.counters.clone();
    let slab = p.new_slab(
        ground(&p),
        vec![DVec2::ZERO, DVec2::X, DVec2::ONE, DVec2::Y],
        Vec::new(),
        0,
    );
    assert_eq!(slab.name, "SL-001");
    h.execute(&mut p, Command::AddElements(vec![slab])).unwrap();
    let mut expected = before.clone();
    expected.bump(KindTag::Slab);
    assert_eq!(p.counters, expected);
    assert_eq!(p.counters.get(KindTag::Wall), 1);
    assert_eq!(p.next_element_name(KindTag::Slab, 0), "SL-002");
    h.undo(&mut p).unwrap();
    assert_eq!(p.counters, before);
}

/// ELM-011: an element whose IFC class is outside its kind's set is rejected
/// and the project stays unchanged.
#[test]
fn a_class_outside_the_kinds_set_is_rejected() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let before = p.clone();
    let mut w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    w.ifc_class = IfcClass::Covering;
    assert!(!KindTag::Wall.ifc_classes().contains(&IfcClass::Covering));
    match h.execute(&mut p, Command::AddElements(vec![w])) {
        Err(CommandError::Invalid(message)) => {
            assert_eq!(message.key(), "model-error-ifc-class-not-allowed");
            assert_eq!(message.arg("class"), Some(&Arg::Text("IfcCovering".into())));
            assert_eq!(message.arg("kind"), Some(&Arg::Kind(KindTag::Wall)));
        }
        other => panic!("expected CommandError::Invalid, got {other:?}"),
    }
    assert_eq!(p, before);
}

/// ELM-011: the test kind is an `IfcBuildingElementProxy` by default, has no
/// drawing tool and names its elements `X-001`, `X-002`, … (ELM-019).
#[cfg(feature = "test-kinds")]
#[test]
fn the_test_kind_has_its_own_descriptor() {
    let d = KindTag::Dummy.descriptor();
    assert_eq!((d.code, d.name_prefix, d.has_tool), ("dummy", "X", false));
    assert_eq!(d.default_class, IfcClass::BuildingElementProxy);
    assert_eq!(IfcClass::from_code("IfcBuildingElementProxy"), Some(d.default_class));
    assert_eq!(KindTag::from_code("dummy"), Some(KindTag::Dummy));
}

/// ELM-001, ELM-011: every kind with a tool has template defaults, a slot
/// of its own kind with its descriptor's default class; the template
/// project holds exactly those slots, in registry order. The test kind has
/// no tool and no slot.
#[test]
fn template_defaults_follow_the_descriptors() {
    let p = default_metric_project();
    let context = TemplateContext::new(&p.materials, &p.layers);
    let mut with_defaults = Vec::new();
    for &kind in KindTag::ALL {
        let descriptor = kind.descriptor();
        assert_eq!(descriptor.template_defaults.is_some(), descriptor.has_tool, "{kind:?}");
        let Some(template) = descriptor.template_defaults else {
            continue;
        };
        let slot = template(&context);
        assert_eq!(slot.tag(), kind);
        assert_eq!(slot.ifc_class, descriptor.default_class, "{kind:?}");
        assert_eq!(p.tool_defaults.get(kind), Some(&slot), "{kind:?}");
        with_defaults.push(kind);
    }
    let slots: Vec<KindTag> = p.tool_defaults.iter().map(|(kind, _)| kind).collect();
    assert_eq!(slots, with_defaults);
}
