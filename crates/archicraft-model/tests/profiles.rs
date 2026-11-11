//! Profiles as project resources (SRS 3.4, PRF-004, PRF-005): lookup,
//! integrity problems and the serde form of a project with and without
//! profiles.

#![allow(clippy::unwrap_used)]

use archicraft_model::glam::DVec2;
use archicraft_model::template::default_metric_project;
use archicraft_model::{IntegrityProblem, MaterialId, Message, ParametricShape, Profile, ProfileRegion, Project};

fn square(min: DVec2, size: f64) -> Vec<DVec2> {
    vec![
        min,
        min + DVec2::new(size, 0.0),
        min + DVec2::splat(size),
        min + DVec2::new(0.0, size),
    ]
}

/// The template project with an IPE 200 and a custom profile of two
/// regions, both of the first material.
fn project_with_profiles() -> Project {
    let mut project = default_metric_project();
    let material = project.materials[0].id;
    let i_shape = ParametricShape::ALL.into_iter().find(|s| s.code() == "i").unwrap();
    let ipe = Profile::parametric("IPE 200", i_shape, material);
    let region = |min| ProfileRegion {
        outer: square(min, 0.1),
        holes: Vec::new(),
        material,
    };
    let custom = Profile::custom("Twin", vec![region(DVec2::ZERO), region(DVec2::new(0.1, 0.0))]);
    project.profiles = vec![ipe, custom];
    project
}

/// PRF-004: a profile is found by its identity.
#[test]
fn profiles_are_found_by_id() {
    let project = project_with_profiles();
    let custom = &project.profiles[1];
    assert_eq!(project.profile(custom.id), Some(custom));
    assert_eq!(project.profile(archicraft_model::ProfileId::new()), None);
}

/// NFR-REL-004, PRF-004: a profile region whose material does not exist is
/// an integrity problem naming the profile; valid profiles are none.
#[test]
fn a_profile_region_with_a_missing_material_is_an_integrity_problem() {
    let mut project = project_with_profiles();
    assert_eq!(project.integrity_problems(), []);
    let archicraft_model::ProfileShape::Custom { regions } = &mut project.profiles[1].shape else {
        panic!("custom");
    };
    regions[1].material = MaterialId::new();
    assert_eq!(
        project.integrity_problems(),
        [IntegrityProblem {
            element: None,
            relation: None,
            message: Message::new("model-problem-profile-material-missing").text("name", "Twin"),
        }]
    );
}

/// PRF-004: profiles survive the serde form of a project, and a serde form
/// without them, as written before profiles existed, still reads with none.
#[test]
fn projects_with_and_without_profiles_deserialise() {
    let project = project_with_profiles();
    let json = serde_json::to_value(&project).unwrap();
    let read: Project = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(read, project);

    let mut without = json;
    without.as_object_mut().unwrap().remove("profiles");
    let read: Project = serde_json::from_value(without).unwrap();
    assert!(read.profiles.is_empty());

    // An empty list is left out, so the form matches what a file stores
    // until profiles are persisted (archicraft-jrtb).
    let empty = serde_json::to_value(default_metric_project()).unwrap();
    assert!(empty.get("profiles").is_none());
}
