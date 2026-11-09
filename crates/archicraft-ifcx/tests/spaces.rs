//! Spaces in IFCX (SPC-001, Table 5-1): each space is an `IfcSpace` node
//! under its home storey (IFX-012) whose `bsi::ifc::prop::Name` is the room
//! name, with a closed `Body` child (IFX-017) that carries its own
//! presentation instead of a material, and its parameters in
//! `archicraft::space` (IFX-060); reading gives them back bit for bit and
//! repairs missing members with a report (IFX-083, PRJ-008).

#![allow(clippy::unwrap_used)]

mod common;

use std::path::Path;

use archicraft_geometry::{ModelGeometry, TriMesh};
use archicraft_ifcx::validate::{self, FlatNode, Severity};
use archicraft_ifcx::{IfcxFile, WriteOptions};
use archicraft_model::glam::{DVec2, DVec3};
use archicraft_model::{BoundarySource, Element, ElementKind, KindTag, Project, Space, TopLink, samples};
use serde_json::Value;

use common::{edit, node_index, options, save};

const CLASS_URI: &str = "https://identifier.buildingsmart.org/uri/buildingsmart/ifc/4.3/class/IfcSpace";

fn spaces(project: &Project) -> Vec<(&Element, &Space)> {
    project
        .elements
        .values()
        .filter_map(|e| e.as_space().map(|s| (e, s)))
        .collect()
}

/// The space `name` of `project`, mutably.
fn space_mut<'p>(project: &'p mut Project, name: &str) -> &'p mut Space {
    project
        .elements
        .values_mut()
        .find(|e| e.name == name)
        .and_then(Element::as_space_mut)
        .unwrap()
}

fn golden() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/two-storey-shell-with-spaces.ifcx");
    std::fs::read(path).unwrap()
}

fn mesh(node: &FlatNode) -> TriMesh {
    let mesh = &node.attributes["usd::usdgeom::mesh"];
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
        .map(|i| u32::try_from(i.as_u64().unwrap()).unwrap())
        .collect();
    TriMesh { points, indices }
}

/// SPC-001, IFX-064, NFR-ACC-002: the sample with spaces saves and reopens
/// to the same project, the detected source with its seed and the space
/// tool's default name "Raum {n}" included, without any problem.
#[test]
fn the_sample_with_spaces_reads_back_with_source_seed_and_default_name() {
    let project = samples::two_storey_shell_with_spaces();
    let loaded = archicraft_ifcx::load(&save(&project)).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    let mut expected = project.clone();
    expected.revision = 0;
    expected.data_version = 7;
    assert_eq!(loaded.project, expected);

    let read = &loaded.project;
    let spaces = spaces(read);
    assert_eq!(spaces.len(), 2);
    assert!(spaces.iter().any(|(_, s)| s.source
        == BoundarySource::Detected {
            seed: DVec2::new(2.5, 4.0),
            auto_update: true
        }));
    assert!(spaces.iter().any(|(_, s)| s.boundary.holes().len() == 1));
    let slot = read.tool_defaults.get(KindTag::Space).unwrap();
    let ElementKind::Space(slot) = &slot.kind else {
        panic!("the space slot holds a space: {slot:?}");
    };
    assert_eq!(slot.long_name, "Raum {n}");
    assert_eq!(read.counters.get(KindTag::Space), 2);
}

/// SPC-001, Table 5-1, IFX-012, IFX-017, IFX-020: the blessed file has one
/// `IfcSpace` node per space, a child of its home storey, named by the room
/// name, with a closed `Body` child that carries its own light blue,
/// translucent presentation and inherits no material; no material node is
/// written for it. The element's own name stays in `archicraft::element`.
#[test]
fn each_space_is_an_ifcspace_node_under_its_storey_with_a_body_of_its_own() {
    let project = samples::two_storey_shell_with_spaces();
    let file = validate::parse(&golden()).unwrap();
    assert!(validate::validate(&file).is_empty(), "{:#?}", validate::validate(&file));
    let nodes = validate::flatten(&file);
    let class = |node: &FlatNode| {
        let class = node.attributes.get("bsi::ifc::class")?;
        class["code"].as_str().map(str::to_owned)
    };
    let written = nodes
        .values()
        .filter(|n| class(n).as_deref() == Some("IfcSpace"))
        .count();
    assert_eq!(written, 2);

    for (element, space) in spaces(&project) {
        let path = element.id.to_string();
        let node = &nodes[&path];
        assert_eq!(node.attributes["bsi::ifc::class"]["uri"], CLASS_URI);
        assert_eq!(node.attributes["bsi::ifc::prop::Name"], space.long_name.as_str());
        assert_eq!(node.attributes["archicraft::element"]["name"], element.name.as_str());
        let storey = &nodes[&element.storey.to_string()];
        let key = storey
            .children
            .iter()
            .find(|(_, child)| child.as_deref() == Some(path.as_str()))
            .map(|(key, _)| key.clone())
            .unwrap_or_else(|| panic!("{} is not a child of its storey", element.name));
        assert!(key.starts_with("Space_"), "{key}");

        assert_eq!(node.children.len(), 1, "{:?}", node.children);
        let body_path = node.children["Body"].clone().unwrap();
        let body = &nodes[&body_path];
        assert!(body.inherits.is_empty(), "{:?}", body.inherits);
        assert_eq!(
            body.attributes["bsi::ifc::presentation::diffuseColor"],
            serde_json::json!([0.72, 0.79, 0.86])
        );
        assert_eq!(body.attributes["bsi::ifc::presentation::opacity"], 0.25);
        assert_eq!(mesh(body).check_closed(), Ok(()), "{}", element.name);
    }

    // Only the materials the construction elements use have nodes.
    let colours: Vec<&Value> = nodes
        .values()
        .filter(|n| !n.attributes.contains_key("usd::usdgeom::mesh"))
        .filter_map(|n| n.attributes.get("bsi::ifc::presentation::diffuseColor"))
        .collect();
    assert!(!colours.is_empty());
    assert!(!colours.contains(&&serde_json::json!([0.72, 0.79, 0.86])));
}

/// IFX-007: saving the sample with spaces twice gives byte-identical files,
/// and saving it at another time with another save counter changes nothing
/// but the header.
#[test]
fn saving_spaces_twice_is_byte_identical_apart_from_the_header() {
    let project = samples::two_storey_shell_with_spaces();
    let geometry = ModelGeometry::evaluate(&project);
    let write = |options: &WriteOptions| archicraft_ifcx::save(&project, &geometry, options, false).0;
    assert_eq!(write(&options()), write(&options()));
    let later = WriteOptions {
        timestamp: "2026-11-09T18:00:00Z".into(),
        data_version: 8,
        ..options()
    };
    let first = validate::parse(&write(&options())).unwrap();
    let mut second: IfcxFile = validate::parse(&write(&later)).unwrap();
    assert_eq!(
        (second.header.timestamp.as_str(), second.header.data_version.as_str()),
        ("2026-11-09T18:00:00Z", "8")
    );
    second.header.timestamp = first.header.timestamp.clone();
    second.header.data_version = first.header.data_version.clone();
    assert_eq!(first, second);
}

/// SPC-001: without a room name the node shows the number; without either
/// it shows the element's name, which then is not repeated in
/// `archicraft::element`. Both read back as they were.
#[test]
fn a_space_without_a_room_name_shows_its_number_or_else_its_own_name() {
    let mut project = samples::two_storey_shell_with_spaces();
    space_mut(&mut project, "SP-001").long_name.clear();
    let unnamed = space_mut(&mut project, "SP-002");
    unnamed.long_name.clear();
    unnamed.number.clear();
    let bytes = save(&project);
    let file = validate::parse(&bytes).unwrap();
    let nodes = validate::flatten(&file);
    let node = |name: &str| {
        let id = project.elements.values().find(|e| e.name == name).unwrap().id;
        nodes[&id.to_string()].clone()
    };
    let numbered = node("SP-001");
    assert_eq!(numbered.attributes["bsi::ifc::prop::Name"], "0.01");
    assert_eq!(numbered.attributes["archicraft::element"]["name"], "SP-001");
    let bare = node("SP-002");
    assert_eq!(bare.attributes["bsi::ifc::prop::Name"], "SP-002");
    assert!(bare.attributes["archicraft::element"].get("name").is_none());

    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    let mut expected = project;
    expected.revision = 0;
    expected.data_version = 7;
    assert_eq!(loaded.project, expected);
}

/// The warnings of `problems` as their message keys' text.
fn repaired(problems: &[archicraft_ifcx::Problem]) -> Vec<String> {
    problems
        .iter()
        .filter(|p| p.severity == Severity::Warning)
        .map(|p| p.message.to_string())
        .collect()
}

/// IFX-083, PRJ-008: a space whose members other than its outline are
/// missing loads with the defaults, each repair reported as a warning
/// against the space: no room name or number, the base at the storey, the
/// top at the next storey, a drawn boundary and the stamp at the interior
/// point.
#[test]
fn missing_space_members_are_repaired_with_warnings() {
    let project = samples::two_storey_shell_with_spaces();
    let members = ["longName", "number", "baseOffset", "top", "source", "stampOffset"];
    let bytes = edit(&project, |v| {
        let i = node_index(v, "Living room");
        let space = v["data"][i]["attributes"]["archicraft::space"].as_object_mut().unwrap();
        for member in members {
            assert!(space.remove(member).is_some(), "{member}");
        }
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert_eq!(loaded.project.elements.len(), project.elements.len());
    let space = loaded.project.elements.values().find(|e| e.name == "SP-001").unwrap();
    let space = space.as_space().unwrap();
    assert_eq!((space.long_name.as_str(), space.number.as_str()), ("", ""));
    assert_eq!(space.link.base_offset, 0.0);
    assert_eq!(
        space.link.top,
        TopLink::Relative {
            storeys: 1,
            offset: 0.0
        }
    );
    assert_eq!(space.source, BoundarySource::Drawn);
    assert_eq!(space.stamp_offset, DVec2::ZERO);
    let warnings = repaired(&loaded.problems);
    for member in members {
        assert!(
            warnings
                .iter()
                .any(|w| w.contains(r#"ifcx-record-element(name="Living room")"#)
                    && w.contains(&format!(r#"ifcx-field-missing(key="{member}")"#))),
            "{member}: {warnings:#?}"
        );
    }
    // The edited file no longer matches its declaration, which validation
    // reports; the space is loaded all the same.
    assert!(
        loaded
            .problems
            .iter()
            .all(|p| !p.message.to_string().starts_with("ifcx-skipped")),
        "{:#?}",
        loaded.problems
    );
}

/// IFX-083, PRJ-008: a space without a valid outline is skipped with the
/// reason; the other elements load. An unknown boundary source is repaired
/// to a drawn one, and an element name in `archicraft::element` that is no
/// text gives way to the node's name, both reported.
#[test]
fn damaged_spaces_are_skipped_or_repaired_and_reported() {
    let project = samples::two_storey_shell_with_spaces();
    let bytes = edit(&project, |v| {
        let i = node_index(v, "Living room");
        v["data"][i]["attributes"]["archicraft::space"]["outline"] = "none".into();
        let i = node_index(v, "Landing");
        v["data"][i]["attributes"]["archicraft::space"]["source"]["type"] = "traced".into();
        v["data"][i]["attributes"]["archicraft::element"]["name"] = 2.into();
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert_eq!(loaded.project.elements.len(), project.elements.len() - 1);
    assert!(
        loaded.problems.iter().any(|p| p.severity == Severity::Error
            && p.message
                .to_string()
                .contains(r#"ifcx-skipped(record=ifcx-record-element(name="Living room")"#)),
        "{:#?}",
        loaded.problems
    );
    let landing = loaded.project.elements.values().find(|e| e.name == "Landing").unwrap();
    assert_eq!(landing.as_space().unwrap().source, BoundarySource::Drawn);
    let warnings = repaired(&loaded.problems);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains(r#"ifcx-field-unknown-value(key="type", value="traced")"#)),
        "{warnings:#?}"
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.contains(r#"ifcx-field-not-text(key="name")"#)),
        "{warnings:#?}"
    );
}

/// SPC-001, IFX-005, IFX-061: every project has the space tool's defaults
/// slot, so its file declares `archicraft::space`, with every member.
#[test]
fn the_file_declares_every_member_of_the_space_attribute() {
    let schemas = validate::parse(&save(&samples::two_storey_shell())).unwrap().schemas;
    let members = schemas["archicraft::space"]["value"]["objectRestrictions"]["values"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        members,
        [
            "outline",
            "holes",
            "longName",
            "number",
            "baseOffset",
            "top",
            "source",
            "stampOffset"
        ]
    );
}
