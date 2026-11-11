//! The test kind's codec (SRS 3.2, IFX-060, ELM-001, ELM-019), which the
//! dev-dependency on this crate with `test-kinds` provides: a dummy element
//! round-trips through `archicraft::dummy` like a wall through
//! `archicraft::wall`, its name counter follows the rule of a kind without a
//! tool, and its tool-defaults slot is written through the codec although
//! no template has one, so the reader treats it as a tool it does not know.

#![allow(clippy::unwrap_used)]

mod common;

use archicraft_ifcx::validate::{self, Severity};
use archicraft_model::glam::DVec2;
use archicraft_model::kinds::{Dummy, VerticalLink};
use archicraft_model::project::DefaultsSlot;
use archicraft_model::{
    Command, ElementId, History, KindTag, Position, Project, StoreyId, StructuralFunction, TopLink, samples,
};
use serde_json::Value;

use common::{edit, node_index, save};

/// The two-storey shell with one dummy per top link (fixed, storey,
/// relative, each with a non-zero offset) added on the ground floor, and the
/// ids of the added elements.
fn shell_with_a_dummy_per_top_link() -> (Project, Vec<ElementId>, StoreyId) {
    let mut p = samples::two_storey_shell();
    let ground = p.storeys.ground();
    let upper = p.storeys.relative(ground, 1).unwrap().id;
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
    let elements: Vec<_> = tops
        .iter()
        .enumerate()
        .map(|(i, top)| {
            let mut d = Dummy::new(
                DVec2::new(20.0 + 3.0 * i as f64, 1.0),
                DVec2::new(2.0, 1.5),
                p.materials[0].id,
            );
            d.link = VerticalLink {
                base_offset: 0.05 * (i + 1) as f64,
                top: *top,
            };
            d.into_element(&p, ground, i as u32)
        })
        .collect();
    let ids = elements.iter().map(|e| e.id).collect();
    History::default()
        .execute(&mut p, Command::AddElements(elements))
        .unwrap();
    (p, ids, upper)
}

fn project_attribute(file: &Value) -> &serde_json::Map<String, Value> {
    file["data"][0]["attributes"]["archicraft::project"]
        .as_object()
        .unwrap()
}

/// The declared members of `declaration`, an object value description.
fn members(declaration: &Value) -> &serde_json::Map<String, Value> {
    declaration["objectRestrictions"]["values"].as_object().unwrap()
}

fn project_declaration(file: &Value) -> &Value {
    &file["schemas"]["archicraft::project"]["value"]
}

/// IFX-060, IFX-064: dummies with each top link (fixed, storey, relative,
/// each with a non-zero offset) round-trip through save and load equal and
/// without problems, through the codec that the `test-kinds` feature adds.
#[test]
fn a_dummy_element_with_each_top_link_round_trips() {
    let (project, ids, _) = shell_with_a_dummy_per_top_link();
    let loaded = archicraft_ifcx::load(&save(&project)).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert_eq!(ids.len(), 3);
    for id in ids {
        let original = project.element(id).unwrap();
        let read = loaded.project.element(id).unwrap();
        assert_eq!(read, original, "{} survives bit for bit", original.name);
    }
}

/// IFX-060, IFX-061, ELM-019: a dummy element's node carries
/// `archicraft::element` and `archicraft::dummy` with the codec's members
/// (the rectangle, the vertical link, the flags, the material); the file
/// declares the attribute with these members and the optional `profile`,
/// which a dummy without a profile leaves out (PRF-004), lists the kind in
/// the kind enumeration and declares the counter of this kind without a
/// tool as optional, which the writer writes once it is non-zero.
#[test]
fn a_dummy_element_is_written_through_its_codec_and_declared() {
    let (project, ids, upper) = shell_with_a_dummy_per_top_link();
    let bytes = save(&project);
    let file: Value = serde_json::from_slice(&bytes).unwrap();
    let linked = ids
        .iter()
        .map(|id| project.element(*id).unwrap())
        .find(|e| matches!(e.as_dummy().unwrap().link.top, TopLink::Storey { .. }))
        .unwrap();
    let node = &file["data"][node_index(&file, &linked.name)];
    let attributes = node["attributes"].as_object().unwrap();
    let own: Vec<&String> = attributes.keys().filter(|k| k.starts_with("archicraft::")).collect();
    assert_eq!(own, ["archicraft::element", "archicraft::dummy"]);
    assert_eq!(attributes["archicraft::element"]["kind"], "dummy");
    assert_eq!(attributes["bsi::ifc::class"]["code"], "IfcBuildingElementProxy");
    let written: Vec<&String> = attributes["archicraft::dummy"].as_object().unwrap().keys().collect();
    assert_eq!(
        written,
        [
            "origin",
            "size",
            "baseOffset",
            "top",
            "parts",
            "panicOnEvaluate",
            "siteLevel",
            "material"
        ]
    );
    let dummy = &attributes["archicraft::dummy"];
    assert_eq!(dummy["top"]["link"], "storey");
    assert_eq!(dummy["top"]["storey"], upper.to_string());
    assert_eq!(dummy["parts"], false);

    let declared = file["schemas"].as_object().unwrap();
    assert!(declared.contains_key("archicraft::dummy"), "{:?}", declared.keys());
    let mut members_written: Vec<&str> = written.iter().map(|k| k.as_str()).collect();
    members_written.push("profile");
    assert_eq!(
        members(&declared["archicraft::dummy"]["value"])
            .keys()
            .collect::<Vec<_>>(),
        members_written
    );
    assert_eq!(
        members(&declared["archicraft::dummy"]["value"])["profile"],
        serde_json::json!({"dataType": "String", "optional": true})
    );
    let kinds = &members(&declared["archicraft::element"]["value"])["kind"]["enumRestrictions"]["options"];
    assert_eq!(kinds, &serde_json::json!(["wall", "slab", "space", "dummy"]));
    let counters = &members(project_declaration(&file))["counters"];
    assert_eq!(
        members(counters)["dummy"],
        serde_json::json!({"dataType": "Integer", "optional": true})
    );
    assert_eq!(members(counters)["wall"], serde_json::json!({"dataType": "Integer"}));
    assert_eq!(project_attribute(&file)["counters"]["dummy"], Value::from(3));

    let parsed = validate::parse(&bytes).unwrap();
    assert!(
        validate::validate(&parsed).is_empty(),
        "{:#?}",
        validate::validate(&parsed)
    );
}

/// ELM-019, IFX-061: a kind without a tool and without elements has no
/// counter in the file and none declared, and reading the file is not a
/// problem.
#[test]
fn a_missing_counter_of_a_kind_without_a_tool_reads_as_zero() {
    let p = samples::storey_with_walls_and_slab();
    let file: Value = serde_json::from_slice(&save(&p)).unwrap();
    let written = project_attribute(&file)["counters"].as_object().unwrap();
    assert!(!written.contains_key("dummy"), "{written:?}");
    let declared = members(&members(project_declaration(&file))["counters"]);
    assert_eq!(
        declared.keys().collect::<Vec<_>>(),
        ["wall", "slab", "space", "text", "group"]
    );
    let loaded = archicraft_ifcx::load(&save(&p)).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert_eq!(loaded.project.counters.get(KindTag::Dummy), 0);
    assert_eq!(loaded.project.counters, p.counters);
}

/// IFX-083: an element whose kind names a codec but whose node lacks the
/// codec's attribute is skipped and reported.
#[test]
fn an_element_without_its_kinds_attribute_is_skipped_and_reported() {
    let p = samples::storey_with_walls_and_slab();
    let bytes = edit(&p, |v| {
        let i = node_index(v, "W-002");
        v["data"][i]["attributes"]["archicraft::element"]["kind"] = "dummy".into();
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert_eq!(loaded.project.elements.len(), p.elements.len() - 1);
    assert!(
        loaded.problems.iter().any(|p| p.severity == Severity::Error
            && p.message.to_string().contains("W-002")
            && p.message.to_string().contains("ifcx-parameters-missing(kind=dummy)")),
        "{:#?}",
        loaded.problems
    );
}

/// ELM-001, IFX-061: the defaults slot of a kind without template defaults
/// is written through its codec (the slot's members, the flags among them,
/// then the common members), declared, and valid; the reader, whose slots
/// come from the template, reads it back as a tool it does not know, as any
/// unknown member, and keeps the other slots.
#[test]
fn tool_defaults_of_a_kind_without_template_defaults_are_written_and_read_back_as_unknown() {
    let mut p = samples::storey_with_walls_and_slab();
    let mut defaults = p.tool_defaults.clone();
    let mut dummy = Dummy::new(DVec2::ZERO, DVec2::ONE, p.materials[0].id);
    dummy.parts = true;
    dummy.site_level = true;
    defaults.insert(DefaultsSlot {
        layer: p.layers[0].id,
        position: Position::Undefined,
        structural: StructuralFunction::LoadBearing,
        ifc_class: KindTag::Dummy.descriptor().default_class,
        predefined_type: None,
        kind: dummy.into(),
    });
    History::default()
        .execute(&mut p, Command::SetToolDefaults(defaults))
        .unwrap();
    let bytes = save(&p);
    let file: Value = serde_json::from_slice(&bytes).unwrap();
    let slots = project_attribute(&file)["toolDefaults"].as_object().unwrap();
    assert_eq!(
        slots.keys().collect::<Vec<_>>(),
        ["wall", "slab", "space", "text", "dummy"]
    );
    let slot = slots["dummy"].as_object().unwrap();
    // IFX-062: the common members, then the kind's parameters apart.
    assert_eq!(
        slot.keys().collect::<Vec<_>>(),
        ["layer", "position", "structuralFunction", "ifcClass", "parameters"]
    );
    let parameters = slot["parameters"].as_object().unwrap();
    assert_eq!(
        parameters.keys().collect::<Vec<_>>(),
        ["baseOffset", "top", "parts", "panicOnEvaluate", "siteLevel", "material"]
    );
    assert_eq!(slot["structuralFunction"], "load-bearing");
    assert_eq!(parameters["parts"], true);
    assert_eq!(parameters["panicOnEvaluate"], false);
    assert_eq!(parameters["siteLevel"], true);
    let declared = members(&members(project_declaration(&file))["toolDefaults"]);
    assert_eq!(
        declared.keys().collect::<Vec<_>>(),
        ["wall", "slab", "space", "text", "dummy"]
    );
    let declared_slot = members(&declared["dummy"]);
    assert_eq!(
        declared_slot.keys().collect::<Vec<_>>(),
        [
            "layer",
            "position",
            "structuralFunction",
            "ifcClass",
            "predefinedType",
            "parameters"
        ]
    );
    assert_eq!(
        members(&declared_slot["parameters"]).keys().collect::<Vec<_>>(),
        parameters.keys().collect::<Vec<_>>()
    );
    // The slot makes the kind present: its counter is declared, optional,
    // and not written while it is zero.
    let counters = members(&members(project_declaration(&file))["counters"]);
    assert_eq!(
        counters.keys().collect::<Vec<_>>(),
        ["wall", "slab", "space", "text", "dummy", "group"]
    );
    assert!(
        !project_attribute(&file)["counters"]
            .as_object()
            .unwrap()
            .contains_key("dummy")
    );
    let parsed = validate::parse(&bytes).unwrap();
    assert!(
        validate::validate(&parsed).is_empty(),
        "{:#?}",
        validate::validate(&parsed)
    );

    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let unknown: Vec<&archicraft_ifcx::Problem> = loaded
        .problems
        .iter()
        .filter(|p| p.message.to_string().contains(r#"ifcx-tool-unknown(key="dummy")"#))
        .collect();
    assert_eq!(unknown.len(), 1, "{:#?}", loaded.problems);
    assert_eq!(unknown[0].severity, Severity::Warning);
    assert_eq!(loaded.problems.len(), 1, "{:#?}", loaded.problems);
    assert!(loaded.project.tool_defaults.get(KindTag::Dummy).is_none());
    assert_eq!(
        loaded.project.tool_defaults.get(KindTag::Wall),
        p.tool_defaults.get(KindTag::Wall)
    );
    assert_eq!(
        loaded.project.tool_defaults.get(KindTag::Slab),
        p.tool_defaults.get(KindTag::Slab)
    );
}

/// The ids of the material nodes in `file`: the nodes with a diffuse colour
/// and no mesh (a body without a material carries its own colour).
fn material_nodes(file: &Value) -> Vec<String> {
    file["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["attributes"].get("bsi::ifc::presentation::diffuseColor").is_some())
        .filter(|n| n["attributes"].get("usd::usdgeom::mesh").is_none())
        .map(|n| n["path"].as_str().unwrap().to_owned())
        .collect()
}

/// SPC-001, ANN-007 (preparation): a project with a test element and a
/// defaults slot without a material round-trips its elements equal; the
/// element's Body has no material inherit but a translucent presentation of
/// its own, its attribute and the slot no material member, the written
/// material list equals that of the project without them and the file is
/// valid. The only problem is the slot's
/// unknown tool, as for any slot of a kind without template defaults; a
/// material-less slot read through a template is tested in the reader.
#[test]
fn a_material_less_dummy_round_trips_without_a_material() {
    let plain = samples::storey_with_walls_and_slab();
    let mut p = plain.clone();
    let ground = p.storeys.ground();
    let dummy = Dummy::without_material(DVec2::new(20.0, 1.0), DVec2::new(2.0, 1.5)).into_element(&p, ground, 0);
    let (id, name) = (dummy.id, dummy.name.clone());
    let mut defaults = p.tool_defaults.clone();
    defaults.insert(DefaultsSlot {
        layer: p.layers[0].id,
        position: Position::Undefined,
        structural: StructuralFunction::Undefined,
        ifc_class: KindTag::Dummy.descriptor().default_class,
        predefined_type: None,
        kind: Dummy::without_material(DVec2::ZERO, DVec2::ONE).into(),
    });
    let mut h = History::default();
    h.execute(&mut p, Command::AddElements(vec![dummy])).unwrap();
    h.execute(&mut p, Command::SetToolDefaults(defaults)).unwrap();

    let bytes = save(&p);
    let file: Value = serde_json::from_slice(&bytes).unwrap();
    let node = &file["data"][node_index(&file, &name)];
    assert!(
        !node["attributes"]["archicraft::dummy"]
            .as_object()
            .unwrap()
            .contains_key("material")
    );
    let body_path = node["children"]["Body"].as_str().unwrap();
    let body = file["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["path"] == body_path)
        .unwrap();
    assert!(body["attributes"].get("usd::usdgeom::mesh").is_some());
    assert!(
        body.get("inherits").is_none_or(|i| i.get("material").is_none()),
        "{body}"
    );
    assert_eq!(body["attributes"]["bsi::ifc::presentation::opacity"], 0.25, "{body}");
    let plain_file: Value = serde_json::from_slice(&save(&plain)).unwrap();
    assert_eq!(material_nodes(&file), material_nodes(&plain_file));
    let slot = project_attribute(&file)["toolDefaults"]["dummy"].as_object().unwrap();
    assert!(!slot.contains_key("material"), "{slot:?}");
    assert!(
        !slot["parameters"].as_object().unwrap().contains_key("material"),
        "{slot:?}"
    );
    let parsed = validate::parse(&bytes).unwrap();
    assert!(
        validate::validate(&parsed).is_empty(),
        "{:#?}",
        validate::validate(&parsed)
    );

    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let messages: Vec<String> = loaded.problems.iter().map(|p| p.message.to_string()).collect();
    assert_eq!(messages.len(), 1, "{messages:#?}");
    assert!(
        messages[0].contains(r#"ifcx-tool-unknown(key="dummy")"#),
        "{messages:#?}"
    );
    assert!(!messages.iter().any(|m| m.contains("material")), "{messages:#?}");
    assert_eq!(loaded.project.element(id), p.element(id));
    assert_eq!(loaded.project.elements, p.elements);
}
