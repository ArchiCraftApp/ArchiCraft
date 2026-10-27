//! The IFC class vocabulary (ELM-011, Table 5-1, Appendix D.5): the classes,
//! their codes and names, the class sets each kind selects from and the
//! classes of the parts of compound elements.

use archicraft_model::message::ALL_KEYS;
use archicraft_model::{CompoundPart, IfcClass, KindTag};

/// Appendix D.5, verbatim: the classes selectable for free-form solids,
/// objects and imported elements, `IfcBuildingElementProxy` (the default)
/// first.
const D5: [&str; 20] = [
    "IfcBuildingElementProxy",
    "IfcWall",
    "IfcSlab",
    "IfcRoof",
    "IfcColumn",
    "IfcBeam",
    "IfcMember",
    "IfcPlate",
    "IfcCovering",
    "IfcFooting",
    "IfcPile",
    "IfcRailing",
    "IfcStair",
    "IfcRamp",
    "IfcChimney",
    "IfcShadingDevice",
    "IfcCurtainWall",
    "IfcFurniture",
    "IfcSpace",
    "IfcGeographicElement",
];

/// The class column of Table 5-1 for elements and compound parts, in table
/// order and without repeats. The spatial structure (`IfcProject`,
/// `IfcSite`, `IfcBuilding`, `IfcBuildingStorey`) is not an element class,
/// and materials and types carry none of their own.
const TABLE_5_1: [&str; 21] = [
    "IfcWall",
    "IfcSlab",
    "IfcColumn",
    "IfcBeam",
    "IfcFooting",
    "IfcRoof",
    "IfcOpeningElement",
    "IfcDoor",
    "IfcWindow",
    "IfcCurtainWall",
    "IfcMember",
    "IfcPlate",
    "IfcStair",
    "IfcStairFlight",
    "IfcRailing",
    "IfcBuildingElementProxy",
    "IfcFurniture",
    "IfcGeographicElement",
    "IfcSpace",
    "IfcGrid",
    "IfcAnnotation",
];

fn codes(classes: &[IfcClass]) -> Vec<&'static str> {
    classes.iter().map(|c| c.code()).collect()
}

/// ELM-011: `SELECTABLE_D5` is the Appendix D.5 list in its order, proxy
/// first.
#[test]
fn selectable_d5_is_the_appendix_d5_list() {
    assert_eq!(codes(&IfcClass::SELECTABLE_D5), D5);
}

/// ELM-011, Table 5-1: `ALL` holds exactly the D.5 classes and the element
/// and part classes of Table 5-1, each once.
#[test]
fn all_covers_appendix_d5_and_table_5_1() {
    let all = codes(&IfcClass::ALL);
    let mut expected: Vec<&str> = D5.to_vec();
    expected.extend(TABLE_5_1.iter().filter(|c| !D5.contains(c)));
    let mut sorted_all = all.clone();
    sorted_all.sort_unstable();
    expected.sort_unstable();
    assert_eq!(sorted_all, expected);
    sorted_all.dedup();
    assert_eq!(sorted_all.len(), all.len(), "{all:?}");
}

/// ELM-011, IFX-020: `code` and `from_code` round-trip for every class, and
/// every code is an IFC 4.3 class name.
#[test]
fn codes_round_trip_for_every_class() {
    for class in IfcClass::ALL {
        assert!(class.code().starts_with("Ifc"), "{class:?}");
        assert_eq!(IfcClass::from_code(class.code()), Some(class), "{class:?}");
    }
    assert_eq!(IfcClass::from_code("IfcBuildingStorey"), None);
    assert_eq!(IfcClass::from_code("ifcwall"), None);
}

/// NFR-L10N-001: every class has its own message key, and every key is a
/// model key (the app's catalogue test checks that each has an English
/// message).
#[test]
fn every_class_has_a_model_label_key() {
    let mut keys: Vec<&str> = IfcClass::ALL.iter().map(|c| c.label_key()).collect();
    for key in &keys {
        assert!(key.starts_with("model-ifc-class-"), "{key}");
        assert!(ALL_KEYS.contains(key), "{key} is not a model key");
    }
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), IfcClass::ALL.len());
}

/// ELM-011: the named class sets are the SRS's sub-bullets, and the wall and
/// slab descriptors select from theirs; every descriptor defaults to the
/// Table 5-1 class of its kind.
#[test]
fn elm_011_class_sets_match_the_srs() {
    use IfcClass::*;
    assert_eq!(IfcClass::WALL_CLASSES, [Wall, Footing]);
    assert_eq!(IfcClass::SLAB_CLASSES, [Slab, Footing, Covering]);
    assert_eq!(IfcClass::BEAM_CLASSES, [Beam, Member, Footing]);
    assert_eq!(IfcClass::COLUMN_CLASSES, [Column, Member]);
    assert_eq!(codes(&IfcClass::SELECTABLE_D5), D5);

    assert_eq!(KindTag::Wall.ifc_classes(), IfcClass::WALL_CLASSES);
    assert_eq!(KindTag::Slab.ifc_classes(), IfcClass::SLAB_CLASSES);
    assert_eq!(KindTag::Wall.descriptor().default_class, Wall);
    assert_eq!(KindTag::Slab.descriptor().default_class, Slab);
    #[cfg(feature = "test-kinds")]
    {
        // The test kind stands in for free-form solids and objects.
        assert_eq!(KindTag::Dummy.ifc_classes(), IfcClass::SELECTABLE_D5);
        assert_eq!(KindTag::Dummy.descriptor().default_class, BuildingElementProxy);
    }
    for &kind in KindTag::ALL {
        assert!(
            kind.ifc_classes().contains(&kind.descriptor().default_class),
            "{kind:?}"
        );
    }
}

/// IFX-016, Table 5-1: the parts of compound elements and their classes,
/// against the table's rows.
#[test]
fn compound_part_classes_are_the_table_5_1_part_rows() {
    let rows: Vec<(&str, &str, &str)> = IfcClass::COMPOUND_PART_CLASSES
        .iter()
        .map(|CompoundPart { compound, part, class }| (compound.code(), *part, class.code()))
        .collect();
    assert_eq!(
        rows,
        [
            ("IfcStair", "flight", "IfcStairFlight"),
            ("IfcStair", "landing", "IfcSlab"),
            ("IfcStair", "stringer", "IfcMember"),
            ("IfcRailing", "post", "IfcMember"),
            ("IfcRailing", "rail", "IfcMember"),
            ("IfcRailing", "infill-panel", "IfcPlate"),
            ("IfcCurtainWall", "frame", "IfcMember"),
            ("IfcCurtainWall", "panel", "IfcPlate"),
            ("IfcRoof", "plane", "IfcSlab"),
        ]
    );
}
