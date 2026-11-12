//! Saved camera views in IFCX (VIEW-004, IFX-001, IFX-061): a 3D view's
//! record in `archicraft::project` holds its saved viewpoint as an optional
//! `camera` member (target, distance, yaw, pitch and vertical field of
//! view), declared in the file's own `archicraft::` schemas. A view without
//! one, as in every file written before cameras existed, reads as a plain
//! viewpoint, and a damaged camera is left out with a warning (PRJ-008).

#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};

use archicraft_ifcx::validate::{self, Severity};
use archicraft_model::glam::DVec3;
use archicraft_model::project::{Projection, View3d, ViewCamera};
use archicraft_model::{Command, History, Project, ViewId, samples};
use serde_json::{Value, json};

use common::{edit, save};

fn tests_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

/// The `.ifcx` files in the folder `name` under `tests/`.
fn files_in(name: &str) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(tests_dir().join(name))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "ifcx"))
        .collect();
    files.sort();
    files
}

/// The view records of `archicraft::project` in a file.
fn view_records(bytes: &[u8]) -> Vec<Value> {
    let value: Value = serde_json::from_slice(bytes).unwrap();
    value["data"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|n| n["attributes"].get("archicraft::project"))
        .unwrap()["views"]
        .as_array()
        .unwrap()
        .clone()
}

/// A perspective camera with values of many digits, such as 4 - 1/3
/// (NFR-ACC-002).
fn perspective_camera() -> ViewCamera {
    ViewCamera {
        target: DVec3::new(5.0 + 0.1 + 0.2, 4.0 - 1.0 / 3.0, 1.5),
        distance: 18.0 + 0.1 + 0.2,
        yaw: -std::f64::consts::FRAC_PI_3,
        pitch: 0.1 + 0.2 + 0.15,
        fov_y: std::f64::consts::FRAC_PI_4,
    }
}

fn parallel_camera() -> ViewCamera {
    ViewCamera {
        target: DVec3::new(-2.5, 1.0 / 7.0, 3.0),
        distance: 30.0,
        yaw: 2.0 * std::f64::consts::PI / 3.0,
        pitch: -std::f64::consts::FRAC_PI_6,
        fov_y: 0.7,
    }
}

/// The walls-and-slab sample with a saved perspective and a saved parallel
/// camera view, saved through `SaveCamera` like the app does (VIEW-004).
fn project_with_cameras() -> (Project, [ViewId; 2]) {
    let mut project = samples::storey_with_walls_and_slab();
    let mut history = History::default();
    let ids = [ViewId::new(), ViewId::new()];
    for (view, projection, camera) in [
        (ids[0], Projection::Perspective, perspective_camera()),
        (ids[1], Projection::Parallel, parallel_camera()),
    ] {
        history
            .execute(
                &mut project,
                Command::SaveCamera {
                    view,
                    base: "Camera".into(),
                    projection,
                    camera,
                },
            )
            .unwrap();
    }
    (project, ids)
}

/// Asserts that `read` is `expected` with its camera within 1e-9.
fn assert_view_close(read: &View3d, expected: &View3d) {
    assert_eq!(
        (read.id, read.name.as_str(), read.projection),
        (expected.id, expected.name.as_str(), expected.projection)
    );
    let (read, expected) = (read.camera.unwrap(), expected.camera.unwrap());
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-9;
    assert!(
        (read.target - expected.target).length() <= 1e-9
            && close(read.distance, expected.distance)
            && close(read.yaw, expected.yaw)
            && close(read.pitch, expected.pitch)
            && close(read.fov_y, expected.fov_y),
        "{read:?} is not {expected:?}"
    );
}

/// VIEW-004, IFX-001: a project with a saved perspective and a saved
/// parallel camera view writes a file that validates on its own and reads
/// back the same views, each with its name, projection and camera (within
/// 1e-9), without a problem.
#[test]
fn camera_views_round_trip_with_their_projection_and_camera() {
    let (project, ids) = project_with_cameras();
    let bytes = save(&project);
    let problems = validate::validate(&validate::parse(&bytes).unwrap());
    assert!(problems.is_empty(), "{problems:#?}");

    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert_eq!(loaded.project.views.len(), project.views.len());
    for (read, expected) in loaded.project.views.iter().zip(&project.views) {
        match expected.camera {
            Some(_) => assert_view_close(read, expected),
            None => assert_eq!(read, expected, "a view without a camera stays without one"),
        }
    }
    let saved = |id| loaded.project.view(id).unwrap();
    assert_eq!(
        (saved(ids[0]).name.as_str(), saved(ids[0]).projection),
        ("Camera 1", Projection::Perspective)
    );
    assert_eq!(
        (saved(ids[1]).name.as_str(), saved(ids[1]).projection),
        ("Camera 2", Projection::Parallel)
    );
    // Parameters are written at full precision (NFR-ACC-002).
    assert_eq!(loaded.project.views, project.views, "bit for bit");
}

/// VIEW-004: the camera is a member of the view's record, with its target
/// as a point of three coordinates and the angles in radians; a view
/// without a camera has no `camera` member.
#[test]
fn the_camera_is_written_as_a_member_of_the_view_record() {
    let (project, ids) = project_with_cameras();
    let records = view_records(&save(&project));
    let record = |id: ViewId| {
        records
            .iter()
            .find(|r| r["id"] == id.to_string().as_str())
            .unwrap()
            .clone()
    };
    let camera = perspective_camera();
    assert_eq!(
        record(ids[0])["camera"],
        json!({
            "target": [camera.target.x, camera.target.y, camera.target.z],
            "distance": camera.distance,
            "yaw": camera.yaw,
            "pitch": camera.pitch,
            "fovY": camera.fov_y,
        })
    );
    assert_eq!(record(ids[1])["projection"], "parallel");
    assert!(record(ids[1])["camera"].is_object());
    let plain = &project.views[0];
    assert!(plain.camera.is_none());
    assert!(record(plain.id).get("camera").is_none(), "{}", record(plain.id));
}

/// IFX-061: the embedded schemas declare the view's `camera` as an optional
/// object of a point of lengths, a length and three plane angles, so files
/// with and without cameras validate on their own.
#[test]
fn the_view_camera_is_declared_as_an_optional_object() {
    let value: Value = serde_json::from_slice(&save(&samples::storey_with_walls_and_slab())).unwrap();
    let members = |desc: &Value| desc["objectRestrictions"]["values"].clone();
    let project = members(&value["schemas"]["archicraft::project"]["value"]);
    let view = members(&project["views"]["arrayRestrictions"]["value"]);
    let camera = &view["camera"];
    assert_eq!(camera["dataType"], "Object");
    assert_eq!(camera["optional"], true);
    let length = json!({"dataType": "Real", "quantityKind": "Length"});
    let angle = json!({"dataType": "Real", "quantityKind": "Plane angle"});
    assert_eq!(
        members(camera),
        json!({
            "target": {"dataType": "Array", "arrayRestrictions": {"value": length}},
            "distance": length,
            "yaw": angle,
            "pitch": angle,
            "fovY": angle,
        })
    );
}

/// VIEW-004, IFX-006: the golden file of the two-storey shell with columns
/// holds a saved perspective and a saved parallel camera view, and reads
/// them back as the sample has them. Every golden file passes the
/// conformance check (conformance/run.sh in CI and in the gate).
#[test]
fn the_columns_golden_file_carries_a_perspective_and_a_parallel_camera() {
    let bytes = std::fs::read(tests_dir().join("golden/two-storey-shell-with-columns.ifcx")).unwrap();
    let cameras: Vec<(String, String)> = view_records(&bytes)
        .iter()
        .filter(|r| r["camera"].is_object())
        .map(|r| {
            let text = |key: &str| r[key].as_str().unwrap().to_owned();
            (text("name"), text("projection"))
        })
        .collect();
    assert_eq!(
        cameras,
        [
            ("Camera 1".to_owned(), "perspective".to_owned()),
            ("Camera 2".to_owned(), "parallel".to_owned())
        ]
    );
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert_eq!(loaded.project.views, samples::two_storey_shell_with_columns().views);
}

/// VIEW-004, IFX-062: the golden files without cameras and the frozen
/// files of every earlier format, which were written before cameras
/// existed, read every view with `camera: None` and report nothing about
/// cameras.
#[test]
fn files_without_cameras_read_their_views_without_one() {
    let without: Vec<PathBuf> = files_in("golden")
        .into_iter()
        .filter(|p| !p.ends_with("two-storey-shell-with-columns.ifcx"))
        .chain(files_in("formats"))
        .collect();
    assert!(without.len() >= 10, "{without:?}");
    for path in without {
        let bytes = std::fs::read(&path).unwrap();
        assert!(
            view_records(&bytes).iter().all(|r| r.get("camera").is_none()),
            "{}",
            path.display()
        );
        let loaded = archicraft_ifcx::load(&bytes).unwrap();
        assert!(!loaded.project.views.is_empty(), "{}", path.display());
        assert!(
            loaded.project.views.iter().all(|v| v.camera.is_none()),
            "{}",
            path.display()
        );
        assert!(
            loaded.problems.iter().all(|p| p.severity == Severity::Info),
            "{}: {:#?}",
            path.display(),
            loaded.problems
        );
    }
}

/// PRJ-008, VIEW-004: a camera with a value of the wrong type, a target of
/// two coordinates, a distance of zero or a field of view of 180 degrees is
/// left out with one warning naming the view record and the reason; the
/// view itself loads, as a plain viewpoint. The wrong type also breaks the
/// camera's declaration, which validation reports as for any attribute
/// (IFX-005); the other damage is valid for the schema and is the only
/// problem.
#[test]
fn an_invalid_camera_is_left_out_with_a_warning() {
    /// One kind of damage to a camera: its name, the change, the reason the
    /// reader reports and whether the damaged camera still fits its
    /// declaration.
    struct Damage {
        case: &'static str,
        damage: fn(&mut Value),
        reason: &'static str,
        schema_valid: bool,
    }
    let (project, ids) = project_with_cameras();
    let cases = [
        Damage {
            case: "wrong type",
            damage: |c| c["yaw"] = json!("east"),
            reason: r#"ifcx-field-camera(key="camera", reason=ifcx-field-not-number(key="yaw"))"#,
            schema_valid: false,
        },
        Damage {
            case: "two coordinates",
            damage: |c| c["target"] = json!([1.0, 2.0]),
            reason: r#"ifcx-field-camera(key="camera", reason=ifcx-field-point(key="target", reason=ifcx-point-three-coordinates))"#,
            schema_valid: true,
        },
        Damage {
            case: "zero distance",
            damage: |c| c["distance"] = json!(0.0),
            reason: r#"ifcx-field-camera(key="camera", reason=model-error-view-camera)"#,
            schema_valid: true,
        },
        Damage {
            case: "field of view",
            damage: |c| c["fovY"] = json!(std::f64::consts::PI),
            reason: r#"ifcx-field-camera(key="camera", reason=model-error-view-camera)"#,
            schema_valid: true,
        },
    ];
    for Damage {
        case,
        damage,
        reason,
        schema_valid,
    } in cases
    {
        let bytes = edit(&project, |v| {
            let views = v["data"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find_map(|n| n["attributes"].get_mut("archicraft::project"))
                .unwrap()["views"]
                .as_array_mut()
                .unwrap();
            let record = views
                .iter_mut()
                .find(|r| r["id"] == ids[0].to_string().as_str())
                .unwrap();
            damage(&mut record["camera"]);
        });
        let loaded = archicraft_ifcx::load(&bytes).unwrap();
        let view = loaded
            .project
            .view(ids[0])
            .unwrap_or_else(|| panic!("{case}: the view loads"));
        assert_eq!(
            (view.name.as_str(), view.projection, view.camera),
            ("Camera 1", Projection::Perspective, None),
            "{case}"
        );
        assert!(loaded.project.view(ids[1]).unwrap().camera.is_some(), "{case}");
        let index = project.views.iter().position(|v| v.id == ids[0]).unwrap();
        let (validation, reports): (Vec<_>, Vec<_>) = loaded
            .problems
            .iter()
            .map(|p| (p.severity, p.message.to_string()))
            .partition(|(_, text)| text.starts_with("ifcx-attribute-invalid"));
        assert_eq!(
            reports,
            [(
                Severity::Warning,
                format!("ifcx-repaired(record=ifcx-record-view(index={index}), reason=ifcx-left-out(reason={reason}))")
            )],
            "{case}: {:#?}",
            loaded.problems
        );
        if schema_valid {
            assert!(validation.is_empty(), "{case}: {validation:#?}");
        } else {
            assert_eq!(validation.len(), 1, "{case}: {validation:#?}");
            assert!(
                validation[0].1.contains(r#"key="camera""#) && validation[0].1.contains(r#"key="yaw""#),
                "{case}: {validation:#?}"
            );
        }
    }
}
