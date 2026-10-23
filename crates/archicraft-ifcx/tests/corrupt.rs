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
use archicraft_model::storey::MIN_STOREY_HEIGHT;
use archicraft_model::template::{TemplateContext, template_tool_defaults};
use archicraft_model::{Command, History, KindTag, Position, Project};
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
    Fixture {
        name: "out-of-range-counters",
        // One past the largest u32, and the largest u32, which leaves no
        // number for the next element.
        damage: |v| project_attr(v)["counters"] = json!({"wall": 4_294_967_296_u64, "slab": u32::MAX}),
    },
    Fixture {
        name: "out-of-range-decimals",
        damage: |v| {
            let settings = &mut project_attr(v)["settings"];
            settings["lengthDecimals"] = json!(12);
            settings["angleDecimals"] = json!(-1);
        },
    },
    Fixture {
        name: "bad-survey-point",
        damage: |v| project_attr(v)["location"]["surveyPoint"] = json!(["east", 2]),
    },
    Fixture {
        name: "storey-below-minimum-height",
        damage: |v| storey_attr(v, "01 First floor")["height"] = json!(0.0),
    },
    Fixture {
        name: "non-uuid-site-and-building",
        damage: |v| {
            rename_node(v, "Site", "site");
            rename_node(v, "Building", "building");
        },
    },
    Fixture {
        name: "bad-cut-fill-background",
        damage: |v| project_attr(v)["materials"][0]["cutFill"]["background"] = json!("white"),
    },
    Fixture {
        name: "bad-storey-cut-plane-height",
        damage: |v| storey_attr(v, "00 Ground floor")["cutPlaneHeight"] = json!("high"),
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

fn storey_attr<'v>(value: &'v mut Value, name: &str) -> &'v mut Value {
    let i = node_index(value, name);
    &mut value["data"][i]["attributes"]["archicraft::storey"]
}

/// Moves the node named `name` to `path`, also where its parent lists it.
fn rename_node(value: &mut Value, name: &str, path: &str) {
    let i = node_index(value, name);
    let old = std::mem::replace(&mut value["data"][i]["path"], json!(path));
    for node in value["data"].as_array_mut().unwrap() {
        let children = node.get_mut("children").and_then(Value::as_object_mut);
        for child in children.into_iter().flat_map(|c| c.values_mut()) {
            if *child == old {
                *child = json!(path);
            }
        }
    }
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

/// The path of the node named `name`.
fn node_path(value: &Value, name: &str) -> String {
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
        .find(|p| p.path.as_deref() == Some(path) && parts.iter().all(|part| p.message.to_string().contains(part)))
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
        &[
            "ifcx-skipped(record=ifcx-record-layer(index=1)",
            "reason=ifcx-not-object",
        ],
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
    problem(
        &outcome,
        &root,
        &["ifcx-skipped(record=ifcx-record-view(index=2)", r#"value="fisheye""#],
    );
    problem(
        &outcome,
        &root,
        &[
            "ifcx-skipped(record=ifcx-record-view(index=3)",
            "reason=ifcx-not-object",
        ],
    );
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
        &[
            "ifcx-repaired(record=ifcx-record-tool-defaults",
            r#"ifcx-field-missing(key="toolDefaults")"#,
            "ifcx-template-defaults-used",
        ],
    );
    let tool_problems = outcome
        .problems
        .iter()
        .filter(|p| p.message.to_string().contains("ifcx-record-tool-defaults"))
        .count();
    assert_eq!(tool_problems, 1, "{:#?}", outcome.problems);
}

/// PRJ-008, ELM-001: a tool defaults member that names no tool is ignored
/// and a missing slot gets the template's defaults, each reported once as a
/// warning; the other slots load as written.
#[test]
fn unknown_and_missing_tool_defaults_slots_are_reported() {
    let base = reference_model();
    let bytes = edit(&base, |v| {
        let defaults = project_attr(v)["toolDefaults"].as_object_mut().unwrap();
        defaults.remove("slab");
        defaults.insert("column".into(), json!({"thickness": 0.3}));
    });
    let outcome = archicraft_ifcx::load(&bytes).unwrap();
    let p = &outcome.project;
    let template = template_tool_defaults(&TemplateContext::new(&p.materials, &p.layers));
    assert_eq!(p.tool_defaults.get(KindTag::Slab), template.get(KindTag::Slab));
    assert_eq!(
        p.tool_defaults.get(KindTag::Slab),
        base.tool_defaults.get(KindTag::Slab)
    );
    assert_eq!(
        p.tool_defaults.get(KindTag::Wall),
        base.tool_defaults.get(KindTag::Wall)
    );

    let value: Value = serde_json::from_slice(&bytes).unwrap();
    let root = root_path(&value);
    problem(
        &outcome,
        &root,
        &[
            "ifcx-repaired(record=ifcx-record-tool-defaults",
            r#"ifcx-tool-unknown(key="column")"#,
        ],
    );
    problem(
        &outcome,
        &root,
        &[
            "ifcx-repaired(record=ifcx-record-tool-defaults",
            r#"ifcx-field-missing(key="slab")"#,
            "ifcx-tool-template-used",
        ],
    );
    let tool_problems: Vec<&Problem> = outcome
        .problems
        .iter()
        .filter(|p| p.message.to_string().contains("ifcx-record-tool-defaults"))
        .collect();
    assert_eq!(tool_problems.len(), 2, "{:#?}", outcome.problems);
    assert!(tool_problems.iter().all(|p| p.severity == Severity::Warning));
}

/// ELM-011, PRJ-008: a tool default class the kind does not allow is
/// replaced by the template's and reported.
#[test]
fn a_tool_default_class_the_kind_does_not_allow_is_repaired() {
    let base = reference_model();
    let bytes = edit(&base, |v| {
        project_attr(v)["toolDefaults"]["wall"]["ifcClass"] = json!("IfcCovering");
    });
    let outcome = archicraft_ifcx::load(&bytes).unwrap();
    let wall = outcome.project.tool_defaults.get(KindTag::Wall).unwrap();
    assert_eq!(wall, base.tool_defaults.get(KindTag::Wall).unwrap());
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    let p = problem(
        &outcome,
        &root_path(&value),
        &[
            "ifcx-repaired(record=ifcx-record-tool-defaults",
            r#"ifcx-tool-class-not-allowed(class="IfcCovering", kind=wall, default="IfcWall")"#,
        ],
    );
    assert_eq!(p.severity, Severity::Warning);
}

/// PRJ-008: invalid counters are repaired to the number of elements of their
/// kind, so automatic names do not repeat (ELM-019), and reported.
#[test]
fn invalid_counters_are_repaired_and_reported() {
    let (outcome, value) = load("bad-counters");
    assert_eq!(
        (
            outcome.project.counters.get(KindTag::Wall),
            outcome.project.counters.get(KindTag::Slab)
        ),
        (4, 1)
    );
    let root = root_path(&value);
    problem(
        &outcome,
        &root,
        &[
            "ifcx-repaired(record=ifcx-record-counters",
            r#"ifcx-counter-negative(key="wall", value=4)"#,
        ],
    );
    problem(
        &outcome,
        &root,
        &[
            "ifcx-repaired(record=ifcx-record-counters",
            r#"ifcx-field-not-integer(key="slab")"#,
            "value=1",
        ],
    );
}

/// PRJ-008: missing counters are repaired the same way and reported.
#[test]
fn missing_counters_are_repaired_and_reported() {
    let (outcome, value) = load("missing-counters");
    assert_eq!(
        (
            outcome.project.counters.get(KindTag::Wall),
            outcome.project.counters.get(KindTag::Slab)
        ),
        (4, 1)
    );
    problem(
        &outcome,
        &root_path(&value),
        &[
            "ifcx-repaired(record=ifcx-record-counters",
            r#"ifcx-field-missing(key="counters")"#,
        ],
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
        &[
            "ifcx-repaired(record=ifcx-record-project-information",
            r#"ifcx-field-not-text(key="client")"#,
        ],
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
    let path = node_path(&value, "W-001");
    problem(
        &outcome,
        &path,
        &[
            r#"ifcx-repaired(record=ifcx-record-element(name="W-001")"#,
            r#"key="locked""#,
        ],
    );
    problem(
        &outcome,
        &path,
        &[
            r#"ifcx-repaired(record=ifcx-record-element(name="W-001")"#,
            r#"key="position""#,
        ],
    );
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
        &[
            "ifcx-repaired(record=ifcx-record-material(index=0)",
            r#"ifcx-field-colour-component(key="color")"#,
        ],
    );
}

/// PRJ-008: an invalid ground storey reference is reported once, at the
/// project, and the ground storey is chosen from the elevations.
#[test]
fn an_invalid_ground_storey_is_repaired_and_reported_once() {
    let (outcome, value) = load("bad-ground-storey");
    let storeys = &outcome.project.storeys;
    assert_eq!(storeys.get(storeys.ground()).unwrap().name, "00 Ground floor");
    problem(
        &outcome,
        &root_path(&value),
        &[
            "ifcx-repaired(record=ifcx-record-project",
            r#"ifcx-field-not-uuid(key="groundStorey")"#,
        ],
    );
    assert_eq!(outcome.problems.len(), 1, "{:#?}", outcome.problems);
}

/// PRJ-008: an element that fails validation is skipped and reported against
/// its own path.
#[test]
fn an_invalid_element_is_skipped_and_reported() {
    let (outcome, value) = load("invalid-wall");
    assert_eq!(outcome.project.elements.len(), reference_model().elements.len() - 1);
    let p = problem(
        &outcome,
        &node_path(&value, "W-002"),
        &[r#"ifcx-skipped(record=ifcx-record-element(name="W-002")"#],
    );
    assert_eq!(p.severity, Severity::Error);
}

/// PRJ-008, ELM-019: a counter that does not fit, or leaves no number for the
/// next element, is repaired to the number of elements of its kind and
/// reported; the next element then gets the next name and advances the
/// counter.
#[test]
fn out_of_range_counters_are_repaired_to_the_element_count() {
    let (outcome, value) = load("out-of-range-counters");
    let root = root_path(&value);
    problem(
        &outcome,
        &root,
        &[
            "ifcx-repaired(record=ifcx-record-counters",
            r#"ifcx-out-of-range-replaced(key="wall", value=4)"#,
        ],
    );
    problem(
        &outcome,
        &root,
        &[
            "ifcx-repaired(record=ifcx-record-counters",
            r#"ifcx-out-of-range-replaced(key="slab", value=1)"#,
        ],
    );

    let mut project = outcome.project;
    assert_eq!(
        (project.counters.get(KindTag::Wall), project.counters.get(KindTag::Slab)),
        (4, 1)
    );
    let ground = project.storeys.ground();
    let wall = project.new_element(KindTag::Wall, ground, 0).unwrap();
    let slab = project.new_element(KindTag::Slab, ground, 0).unwrap();
    assert_eq!((wall.name.as_str(), slab.name.as_str()), ("W-005", "SL-002"));
    History::default()
        .execute(&mut project, Command::AddElements(vec![wall, slab]))
        .unwrap();
    assert_eq!(
        (project.counters.get(KindTag::Wall), project.counters.get(KindTag::Slab)),
        (5, 2)
    );
}

/// PRJ-008: display decimals outside 0 to 8 are clamped to that range and
/// reported.
#[test]
fn out_of_range_decimals_are_clamped_and_reported() {
    let (outcome, value) = load("out-of-range-decimals");
    let units = &outcome.project.settings.units;
    assert_eq!((units.length_decimals, units.angle_decimals), (8, 0));
    let root = root_path(&value);
    problem(
        &outcome,
        &root,
        &[
            "ifcx-repaired(record=ifcx-record-project-settings",
            r#"ifcx-out-of-range-replaced(key="lengthDecimals", value=8)"#,
        ],
    );
    problem(
        &outcome,
        &root,
        &[
            "ifcx-repaired(record=ifcx-record-project-settings",
            r#"ifcx-out-of-range-replaced(key="angleDecimals", value=0)"#,
        ],
    );
}

/// PRJ-008: a survey point that is not a point is repaired to the origin and
/// reported; the rest of the location is kept.
#[test]
fn an_invalid_survey_point_is_repaired_and_reported() {
    let (outcome, value) = load("bad-survey-point");
    let location = &outcome.project.site.location;
    assert_eq!(location.survey_point, [0.0, 0.0]);
    assert_eq!(*location, reference_model().site.location);
    let p = problem(
        &outcome,
        &root_path(&value),
        &[
            "ifcx-repaired(record=ifcx-record-project-location",
            r#"ifcx-field-point(key="surveyPoint", reason=ifcx-point-not-numeric)"#,
        ],
    );
    assert_eq!(p.severity, Severity::Warning);
}

/// PRJ-008: a storey height below the minimum is raised to the minimum and
/// reported against the storey.
#[test]
fn a_storey_height_below_the_minimum_is_repaired_and_reported() {
    let (outcome, value) = load("storey-below-minimum-height");
    let storeys = outcome.project.storeys.as_slice();
    let first = storeys.iter().find(|s| s.name == "01 First floor").unwrap();
    assert_eq!(first.height, MIN_STOREY_HEIGHT);
    let p = problem(
        &outcome,
        &node_path(&value, "01 First floor"),
        &[
            r#"ifcx-repaired(record=ifcx-record-storey(name="01 First floor")"#,
            "ifcx-storey-height-min",
        ],
    );
    assert_eq!(p.severity, Severity::Warning);
}

/// PRJ-008: a site or building whose path is not a UUID gets a new identity
/// and is reported against its node; everything below it loads.
#[test]
fn a_site_or_building_path_that_is_not_a_uuid_is_repaired_and_reported() {
    let (outcome, _) = load("non-uuid-site-and-building");
    let (project, reference) = (&outcome.project, reference_model());
    assert_ne!(project.site.id, reference.site.id);
    assert_ne!(project.building.id, reference.building.id);
    assert_eq!(
        (project.site.name.as_str(), project.building.name.as_str()),
        (reference.site.name.as_str(), reference.building.name.as_str())
    );
    assert_eq!(project.storeys, reference.storeys);
    assert_eq!(project.elements.len(), reference.elements.len());
    for (path, record) in [
        ("site", "ifcx-repaired(record=ifcx-record-site"),
        ("building", "ifcx-repaired(record=ifcx-record-building"),
    ] {
        let p = problem(&outcome, path, &[record, "reason=ifcx-path-not-uuid"]);
        assert_eq!(p.severity, Severity::Warning);
    }
}

/// PRJ-008: an invalid cut-fill background, an optional field, is reported
/// and left out; the rest of the material loads.
#[test]
fn an_invalid_cut_fill_background_is_left_out_and_reported() {
    let (outcome, value) = load("bad-cut-fill-background");
    let material = &outcome.project.materials[0];
    assert_eq!(material.cut_fill.background, None);
    assert_eq!(*material, reference_model().materials[0]);
    problem(
        &outcome,
        &root_path(&value),
        &[
            "ifcx-repaired(record=ifcx-record-material(index=0)",
            r#"ifcx-left-out(reason=ifcx-field-not-list(key="background"))"#,
        ],
    );
}

/// PRJ-008: an invalid storey cut-plane height, an optional field, is
/// reported against the storey and left out, so the project's cut plane
/// applies.
#[test]
fn an_invalid_storey_cut_plane_height_is_left_out_and_reported() {
    let (outcome, value) = load("bad-storey-cut-plane-height");
    let storeys = outcome.project.storeys.as_slice();
    let ground = storeys.iter().find(|s| s.name == "00 Ground floor").unwrap();
    assert_eq!(ground.cut_plane_height, None);
    assert_eq!(outcome.project.storeys, reference_model().storeys);
    problem(
        &outcome,
        &node_path(&value, "00 Ground floor"),
        &[
            r#"ifcx-repaired(record=ifcx-record-storey(name="00 Ground floor")"#,
            r#"ifcx-left-out(reason=ifcx-field-not-number(key="cutPlaneHeight"))"#,
        ],
    );
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
        .filter(|p| {
            p.message
                .to_string()
                .starts_with(&format!("ifcx-skipped(record=ifcx-record-{collection}("))
        })
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
/// and counted: `ifcx-skipped` or `ifcx-repaired`, each with its record and
/// reason.
#[test]
fn every_fixture_reports_a_skipped_or_repaired_record() {
    for fixture in FIXTURES {
        let (outcome, _) = load(fixture.name);
        assert!(
            outcome
                .problems
                .iter()
                .any(|p| p.path.is_some() && ["ifcx-skipped", "ifcx-repaired"].contains(&p.message.key())),
            "{}: {:#?}",
            fixture.name,
            outcome.problems
        );
    }
}
