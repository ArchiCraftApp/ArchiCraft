//! IFCX writing and reading (SRS 5.2): validity, round trip, determinism,
//! golden files and lenient reading.

#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};

use archicraft_geometry::{ModelGeometry, TriMesh};
use archicraft_ifcx::validate::{self, Severity};
use archicraft_ifcx::{IfcxFile, WriteOptions};
use archicraft_model::glam::{DVec2, DVec3};
use archicraft_model::{PlanGrid, samples};
use serde_json::Value;

use common::{edit, node_index, samples, save};

/// IFX-061: every file this release writes validates on its own, the
/// sample catalogue's and the profile sample's, whose profile records the
/// declarations describe (PRF-004). That every member of the records is
/// declared, which validation does not check, is
/// `ifx_061_every_archicraft_attribute_has_a_complete_value_description`
/// (tests/done_requirements.rs).
#[test]
fn written_files_validate_without_problems() {
    for project in samples().into_iter().chain([common::profile_sample()]) {
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
        // One mesh per body part: a column of a custom profile has one per
        // region (PRF-004), a body-less element, like a text (ANN-007), none.
        let geometry = ModelGeometry::evaluate(&project);
        for e in project.elements.values().filter(|e| e.kind.descriptor().has_body) {
            assert!(geometry.get(e.id).is_some_and(|g| !g.parts.is_empty()), "{}", e.name);
        }
        let parts: usize = project
            .elements
            .keys()
            .filter_map(|id| geometry.get(*id))
            .map(|g| g.parts.len())
            .sum();
        assert_eq!(meshes.len(), parts);
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
            // Catalogue samples are regenerated from the sample itself by
            // `catalogue_samples_reproduce_the_golden_files`.
            if catalogue_name(&path).is_none() {
                std::fs::write(&path, &bytes).unwrap();
            }
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

/// The catalogue sample a golden file belongs to, by file name.
fn catalogue_name(path: &Path) -> Option<&'static str> {
    let stem = path.file_stem()?.to_str()?;
    samples::catalogue()
        .iter()
        .map(|(name, _)| *name)
        .find(|name| *name == stem)
}

/// The header options of the reference models, for a golden file that does
/// not exist yet (as in `examples/reference_models.rs`).
fn reference_options() -> WriteOptions {
    WriteOptions {
        author: "ArchiCraft reference models".into(),
        timestamp: "2026-10-16T00:00:00Z".into(),
        data_version: 1,
    }
}

#[test]
fn catalogue_samples_reproduce_the_golden_files() {
    // NFR-MNT-003, IFX-010, IFX-011: the golden file is the reference
    // model's output. Every catalogue sample, built with seeded identities
    // and saved with its golden file's header options, gives the golden
    // file byte for byte. ARCHICRAFT_BLESS=1 regenerates the golden files
    // from the samples.
    let bless = std::env::var_os("ARCHICRAFT_BLESS").is_some();
    for (name, build) in samples::catalogue() {
        let path = golden_dir().join(format!("{name}.ifcx"));
        let original = std::fs::read(&path).ok();
        let options = match &original {
            Some(bytes) => {
                let header = validate::parse(bytes).unwrap().header;
                WriteOptions {
                    author: header.author,
                    timestamp: header.timestamp,
                    data_version: header.data_version.parse().unwrap(),
                }
            }
            None => reference_options(),
        };
        let project = build();
        let geometry = ModelGeometry::evaluate(&project);
        let (bytes, warnings) = archicraft_ifcx::save(&project, &geometry, &options, false);
        assert!(warnings.is_empty(), "{name}: {warnings:?}");
        if bless {
            std::fs::write(&path, &bytes).unwrap();
        } else {
            assert!(
                original.as_deref() == Some(bytes.as_slice()),
                "{} differs from the sample {name}; if intended, regenerate with ARCHICRAFT_BLESS=1",
                path.display()
            );
        }
    }
}

#[test]
fn separate_builds_of_a_sample_save_the_same_bytes() {
    // IFX-010, IFX-011: two builds of a catalogue sample, as two runs of
    // `examples/reference_models.rs` make them, write identical files, compact
    // and indented.
    for (name, build) in samples::catalogue() {
        for indented in [false, true] {
            let write = || {
                let project = build();
                archicraft_ifcx::save(
                    &project,
                    &ModelGeometry::evaluate(&project),
                    &reference_options(),
                    indented,
                )
                .0
            };
            assert!(write() == write(), "{name}, indented {indented}");
        }
    }
}

/// Saves and reopens a stand-in and checks that it comes back equal to
/// itself (IFX-064), returning how long that took.
fn assert_stand_in_round_trips(n: usize) -> std::time::Duration {
    let started = std::time::Instant::now();
    let project = samples::stand_in(n);
    let loaded = archicraft_ifcx::load(&save(&project)).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    let mut expected = project;
    expected.revision = 0;
    expected.data_version = 7;
    assert!(loaded.project == expected, "stand_in({n}) round-trips equal to itself");
    started.elapsed()
}

#[test]
fn a_stand_in_of_1000_round_trips_equal_to_itself() {
    // NFR-PERF-006, VIEW-001: the stand-ins are usable models.
    let elapsed = assert_stand_in_round_trips(1_000);
    eprintln!("stand_in(1_000) round trip: {elapsed:?}");
}

/// The RM-M-sized stand-in. Ignored to keep the normal suite lean: build,
/// save and load take about 0.4 s in the dev profile on an M3 Max (2026-10-21);
/// run it with `cargo test -p archicraft-ifcx --test roundtrip -- --ignored`.
#[test]
#[ignore = "RM-M size, about 0.4 s in the dev profile"]
fn a_stand_in_of_5000_round_trips_equal_to_itself() {
    let elapsed = assert_stand_in_round_trips(5_000);
    eprintln!("stand_in(5_000) round trip: {elapsed:?}");
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
        loaded.problems.iter().any(|p| p.severity == Severity::Error
            && p.message
                .to_string()
                .contains(r#"ifcx-skipped(record=ifcx-record-element(name="W-002")"#)),
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
    let messages: Vec<String> = loaded.problems.iter().map(|p| p.message.to_string()).collect();
    assert!(
        messages.iter().any(|m| m.contains("ifcx-element-material-missing")),
        "{messages:#?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("ifcx-element-layer-missing")),
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
                .any(|p| p.message.to_string().contains("ifcx-ground-storey-undefined")),
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
    assert!(loaded.problems.iter().any(|p| {
        p.message
            .to_string()
            .contains(r#"ifcx-attribute-undeclared(id="vendor::unknown")"#)
    }));
}

/// IFX-062: a file of a newer format is flagged, not migrated, and its
/// project is loaded.
#[test]
fn newer_format_versions_are_flagged() {
    let project = samples::storey_with_walls_and_slab();
    let bytes = edit(&project, |v| {
        v["data"][0]["attributes"]["archicraft::project"]["formatVersion"] = "99".into();
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.newer_format);
    assert_eq!(loaded.migrated, None);
    assert!(
        loaded
            .problems
            .iter()
            .any(|p| p.severity == Severity::Warning && p.message.to_string() == "ifcx-newer-format(version=99)"),
        "{:#?}",
        loaded.problems
    );
    assert_eq!(loaded.project.elements.len(), project.elements.len());
}

/// IFX-062: a file whose format version is missing or not a number loads
/// with the warning that says so, is read as it is, without a migration,
/// and is not flagged as newer.
#[test]
fn missing_or_non_numeric_format_versions_load_without_a_migration() {
    let project = samples::storey_with_walls_and_slab();
    for version in [
        None,
        Some(Value::from("four")),
        Some(Value::from("")),
        Some(Value::from(4)),
    ] {
        let bytes = edit(&project, |v| {
            let attribute = v["data"][0]["attributes"]["archicraft::project"]
                .as_object_mut()
                .unwrap();
            match &version {
                Some(version) => attribute.insert("formatVersion".into(), version.clone()),
                None => attribute.remove("formatVersion"),
            };
        });
        let loaded = archicraft_ifcx::load(&bytes).unwrap();
        assert_eq!(loaded.migrated, None, "{version:?}");
        assert!(!loaded.newer_format, "{version:?}");
        assert!(
            loaded
                .problems
                .iter()
                .any(|p| p.severity == Severity::Warning
                    && p.message.to_string().contains("ifcx-format-version-invalid")),
            "{version:?}: {:#?}",
            loaded.problems
        );
        assert!(
            !loaded.problems.iter().any(|p| p.severity == Severity::Info),
            "{version:?}: {:#?}",
            loaded.problems
        );
        assert_eq!(loaded.project.elements.len(), project.elements.len(), "{version:?}");
    }
}

/// NFR-REL-007: malformed input is rejected with an error, not a panic.
///
/// These eight inputs are also the first files of the fuzz-regression
/// corpus, `tests/fuzz-regressions/` (`empty.ifcx` to `unknown-version.ifcx`),
/// and `the_regression_corpus_holds_the_malformed_inputs`
/// (tests/fuzz_regressions.rs) pins those files to these bytes: change an
/// input here, there and in the corpus together.
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
    assert!(error.message.to_string().contains("ifcx-no-project"));
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
    assert!(
        problems.iter().any(|p| p.message.to_string().contains("cycle")),
        "{problems:#?}"
    );
}

fn project_settings(value: &mut Value) -> &mut serde_json::Map<String, Value> {
    value["data"][0]["attributes"]["archicraft::project"]["settings"]
        .as_object_mut()
        .unwrap()
}

/// INP-013, PRJ-007: the plan grid is a project setting kept in the file,
/// bit for bit.
#[test]
fn a_non_default_plan_grid_round_trips_exactly() {
    let mut project = samples::storey_with_walls_and_slab();
    project.settings.plan_grid = PlanGrid {
        snap_step: DVec2::new(0.1, 0.05),
        snap_enabled: false,
        main_spacing: DVec2::new(1.2, 0.9),
        auxiliary_spacing: DVec2::new(0.3, 0.15),
        rotation: 30f64.to_radians(),
        origin: DVec2::new(1.0, 1.0),
        visible: true,
    };
    let bytes = save(&project);
    let problems = validate::validate(&validate::parse(&bytes).unwrap());
    assert!(problems.is_empty(), "{problems:#?}");
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert_eq!(loaded.project.settings, project.settings);
}

/// INP-013: files without a plan grid (written before it existed) load with
/// the default grid and no problem, because the object is optional.
#[test]
fn a_missing_plan_grid_loads_the_defaults_without_problems() {
    let mut project = samples::storey_with_walls_and_slab();
    project.settings.plan_grid.visible = true;
    let bytes = edit(&project, |v| {
        assert!(project_settings(v).remove("planGrid").is_some());
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert_eq!(loaded.project.settings.plan_grid, PlanGrid::default());
}

/// INP-013: one invalid field of a present plan grid is reported once and
/// replaced by its default; the other fields are kept.
#[test]
fn an_invalid_plan_grid_field_falls_back_to_its_default() {
    let mut project = samples::storey_with_walls_and_slab();
    project.settings.plan_grid.snap_step = DVec2::new(0.1, 0.05);
    project.settings.plan_grid.main_spacing = DVec2::new(2.0, 2.0);
    project.settings.plan_grid.auxiliary_spacing = DVec2::new(0.5, 0.5);
    project.settings.plan_grid.origin = DVec2::new(3.0, 4.0);
    project.settings.plan_grid.rotation = 0.5;
    // Values the schema accepts but the model does not, so that the reader is
    // the only one to report them.
    let fields = [
        ("snapStep", serde_json::json!([0.0, 0.05])),
        ("mainSpacing", serde_json::json!([1.0, -1.0])),
        ("auxiliarySpacing", serde_json::json!([0.25])),
        ("origin", serde_json::json!([1.0, 2.0, 3.0])),
    ];
    let defaults = PlanGrid::default();
    for (key, value) in fields {
        let bytes = edit(&project, |v| {
            project_settings(v)["planGrid"][key] = value;
        });
        let loaded = archicraft_ifcx::load(&bytes).unwrap();
        assert_eq!(loaded.problems.len(), 1, "{key}: {:#?}", loaded.problems);
        assert!(
            loaded.problems[0].message.to_string().contains(key),
            "{:#?}",
            loaded.problems
        );
        let mut expected = project.settings.plan_grid.clone();
        match key {
            "snapStep" => expected.snap_step = defaults.snap_step,
            "mainSpacing" => expected.main_spacing = defaults.main_spacing,
            "auxiliarySpacing" => expected.auxiliary_spacing = defaults.auxiliary_spacing,
            _ => expected.origin = defaults.origin,
        }
        assert_ne!(expected, project.settings.plan_grid, "{key} differs from its default");
        assert_eq!(
            loaded.project.settings.plan_grid, expected,
            "{key} falls back to its default, the other fields are kept"
        );
    }
}

/// INP-013: an auxiliary spacing larger than the main spacing is reported
/// once, and both spacings fall back to their defaults.
#[test]
fn an_auxiliary_spacing_above_the_main_spacing_falls_back_to_the_defaults() {
    let mut project = samples::storey_with_walls_and_slab();
    project.settings.plan_grid.main_spacing = DVec2::new(2.0, 2.0);
    let bytes = edit(&project, |v| {
        project_settings(v)["planGrid"]["auxiliarySpacing"] = serde_json::json!([3.0, 0.5]);
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert_eq!(loaded.problems.len(), 1, "{:#?}", loaded.problems);
    let grid = &loaded.project.settings.plan_grid;
    assert_eq!(grid.main_spacing, PlanGrid::default().main_spacing);
    assert_eq!(grid.auxiliary_spacing, PlanGrid::default().auxiliary_spacing);
}

/// INP-013: an invalid auxiliary spacing is the one problem of a grid whose
/// main spacing lies below the default auxiliary spacing. The main spacing is
/// kept, and the auxiliary spacing is not larger than it.
#[test]
fn an_invalid_auxiliary_spacing_keeps_a_fine_main_spacing() {
    let mut project = samples::storey_with_walls_and_slab();
    project.settings.plan_grid.main_spacing = DVec2::new(0.1, 0.1);
    project.settings.plan_grid.auxiliary_spacing = DVec2::new(0.05, 0.05);
    let bytes = edit(&project, |v| {
        project_settings(v)["planGrid"]["auxiliarySpacing"] = serde_json::json!([0.25]);
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert_eq!(loaded.problems.len(), 1, "{:#?}", loaded.problems);
    assert!(
        loaded.problems[0].message.to_string().contains("auxiliarySpacing"),
        "{:#?}",
        loaded.problems
    );
    let grid = &loaded.project.settings.plan_grid;
    assert_eq!(grid.main_spacing, DVec2::new(0.1, 0.1));
    assert!(grid.auxiliary_spacing.cmple(grid.main_spacing).all(), "{grid:?}");
    assert_eq!(grid.check(), Ok(()));
}

/// INP-013: an invalid main spacing is the one problem of a grid whose
/// auxiliary spacing lies above the default main spacing. The main spacing
/// takes its default, and the auxiliary spacing is not larger than it.
#[test]
fn an_invalid_main_spacing_keeps_the_grid_valid() {
    let mut project = samples::storey_with_walls_and_slab();
    project.settings.plan_grid.main_spacing = DVec2::new(4.0, 4.0);
    project.settings.plan_grid.auxiliary_spacing = DVec2::new(2.0, 2.0);
    let bytes = edit(&project, |v| {
        project_settings(v)["planGrid"]["mainSpacing"] = serde_json::json!([1.0, -1.0]);
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert_eq!(loaded.problems.len(), 1, "{:#?}", loaded.problems);
    assert!(
        loaded.problems[0].message.to_string().contains("mainSpacing"),
        "{:#?}",
        loaded.problems
    );
    let grid = &loaded.project.settings.plan_grid;
    assert_eq!(grid.main_spacing, PlanGrid::default().main_spacing);
    assert!(grid.auxiliary_spacing.cmple(grid.main_spacing).all(), "{grid:?}");
    assert_eq!(grid.check(), Ok(()));
}

/// ELM-019: name counters are keyed by kind code. A counter of a kind this
/// build does not know (written by a later version) reads without a
/// problem, and the known counters are kept.
#[test]
fn a_counter_of_an_unknown_kind_reads_without_problems() {
    let project = samples::storey_with_walls_and_slab();
    let bytes = edit(&project, |v| {
        v["data"][0]["attributes"]["archicraft::project"]["counters"]["curtain-wall"] = 3.into();
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert_eq!(loaded.project.counters, project.counters);
}

/// ELM-011: an IFC class the element's kind does not allow is repaired to
/// the kind's default class (the descriptor's `default_class`).
#[test]
fn a_class_outside_the_kinds_set_falls_back_to_the_kind_default() {
    let project = samples::storey_with_walls_and_slab();
    for (name, class, expected) in [("W-001", "IfcCovering", "IfcWall"), ("SL-001", "IfcWall", "IfcSlab")] {
        let bytes = edit(&project, |v| {
            let i = node_index(v, name);
            v["data"][i]["attributes"]["bsi::ifc::class"]["code"] = class.into();
        });
        let loaded = archicraft_ifcx::load(&bytes).unwrap();
        let element = loaded.project.elements.values().find(|e| e.name == name).unwrap();
        assert_eq!(element.ifc_class.code(), expected, "{name}");
        assert_eq!(element.ifc_class, element.tag().descriptor().default_class, "{name}");
        assert!(
            loaded
                .problems
                .iter()
                .any(|p| p.message.to_string().contains("ifcx-element-class-unsupported")
                    && p.message.to_string().contains(expected)),
            "{name}: {:#?}",
            loaded.problems
        );
    }
}

/// INP-017: an element and a layer excluded from producing snap targets
/// keep both flags through a save and a load, and the file validates.
#[test]
fn snap_exclusion_flags_round_trip() {
    let mut project = samples::storey_with_walls_and_slab();
    let wall = project.elements.values().find(|e| e.name == "W-001").unwrap().id;
    project.elements.get_mut(&wall).unwrap().snap_excluded = true;
    let layer = project.layers.len() - 1;
    project.layers[layer].snap_excluded = true;
    let bytes = save(&project);
    let problems = validate::validate(&validate::parse(&bytes).unwrap());
    assert!(problems.is_empty(), "{problems:#?}");
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert!(loaded.project.element(wall).unwrap().snap_excluded);
    assert!(loaded.project.layers[layer].snap_excluded);
    let excluded = |p: &archicraft_model::Project| {
        let elements = p.elements.values().filter(|e| e.snap_excluded).count();
        let layers = p.layers.iter().filter(|l| l.snap_excluded).count();
        (elements, layers)
    };
    assert_eq!(excluded(&loaded.project), (1, 1));
}

/// INP-017: the flag is written only when set, so the data of a project
/// without it carries no flag, like every file written before it existed,
/// and reads as not excluded without a problem.
#[test]
fn a_missing_snap_exclusion_flag_reads_as_false() {
    for mut project in samples() {
        // The two-storey shell excludes its upper slab and a layer.
        project.elements.values_mut().for_each(|e| e.snap_excluded = false);
        project.layers.iter_mut().for_each(|l| l.snap_excluded = false);
        let bytes = save(&project);
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        let data = value["data"].to_string();
        assert!(!data.contains("snapExcluded"), "{}", project.info.name);
        let loaded = archicraft_ifcx::load(&bytes).unwrap();
        assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
        assert!(loaded.project.elements.values().all(|e| !e.snap_excluded));
        assert!(loaded.project.layers.iter().all(|l| !l.snap_excluded));
    }
}

/// INP-017, IFX-061: the embedded schemas declare `snapExcluded` of
/// `archicraft::element` and of the project's layers entries as an optional
/// boolean, so files with and without the flag validate on their own.
#[test]
fn the_snap_exclusion_flag_is_declared_as_an_optional_boolean() {
    let value: Value = serde_json::from_slice(&save(&samples::storey_with_walls_and_slab())).unwrap();
    let declared = serde_json::json!({"dataType": "Boolean", "optional": true});
    let members = |desc: &Value| desc["objectRestrictions"]["values"].clone();
    let element = members(&value["schemas"]["archicraft::element"]["value"]);
    assert_eq!(element["snapExcluded"], declared);
    let project = members(&value["schemas"]["archicraft::project"]["value"]);
    let layer = members(&project["layers"]["arrayRestrictions"]["value"]);
    assert_eq!(layer["snapExcluded"], declared);
}

/// IFX-006, INP-017: the golden file of the two-storey shell carries the
/// `snapExcluded` flag of its upper slab and of its References layer and
/// validates. The conformance check runs on every golden file
/// (conformance/run.sh in CI and in the gate), so buildingSMART's reference
/// implementation and viewer load a file with both flags.
#[test]
fn the_shell_golden_file_carries_both_snap_exclusion_flags() {
    let bytes = std::fs::read(golden_dir().join("two-storey-shell.ifcx")).unwrap();
    let problems = validate::validate(&validate::parse(&bytes).unwrap());
    assert!(problems.is_empty(), "{problems:#?}");
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    let elements: Vec<&str> = value["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["attributes"]["archicraft::element"]["snapExcluded"] == true)
        .map(|n| n["attributes"]["bsi::ifc::prop::Name"].as_str().unwrap())
        .collect();
    assert_eq!(elements, ["SL-002"]);
    let layers: Vec<&str> = value["data"][0]["attributes"]["archicraft::project"]["layers"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["snapExcluded"] == true)
        .map(|l| l["name"].as_str().unwrap())
        .collect();
    assert_eq!(layers, ["References"]);
}

/// INP-017: a flag that is not true or false breaks its declaration in the
/// embedded schemas, so validation reports it; the reader reports it too and
/// reads it as not excluded.
#[test]
fn an_invalid_snap_exclusion_flag_is_repaired_and_reported() {
    let project = samples::storey_with_walls_and_slab();
    let bytes = edit(&project, |v| {
        let i = node_index(v, "W-001");
        v["data"][i]["attributes"]["archicraft::element"]["snapExcluded"] = "yes".into();
        v["data"][0]["attributes"]["archicraft::project"]["layers"][0]["snapExcluded"] = 1.into();
    });
    let invalid = validate::validate(&validate::parse(&bytes).unwrap());
    assert_eq!(invalid.len(), 2, "{invalid:#?}");
    for problem in &invalid {
        let text = problem.message.to_string();
        assert!(
            text.contains("ifcx-attribute-invalid")
                && text.contains(r#"key="snapExcluded""#)
                && text.contains(r#"expected="boolean""#),
            "{text}"
        );
    }
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.project.elements.values().all(|e| !e.snap_excluded));
    assert!(loaded.project.layers.iter().all(|l| !l.snap_excluded));
    let repaired = loaded
        .problems
        .iter()
        .filter(|p| {
            p.message
                .to_string()
                .contains(r#"ifcx-field-not-boolean(key="snapExcluded")"#)
        })
        .count();
    assert_eq!(repaired, 2, "{:#?}", loaded.problems);
}

/// STR-009, PRJ-007: non-default slope, area and volume units are project
/// settings kept in the file.
#[test]
fn non_default_slope_area_and_volume_units_round_trip() {
    use archicraft_model::units::{AreaUnit, SlopeFormat, VolumeUnit};
    for (slope, area, volume) in [
        (SlopeFormat::Percent, AreaUnit::SquareFoot, VolumeUnit::CubicFoot),
        (SlopeFormat::Ratio, AreaUnit::SquareMeter, VolumeUnit::CubicFoot),
    ] {
        let mut project = samples::storey_with_walls_and_slab();
        project.settings.units.slope = slope;
        project.settings.units.area = area;
        project.settings.units.volume = volume;
        let bytes = save(&project);
        let problems = validate::validate(&validate::parse(&bytes).unwrap());
        assert!(problems.is_empty(), "{problems:#?}");
        let loaded = archicraft_ifcx::load(&bytes).unwrap();
        assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
        assert_eq!(loaded.project.settings.units, project.settings.units);
    }
}

/// The element named `name`.
fn element_named<'p>(project: &'p archicraft_model::Project, name: &str) -> &'p archicraft_model::Element {
    project.elements.values().find(|e| e.name == name).unwrap()
}

/// IFX-024: a slab with a floor type, a footing slab with a strip type and
/// a footing wall with a pad type save and load equal. Their nodes carry
/// `archicraft::element.predefinedType` with the IFC 4.3 literal, elements
/// without a type carry none, and the file validates.
#[test]
fn predefined_types_round_trip() {
    use archicraft_model::{Command, History, IfcClass, PredefinedType};

    let mut project = samples::storey_with_walls_and_slab();
    let mut history = History::default();
    let ground = project.storeys.ground();
    let outline = vec![
        DVec2::ZERO,
        DVec2::new(2.0, 0.0),
        DVec2::new(2.0, 1.0),
        DVec2::new(0.0, 1.0),
    ];
    let mut footing = project.new_slab(ground, outline, Vec::new(), 0);
    footing.ifc_class = IfcClass::Footing;
    footing.predefined_type = Some(PredefinedType::StripFooting);
    history
        .execute(&mut project, Command::AddElements(vec![footing]))
        .unwrap();
    let mut slab = element_named(&project, "SL-001").clone();
    slab.predefined_type = Some(PredefinedType::Floor);
    let mut wall = element_named(&project, "W-001").clone();
    wall.ifc_class = IfcClass::Footing;
    wall.predefined_type = Some(PredefinedType::PadFooting);
    history
        .execute(&mut project, Command::UpdateElements(vec![slab, wall]))
        .unwrap();

    let bytes = save(&project);
    let problems = validate::validate(&validate::parse(&bytes).unwrap());
    assert!(problems.is_empty(), "{problems:#?}");
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    for (name, literal) in [
        ("SL-001", Some("FLOOR")),
        ("SL-002", Some("STRIP_FOOTING")),
        ("W-001", Some("PAD_FOOTING")),
        ("W-002", None),
    ] {
        let common = &value["data"][node_index(&value, name)]["attributes"]["archicraft::element"];
        assert_eq!(common.get("predefinedType").and_then(Value::as_str), literal, "{name}");
    }

    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert_eq!(loaded.project.elements.len(), project.elements.len());
    for element in project.elements.values() {
        assert_eq!(loaded.project.element(element.id), Some(element), "{}", element.name);
    }
}

/// IFX-024: a type this version does not know, declared by the file's own
/// schema as a later version would write it, loads with one warning and no
/// type; the element itself loads.
#[test]
fn an_unknown_predefined_type_loads_with_one_warning_and_no_type() {
    let project = samples::two_storey_shell();
    let bytes = edit(&project, |v| {
        let i = node_index(v, "SL-001");
        v["data"][i]["attributes"]["archicraft::element"]["predefinedType"] = "PAVING".into();
        let declared =
            &mut v["schemas"]["archicraft::element"]["value"]["objectRestrictions"]["values"]["predefinedType"];
        declared["enumRestrictions"]["options"]
            .as_array_mut()
            .unwrap()
            .push("PAVING".into());
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert_eq!(loaded.problems.len(), 1, "{:#?}", loaded.problems);
    let problem = &loaded.problems[0];
    assert_eq!(problem.severity, Severity::Warning);
    let text = problem.message.to_string();
    assert!(
        text.contains(r#"name="SL-001""#)
            && text.contains(r#"ifcx-field-unknown-value(key="predefinedType", value="PAVING")"#),
        "{text}"
    );
    assert_eq!(element_named(&loaded.project, "SL-001").predefined_type, None);
    let upper = element_named(&loaded.project, "SL-002");
    assert_eq!(upper.predefined_type, element_named(&project, "SL-002").predefined_type);
}

/// IFX-024: a known type that the element's class does not offer (a floor
/// on an IfcWall wall) loads with one warning and no type, instead of
/// skipping the element.
#[test]
fn a_predefined_type_the_class_does_not_offer_loads_with_one_warning() {
    let project = samples::storey_with_walls_and_slab();
    let bytes = edit(&project, |v| {
        let i = node_index(v, "W-001");
        v["data"][i]["attributes"]["archicraft::element"]["predefinedType"] = "FLOOR".into();
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert_eq!(loaded.problems.len(), 1, "{:#?}", loaded.problems);
    assert_eq!(loaded.problems[0].severity, Severity::Warning);
    let text = loaded.problems[0].message.to_string();
    assert!(
        text.contains(r#"ifcx-predefined-type-not-offered(type="FLOOR", class="IfcWall")"#),
        "{text}"
    );
    let wall = element_named(&loaded.project, "W-001");
    assert_eq!(wall.predefined_type, None);
    assert_eq!(loaded.project.elements.len(), project.elements.len());
}

/// IFX-024, ELM-001: the tool defaults keep their predefined types through
/// a save and a load, and a slot type that the slot's class does not offer
/// loads with one warning and no type.
#[test]
fn defaults_slots_with_predefined_types_round_trip() {
    use archicraft_model::{Command, History, IfcClass, KindTag, PredefinedType};

    let mut project = samples::storey_with_walls_and_slab();
    let mut defaults = project.tool_defaults.clone();
    let wall = defaults.get_mut(KindTag::Wall).unwrap();
    wall.ifc_class = IfcClass::Footing;
    wall.predefined_type = Some(PredefinedType::StripFooting);
    defaults.get_mut(KindTag::Slab).unwrap().predefined_type = Some(PredefinedType::Roof);
    History::default()
        .execute(&mut project, Command::SetToolDefaults(defaults.clone()))
        .unwrap();

    let bytes = save(&project);
    let problems = validate::validate(&validate::parse(&bytes).unwrap());
    assert!(problems.is_empty(), "{problems:#?}");
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    let slots = &value["data"][0]["attributes"]["archicraft::project"]["toolDefaults"];
    // A common member of the slot, not a parameter (IFX-062, format 4).
    assert_eq!(slots["wall"]["predefinedType"], "STRIP_FOOTING");
    assert_eq!(slots["slab"]["predefinedType"], "ROOF");
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert_eq!(loaded.project.tool_defaults, defaults);

    let bytes = edit(&project, |v| {
        v["data"][0]["attributes"]["archicraft::project"]["toolDefaults"]["slab"]["predefinedType"] =
            "PAD_FOOTING".into();
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert_eq!(loaded.problems.len(), 1, "{:#?}", loaded.problems);
    assert_eq!(loaded.problems[0].severity, Severity::Warning);
    let slots = &loaded.project.tool_defaults;
    assert_eq!(slots.get(KindTag::Slab).unwrap().predefined_type, None);
    assert_eq!(slots.get(KindTag::Wall), defaults.get(KindTag::Wall));
}

/// The walls-and-slab sample grouped by command into Outer { Inner { W-001,
/// W-002 }, W-003 } and Pair { SL-001, W-004 }, with Inner renamed. The
/// groups are in the order Inner, Outer, Pair.
fn grouped_sample() -> (archicraft_model::Project, [archicraft_model::GroupId; 3]) {
    use archicraft_model::{Command, GroupId, History};

    let mut project = samples::storey_with_walls_and_slab();
    let mut history = History::default();
    let [w1, w2, w3, w4, s1] = ["W-001", "W-002", "W-003", "W-004", "SL-001"].map(|n| element_named(&project, n).id);
    let groups = [GroupId::new(), GroupId::new(), GroupId::new()];
    let [inner, outer, pair] = groups;
    for (group, elements) in [(inner, vec![w1, w2]), (outer, vec![w2, w3]), (pair, vec![s1, w4])] {
        history
            .execute(&mut project, Command::Group { group, elements })
            .unwrap();
    }
    history
        .execute(
            &mut project,
            Command::RenameGroup {
                group: inner,
                name: "Inner".into(),
            },
        )
        .unwrap();
    (project, groups)
}

fn groups_of(value: &mut Value) -> &mut Vec<Value> {
    value["data"][0]["attributes"]["archicraft::project"]["groups"]
        .as_array_mut()
        .unwrap()
}

/// The reports whose text contains `part`, with their severities.
fn reports_with(loaded: &archicraft_ifcx::ReadOutcome, part: &str) -> Vec<(Severity, String)> {
    loaded
        .problems
        .iter()
        .map(|p| (p.severity, p.message.to_string()))
        .filter(|(_, text)| text.contains(part))
        .collect()
}

/// ELM-015: nested groups, their names and every membership survive a save
/// and a load bit for bit, and the file validates. Only a nested group
/// carries a parent and only a grouped element a group; the group counter
/// is saved with the name counters.
#[test]
fn nested_groups_and_memberships_round_trip() {
    let (project, [inner, outer, pair]) = grouped_sample();
    let bytes = save(&project);
    let problems = validate::validate(&validate::parse(&bytes).unwrap());
    assert!(problems.is_empty(), "{problems:#?}");
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    let attr = &value["data"][0]["attributes"]["archicraft::project"];
    assert_eq!(
        attr["groups"],
        serde_json::json!([
            {"id": inner.to_string(), "name": "Inner", "parent": outer.to_string()},
            {"id": outer.to_string(), "name": "Group 2"},
            {"id": pair.to_string(), "name": "Group 3"},
        ])
    );
    assert_eq!(attr["counters"]["group"], 3);
    let group_of = |name| value["data"][node_index(&value, name)]["attributes"]["archicraft::element"]["group"].clone();
    assert_eq!(group_of("W-001"), inner.to_string());
    assert_eq!(group_of("W-003"), outer.to_string());
    assert_eq!(group_of("SL-001"), pair.to_string());

    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    let mut expected = project.clone();
    expected.revision = 0;
    expected.data_version = 7;
    assert_eq!(loaded.project, expected);
    assert!(
        loaded.project.groups.keys().eq(project.groups.keys()),
        "the order is kept"
    );
    let w1 = element_named(&loaded.project, "W-001").id;
    assert_eq!(loaded.project.group_path(w1), [outer, inner]);
}

/// ELM-015, PRJ-008: a parent cycle in the file is broken at the first of
/// its groups in file order, with a warning; every group and membership is
/// kept.
#[test]
fn a_parent_cycle_in_the_file_is_broken_with_a_warning() {
    let (project, [inner, outer, pair]) = grouped_sample();
    let bytes = edit(&project, |v| {
        let groups = groups_of(v);
        // Inner -> Outer -> Inner, and Pair -> Pair.
        groups[1]["parent"] = inner.to_string().into();
        groups[2]["parent"] = pair.to_string().into();
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let cycles = reports_with(&loaded, "ifcx-group-cycle");
    assert_eq!(cycles.len(), 2, "{:#?}", loaded.problems);
    assert!(cycles[0].1.contains("ifcx-record-group(index=0)"), "{cycles:#?}");
    assert!(cycles[1].1.contains("ifcx-record-group(index=2)"), "{cycles:#?}");
    assert!(cycles.iter().all(|(severity, _)| *severity == Severity::Warning));
    assert_eq!(loaded.problems.len(), 2, "{:#?}", loaded.problems);
    let p = &loaded.project;
    assert_eq!(p.groups.len(), 3);
    assert_eq!(p.group(inner).unwrap().parent, None);
    assert_eq!(p.group(outer).unwrap().parent, Some(inner));
    assert_eq!(p.group(pair).unwrap().parent, None);
    let w3 = element_named(p, "W-003").id;
    assert_eq!(p.group_path(w3), [inner, outer]);
}

/// ELM-015, PRJ-008: a group whose parent does not exist becomes an
/// outermost group, and an element that names a missing group is
/// ungrouped, each with a warning. The group left without members stays.
#[test]
fn a_dangling_parent_and_a_missing_group_reference_are_dropped_with_warnings() {
    let (project, [inner, outer, _]) = grouped_sample();
    let bytes = edit(&project, |v| {
        groups_of(v)[0]["parent"] = uuid::Uuid::new_v4().to_string().into();
        let i = node_index(v, "W-003");
        v["data"][i]["attributes"]["archicraft::element"]["group"] = uuid::Uuid::new_v4().to_string().into();
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert_eq!(loaded.problems.len(), 2, "{:#?}", loaded.problems);
    let parent = reports_with(&loaded, "ifcx-group-parent-missing");
    assert_eq!(parent.len(), 1, "{:#?}", loaded.problems);
    assert!(parent[0].1.contains("ifcx-record-group(index=0)"));
    let member = reports_with(&loaded, "ifcx-element-group-missing");
    assert_eq!(member.len(), 1, "{:#?}", loaded.problems);
    assert!(member[0].1.contains(r#"ifcx-record-element(name="W-003")"#));
    assert!([&parent[0], &member[0]].iter().all(|(s, _)| *s == Severity::Warning));
    let p = &loaded.project;
    assert_eq!(p.group(inner).unwrap().parent, None);
    assert_eq!(element_named(p, "W-003").group, None);
    assert_eq!(p.groups.len(), 3);
    assert!(p.group_members(outer).is_empty());
}

/// ELM-015: groups, an element's group and the group counter are written
/// only when present, so the file of a project without groups carries none
/// of them, like every file written before groups existed, and reads
/// without groups and without a problem.
#[test]
fn a_file_without_groups_loads_without_any_and_without_a_warning() {
    for project in samples() {
        assert!(project.groups.is_empty());
        let bytes = save(&project);
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        let attr = &value["data"][0]["attributes"]["archicraft::project"];
        assert!(attr.get("groups").is_none(), "{}", project.info.name);
        assert!(attr["counters"].get("group").is_none(), "{}", project.info.name);
        let nodes = value["data"].as_array().unwrap();
        assert!(
            nodes
                .iter()
                .all(|n| n["attributes"]["archicraft::element"].get("group").is_none())
        );
        let loaded = archicraft_ifcx::load(&bytes).unwrap();
        assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
        assert!(loaded.project.groups.is_empty());
        assert_eq!(loaded.project.group_counter, 0);
        assert!(loaded.project.elements.values().all(|e| e.group.is_none()));
    }
}

/// ELM-015, IFX-061: the embedded schemas declare the project's groups, an
/// element's group and the group counter as optional members, so files with
/// and without groups validate on their own.
#[test]
fn groups_are_declared_as_optional_members() {
    use serde_json::json;

    let value: Value = serde_json::from_slice(&save(&samples::storey_with_walls_and_slab())).unwrap();
    let members = |desc: &Value| desc["objectRestrictions"]["values"].clone();
    let project = members(&value["schemas"]["archicraft::project"]["value"]);
    assert_eq!(project["groups"]["dataType"], "Array");
    assert_eq!(project["groups"]["optional"], true);
    let group = members(&project["groups"]["arrayRestrictions"]["value"]);
    assert_eq!(group["id"], json!({"dataType": "String"}));
    assert_eq!(group["name"], json!({"dataType": "String"}));
    assert_eq!(group["parent"], json!({"dataType": "String", "optional": true}));
    let element = members(&value["schemas"]["archicraft::element"]["value"]);
    assert_eq!(element["group"], json!({"dataType": "String", "optional": true}));
    let counters = members(&project["counters"]);
    assert_eq!(counters["group"], json!({"dataType": "Integer", "optional": true}));
    // The group counter shares the counters object with the kinds' counters.
    assert!(archicraft_model::KindTag::ALL.iter().all(|k| k.code() != "group"));
}

/// PRJ-008: a group record that is not an object, one without an identity
/// and a second record of an identity already read are skipped and
/// reported; a missing name is repaired with a default.
#[test]
fn damaged_group_records_are_skipped_or_repaired_and_reported() {
    let (project, [inner, outer, pair]) = grouped_sample();
    let bytes = edit(&project, |v| {
        let groups = groups_of(v);
        groups[2].as_object_mut().unwrap().remove("name");
        groups.push("a group".into());
        groups.push(serde_json::json!({"name": "No id"}));
        groups.push(serde_json::json!({"id": outer.to_string(), "name": "Again"}));
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let skipped = reports_with(&loaded, "ifcx-skipped(record=ifcx-record-group");
    assert_eq!(skipped.len(), 3, "{:#?}", loaded.problems);
    assert!(skipped[0].1.contains("index=3") && skipped[0].1.contains("ifcx-not-object"));
    assert!(skipped[1].1.contains("index=4") && skipped[1].1.contains(r#"ifcx-field-missing(key="id")"#));
    assert!(skipped[2].1.contains("index=5") && skipped[2].1.contains("ifcx-group-listed-twice"));
    let repaired = reports_with(&loaded, "ifcx-repaired(record=ifcx-record-group(index=2)");
    assert_eq!(repaired.len(), 1, "{:#?}", loaded.problems);
    let p = &loaded.project;
    assert!(p.groups.keys().eq([inner, outer, pair].iter()));
    assert_eq!(p.group(outer).unwrap().name, "Group 2");
    assert_eq!(p.group(pair).unwrap().name, "Group");
}

/// ELM-015, PRJ-008: an invalid group counter is repaired to the number of
/// groups, so that automatic group names do not repeat.
#[test]
fn an_invalid_group_counter_is_repaired_to_the_number_of_groups() {
    let (project, _) = grouped_sample();
    for (counter, reason) in [
        (serde_json::json!(-1), "ifcx-counter-negative"),
        (serde_json::json!("x"), "ifcx-value-used"),
    ] {
        let bytes = edit(&project, |v| {
            v["data"][0]["attributes"]["archicraft::project"]["counters"]["group"] = counter.clone();
        });
        let loaded = archicraft_ifcx::load(&bytes).unwrap();
        assert_eq!(loaded.project.group_counter, 3);
        let repaired = reports_with(&loaded, "ifcx-record-counters");
        assert_eq!(repaired.len(), 1, "{:#?}", loaded.problems);
        assert!(repaired[0].1.contains(reason), "{repaired:#?}");
    }
}

/// SLB-003 ("any number of holes"), IFX-064, archicraft-ysf8.3: a slab with
/// two holes is written with both in its `holes` array and a closed body
/// (IFX-032), validates, and reads back equal to the saved model.
#[test]
fn a_slab_with_two_holes_round_trips() {
    let mut project = archicraft_model::template::default_metric_project();
    let rect = |x0: f64, y0: f64, x1: f64, y1: f64| {
        vec![
            DVec2::new(x0, y0),
            DVec2::new(x1, y0),
            DVec2::new(x1, y1),
            DVec2::new(x0, y1),
        ]
    };
    let holes = vec![rect(1.0, 1.0, 3.0, 2.0), rect(6.0, 4.0, 7.5, 5.5)];
    let slab = project.new_slab(project.storeys.ground(), rect(0.0, 0.0, 10.0, 8.0), holes, 0);
    archicraft_model::History::default()
        .execute(&mut project, archicraft_model::Command::AddElements(vec![slab]))
        .unwrap();

    let bytes = save(&project);
    let file = validate::parse(&bytes).unwrap();
    assert!(validate::validate(&file).is_empty());
    let written: Vec<&Value> = file
        .data
        .iter()
        .filter_map(|node| node.attributes.get("archicraft::slab"))
        .collect();
    assert_eq!(written.len(), 1);
    assert_eq!(written[0]["holes"].as_array().unwrap().len(), 2);
    let meshes = meshes(&file);
    assert_eq!(meshes.len(), 1);
    assert_eq!(meshes[0].1.check_closed(), Ok(()));

    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    let mut expected = project;
    expected.revision = 0;
    expected.data_version = 7;
    assert_eq!(loaded.project, expected);
}

/// The element named `name` in `project`, mutably.
fn element_named_mut<'p>(project: &'p mut archicraft_model::Project, name: &str) -> &'p mut archicraft_model::Element {
    project.elements.values_mut().find(|e| e.name == name).unwrap()
}

/// ELM-012, ELM-020, IFX-022: three walls (exterior and load-bearing,
/// interior and non-load-bearing, both undefined) and a slab keep their
/// position and structural function through a save and a load. The wall
/// nodes carry `bsi::ifc::prop::IsExternal` true, false and none, and
/// `archicraft::element` keeps its members in the order kind, layer,
/// locked, position, structuralFunction; an undefined function is not
/// written.
#[test]
fn position_and_structural_function_round_trip() {
    use archicraft_model::{Position, StructuralFunction};

    let mut project = samples::storey_with_walls_and_slab();
    let flags = [
        ("W-001", Position::Exterior, StructuralFunction::LoadBearing),
        ("W-002", Position::Interior, StructuralFunction::NonLoadBearing),
        ("W-003", Position::Undefined, StructuralFunction::Undefined),
        ("SL-001", Position::Interior, StructuralFunction::LoadBearing),
    ];
    for (name, position, structural) in flags {
        let e = element_named_mut(&mut project, name);
        e.position = position;
        e.structural = structural;
    }
    let bytes = save(&project);
    let problems = validate::validate(&validate::parse(&bytes).unwrap());
    assert!(problems.is_empty(), "{problems:#?}");

    let value: Value = serde_json::from_slice(&bytes).unwrap();
    let node = |name: &str| &value["data"][node_index(&value, name)]["attributes"];
    assert_eq!(node("W-001")["bsi::ifc::prop::IsExternal"], Value::Bool(true));
    assert_eq!(node("W-002")["bsi::ifc::prop::IsExternal"], Value::Bool(false));
    assert!(node("W-003").get("bsi::ifc::prop::IsExternal").is_none());
    let written = |name: &str| node(name)["archicraft::element"].get("structuralFunction").cloned();
    assert_eq!(written("W-001"), Some("load-bearing".into()));
    assert_eq!(written("W-002"), Some("non-load-bearing".into()));
    assert_eq!(written("W-003"), None, "an undefined function is not written");
    assert_eq!(written("SL-001"), Some("load-bearing".into()));
    let keys: Vec<&str> = node("W-001")["archicraft::element"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys[..5], ["kind", "layer", "locked", "position", "structuralFunction"]);

    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    for (name, position, structural) in flags {
        let e = element_named(&loaded.project, name);
        assert_eq!((e.position, e.structural), (position, structural), "{name}");
    }
    let mut expected = project.clone();
    expected.revision = 0;
    expected.data_version = 7;
    assert_eq!(loaded.project, expected);
}

/// ELM-020, ELM-001: a tool's default structural function is saved with its
/// slot when set and read back; the template's undefined one is not written.
#[test]
fn a_default_structural_function_round_trips_with_its_slot() {
    use archicraft_model::{KindTag, StructuralFunction};

    let mut project = samples::storey_with_walls_and_slab();
    let slots = |value: &Value| value["data"][0]["attributes"]["archicraft::project"]["toolDefaults"].clone();
    let untouched: Value = serde_json::from_slice(&save(&project)).unwrap();
    assert!(slots(&untouched)["wall"].get("structuralFunction").is_none());

    project.tool_defaults.get_mut(KindTag::Wall).unwrap().structural = StructuralFunction::LoadBearing;
    let bytes = save(&project);
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(slots(&value)["wall"]["structuralFunction"], "load-bearing");
    assert!(slots(&value)["slab"].get("structuralFunction").is_none());
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert_eq!(loaded.project.tool_defaults, project.tool_defaults);
}

/// ELM-020, PRJ-008: an unknown structural function breaks its declaration,
/// which validation reports, and the reader loads the element with an
/// undefined function and exactly one warning naming the element.
#[test]
fn an_unknown_structural_function_loads_as_undefined_with_one_warning() {
    use archicraft_model::StructuralFunction;

    let project = samples::storey_with_walls_and_slab();
    let bytes = edit(&project, |v| {
        let i = node_index(v, "W-001");
        v["data"][i]["attributes"]["archicraft::element"]["structuralFunction"] = "bogus".into();
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert_eq!(
        element_named(&loaded.project, "W-001").structural,
        StructuralFunction::Undefined
    );
    let naming: Vec<_> = loaded
        .problems
        .iter()
        .filter(|p| p.message.to_string().contains(r#"name="W-001""#))
        .collect();
    assert_eq!(naming.len(), 1, "{:#?}", loaded.problems);
    assert_eq!(naming[0].severity, Severity::Warning);
    let text = naming[0].message.to_string();
    assert!(
        text.contains(r#"key="structuralFunction""#) && text.contains(r#"value="bogus""#),
        "{text}"
    );
    // The other report is the schema check's (IFX-061), at the same node.
    let others: Vec<String> = loaded
        .problems
        .iter()
        .filter(|p| !p.message.to_string().contains(r#"name="W-001""#))
        .map(|p| p.message.to_string())
        .collect();
    assert!(
        others.len() == 1 && others[0].contains("ifcx-attribute-invalid") && others[0].contains("structuralFunction"),
        "{others:#?}"
    );
}

/// ELM-020: a file without the member, like every file written before it
/// existed and every element whose function is undefined, loads without
/// any problem and with undefined functions.
#[test]
fn a_missing_structural_function_reads_as_undefined_without_problems() {
    use archicraft_model::StructuralFunction;

    let mut project = samples::storey_with_walls_and_slab();
    element_named_mut(&mut project, "W-001").structural = StructuralFunction::LoadBearing;
    let bytes = edit(&project, |v| {
        let i = node_index(v, "W-001");
        let element = v["data"][i]["attributes"]["archicraft::element"]
            .as_object_mut()
            .unwrap();
        assert!(element.remove("structuralFunction").is_some());
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert!(
        loaded
            .project
            .elements
            .values()
            .all(|e| e.structural == StructuralFunction::Undefined)
    );
    for project in samples() {
        let value: Value = serde_json::from_slice(&save(&project)).unwrap();
        assert!(
            !value["data"].to_string().contains("structuralFunction"),
            "{}",
            project.info.name
        );
    }
}

/// ELM-020, IFX-061: the embedded schemas declare `structuralFunction` of
/// `archicraft::element` and of every tool's defaults slot as an optional
/// enumeration of the codes, so files with and without it validate.
#[test]
fn the_structural_function_is_declared_as_an_optional_enumeration() {
    use archicraft_model::StructuralFunction;

    let value: Value = serde_json::from_slice(&save(&samples::storey_with_walls_and_slab())).unwrap();
    let codes: Vec<&str> = StructuralFunction::ALL.map(StructuralFunction::code).to_vec();
    let declared = serde_json::json!({"dataType": "Enum", "enumRestrictions": {"options": codes}, "optional": true});
    let members = |desc: &Value| desc["objectRestrictions"]["values"].clone();
    let element = members(&value["schemas"]["archicraft::element"]["value"]);
    assert_eq!(element["structuralFunction"], declared);
    let project = members(&value["schemas"]["archicraft::project"]["value"]);
    let slots = members(&project["toolDefaults"]);
    for kind in ["wall", "slab"] {
        assert_eq!(members(&slots[kind])["structuralFunction"], declared, "{kind}");
    }
}

/// The bit patterns of every number of `profiles` the file stores: the
/// origins, the parametric dimensions and the ring points, in order.
fn profile_bits(profiles: &[archicraft_model::Profile]) -> Vec<u64> {
    use archicraft_model::ProfileShape;

    let mut bits = Vec::new();
    for profile in profiles {
        bits.extend([profile.origin.x, profile.origin.y].map(f64::to_bits));
        match &profile.shape {
            ProfileShape::Parametric { shape, .. } => {
                bits.extend(shape.dimensions().into_iter().map(|(_, value)| value.to_bits()));
            }
            ProfileShape::Custom { regions } => {
                for region in regions {
                    for point in std::iter::once(&region.outer).chain(&region.holes).flatten() {
                        bits.extend([point.x.to_bits(), point.y.to_bits()]);
                    }
                }
            }
        }
    }
    bits
}

/// PRF-004, PRF-005, PRJ-007, IFX-060, IFX-011, NFR-ACC-002: a project with
/// a profile of every parametric shape, a custom profile of two regions of
/// different materials, one with a hole, and a catalogue profile saves and
/// loads equal, in list order: identities, names, origins, usage flags,
/// catalogue designations, dimensions and ring points, these bit for bit.
/// An element that uses a profile, the test kind standing in for a column,
/// keeps its reference.
#[test]
fn profiles_round_trip() {
    use archicraft_model::kinds::Dummy;
    use archicraft_model::{Command, History, ParametricShape, ProfileShape};

    let mut project = common::profile_sample();
    let codes: Vec<&str> = project
        .profiles
        .iter()
        .filter_map(|p| match &p.shape {
            ProfileShape::Parametric { shape, .. } => Some(shape.code()),
            ProfileShape::Custom { .. } => None,
        })
        .collect();
    for shape in ParametricShape::ALL {
        assert!(codes.contains(&shape.code()), "{codes:?}");
    }
    let custom = project
        .profiles
        .iter()
        .find(|p| p.name == common::CUSTOM_PROFILE)
        .unwrap();
    let ProfileShape::Custom { regions } = &custom.shape else {
        panic!("{custom:?}");
    };
    assert_eq!(regions.len(), 2);
    assert_ne!(regions[0].material, regions[1].material);
    assert_eq!(regions[0].holes.len(), 1);
    let catalogue = project.profiles.iter().find(|p| p.catalogue.is_some()).unwrap().id;

    let column = Dummy {
        profile: Some(catalogue),
        ..Dummy::new(DVec2::ZERO, DVec2::ONE, project.materials[0].id)
    };
    let element = column.into_element(&project, project.storeys.ground(), 0);
    History::default()
        .execute(&mut project, Command::AddElements(vec![element]))
        .unwrap();

    let loaded = archicraft_ifcx::load(&save(&project)).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert_eq!(loaded.project.profiles, project.profiles);
    assert_eq!(profile_bits(&loaded.project.profiles), profile_bits(&project.profiles));
    let used = loaded.project.elements.values().find_map(|e| e.as_dummy()?.profile);
    assert_eq!(used, Some(catalogue));
    let mut expected = project.clone();
    expected.revision = 0;
    expected.data_version = 7;
    assert_eq!(loaded.project, expected);
}

/// The profile sample saved, with `change` applied to the `profiles` array
/// of its project attribute.
fn with_profile_records(project: &archicraft_model::Project, change: impl FnOnce(&mut Vec<Value>)) -> Vec<u8> {
    edit(project, |v| {
        change(
            v["data"][0]["attributes"]["archicraft::project"]["profiles"]
                .as_array_mut()
                .unwrap(),
        );
    })
}

/// The one problem of `loaded`, asserted to have `severity`.
fn only_problem(loaded: &archicraft_ifcx::ReadOutcome, severity: Severity) -> String {
    assert_eq!(loaded.problems.len(), 1, "{:#?}", loaded.problems);
    assert_eq!(loaded.problems[0].severity, severity, "{:#?}", loaded.problems);
    loaded.problems[0].message.to_string()
}

/// The one problem the reader reports for `loaded`, asserted to have
/// `severity`, besides validation's report of a value that breaks its
/// declaration (IFX-061), which a record of the wrong type also gets.
fn only_reader_problem(loaded: &archicraft_ifcx::ReadOutcome, severity: Severity) -> String {
    let (validation, reader): (Vec<_>, Vec<_>) = loaded
        .problems
        .iter()
        .partition(|p| p.message.to_string().starts_with("ifcx-attribute-invalid("));
    assert!(validation.len() <= 1, "{validation:#?}");
    assert_eq!(reader.len(), 1, "{:#?}", loaded.problems);
    assert_eq!(reader[0].severity, severity, "{reader:#?}");
    reader[0].message.to_string()
}

/// PRF-004, PRF-005, PRJ-008, IFX-083: a profile that breaks the model's
/// rules, a rectangle of negative width, is skipped with one error that
/// names it and gives the model's reason; the other profiles load.
#[test]
fn a_profile_with_a_negative_width_is_skipped_with_one_error_naming_it() {
    use archicraft_model::{ParametricShape, ProfileShape};

    let project = common::profile_sample();
    let at = project
        .profiles
        .iter()
        .position(|p| {
            matches!(
                p.shape,
                ProfileShape::Parametric {
                    shape: ParametricShape::Rectangle { .. },
                    ..
                }
            )
        })
        .unwrap();
    let bytes = with_profile_records(&project, |records| {
        records[at]["shape"]["dimensions"]["width"] = (-0.3).into();
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let message = only_problem(&loaded, Severity::Error);
    assert!(message.starts_with("ifcx-skipped("), "{message}");
    assert!(message.contains(&project.profiles[at].name), "{message}");
    assert!(message.contains("model-error-profile-dimension-range"), "{message}");
    let mut expected = project.profiles.clone();
    expected.remove(at);
    assert_eq!(loaded.project.profiles, expected);
}

/// PRF-004, PRJ-008: a region, or a parametric shape, made of a material
/// the project does not have loads with the project's first material and
/// one warning that names the profile; the rest of the profile is kept.
#[test]
fn an_unknown_profile_material_is_repaired_with_one_warning() {
    use archicraft_model::{MaterialId, ProfileShape};

    let project = common::profile_sample();
    let first = project.materials[0].id;
    let custom = project
        .profiles
        .iter()
        .position(|p| p.name == common::CUSTOM_PROFILE)
        .unwrap();
    let bytes = with_profile_records(&project, |records| {
        records[custom]["shape"]["regions"][1]["material"] = MaterialId::new().to_string().into();
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let message = only_problem(&loaded, Severity::Warning);
    assert!(message.starts_with("ifcx-repaired("), "{message}");
    assert!(message.contains(common::CUSTOM_PROFILE), "{message}");
    assert!(message.contains("ifcx-profile-material-missing"), "{message}");
    let mut expected = project.profiles.clone();
    if let ProfileShape::Custom { regions } = &mut expected[custom].shape {
        regions[1].material = first;
    }
    assert_eq!(loaded.project.profiles, expected);

    let bytes = with_profile_records(&project, |records| {
        records[0]["shape"]["material"] = MaterialId::new().to_string().into();
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let message = only_problem(&loaded, Severity::Warning);
    assert!(message.contains(&project.profiles[0].name), "{message}");
    assert!(message.contains("ifcx-profile-material-missing"), "{message}");
    assert_eq!(loaded.project.profiles[0].materials(), [first]);
    assert_eq!(loaded.project.profiles[1..], project.profiles[1..]);
}

/// PRF-004, PRJ-008, IFX-011: a later record of a loaded profile's
/// identity is skipped with one error that names it, before the rest of it
/// is read, so the first record is kept and the twin's own defects go
/// unreported. A record whose earlier twin was rejected is the profile.
#[test]
fn a_duplicate_profile_id_is_reported_and_the_first_record_kept() {
    use archicraft_model::MaterialId;

    let project = common::profile_sample();
    let bytes = with_profile_records(&project, |records| {
        let mut twin = records[0].clone();
        twin["name"] = "Twin".into();
        // Each repaired with a warning if the twin were read.
        twin["shape"]["material"] = MaterialId::new().to_string().into();
        twin["shape"]["dimensions"]["diameter"] = 0.2.into();
        records.push(twin);
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let message = only_problem(&loaded, Severity::Error);
    assert!(message.starts_with("ifcx-skipped("), "{message}");
    assert!(message.contains("ifcx-profile-listed-twice"), "{message}");
    assert!(message.contains("Twin"), "{message}");
    assert_eq!(loaded.project.profiles, project.profiles);

    let bytes = with_profile_records(&project, |records| {
        let mut twin = records[0].clone();
        twin["name"] = "Twin".into();
        records[0]["shape"]["dimensions"]["width"] = (-0.3).into();
        records.push(twin);
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let message = only_problem(&loaded, Severity::Error);
    assert!(message.contains("model-error-profile-dimension-range"), "{message}");
    let mut expected = project.profiles.clone();
    let mut twin = expected.remove(0);
    twin.name = "Twin".into();
    expected.push(twin);
    assert_eq!(loaded.project.profiles, expected);
}

/// PRF-004, PRF-005, PRJ-008, IFX-083: a profile record that cannot be read
/// is skipped with one error: not an object, without an identity or a
/// name, of a shape this release does not know, without a dimension its
/// shape needs or a ring of its regions. A field that has a default takes
/// it with one warning: the usage flags and the origin; a catalogue
/// designation that is not text, a dimension the shape does not have and a
/// member of the other kind of shape, the regions of a parametric shape or
/// the material of a custom one, are left out with one (IFX-083).
/// Validation reports a value of the wrong type on its own as well
/// (IFX-061).
#[test]
fn unreadable_profile_records_are_skipped_or_repaired_with_one_report() {
    use archicraft_model::{MaterialId, ProfileUsage};

    let project = common::profile_sample();
    let custom = project
        .profiles
        .iter()
        .position(|p| p.name == common::CUSTOM_PROFILE)
        .unwrap();
    // Profile 0 is the rectangle.
    type Change = Box<dyn Fn(&mut Vec<Value>)>;
    let skipped: Vec<(&str, Change)> = vec![
        ("ifcx-not-object", Box::new(|r| r[0] = "rectangle".into())),
        (
            "ifcx-field-missing(key=\"id\")",
            Box::new(|r| {
                r[0].as_object_mut().unwrap().remove("id");
            }),
        ),
        (
            "ifcx-field-not-text(key=\"name\")",
            Box::new(|r| r[0]["name"] = 5.into()),
        ),
        (
            "ifcx-field-unknown-value(key=\"kind\", value=\"hexagon\")",
            Box::new(|r| r[0]["shape"]["kind"] = "hexagon".into()),
        ),
        (
            "ifcx-field-missing(key=\"height\")",
            Box::new(|r| {
                r[0]["shape"]["dimensions"].as_object_mut().unwrap().remove("height");
            }),
        ),
        (
            "ifcx-field-point(key=\"outer\"",
            Box::new(move |r| r[custom]["shape"]["regions"][1]["outer"] = "square".into()),
        ),
        (
            "ifcx-field-point(key=\"holes\"",
            Box::new(move |r| r[custom]["shape"]["regions"][0]["holes"] = Value::Array(vec![1.into()])),
        ),
    ];
    for (reason, change) in &skipped {
        let loaded = archicraft_ifcx::load(&with_profile_records(&project, change)).unwrap();
        let message = only_reader_problem(&loaded, Severity::Error);
        assert!(
            message.starts_with("ifcx-skipped(") && message.contains(reason),
            "{reason}: {message}"
        );
        assert_eq!(loaded.project.profiles.len(), project.profiles.len() - 1, "{reason}");
    }

    // Profile 4 is the I shape with an origin, 8 the Z shape with other
    // usage flags than the default, the last the catalogue profile.
    let last = project.profiles.len() - 1;
    assert_ne!(project.profiles[8].usage, ProfileUsage::default());
    type Repair = fn(&mut archicraft_model::Profile);
    let repaired: Vec<(&str, usize, Change, Repair)> = vec![
        (
            "ifcx-default-used",
            8,
            Box::new(|r| r[8]["usage"]["walls"] = "yes".into()),
            |p| p.usage.walls = ProfileUsage::default().walls,
        ),
        (
            "ifcx-default-used",
            8,
            Box::new(|r| {
                r[8].as_object_mut().unwrap().remove("usage");
            }),
            |p| p.usage = ProfileUsage::default(),
        ),
        (
            "ifcx-default-used",
            4,
            Box::new(|r| r[4]["origin"] = Value::Array(vec![1.into()])),
            |p| p.origin = DVec2::ZERO,
        ),
        (
            "ifcx-left-out",
            last,
            Box::new(move |r| r[last]["catalogue"] = 5.into()),
            |p| p.catalogue = None,
        ),
        (
            "ifcx-profile-member-unused(key=\"diameter\")",
            0,
            Box::new(|r| r[0]["shape"]["dimensions"]["diameter"] = 0.2.into()),
            |_| {},
        ),
        (
            "ifcx-profile-member-unused(key=\"regions\")",
            0,
            Box::new(|r| r[0]["shape"]["regions"] = Value::Array(Vec::new())),
            |_| {},
        ),
        (
            "ifcx-profile-member-unused(key=\"material\")",
            custom,
            Box::new(move |r| r[custom]["shape"]["material"] = MaterialId::new().to_string().into()),
            |_| {},
        ),
    ];
    for (reason, at, change, repair) in &repaired {
        let loaded = archicraft_ifcx::load(&with_profile_records(&project, change)).unwrap();
        let message = only_reader_problem(&loaded, Severity::Warning);
        assert!(
            message.starts_with("ifcx-repaired(") && message.contains(reason),
            "{reason}: {message}"
        );
        let mut expected = project.profiles.clone();
        repair(&mut expected[*at]);
        assert_eq!(loaded.project.profiles, expected, "{reason}");
    }
}

/// PRF-004, IFX-062: a file without a `profiles` member, as every file
/// before format 5 is, loads without profiles and without a problem.
#[test]
fn a_file_without_profiles_loads_without_any_and_without_a_problem() {
    let project = samples::storey_with_walls_and_slab();
    let bytes = edit(&project, |v| {
        let attr = v["data"][0]["attributes"]["archicraft::project"]
            .as_object_mut()
            .unwrap();
        assert_eq!(attr.remove("profiles"), Some(Value::Array(Vec::new())));
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert!(loaded.project.profiles.is_empty());
}

/// PRF-004, PRJ-008: an element's profile that the file does not hold is
/// left out of the element with one warning; the element loads.
#[test]
fn an_element_profile_the_file_does_not_hold_is_left_out_with_one_warning() {
    use archicraft_model::kinds::Dummy;
    use archicraft_model::{Command, History};

    let mut project = common::profile_sample();
    let last = project.profiles.len() - 1;
    let column = Dummy {
        profile: Some(project.profiles[last].id),
        ..Dummy::new(DVec2::ZERO, DVec2::ONE, project.materials[0].id)
    };
    let element = column.into_element(&project, project.storeys.ground(), 0);
    History::default()
        .execute(&mut project, Command::AddElements(vec![element.clone()]))
        .unwrap();
    let bytes = with_profile_records(&project, |records| {
        records.pop();
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let message = only_problem(&loaded, Severity::Warning);
    assert!(message.contains("ifcx-element-profile-missing"), "{message}");
    assert!(message.contains(&element.name), "{message}");
    let read = loaded.project.element(element.id).and_then(|e| e.as_dummy()).unwrap();
    assert_eq!(read.profile, None);
}
