//! Helpers shared by the IFCX integration tests: fixed write options, saving
//! with the warnings asserted away, the sample projects and editing a saved
//! file as JSON.

// Each test crate that includes this module uses only some of the helpers.
#![allow(dead_code)]

use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::WriteOptions;
use archicraft_model::{Project, samples};
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

pub fn samples() -> Vec<Project> {
    vec![samples::storey_with_walls_and_slab(), samples::two_storey_shell()]
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
