//! The per-kind attribute codecs (SRS 3.2, IFX-060, IFX-061): every
//! registered kind stores its parameters, reference lines and storey links
//! through its own codec under `alpha/kinds/`, and the format-level code
//! (schema, writer, reader) has no per-kind arm.

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use archicraft_ifcx::alpha::kinds::{PROJECT_FIELDS, Stored, fields, placeholders, registered};
use archicraft_ifcx::alpha::schema;
use archicraft_ifcx::validate::{self, Severity};
use archicraft_model::glam::DVec2;
use archicraft_model::kinds::{Dummy, Note};
use archicraft_model::{Command, Element, ElementKind, History, KindTag, ReferenceLine, StoreyId, TopLink, samples};
use serde_json::Value;

use common::{edit, node_index, save};

/// IFX-060, IFX-061: the codec table covers every kind of this build, no
/// codec lacks a kind, and the attribute ids are distinct, one per kind code
/// in the `archicraft::` namespace.
#[test]
fn every_kind_has_a_codec() {
    let codecs = registered();
    let tags: BTreeSet<KindTag> = codecs.iter().map(|(tag, _)| *tag).collect();
    assert_eq!(tags.len(), codecs.len(), "no two codecs share a kind: {codecs:?}");
    assert_eq!(
        tags,
        KindTag::ALL.iter().copied().collect(),
        "every kind of this build has a codec and every codec a kind: {codecs:?}"
    );
    let ids: BTreeSet<&str> = codecs.iter().map(|(_, attribute)| *attribute).collect();
    assert_eq!(ids.len(), codecs.len(), "attribute ids are unique: {codecs:?}");
    for (tag, attribute) in &codecs {
        assert_eq!(*attribute, format!("archicraft::{}", tag.code()));
    }
}

/// The declared members of `declaration`, an object value description.
fn members(declaration: &Value) -> BTreeSet<&str> {
    declaration["value"]["objectRestrictions"]["values"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect()
}

/// IFX-060: every top-level field of the project's serde form (without the
/// `#[serde(skip)]` fields) and of every kind's payload is mapped to the
/// `archicraft::` member or node that stores it, by the tables reachable
/// from `alpha/kinds/mod.rs`, and every member the tables name is declared.
/// A field added without a mapping, or a kind without a sample payload
/// here, fails the test. A kind that is not an IFCX node (ANN-010) declares
/// its members under its kind code in the entries of
/// `archicraft::annotations`, not as an attribute of its own. No codec is a
/// placeholder since columns are stored (format 6), and the text styles are
/// `textStyles` (ANN-007) and the profiles `profiles` of
/// `archicraft::project` (PRF-004).
#[test]
fn every_project_field_is_persisted() {
    let project = samples::storey_with_walls_and_slab();
    let serialized = serde_json::to_value(&project).unwrap();
    let fields_of_project: BTreeSet<&str> = serialized.as_object().unwrap().keys().map(String::as_str).collect();
    assert!(!fields_of_project.contains("revision"), "not saved, not mapped");
    let mapped: BTreeSet<&str> = PROJECT_FIELDS.iter().map(|(field, _)| *field).collect();
    assert_eq!(fields_of_project, mapped, "Project's fields and PROJECT_FIELDS");
    assert_eq!(PROJECT_FIELDS.len(), mapped.len(), "each field once");
    let declared = schema::declarations(KindTag::ALL, true);
    let project_members = members(&declared["archicraft::project"]);
    for (field, stored) in PROJECT_FIELDS {
        assert!(!stored.is_empty(), "{field} is stored somewhere");
        for place in *stored {
            if let Stored::Member(path) = place {
                let member = path.split('.').next().unwrap();
                assert!(project_members.contains(member), "{field}: {member} is declared");
            }
        }
    }
    let text_styles = PROJECT_FIELDS.iter().find(|(field, _)| *field == "text_styles");
    assert_eq!(text_styles, Some(&("text_styles", &[Stored::Member("textStyles")][..])));
    let profiles = PROJECT_FIELDS.iter().find(|(field, _)| *field == "profiles");
    assert_eq!(profiles, Some(&("profiles", &[Stored::Member("profiles")][..])));

    let mut payloads: BTreeMap<KindTag, Value> = BTreeMap::new();
    for element in project.elements.values() {
        payloads
            .entry(element.tag())
            .or_insert_with(|| serde_json::to_value(&element.kind).unwrap());
    }
    let dummy: ElementKind = Dummy::new(DVec2::ZERO, DVec2::ONE, project.materials[0].id).into();
    payloads.insert(KindTag::Dummy, serde_json::to_value(&dummy).unwrap());
    let note: ElementKind = Note::new(project.storeys.ground(), DVec2::ONE, "Hall").into();
    payloads.insert(KindTag::Note, serde_json::to_value(&note).unwrap());
    let space = project.new_space(
        project.storeys.ground(),
        vec![DVec2::ZERO, DVec2::X, DVec2::ONE],
        Vec::new(),
        0,
    );
    payloads.insert(KindTag::Space, serde_json::to_value(&space.kind).unwrap());
    let text = project.new_text(project.storeys.ground(), DVec2::ONE, "Hall", 0);
    payloads.insert(KindTag::Text, serde_json::to_value(&text.kind).unwrap());
    let column = project.new_column(project.storeys.ground(), DVec2::ONE, 0.0, 0);
    payloads.insert(KindTag::Column, serde_json::to_value(&column.kind).unwrap());
    assert_eq!(
        payloads.keys().copied().collect::<BTreeSet<_>>(),
        KindTag::ALL.iter().copied().collect(),
        "a sample payload of every kind"
    );
    assert_eq!(placeholders(), [], "the kinds that store nothing yet");
    let tables = fields();
    assert_eq!(tables.len(), KindTag::ALL.len());
    for (tag, table) in tables {
        if placeholders().contains(&tag) {
            assert!(table.is_empty(), "{tag:?}: a placeholder maps no field");
            assert!(declared.get(attribute_of(tag)).is_none(), "{tag:?} is not declared");
            continue;
        }
        // Externally tagged: `{"Wall": {…}}`.
        let payload = payloads[&tag].as_object().unwrap().values().next().unwrap();
        let fields_of_payload: BTreeSet<&str> = payload.as_object().unwrap().keys().map(String::as_str).collect();
        let mapped: BTreeSet<&str> = table.iter().map(|(field, _)| *field).collect();
        assert_eq!(fields_of_payload, mapped, "{tag:?}: the payload's fields and FIELDS");
        assert_eq!(table.len(), mapped.len(), "{tag:?}: each field once");
        let declared = if tag.descriptor().ifcx_node {
            assert!(
                declared.get(attribute_of(tag)).is_some(),
                "{tag:?}: a node kind declares its attribute"
            );
            members(&declared[attribute_of(tag)])
        } else {
            assert!(
                declared.get(attribute_of(tag)).is_none(),
                "{tag:?}: an annotation kind has no attribute of its own"
            );
            let entry = &declared["archicraft::annotations"]["value"]["arrayRestrictions"]["value"];
            let member = &entry["objectRestrictions"]["values"][tag.code()];
            assert_eq!(member["optional"], true, "{tag:?}: one kind's members per entry");
            member["objectRestrictions"]["values"]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect()
        };
        for (field, stored) in table {
            assert!(!stored.is_empty(), "{tag:?}.{field} names a member");
            for member in *stored {
                assert!(declared.contains(member), "{tag:?}.{field}: {member} is declared");
            }
        }
    }
}

/// IFX-061: the declarations cover the kinds the project stores (an
/// element, a defaults slot or a non-zero counter) and nothing else, so a
/// test build writes a project without the test kind exactly as a release
/// build does: a project with walls only declares `archicraft::wall` but not
/// `archicraft::slab`, its kind enumeration is `["wall"]`, its `toolDefaults`
/// and `counters` know only the wall, and it validates without problems.
#[test]
fn declarations_cover_only_the_kinds_the_project_stores() {
    let mut b = samples::FixtureBuilder::new("walls-only");
    let ground = b.storey(0);
    b.wall(ground, DVec2::ZERO, DVec2::new(5.0, 0.0));
    b.wall(ground, DVec2::new(5.0, 0.0), DVec2::new(5.0, 4.0));
    let mut project = b.finish();
    project.tool_defaults = project
        .tool_defaults
        .iter()
        .filter(|(kind, _)| *kind == KindTag::Wall)
        .map(|(_, slot)| slot.clone())
        .collect();
    assert_eq!(project.counters.get(KindTag::Slab), 0);

    let bytes = save(&project);
    let parsed = validate::parse(&bytes).unwrap();
    assert!(
        validate::validate(&parsed).is_empty(),
        "{:#?}",
        validate::validate(&parsed)
    );
    let file: Value = serde_json::from_slice(&bytes).unwrap();
    let schemas = file["schemas"].as_object().unwrap();
    let own: Vec<&String> = schemas.keys().filter(|k| k.starts_with("archicraft::")).collect();
    assert_eq!(
        own,
        [
            "archicraft::project",
            "archicraft::storey",
            "archicraft::element",
            "archicraft::wall"
        ]
    );
    let element = &schemas["archicraft::element"]["value"]["objectRestrictions"]["values"];
    assert_eq!(
        element["kind"]["enumRestrictions"]["options"],
        serde_json::json!(["wall"])
    );
    let project_members = &schemas["archicraft::project"]["value"]["objectRestrictions"]["values"];
    let slots: Vec<&String> = project_members["toolDefaults"]["objectRestrictions"]["values"]
        .as_object()
        .unwrap()
        .keys()
        .collect();
    assert_eq!(slots, ["wall"]);
    let counters: Vec<&String> = project_members["counters"]["objectRestrictions"]["values"]
        .as_object()
        .unwrap()
        .keys()
        .collect();
    assert_eq!(counters, ["wall", "group"]);

    // The samples store walls and slabs, and every project has the space,
    // column and text tools' defaults slots, so the goldens declare all
    // five, the texts' members in the annotation entries
    // (golden_files_are_reproduced).
    for sample in common::samples() {
        let file: Value = serde_json::from_slice(&save(&sample)).unwrap();
        let own: Vec<&String> = file["schemas"]
            .as_object()
            .unwrap()
            .keys()
            .filter(|k| k.starts_with("archicraft::"))
            .collect();
        assert_eq!(
            own,
            [
                "archicraft::project",
                "archicraft::storey",
                "archicraft::element",
                "archicraft::wall",
                "archicraft::slab",
                "archicraft::space",
                "archicraft::column",
                "archicraft::annotations"
            ]
        );
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
/// writer and reader, and the annotation entries of the kinds that are not
/// IFCX nodes (ANN-010), reach a kind's parameters only through its codec
/// under `alpha/kinds/`, so a new kind is one codec module plus one line in
/// the codec list.
#[test]
fn format_code_has_no_per_kind_arm() {
    let alpha = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/alpha");
    for file in [
        "kinds/mod.rs",
        "kinds/common.rs",
        "kinds/wall.rs",
        "kinds/slab.rs",
        "kinds/column.rs",
        "kinds/dummy.rs",
        "kinds/note.rs",
        "kinds/annotations.rs",
    ] {
        assert!(alpha.join(file).is_file(), "src/alpha/{file} exists");
    }
    let patterns = [
        "ElementKind::Wall",
        "ElementKind::Slab",
        "KindTag::Wall",
        "KindTag::Slab",
        "\"archicraft::wall\"",
        "\"archicraft::slab\"",
        "ElementKind::Column",
        "KindTag::Column",
        "\"archicraft::column\"",
        "ElementKind::Note",
        "KindTag::Note",
        "\"archicraft::note\"",
        "\"note\"",
    ];
    for file in ["writer.rs", "reader.rs", "schema.rs", "kinds/annotations.rs"] {
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
