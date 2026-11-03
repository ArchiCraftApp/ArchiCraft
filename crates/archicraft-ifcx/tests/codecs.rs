//! The per-kind attribute codecs (SRS 3.2, IFX-060, IFX-061): every
//! registered kind stores its parameters, reference lines and storey links
//! through its own codec under `alpha/kinds/`, and the format-level code
//! (schema, writer, reader) has no per-kind arm.

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use archicraft_ifcx::alpha::kinds::registered;
use archicraft_ifcx::validate::Severity;
use archicraft_model::glam::DVec2;
use archicraft_model::{Command, Element, History, KindTag, ReferenceLine, StoreyId, TopLink, samples};
use serde_json::Value;

use common::{edit, node_index, save};

/// IFX-060, IFX-061: the registered codecs name distinct kinds and distinct
/// attribute ids in the `archicraft::` namespace, one per kind code. Walls
/// and slabs are among them.
#[test]
fn every_kind_has_a_codec() {
    let codecs = registered();
    let tags: BTreeSet<KindTag> = codecs.iter().map(|(tag, _)| *tag).collect();
    assert_eq!(tags.len(), codecs.len(), "no two codecs share a kind: {codecs:?}");
    let ids: BTreeSet<&str> = codecs.iter().map(|(_, attribute)| *attribute).collect();
    assert_eq!(ids.len(), codecs.len(), "attribute ids are unique: {codecs:?}");
    for (tag, attribute) in &codecs {
        assert_eq!(*attribute, format!("archicraft::{}", tag.code()));
    }
    for kind in [KindTag::Wall, KindTag::Slab] {
        assert!(tags.contains(&kind), "{kind:?} has a codec: {codecs:?}");
    }
}

/// The attribute of `kind`'s codec.
fn attribute_of(kind: KindTag) -> &'static str {
    registered()
        .into_iter()
        .find(|(tag, _)| *tag == kind)
        .unwrap_or_else(|| panic!("{kind:?} has a codec"))
        .1
}

/// The two-storey shell with one wall per combination of reference line
/// (WAL-003) and top link (ELM-006) added on the ground floor, and the ids
/// of the added walls.
fn shell_with_every_wall_variant() -> (archicraft_model::Project, Vec<archicraft_model::ElementId>, StoreyId) {
    let mut project = samples::two_storey_shell();
    let ground = project.storeys.ground();
    let upper = project.storeys.relative(ground, 1).unwrap().id;
    let lines = [
        ReferenceLine::Outside,
        ReferenceLine::Center,
        ReferenceLine::Inside,
        ReferenceLine::Custom(0.07),
    ];
    let tops = [
        TopLink::Fixed { height: 2.8 },
        TopLink::Storey {
            storey: upper,
            offset: 0.35,
        },
        TopLink::Relative {
            storeys: 1,
            offset: -0.15,
        },
    ];
    let mut walls: Vec<Element> = Vec::new();
    for (i, (line, top)) in lines
        .iter()
        .flat_map(|l| tops.iter().map(move |t| (*l, *t)))
        .enumerate()
    {
        let y = 20.0 + 2.0 * i as f64;
        let mut wall = project.new_wall(ground, DVec2::new(0.0, y), DVec2::new(4.0, y), i as u32);
        let w = wall.as_wall_mut().unwrap();
        w.reference_line = line;
        w.top = top;
        w.base_offset = 0.05 * i as f64;
        walls.push(wall);
    }
    let ids = walls.iter().map(|w| w.id).collect();
    History::default()
        .execute(&mut project, Command::AddElements(walls))
        .unwrap();
    (project, ids, upper)
}

/// IFX-060, IFX-064: walls with each reference line (outside, center,
/// inside, custom with an offset) and each top link (fixed, storey, relative,
/// each with a non-zero offset) round-trip through save and load equal and
/// without problems.
#[test]
fn walls_with_each_reference_line_and_top_link_round_trip() {
    let (project, ids, _) = shell_with_every_wall_variant();
    let loaded = archicraft_ifcx::load(&save(&project)).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert_eq!(ids.len(), 12);
    for id in ids {
        let original = project.element(id).unwrap();
        let read = loaded.project.element(id).unwrap();
        assert_eq!(read, original, "{} survives bit for bit", original.name);
    }
}

/// IFX-060: every element node carries `archicraft::element` and exactly the
/// attribute of its kind's codec, where the codec writes the parameters and
/// the storey links (a wall's `top.storey` names the linked storey).
#[test]
fn every_element_is_written_through_its_kinds_codec() {
    let (project, ids, upper) = shell_with_every_wall_variant();
    let file: Value = serde_json::from_slice(&save(&project)).unwrap();
    for element in project.elements.values() {
        let node = &file["data"][node_index(&file, &element.name)];
        let attributes = node["attributes"].as_object().unwrap();
        let own: Vec<&String> = attributes.keys().filter(|k| k.starts_with("archicraft::")).collect();
        assert_eq!(
            own,
            ["archicraft::element", attribute_of(element.tag())],
            "{}: the common data and the codec's attribute, nothing else",
            element.name
        );
    }
    let linked = ids
        .iter()
        .map(|id| project.element(*id).unwrap())
        .find(|e| matches!(e.as_wall().unwrap().top, TopLink::Storey { .. }))
        .unwrap();
    let node = &file["data"][node_index(&file, &linked.name)];
    let top = &node["attributes"][attribute_of(KindTag::Wall)]["top"];
    assert_eq!(top["link"], "storey");
    assert_eq!(top["storey"], upper.to_string());
}

/// IFX-083, PRJ-008: a wall node without `thickness` loads with 0.25 m, the
/// repair reported once against the wall as a warning, through the codec's
/// lenient reading. Validation reports the missing member once more
/// (IFX-005), against the attribute, not the element.
#[test]
fn a_wall_without_thickness_loads_with_a_quarter_metre_and_one_warning() {
    let project = samples::storey_with_walls_and_slab();
    let bytes = edit(&project, |v| {
        let i = node_index(v, "W-001");
        v["data"][i]["attributes"]["archicraft::wall"]
            .as_object_mut()
            .unwrap()
            .remove("thickness");
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let wall = loaded
        .project
        .elements
        .values()
        .find(|e| e.name == "W-001")
        .unwrap()
        .as_wall()
        .unwrap();
    assert_eq!(wall.thickness, 0.25);
    assert_eq!(loaded.project.elements.len(), project.elements.len());
    let messages: Vec<String> = loaded.problems.iter().map(|p| p.message.to_string()).collect();
    let repairs: Vec<&archicraft_ifcx::Problem> = loaded
        .problems
        .iter()
        .filter(|p| p.message.to_string().contains("ifcx-record-element"))
        .collect();
    assert_eq!(repairs.len(), 1, "one report against the wall: {messages:#?}");
    assert_eq!(repairs[0].severity, Severity::Warning, "{messages:#?}");
    let repair = repairs[0].message.to_string();
    assert!(
        repair.starts_with(r#"ifcx-repaired(record=ifcx-record-element(name="W-001")"#)
            && repair.contains(r#"ifcx-field-missing(key="thickness")"#),
        "{repair}"
    );
    assert_eq!(messages.len(), 2, "that and the validator's: {messages:#?}");
    assert!(
        messages[0].starts_with(r#"ifcx-attribute-invalid(id="archicraft::wall""#),
        "{messages:#?}"
    );
}

/// SRS 3.2, IFX-060: the format-level code has no per-kind arm. The schema,
/// writer and reader reach a kind's parameters only through its codec under
/// `alpha/kinds/`, so a new kind is one codec module plus one line in the
/// codec list.
#[test]
fn format_code_has_no_per_kind_arm() {
    let alpha = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/alpha");
    for file in ["kinds/mod.rs", "kinds/common.rs", "kinds/wall.rs", "kinds/slab.rs"] {
        assert!(alpha.join(file).is_file(), "src/alpha/{file} exists");
    }
    let patterns = [
        "ElementKind::Wall",
        "ElementKind::Slab",
        "KindTag::Wall",
        "KindTag::Slab",
        "\"archicraft::wall\"",
        "\"archicraft::slab\"",
    ];
    for file in ["writer.rs", "reader.rs", "schema.rs"] {
        let text = fs::read_to_string(alpha.join(file)).unwrap();
        for (n, line) in text.lines().enumerate() {
            for pattern in patterns {
                assert!(
                    !line.contains(pattern),
                    "src/alpha/{file}:{}: `{pattern}` in {line:?}",
                    n + 1
                );
            }
        }
    }
}
