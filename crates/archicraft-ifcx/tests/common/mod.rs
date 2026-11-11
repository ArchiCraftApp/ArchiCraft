//! Helpers shared by the IFCX integration tests: fixed write options, saving
//! with the warnings asserted away, the sample projects and editing a saved
//! file as JSON.

// Each test crate that includes this module uses only some of the helpers.
#![allow(dead_code)]

use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::WriteOptions;
use archicraft_model::glam::DVec2;
use archicraft_model::{
    Command, History, ParametricShape, Profile, ProfileDimension, ProfileId, ProfileRegion, ProfileUsage, Project,
    samples,
};
use serde_json::Value;

pub fn options() -> WriteOptions {
    WriteOptions {
        author: "Test".into(),
        timestamp: "2026-10-16T12:00:00Z".into(),
        data_version: 7,
    }
}

pub fn save(project: &Project) -> Vec<u8> {
    let geometry = ModelGeometry::evaluate(project);
    let (bytes, warnings) = archicraft_ifcx::save(project, &geometry, &options(), false);
    assert!(warnings.is_empty(), "{warnings:?}");
    bytes
}

/// The sample catalogue's projects (NFR-MNT-003).
pub fn samples() -> Vec<Project> {
    samples::catalogue().iter().map(|(_, build)| build()).collect()
}

/// The name of the custom profile of [`profile_sample`].
pub const CUSTOM_PROFILE: &str = "Composite";

/// A project of the template with profiles and no elements (PRF-004,
/// PRF-005), added through `AddProfile`, so they are stored as the model
/// normalises them: one profile of each parametric shape, its dimensions
/// scaled by 1.1 so that they need every digit (NFR-ACC-002), the I shape
/// with its origin off the centre and the Z shape with other usage flags;
/// the custom profile [`CUSTOM_PROFILE`] of two touching regions of
/// different materials, the first with a hole; and the catalogue profile
/// `IPE 200`. Identities are name-based, as in the samples (IFX-011).
pub fn profile_sample() -> Project {
    let mut b = samples::FixtureBuilder::new("profiles").titled("Profiles");
    let (steel, concrete) = (b.material("Steel"), b.material("Reinforced concrete"));
    let mut profiles: Vec<Profile> = ParametricShape::ALL
        .iter()
        .map(|shape| {
            let mut shape = *shape;
            for dimension in ProfileDimension::ALL {
                if let Some(value) = shape.dimension_mut(dimension) {
                    *value *= 1.1;
                }
            }
            Profile::parametric(format!("Parametric {}", shape.code()), shape, steel)
        })
        .collect();
    profiles[4].origin = DVec2::new(0.1 + 0.2, -1.0 / 3.0);
    profiles[8].usage = ProfileUsage {
        columns: false,
        beams: true,
        walls: true,
        railings: false,
    };
    let square = |min: DVec2, size: f64| {
        vec![
            min,
            min + DVec2::new(size, 0.0),
            min + DVec2::splat(size),
            min + DVec2::new(0.0, size),
        ]
    };
    let side = 0.1 + 0.2;
    profiles.push(Profile::custom(
        CUSTOM_PROFILE,
        vec![
            ProfileRegion {
                outer: square(DVec2::ZERO, side),
                holes: vec![square(DVec2::splat(0.1), 0.1)],
                material: concrete,
            },
            ProfileRegion {
                outer: vec![
                    DVec2::new(side, 0.0),
                    DVec2::new(side + 0.1, 0.0),
                    DVec2::new(side + 0.1, side),
                    DVec2::new(side, side),
                ],
                holes: Vec::new(),
                material: steel,
            },
        ],
    ));
    let mut ipe = Profile::parametric("IPE 200", ParametricShape::ALL[4], steel);
    ipe.catalogue = Some("IPE 200".into());
    profiles.push(ipe);
    for profile in &mut profiles {
        profile.id = ProfileId::from_uuid(b.id("profile"));
    }
    let mut project = b.finish();
    let mut history = History::default();
    for profile in profiles {
        history.execute(&mut project, Command::AddProfile(profile)).unwrap();
    }
    project
}

pub fn edit(project: &Project, change: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(&save(project)).unwrap();
    change(&mut value);
    serde_json::to_vec(&value).unwrap()
}

pub fn node_index(value: &Value, name: &str) -> usize {
    value["data"]
        .as_array()
        .unwrap()
        .iter()
        .position(|n| n["attributes"]["bsi::ifc::prop::Name"] == name)
        .unwrap()
}
