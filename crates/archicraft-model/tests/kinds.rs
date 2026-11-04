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

/// The registry lists wall and slab, plus the test kinds (tests always build
/// with the feature `test-kinds`, tests/test_kinds_feature.rs), in the order
/// of `KindTag::ALL`, and each tag's descriptor is its registry entry.
#[test]
fn the_registry_has_wall_and_slab() {
    let codes: Vec<&str> = KINDS.iter().map(|d| d.code).collect();
    assert_eq!(codes, ["wall", "slab", "dummy", "note"]);
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

/// ANN-002, ANN-010: every descriptor declares whether its elements have a
/// body and whether they are IFCX nodes. Walls, slabs and the test kind's
/// boxes have both; the body-less test kind, standing in for texts and
/// dimensions, has neither.
#[test]
fn every_kind_declares_its_body_and_its_ifcx_node() {
    let flags: Vec<(&str, bool, bool)> = KINDS.iter().map(|d| (d.code, d.has_body, d.ifcx_node)).collect();
    assert_eq!(
        flags,
        [
            ("wall", true, true),
            ("slab", true, true),
            ("dummy", true, true),
            ("note", false, false),
        ]
    );
}

/// ANN-002, ELM-019: the body-less test kind is an `IfcAnnotation` (Table
/// 5-1), has no drawing tool and names its elements `NT-001`, `NT-002`, ….
#[cfg(feature = "test-kinds")]
#[test]
fn the_body_less_test_kind_has_its_own_descriptor() {
    let d = KindTag::Note.descriptor();
    assert_eq!(
        (d.code, d.name_prefix, d.node_prefix, d.has_tool),
        ("note", "NT", "Note", false)
    );
    assert_eq!(d.default_class, IfcClass::Annotation);
    assert!(d.template_defaults.is_none());
    assert_eq!(KindTag::from_code("note"), Some(KindTag::Note));
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

/// SLB-003 ("any number of holes"), archicraft-ysf8.3: a 10 m × 8 m slab
/// with two holes, its outline drawn clockwise and its holes counter-
/// clockwise, validates when added: the stored outline is counter-clockwise
/// and both holes clockwise with their points kept, the area is the outline
/// minus both holes (80 − 2 − 2.25 m²), and a point in either hole is
/// outside the slab.
#[test]
fn a_slab_with_two_holes_validates_and_deducts_both() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let rect = |x0: f64, y0: f64, x1: f64, y1: f64| {
        vec![
            DVec2::new(x0, y0),
            DVec2::new(x1, y0),
            DVec2::new(x1, y1),
            DVec2::new(x0, y1),
        ]
    };
    let mut outline = rect(0.0, 0.0, 10.0, 8.0);
    outline.reverse();
    let holes = vec![rect(1.0, 1.0, 3.0, 2.0), rect(6.0, 4.0, 7.5, 5.5)];
    let slab = p.new_slab(ground(&p), outline.clone(), holes.clone(), 0);
    let id = slab.id;
    h.execute(&mut p, Command::AddElements(vec![slab])).unwrap();

    let stored = &p.element(id).unwrap().as_slab().unwrap().outline;
    assert!(stored.outer().signed_area() > 0.0, "outline counter-clockwise");
    assert_eq!(stored.holes().len(), 2);
    assert!(stored.holes().iter().all(|h| h.signed_area() < 0.0), "holes clockwise");
    let sorted = |ring: Vec<DVec2>| {
        let mut points = ring;
        points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
        points
    };
    assert_eq!(sorted(stored.outer().points().collect()), sorted(outline));
    for (stored, given) in stored.holes().iter().zip(holes) {
        assert_eq!(sorted(stored.points().collect()), sorted(given));
    }
    assert!((stored.area() - (80.0 - 2.0 - 2.25)).abs() < 1e-9, "{}", stored.area());
    assert!(stored.contains(DVec2::new(5.0, 3.0)));
    assert!(!stored.contains(DVec2::new(2.0, 1.5)), "inside the first hole");
    assert!(!stored.contains(DVec2::new(6.75, 4.75)), "inside the second hole");
}
