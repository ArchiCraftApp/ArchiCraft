//! Element kinds this format version cannot store (SRS 3.2, ELM-019,
//! IFX-060), with the test kind that the dev-dependency on archicraft-model
//! with `test-kinds` provides: the writer saves such an element without its
//! kind and parameters and warns, the name counter of a kind without a tool
//! is written only when it is non-zero, and the reader skips elements of a
//! kind it cannot read.

#![allow(clippy::unwrap_used)]

mod common;

use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::validate::{self, Severity};
use archicraft_model::glam::DVec2;
use archicraft_model::kinds::Dummy;
use archicraft_model::project::DefaultsSlot;
use archicraft_model::{Command, History, KindTag, Position, Project, samples};
use serde_json::Value;

use common::{edit, node_index, options, save};

/// The sample with one test-kind element added.
fn with_test_element() -> Project {
    let mut p = samples::storey_with_walls_and_slab();
    let g = p.storeys.ground();
    let d = Dummy::new(DVec2::new(1.0, 1.0), DVec2::ONE, p.materials[0].id).into_element(&p, g, 0);
    History::default()
        .execute(&mut p, Command::AddElements(vec![d]))
        .unwrap();
    p
}

fn counters(file: &Value) -> &serde_json::Map<String, Value> {
    file["data"][0]["attributes"]["archicraft::project"]["counters"]
        .as_object()
        .unwrap()
}

#[test]
fn an_element_of_an_unstorable_kind_is_saved_without_its_kind_and_warned_about() {
    let p = with_test_element();
    let geometry = ModelGeometry::evaluate(&p);
    let (bytes, warnings) = archicraft_ifcx::save(&p, &geometry, &options(), false);
    let d = p.elements.values().find(|e| e.tag() == KindTag::Dummy).unwrap();
    let about: Vec<String> = warnings
        .iter()
        .filter(|w| w.path.as_deref() == Some(d.id.to_string().as_str()))
        .map(|w| w.message.to_string())
        .collect();
    assert!(
        about
            .iter()
            .any(|m| m.starts_with(r#"ifcx-kind-not-storable(name="X-001", kind=dummy)"#)),
        "{warnings:#?}"
    );
    assert!(
        warnings.iter().all(|w| w.severity == Severity::Warning),
        "{warnings:#?}"
    );

    let file: Value = serde_json::from_slice(&bytes).unwrap();
    let node = &file["data"][node_index(&file, "X-001")];
    let attributes = node["attributes"].as_object().unwrap();
    assert_eq!(attributes["bsi::ifc::class"]["code"], "IfcBuildingElementProxy");
    assert!(
        attributes.keys().all(|k| !k.starts_with("archicraft::")),
        "no kind and no parameters: {attributes:#?}"
    );
    // ELM-019: the counter of a kind without a tool is written once it is
    // non-zero, besides those of every kind with a tool.
    let written = counters(&file);
    assert_eq!(written.get("dummy"), Some(&Value::from(1)), "{written:?}");
    assert_eq!(written.len(), 3, "{written:?}");
    let parsed = validate::parse(&bytes).unwrap();
    assert!(
        validate::validate(&parsed).is_empty(),
        "the undeclared counter is valid"
    );

    // The node reads back as unsupported data, not as an element, and the
    // counter is kept so automatic names do not repeat.
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert_eq!(loaded.project.elements.len(), p.elements.len() - 1);
    assert!(loaded.project.element(d.id).is_none());
    assert_eq!(loaded.project.counters, p.counters);
}

/// ELM-019: a kind without a tool and without elements has no counter in the
/// file, and reading one without it is not a problem.
#[test]
fn a_missing_counter_of_a_kind_without_a_tool_reads_as_zero() {
    let p = samples::storey_with_walls_and_slab();
    let file: Value = serde_json::from_slice(&save(&p)).unwrap();
    let written = counters(&file);
    assert!(!written.contains_key("dummy"), "{written:?}");
    let loaded = archicraft_ifcx::load(&save(&p)).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert_eq!(loaded.project.counters.get(KindTag::Dummy), 0);
    assert_eq!(loaded.project.counters, p.counters);
}

/// An element whose kind this build knows but this format version cannot
/// read is skipped and reported.
#[test]
fn an_element_of_an_unreadable_kind_is_skipped_and_reported() {
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
            && p.message.to_string().contains("ifcx-kind-unreadable(kind=dummy)")),
        "{:#?}",
        loaded.problems
    );
}

/// ELM-001: tool defaults of a kind this format has no defaults shape for
/// are left out with a warning; `toolDefaults` stays `{wall, slab}` and
/// reads back to the other slots.
#[test]
fn tool_defaults_of_an_unstorable_kind_are_left_out_and_warned_about() {
    let mut p = samples::storey_with_walls_and_slab();
    let mut defaults = p.tool_defaults.clone();
    defaults.insert(DefaultsSlot {
        layer: p.layers[0].id,
        position: Position::Undefined,
        ifc_class: KindTag::Dummy.descriptor().default_class,
        predefined_type: None,
        kind: Dummy::new(DVec2::ZERO, DVec2::ONE, p.materials[0].id).into(),
    });
    History::default()
        .execute(&mut p, Command::SetToolDefaults(defaults))
        .unwrap();
    let geometry = ModelGeometry::evaluate(&p);
    let (bytes, warnings) = archicraft_ifcx::save(&p, &geometry, &options(), false);
    assert!(
        warnings.iter().any(|w| w.severity == Severity::Warning
            && w.message
                .to_string()
                .contains("ifcx-tool-defaults-not-saved(kind=dummy)")),
        "{warnings:#?}"
    );
    let file: Value = serde_json::from_slice(&bytes).unwrap();
    let written: Vec<&String> = file["data"][0]["attributes"]["archicraft::project"]["toolDefaults"]
        .as_object()
        .unwrap()
        .keys()
        .collect();
    assert_eq!(written, ["wall", "slab"]);
    let loaded = archicraft_ifcx::load(&bytes).unwrap().project;
    assert!(loaded.tool_defaults.get(KindTag::Dummy).is_none());
    assert_eq!(
        loaded.tool_defaults.get(KindTag::Wall),
        p.tool_defaults.get(KindTag::Wall)
    );
    assert_eq!(
        loaded.tool_defaults.get(KindTag::Slab),
        p.tool_defaults.get(KindTag::Slab)
    );
}
