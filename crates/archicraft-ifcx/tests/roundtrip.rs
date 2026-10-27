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

/// IFX-062: a file of a newer format is flagged, not migrated.
#[test]
fn newer_format_versions_are_flagged() {
    let project = samples::storey_with_walls_and_slab();
    let bytes = edit(&project, |v| {
        v["data"][0]["attributes"]["archicraft::project"]["formatVersion"] = "99".into();
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.newer_format);
    assert_eq!(loaded.migrated_from, None);
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
