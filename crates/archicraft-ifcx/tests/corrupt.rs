//! Damaged files (PRJ-008, NFR-REL-004): every valid part is loaded, and
//! every record that is skipped or repaired is reported against its node.
//! Nothing disappears silently.
//!
//! The fixtures under `tests/corrupt/` are the walls-and-slab reference model
//! (`samples::storey_with_walls_and_slab`, read from its golden file so the
//! identities are stable) with one kind of damage each. They are committed so
//! the fuzz corpus can be seeded with them; after an intended format change,
//! regenerate them with `ARCHICRAFT_BLESS=1` like the golden files.

#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};

use archicraft_ifcx::ReadOutcome;
use archicraft_ifcx::validate::{Problem, Severity};
use archicraft_model::material::Rgb;
use archicraft_model::{Position, Project};
use serde_json::{Value, json};

use common::{edit, node_index};

/// One kind of damage, applied to the reference model's JSON.
struct Fixture {
    name: &'static str,
    damage: fn(&mut Value),
}

const FIXTURES: &[Fixture] = &[
    Fixture {
        name: "non-object-layer",
        damage: |v| project_attr(v)["layers"][1] = json!(42),
    },
    Fixture {
        name: "malformed-views",
        damage: |v| {
            let views = project_attr(v)["views"].as_array_mut().unwrap();
            views.push(json!({
                "id": "6f1d3c52-2b8e-4c4f-9a59-0d2b6f0c7e11",
                "name": "Fisheye",
                "projection": "fisheye",
            }));
            views.push(json!("a view"));
        },
    },
    Fixture {
        name: "missing-tool-defaults",
        damage: |v| {
            project_attr(v).as_object_mut().unwrap().remove("toolDefaults");
        },
    },
    Fixture {
        name: "bad-counters",
        damage: |v| project_attr(v)["counters"] = json!({"wall": -5, "slab": "x"}),
    },
    Fixture {
        name: "missing-counters",
        damage: |v| {
            project_attr(v).as_object_mut().unwrap().remove("counters");
        },
    },
    Fixture {
        name: "bad-info-client",
        damage: |v| project_attr(v)["info"]["client"] = json!(7),
    },
    Fixture {
        name: "bad-element-flags",
        damage: |v| {
            let i = node_index(v, "W-001");
            let element = &mut v["data"][i]["attributes"]["archicraft::element"];
            element["locked"] = json!("yes");
            element["position"] = json!(3);
        },
    },
    Fixture {
        name: "bad-material-colour",
        damage: |v| project_attr(v)["materials"][0]["color"] = json!(["a", 0, 0]),
    },
    Fixture {
        name: "bad-ground-storey",
        damage: |v| project_attr(v)["groundStorey"] = json!("not-a-uuid"),
    },
    Fixture {
        name: "invalid-wall",
        damage: |v| {
            let i = node_index(v, "W-002");
            v["data"][i]["attributes"]["archicraft::wall"]["thickness"] = json!(-1.0);
        },
    },
];

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn fixture_path(name: &str) -> PathBuf {
    manifest_dir().join("tests/corrupt").join(format!("{name}.ifcx"))
}

/// The walls-and-slab reference model with stable identities.
fn reference_model() -> Project {
    let bytes = std::fs::read(manifest_dir().join("tests/golden/walls-and-slab.ifcx")).unwrap();
    archicraft_ifcx::load(&bytes).unwrap().project
}

fn project_node(value: &mut Value) -> &mut Value {
    value["data"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|n| n["attributes"].get("archicraft::project").is_some())
        .unwrap()
}

fn project_attr(value: &mut Value) -> &mut Value {
    &mut project_node(value)["attributes"]["archicraft::project"]
}

/// A committed fixture, as bytes and as JSON.
fn fixture(name: &str) -> (Vec<u8>, Value) {
    let path = fixture_path(name);
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; generate the fixtures with ARCHICRAFT_BLESS=1", path.display()));
    let value = serde_json::from_slice(&bytes).unwrap();
    (bytes, value)
}

fn load(name: &str) -> (ReadOutcome, Value) {
    let (bytes, value) = fixture(name);
    let outcome = archicraft_ifcx::load(&bytes).unwrap_or_else(|e| panic!("{name} does not load: {e}"));
    (outcome, value)
}

fn root_path(value: &Value) -> String {
    let mut value = value.clone();
    project_node(&mut value)["path"].as_str().unwrap().to_owned()
}

fn element_path(value: &Value, name: &str) -> String {
    value["data"][node_index(value, name)]["path"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// The problem reported at `path` whose message contains every part, or a
/// panic listing all problems.
fn problem<'o>(outcome: &'o ReadOutcome, path: &str, parts: &[&str]) -> &'o Problem {
    outcome
        .problems
        .iter()
        .find(|p| p.path.as_deref() == Some(path) && parts.iter().all(|part| p.message.contains(part)))
        .unwrap_or_else(|| panic!("no problem at {path} containing {parts:?}: {:#?}", outcome.problems))
}

/// The fixtures are reproduced from the reference model; with
/// `ARCHICRAFT_BLESS=1` they are rewritten.
#[test]
fn corrupt_fixtures_are_reproduced() {
    let bless = std::env::var_os("ARCHICRAFT_BLESS").is_some();
    let base = reference_model();
    if bless {
        std::fs::create_dir_all(manifest_dir().join("tests/corrupt")).unwrap();
    }
    for fixture in FIXTURES {
        let bytes = edit(&base, fixture.damage);
        let path = fixture_path(fixture.name);
        if bless {
            std::fs::write(&path, &bytes).unwrap();
        } else {
            let committed = std::fs::read(&path).unwrap_or_default();
            assert!(
                committed == bytes,
                "{} differs from its generator; if intended, regenerate with ARCHICRAFT_BLESS=1",
                path.display()
            );
        }
    }
    let committed = std::fs::read_dir(manifest_dir().join("tests/corrupt"))
        .unwrap()
        .filter(|e| e.as_ref().unwrap().path().extension().is_some_and(|x| x == "ifcx"))
        .count();
    assert_eq!(committed, FIXTURES.len(), "every committed fixture has a generator");
}

/// PRJ-008: a layer record that is not an object is skipped and reported;
/// the other layers load.
#[test]
fn a_layer_record_that_is_not_an_object_is_skipped_and_reported() {
    let (outcome, value) = load("non-object-layer");
    let names: Vec<&str> = outcome.project.layers.iter().map(|l| l.name.as_str()).collect();
    let mut expected: Vec<String> = reference_model().layers.iter().map(|l| l.name.clone()).collect();
    expected.remove(1);
    assert_eq!(names, expected);
    let p = problem(
        &outcome,
        &root_path(&value),
        &["layer record 1 skipped", "not an object"],
    );
    assert_eq!(p.severity, Severity::Error);
}

/// PRJ-008: each malformed view record is skipped and reported with its
/// index and reason; the valid views load.
#[test]
fn malformed_view_records_are_skipped_and_reported() {
    let (outcome, value) = load("malformed-views");
    assert_eq!(outcome.project.views, reference_model().views);
    let root = root_path(&value);
    problem(&outcome, &root, &["view record 2 skipped", "fisheye"]);
    problem(&outcome, &root, &["view record 3 skipped", "not an object"]);
}

/// PRJ-008: missing tool defaults are reported once and the template's
/// defaults are used, with the template's material and layer found by name.
#[test]
fn missing_tool_defaults_are_repaired_from_the_template() {
    let (outcome, value) = load("missing-tool-defaults");
    assert_eq!(outcome.project.tool_defaults, reference_model().tool_defaults);
    problem(
        &outcome,
        &root_path(&value),
        &["tool defaults repaired", "“toolDefaults” is missing", "template"],
    );
    let tool_problems = outcome
        .problems
        .iter()
        .filter(|p| p.message.contains("tool defaults"))
        .count();
    assert_eq!(tool_problems, 1, "{:#?}", outcome.problems);
}

/// PRJ-008: invalid counters are repaired to the number of elements of their
/// kind, so automatic names do not repeat (ELM-019), and reported.
#[test]
fn invalid_counters_are_repaired_and_reported() {
    let (outcome, value) = load("bad-counters");
    assert_eq!((outcome.project.counters.wall, outcome.project.counters.slab), (4, 1));
    let root = root_path(&value);
    problem(&outcome, &root, &["counters repaired", "“wall” is negative", "4"]);
    problem(&outcome, &root, &["counters repaired", "“slab” is not an integer", "1"]);
}

/// PRJ-008: missing counters are repaired the same way and reported.
#[test]
fn missing_counters_are_repaired_and_reported() {
    let (outcome, value) = load("missing-counters");
    assert_eq!((outcome.project.counters.wall, outcome.project.counters.slab), (4, 1));
    problem(
        &outcome,
        &root_path(&value),
        &["counters repaired", "“counters” is missing"],
    );
}

/// PRJ-008: an invalid project information field is repaired and reported;
/// the other fields are kept.
#[test]
fn an_invalid_info_field_is_repaired_and_reported() {
    let (outcome, value) = load("bad-info-client");
    assert_eq!(outcome.project.info, reference_model().info);
    problem(
        &outcome,
        &root_path(&value),
        &["project information repaired", "“client” is not text"],
    );
}

/// PRJ-008: an element's position and locked flag of the wrong type are
/// repaired and reported against the element.
#[test]
fn invalid_element_flags_are_repaired_and_reported() {
    let (outcome, value) = load("bad-element-flags");
    let reference = reference_model();
    assert_eq!(outcome.project.elements.len(), reference.elements.len());
    let wall = outcome.project.elements.values().find(|e| e.name == "W-001").unwrap();
    assert!(!wall.locked);
    assert_eq!(wall.position, Position::default());
    let path = element_path(&value, "W-001");
    problem(&outcome, &path, &["element W-001 repaired", "“locked”"]);
    problem(&outcome, &path, &["element W-001 repaired", "“position”"]);
}

/// PRJ-008: a colour with a non-numeric component is reported and the
/// default colour is used; the material itself loads.
#[test]
fn a_non_numeric_colour_component_is_repaired_and_reported() {
    let (outcome, value) = load("bad-material-colour");
    let reference = reference_model();
    assert_eq!(outcome.project.materials.len(), reference.materials.len());
    assert_eq!(outcome.project.materials[0].color, Rgb([0.7, 0.7, 0.7]));
    assert_eq!(outcome.project.materials[0].name, reference.materials[0].name);
    problem(
        &outcome,
        &root_path(&value),
        &["material record 0 repaired", "“color” has a non-numeric component"],
    );
}

/// PRJ-008: an invalid ground storey reference is reported, and the ground
/// storey is chosen from the elevations.
#[test]
fn an_invalid_ground_storey_is_repaired_and_reported() {
    let (outcome, value) = load("bad-ground-storey");
    let storeys = &outcome.project.storeys;
    assert_eq!(storeys.get(storeys.ground()).unwrap().name, "00 Ground floor");
    problem(
        &outcome,
        &root_path(&value),
        &["project repaired", "“groundStorey” is not a UUID"],
    );
}

/// PRJ-008: an element that fails validation is skipped and reported against
/// its own path.
#[test]
fn an_invalid_element_is_skipped_and_reported() {
    let (outcome, value) = load("invalid-wall");
    assert_eq!(outcome.project.elements.len(), reference_model().elements.len() - 1);
    let p = problem(&outcome, &element_path(&value, "W-002"), &["element W-002 skipped"]);
    assert_eq!(p.severity, Severity::Error);
}

/// The number of records of each collection in a file, and how many the
/// reader reported as skipped.
fn present_and_skipped(value: &Value, outcome: &ReadOutcome, collection: &str) -> (usize, usize) {
    let nodes = value["data"].as_array().unwrap();
    let with_attribute = |attribute: &str| {
        nodes
            .iter()
            .filter(|n| n["attributes"].get(attribute).is_some())
            .count()
    };
    let project = |key: &str| {
        let mut value = value.clone();
        project_attr(&mut value)[key].as_array().map_or(0, Vec::len)
    };
    let present = match collection {
        "material" => project("materials"),
        "layer" => project("layers"),
        "view" => project("views"),
        "storey" => with_attribute("archicraft::storey"),
        _ => with_attribute("archicraft::element"),
    };
    let skipped = outcome
        .problems
        .iter()
        .filter(|p| p.message.starts_with(&format!("{collection} ")) && p.message.contains(" skipped: "))
        .count();
    (present, skipped)
}

fn loaded(project: &Project, collection: &str) -> usize {
    match collection {
        "material" => project.materials.len(),
        "layer" => project.layers.len(),
        "view" => project.views.len(),
        "storey" => project.storeys.as_slice().len(),
        _ => project.elements.len(),
    }
}

/// PRJ-008, NFR-REL-004: in every fixture and golden file, each record is
/// either loaded or reported as skipped, so no record disappears silently.
#[test]
fn no_record_disappears_silently() {
    let golden = std::fs::read_dir(manifest_dir().join("tests/golden"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "ifcx"));
    let files: Vec<PathBuf> = FIXTURES.iter().map(|f| fixture_path(f.name)).chain(golden).collect();
    for path in files {
        let bytes = std::fs::read(&path).unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        let outcome = archicraft_ifcx::load(&bytes).unwrap();
        for collection in ["material", "layer", "view", "storey", "element"] {
            let (present, skipped) = present_and_skipped(&value, &outcome, collection);
            assert_eq!(
                loaded(&outcome.project, collection) + skipped,
                present,
                "{}: {collection} records: {:#?}",
                path.display(),
                outcome.problems
            );
        }
    }
}

/// The repair and skip messages follow one pattern, so they can be found
/// and counted: "<record> skipped: <reason>" or "<record> repaired: <reason>".
#[test]
fn every_fixture_reports_a_skipped_or_repaired_record() {
    for fixture in FIXTURES {
        let (outcome, _) = load(fixture.name);
        assert!(
            outcome
                .problems
                .iter()
                .any(|p| p.path.is_some() && (p.message.contains(" skipped: ") || p.message.contains(" repaired: "))),
            "{}: {:#?}",
            fixture.name,
            outcome.problems
        );
    }
}
