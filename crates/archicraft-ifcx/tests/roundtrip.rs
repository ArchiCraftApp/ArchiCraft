//! IFCX writing and reading (SRS 5.2): validity, round trip, determinism,
//! golden files and lenient reading.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use archicraft_geometry::{ModelGeometry, TriMesh};
use archicraft_ifcx::validate::{self, Severity};
use archicraft_ifcx::{IfcxFile, WriteOptions};
use archicraft_model::glam::DVec3;
use archicraft_model::{Project, samples};
use serde_json::Value;

fn options() -> WriteOptions {
    WriteOptions {
        author: "Test".into(),
        timestamp: "2026-10-16T12:00:00Z".into(),
        data_version: 7,
    }
}

fn save(project: &Project) -> Vec<u8> {
    let geometry = ModelGeometry::evaluate(project);
    let (bytes, warnings) = archicraft_ifcx::save(project, &geometry, &options(), false);
    assert!(warnings.is_empty(), "{warnings:?}");
    bytes
}

fn samples() -> Vec<Project> {
    vec![samples::storey_with_walls_and_slab(), samples::two_storey_shell()]
}

#[test]
fn written_files_validate_without_problems() {
    for project in samples() {
        let bytes = save(&project);
        let file = validate::parse(&bytes).unwrap();
        let problems = validate::validate(&file);
        assert!(problems.is_empty(), "{problems:#?}");
        assert_eq!(file.header.ifcx_version, "ifcx_alpha");
        assert_eq!(file.header.id, format!("archicraft:project/{}", project.id));
        assert_eq!(file.header.data_version, "7");
    }
}

#[test]
fn round_trip_restores_the_editable_model() {
    for project in samples() {
        let bytes = save(&project);
        let loaded = archicraft_ifcx::load(&bytes).unwrap();
        assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
        assert!(!loaded.newer_format);
        let mut expected = project.clone();
        expected.revision = 0;
        expected.data_version = 7;
        assert_eq!(
            loaded.project, expected,
            "every parameter survives bit for bit (IFX-064, NFR-ACC-002)"
        );
    }
}

#[test]
fn regenerated_geometry_matches_the_stored_geometry() {
    // IFX-064: save, open, regenerate and save again gives the same file.
    for project in samples() {
        let first = save(&project);
        let loaded = archicraft_ifcx::load(&first).unwrap().project;
        let second = save(&loaded);
        assert_eq!(String::from_utf8(first).unwrap(), String::from_utf8(second).unwrap());
    }
}

#[test]
fn output_is_deterministic_and_plain() {
    let project = samples::two_storey_shell();
    let a = save(&project);
    let b = save(&project);
    assert_eq!(a, b, "IFX-007: unchanged model, identical bytes");
    // Remove string literals, then look for exponents in what is left.
    let mut outside_strings = Vec::with_capacity(a.len());
    let (mut in_string, mut escaped) = (false, false);
    for &c in &a {
        match (in_string, escaped, c) {
            (true, true, _) => escaped = false,
            (true, false, b'\\') => escaped = true,
            (true, false, b'"') => in_string = false,
            (true, false, _) => {}
            (false, _, b'"') => in_string = true,
            (false, _, c) => outside_strings.push(c),
        }
    }
    // Outside strings, an `e` after a digit can only be an exponent.
    assert!(
        !outside_strings
            .windows(2)
            .any(|w| w[0].is_ascii_digit() && (w[1] == b'e' || w[1] == b'E')),
        "no exponent notation in numbers (IFX-007)"
    );
    assert!(outside_strings.len() > 1000);
}

fn meshes(file: &IfcxFile) -> Vec<(String, TriMesh)> {
    let mut out = Vec::new();
    for node in &file.data {
        let Some(mesh) = node.attributes.get("usd::usdgeom::mesh") else {
            continue;
        };
        let points = mesh["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                let c: Vec<f64> = p.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
                DVec3::new(c[0], c[1], c[2])
            })
            .collect();
        let indices = mesh["faceVertexIndices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u32)
            .collect();
        out.push((node.path.clone(), TriMesh { points, indices }));
    }
    out
}

#[test]
fn written_meshes_are_closed_bodies() {
    for project in samples() {
        let file = validate::parse(&save(&project)).unwrap();
        let meshes = meshes(&file);
        assert_eq!(meshes.len(), project.elements.len());
        for (path, mesh) in meshes {
            assert_eq!(mesh.check_closed(), Ok(()), "IFX-032 for {path}");
        }
    }
}

#[test]
fn spatial_tree_and_identities() {
    let project = samples::storey_with_walls_and_slab();
    let file = validate::parse(&save(&project)).unwrap();
    let nodes = validate::flatten(&file);
    let roots = validate::roots(&nodes).unwrap();
    assert_eq!(
        roots,
        vec![project.id.to_string()],
        "IFX-013: the project is the only root"
    );
    let class = |path: &str| {
        nodes[path].attributes["bsi::ifc::class"]["code"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let root = &nodes[&project.id.to_string()];
    let (site_key, site) = root.children.iter().next().unwrap();
    assert!(site_key.starts_with("Site_"));
    let site = site.as_ref().unwrap();
    assert_eq!(class(site), "IfcSite");
    let building = nodes[site.as_str()].children.values().next().unwrap().as_ref().unwrap();
    assert_eq!(class(building), "IfcBuilding");
    let storeys: Vec<&String> = nodes[building.as_str()].children.values().flatten().collect();
    assert_eq!(storeys.len(), 2);
    let ground = &nodes[storeys[0].as_str()];
    assert_eq!(
        ground.attributes["usd::xformop"]["transform"][3],
        serde_json::json!([0, 0, 0, 1])
    );
    let upper = &nodes[storeys[1].as_str()];
    assert_eq!(
        upper.attributes["usd::xformop"]["transform"][3][2],
        serde_json::json!(3)
    );
    for (key, element) in &ground.children {
        let element = element.as_ref().unwrap();
        let code = class(element);
        assert!(
            key.starts_with(if code == "IfcWall" { "Wall_" } else { "Slab_" }),
            "{key}"
        );
        let body = nodes[element.as_str()].children["Body"].as_ref().unwrap();
        // IFX-010: name-based UUID of owner and role.
        let owner = uuid::Uuid::parse_str(element).unwrap();
        assert_eq!(body, &uuid::Uuid::new_v5(&owner, b"Body").to_string());
        assert!(nodes[body.as_str()].inherits.contains_key("material"));
    }
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

#[test]
fn golden_files_are_reproduced() {
    // NFR-MNT-003: the reference models' IFCX output is the golden file.
    // Opening and saving a golden file must give it back unchanged. After an
    // intended format change, run with ARCHICRAFT_BLESS=1 to update them.
    let bless = std::env::var_os("ARCHICRAFT_BLESS").is_some();
    let mut count = 0;
    for entry in std::fs::read_dir(golden_dir()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "ifcx") {
            continue;
        }
        count += 1;
        let original = std::fs::read(&path).unwrap();
        let file = validate::parse(&original).unwrap();
        let loaded = archicraft_ifcx::load(&original).unwrap();
        assert!(loaded.problems.is_empty(), "{}: {:#?}", path.display(), loaded.problems);
        let geometry = ModelGeometry::evaluate(&loaded.project);
        let options = WriteOptions {
            author: file.header.author.clone(),
            timestamp: file.header.timestamp.clone(),
            data_version: loaded.project.data_version,
        };
        let (bytes, _) = archicraft_ifcx::save(&loaded.project, &geometry, &options, false);
        if bless {
            std::fs::write(&path, &bytes).unwrap();
        } else {
            assert!(
                bytes == original,
                "{} changed; if intended, regenerate with ARCHICRAFT_BLESS=1",
                path.display()
            );
        }
    }
    assert!(count >= 2);
}

#[test]
fn container_matches_the_ifcx_json_schema() {
    let mut bundle: Value = serde_json::from_str(include_str!("../schemas/ifcx-container.schema.json")).unwrap();
    // The `path` pattern in ifcx.tsp matches no real file (see schemas/README.md).
    bundle["$defs"]["path"].as_object_mut().unwrap().remove("pattern");
    bundle["$id"] = "https://ifcx.example/ifcx.json".into();
    bundle["$ref"] = "IfcxFile.json".into();
    let validator = jsonschema::validator_for(&bundle).unwrap();
    for project in samples() {
        let instance: Value = serde_json::from_slice(&save(&project)).unwrap();
        let errors: Vec<String> = validator.iter_errors(&instance).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{errors:#?}");
    }
}

#[test]
fn the_srs_appendix_c_example_validates() {
    let srs = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/SRS.md")).unwrap();
    let appendix = &srs[srs.find("## Appendix C").unwrap()..];
    let start = appendix.find("```json").unwrap() + "```json".len();
    let end = start + appendix[start..].find("```").unwrap();
    let file = validate::parse(&appendix.as_bytes()[start..end]).unwrap();
    let problems = validate::validate(&file);
    assert!(problems.is_empty(), "{problems:#?}");
}

fn edit(project: &Project, change: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(&save(project)).unwrap();
    change(&mut value);
    serde_json::to_vec(&value).unwrap()
}

fn node_index(value: &Value, name: &str) -> usize {
    value["data"]
        .as_array()
        .unwrap()
        .iter()
        .position(|n| n["attributes"]["bsi::ifc::prop::Name"] == name)
        .unwrap()
}

#[test]
fn invalid_elements_are_skipped_and_reported() {
    let project = samples::storey_with_walls_and_slab();
    let bytes = edit(&project, |v| {
        let i = node_index(v, "W-002");
        v["data"][i]["attributes"]["archicraft::wall"]["thickness"] = (-1.0).into();
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert_eq!(loaded.project.elements.len(), project.elements.len() - 1);
    assert!(
        loaded
            .problems
            .iter()
            .any(|p| p.severity == Severity::Error && p.message.contains("W-002 skipped")),
        "{:#?}",
        loaded.problems
    );
}

#[test]
fn missing_references_are_repaired_and_reported() {
    let project = samples::storey_with_walls_and_slab();
    let bytes = edit(&project, |v| {
        let i = node_index(v, "W-001");
        v["data"][i]["attributes"]["archicraft::wall"]["material"] = uuid::Uuid::new_v4().to_string().into();
        v["data"][i]["attributes"]["archicraft::element"]["layer"] = uuid::Uuid::new_v4().to_string().into();
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert_eq!(loaded.project.elements.len(), project.elements.len());
    let messages: Vec<&str> = loaded.problems.iter().map(|p| p.message.as_str()).collect();
    assert!(
        messages.iter().any(|m| m.contains("material does not exist")),
        "{messages:#?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("layer does not exist")),
        "{messages:#?}"
    );
    assert!(loaded.project.integrity_problems().is_empty());
}

#[test]
fn ground_storey_fallback_uses_the_model_tolerance() {
    // Without a ground storey reference, the lowest storey at or above
    // project zero is the ground floor; within the model tolerance below
    // zero is at zero (NFR-ACC-003).
    let ground_after_moving = |elevation: f64| {
        let bytes = edit(&samples::storey_with_walls_and_slab(), |v| {
            for node in v["data"].as_array_mut().unwrap() {
                if let Some(Value::Object(attr)) = node["attributes"].get_mut("archicraft::project") {
                    attr.remove("groundStorey");
                }
            }
            let i = node_index(v, "00 Ground floor");
            v["data"][i]["attributes"]["archicraft::storey"]["elevation"] = elevation.into();
        });
        let loaded = archicraft_ifcx::load(&bytes).unwrap();
        assert!(
            loaded
                .problems
                .iter()
                .any(|p| p.message.contains("ground storey is not defined")),
            "{:#?}",
            loaded.problems
        );
        let storeys = &loaded.project.storeys;
        storeys.get(storeys.ground()).unwrap().name.clone()
    };
    assert_eq!(ground_after_moving(-0.000_005), "00 Ground floor");
    assert_eq!(ground_after_moving(-0.000_020), "01 First floor");
}

#[test]
fn schema_violations_are_reported() {
    let project = samples::storey_with_walls_and_slab();
    let bytes = edit(&project, |v| {
        v["data"][0]["attributes"]["vendor::unknown"] = 1.into();
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(
        loaded
            .problems
            .iter()
            .any(|p| p.message.contains("“vendor::unknown” is not declared"))
    );
}

#[test]
fn newer_format_versions_are_flagged() {
    let project = samples::storey_with_walls_and_slab();
    let bytes = edit(&project, |v| {
        v["data"][0]["attributes"]["archicraft::project"]["formatVersion"] = "99".into();
    });
    assert!(archicraft_ifcx::load(&bytes).unwrap().newer_format);
}

#[test]
fn malformed_input_is_rejected_without_panicking() {
    let deep = format!("{}{}", "[".repeat(200), "]".repeat(200));
    for input in [
        b"".as_slice(),
        b"not json",
        b"{}",
        b"[]",
        b"{\"header\": 1}",
        deep.as_bytes(),
        br#"{"header":{"id":"x","ifcxVersion":"ifcx_alpha","dataVersion":"1","author":"a","timestamp":"t"},"imports":[],"schemas":{},"data":[]}"#,
        br#"{"header":{"id":"x","ifcxVersion":"ifcx_beta","dataVersion":"1","author":"a","timestamp":"t"},"imports":[],"schemas":{},"data":[]}"#,
    ] {
        assert!(archicraft_ifcx::load(input).is_err());
    }
}

#[test]
fn third_party_files_are_reported_as_unsupported() {
    let third_party = br#"{
        "header": {"id": "hello", "ifcxVersion": "ifcx_alpha", "dataVersion": "1", "author": "x", "timestamp": "t"},
        "imports": [{"uri": "https://ifcx.dev/@standards.buildingsmart.org/ifc/core/ifc@v5a.ifcx"}],
        "schemas": {},
        "data": [{"path": "p", "attributes": {"bsi::ifc::class": {"code": "IfcProject", "uri": "u"}}}]
    }"#;
    let error = archicraft_ifcx::load(third_party).unwrap_err();
    assert!(error.message.contains("not supported yet"));
}

#[test]
fn cycles_are_detected() {
    let project = samples::storey_with_walls_and_slab();
    let bytes = edit(&project, |v| {
        let a = v["data"][0]["path"].clone();
        let b = v["data"][1]["path"].clone();
        v["data"][0]["inherits"] = serde_json::json!({"loop": b});
        v["data"][1]["inherits"] = serde_json::json!({"loop": a});
    });
    let file = validate::parse(&bytes).unwrap();
    let problems = validate::validate(&file);
    assert!(problems.iter().any(|p| p.message.contains("cycle")), "{problems:#?}");
}
