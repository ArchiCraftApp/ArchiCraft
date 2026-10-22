//! Predefined types (IFX-024): the IFC 4.3 enumeration literals a slab or
//! footing carries, the types each kind offers per IFC class, and the checks
//! of elements and tool defaults.

#![allow(clippy::unwrap_used)]

mod common;

use archicraft_model::glam::DVec2;
use archicraft_model::message::ALL_KEYS;
use archicraft_model::template::default_metric_project;
use archicraft_model::{Arg, Command, CommandError, Element, History, IfcClass, KindTag, PredefinedType, Project};
use common::{ground, wall};

/// IFX-024: the predefined types are the literals of IfcSlabTypeEnum and
/// IfcFootingTypeEnum in IFC 4.3, so a future standard attribute can be
/// written verbatim.
#[test]
fn predefined_type_codes_are_the_ifc_4_3_literals() {
    let codes: Vec<&str> = PredefinedType::ALL.iter().map(|t| t.code()).collect();
    assert_eq!(
        codes,
        ["FLOOR", "ROOF", "LANDING", "BASESLAB", "PAD_FOOTING", "STRIP_FOOTING"]
    );
    for t in PredefinedType::ALL {
        assert_eq!(PredefinedType::from_code(t.code()), Some(t), "{t:?}");
        assert!(
            ALL_KEYS.contains(&t.label_key()),
            "{} is not a model key",
            t.label_key()
        );
    }
    assert_eq!(PredefinedType::from_code("floor"), None);
    assert_eq!(PredefinedType::from_code("USERDEFINED"), None);
}

/// IFX-024: slabs offer floor, roof, landing and base slab, footings pad and
/// strip, and every other class none.
#[test]
fn predefined_types_follow_the_ifc_class() {
    use PredefinedType::*;
    assert_eq!(
        PredefinedType::for_class(IfcClass::Slab),
        [Floor, Roof, Landing, BaseSlab]
    );
    assert_eq!(PredefinedType::for_class(IfcClass::Footing), [PadFooting, StripFooting]);
    for class in [IfcClass::Wall, IfcClass::Covering, IfcClass::BuildingElementProxy] {
        assert!(PredefinedType::for_class(class).is_empty(), "{class:?}");
    }
}

/// IFX-024: the descriptor hook of every kind offers the class's types,
/// for each class the kind allows.
#[test]
fn every_kind_offers_the_types_of_its_classes() {
    for &kind in KindTag::ALL {
        for &class in kind.ifc_classes() {
            assert_eq!(
                kind.predefined_types(class),
                PredefinedType::for_class(class),
                "{kind:?} {class:?}"
            );
        }
    }
}

fn slab(p: &Project) -> Element {
    let outline = vec![
        DVec2::ZERO,
        DVec2::new(4.0, 0.0),
        DVec2::new(4.0, 3.0),
        DVec2::new(0.0, 3.0),
    ];
    p.new_slab(ground(p), outline, Vec::new(), 0)
}

/// True if the projects are equal apart from their revision, which counts
/// every change including undos.
fn same_model(a: &Project, b: &Project) -> bool {
    Project {
        revision: b.revision,
        ..a.clone()
    } == *b
}

/// The rejection of a command as `CommandError::Invalid`.
fn invalid(result: Result<(), CommandError>) -> archicraft_model::Message {
    match result {
        Err(CommandError::Invalid(message)) => message,
        other => panic!("expected CommandError::Invalid, got {other:?}"),
    }
}

/// Checks the message that rejects `t` on an element of `class`, also as
/// the reason of an UpdateElements rejection naming the element.
fn assert_type_not_allowed(message: &archicraft_model::Message, t: PredefinedType, class: IfcClass) {
    let message = match (message.key(), message.arg("reason")) {
        ("model-error-element-invalid", Some(Arg::Nested(reason))) => reason,
        _ => message,
    };
    assert_eq!(message.key(), "model-error-predefined-type-not-allowed");
    assert_eq!(message.arg("class"), Some(&Arg::Text(class.code().into())));
    match message.arg("type") {
        Some(Arg::Nested(label)) => assert_eq!(label.key(), t.label_key()),
        other => panic!("expected the type's label, got {other:?}"),
    }
}

/// IFX-024: an element keeps only a type its kind offers for its class. A
/// floor on an IfcWall wall and a pad footing on an IfcSlab slab are
/// rejected with `CommandError::Invalid` naming the class, and the project
/// stays unchanged; a pad footing on an IfcFooting wall is accepted and
/// undone like any other edit.
#[test]
fn update_elements_rejects_a_type_the_class_does_not_offer() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    let s = slab(&p);
    let (wall_id, slab_id) = (w.id, s.id);
    h.execute(&mut p, Command::AddElements(vec![w, s])).unwrap();
    let before = p.clone();

    let mut floor_wall = p.element(wall_id).unwrap().clone();
    assert_eq!(floor_wall.ifc_class, IfcClass::Wall);
    floor_wall.predefined_type = Some(PredefinedType::Floor);
    let message = invalid(h.execute(&mut p, Command::UpdateElements(vec![floor_wall])));
    assert_type_not_allowed(&message, PredefinedType::Floor, IfcClass::Wall);

    let mut pad_slab = p.element(slab_id).unwrap().clone();
    assert_eq!(pad_slab.ifc_class, IfcClass::Slab);
    pad_slab.predefined_type = Some(PredefinedType::PadFooting);
    let message = invalid(h.execute(&mut p, Command::UpdateElements(vec![pad_slab])));
    assert_type_not_allowed(&message, PredefinedType::PadFooting, IfcClass::Slab);
    assert!(same_model(&p, &before));

    let mut pad_wall = p.element(wall_id).unwrap().clone();
    pad_wall.ifc_class = IfcClass::Footing;
    pad_wall.predefined_type = Some(PredefinedType::PadFooting);
    h.execute(&mut p, Command::UpdateElements(vec![pad_wall.clone()]))
        .unwrap();
    assert_eq!(p.element(wall_id), Some(&pad_wall));
    h.undo(&mut p).unwrap();
    assert!(same_model(&p, &before));
}

/// IFX-024: new elements are checked too: AddElements rejects a slab with a
/// strip footing type while it is an IfcSlab.
#[test]
fn add_elements_rejects_a_type_the_class_does_not_offer() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let before = p.clone();
    let mut s = slab(&p);
    s.predefined_type = Some(PredefinedType::StripFooting);
    let message = invalid(h.execute(&mut p, Command::AddElements(vec![s.clone()])));
    assert_type_not_allowed(&message, PredefinedType::StripFooting, IfcClass::Slab);
    assert!(same_model(&p, &before));

    s.ifc_class = IfcClass::Footing;
    h.execute(&mut p, Command::AddElements(vec![s.clone()])).unwrap();
    assert_eq!(
        p.element(s.id).unwrap().predefined_type,
        Some(PredefinedType::StripFooting)
    );
}

/// IFX-024, ELM-001: SetToolDefaults checks a slot's type against its
/// class like an element's, and new elements of the tool take the slot's
/// type.
#[test]
fn tool_defaults_reject_a_type_the_class_does_not_offer() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let before = p.clone();

    let mut floor_wall = p.tool_defaults.clone();
    floor_wall.get_mut(KindTag::Wall).unwrap().predefined_type = Some(PredefinedType::Floor);
    let message = invalid(h.execute(&mut p, Command::SetToolDefaults(floor_wall)));
    assert_eq!(message.key(), "model-error-default-invalid");
    assert_eq!(message.arg("kind"), Some(&Arg::Kind(KindTag::Wall)));
    match message.arg("reason") {
        Some(Arg::Nested(reason)) => assert_type_not_allowed(reason, PredefinedType::Floor, IfcClass::Wall),
        other => panic!("expected a nested reason, got {other:?}"),
    }
    assert!(same_model(&p, &before));

    let mut base_slab = p.tool_defaults.clone();
    base_slab.get_mut(KindTag::Slab).unwrap().predefined_type = Some(PredefinedType::BaseSlab);
    h.execute(&mut p, Command::SetToolDefaults(base_slab.clone())).unwrap();
    assert_eq!(p.tool_defaults, base_slab);
    assert_eq!(slab(&p).predefined_type, Some(PredefinedType::BaseSlab));
    assert_eq!(wall(&p, (0.0, 0.0), (1.0, 0.0)).predefined_type, None);
    h.undo(&mut p).unwrap();
    assert!(same_model(&p, &before));
}
