//! Regression net for the done IFCX requirements about the container, the
//! header, namespaces, identities, classes and names (IFX-001, IFX-002,
//! IFX-005, IFX-010, IFX-013, IFX-014, IFX-020, IFX-021, IFX-022, IFX-025,
//! IFX-061, IFX-090, NFR-PORT-004), and about geometry, precision, round-trip
//! fidelity, editability, layers and project information (IFX-030, IFX-031,
//! IFX-043, IFX-063, IFX-064, IFX-067, IFX-080, XCH-008, LAY-005, PRJ-010,
//! NFR-ACC-001, NFR-ACC-002), and about reporting problems (IFX-083).
//!
//! Every test is named after the requirement it pins, so the traceability
//! table in docs/testing.md and the status script can find it. Projects are
//! `samples::two_storey_shell` or small projects built from the default
//! template through commands. Tests that already exist in tests/roundtrip.rs
//! (for example `container_matches_the_ifcx_json_schema` and
//! `spatial_tree_and_identities`) are referenced in docs/testing.md rather
//! than repeated here.

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use archicraft_geometry::{ModelGeometry, TriMesh};
use archicraft_ifcx::alpha::writer::sub_node_id;
use archicraft_ifcx::container::{DataType, SchemaEntry, ValueDescription};
use archicraft_ifcx::json::format_f64;
use archicraft_ifcx::validate::{self, FlatNode};
use archicraft_ifcx::{IfcxFile, WRITTEN_VERSION, WriteOptions, registry};
use archicraft_model::glam::{DVec2, DVec3};
use archicraft_model::project::Location;
use archicraft_model::template::default_metric_project;
use archicraft_model::units::{LengthUnit, NumberFormat, parse_length};
use archicraft_model::{
    Command, ElementId, ElementKind, History, IfcClass, KindTag, Layer, MaterialId, Position, Project, ProjectInfo,
    TopLink, Wall, samples,
};
use indexmap::IndexMap;
use serde_json::{Value, json};
use uuid::{Uuid, Variant};

use common::{edit, options, save};

const CLASS_URI: &str = "https://identifier.buildingsmart.org/uri/buildingsmart/ifc/4.3/class/";

fn execute(project: &mut Project, history: &mut History, command: Command) {
    let label = command.label();
    history
        .execute(project, command)
        .unwrap_or_else(|e| panic!("{label}: {e}"));
}

fn parse(bytes: &[u8]) -> IfcxFile {
    validate::parse(bytes).unwrap()
}

fn nodes_of(project: &Project) -> IndexMap<String, FlatNode> {
    validate::flatten(&parse(&save(project)))
}

/// A small project built through commands: an exterior and an interior wall
/// and a slab with an undefined position on the ground storey.
fn small_project() -> (Project, History) {
    let mut project = default_metric_project();
    let mut history = History::default();
    let ground = project.storeys.ground();
    let exterior = project.new_wall(ground, DVec2::ZERO, DVec2::new(6.0, 0.0), 0);
    let mut interior = project.new_wall(ground, DVec2::new(3.0, 0.0), DVec2::new(3.0, 4.0), 1);
    interior.position = Position::Interior;
    let outline = vec![
        DVec2::ZERO,
        DVec2::new(6.0, 0.0),
        DVec2::new(6.0, 4.0),
        DVec2::new(0.0, 4.0),
    ];
    let mut slab = project.new_slab(ground, outline, Vec::new(), 0);
    slab.position = Position::Undefined;
    execute(
        &mut project,
        &mut history,
        Command::AddElements(vec![exterior, interior, slab]),
    );
    (project, history)
}

/// A new directory under the system temp folder, unique per call, removed
/// when dropped, also when the test fails.
struct ScratchDir(PathBuf);

impl ScratchDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("archicraft-{name}-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `YYYY-MM-DDTHH:MM:SS[.fraction]Z`: ISO 8601 in UTC.
fn is_iso8601_utc(text: &str) -> bool {
    let b = text.as_bytes();
    let digits = |range: std::ops::Range<usize>| b.get(range).is_some_and(|d| d.iter().all(u8::is_ascii_digit));
    let fraction_ok = match b.get(19) {
        Some(b'Z') => b.len() == 20,
        Some(b'.') => b.len() > 21 && digits(20..b.len() - 1) && b[b.len() - 1] == b'Z',
        _ => false,
    };
    digits(0..4)
        && b[4] == b'-'
        && digits(5..7)
        && b[7] == b'-'
        && digits(8..10)
        && b[10] == b'T'
        && digits(11..13)
        && b[13] == b':'
        && digits(14..16)
        && b[16] == b':'
        && digits(17..19)
        && fraction_ok
}

/// `^[A-Z][A-Za-z]+_[0-9a-f]{8}$`, the child key form of IFX-014.
fn is_child_key(key: &str) -> bool {
    let Some((kind, hex)) = key.split_once('_') else {
        return false;
    };
    let mut kind_chars = kind.chars();
    kind_chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && kind.len() >= 2
        && kind_chars.all(|c| c.is_ascii_alphabetic())
        && hex.len() == 8
        && hex.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
}

/// Parses an RFC 9562 UUID in its canonical lowercase hyphenated form.
#[track_caller]
fn rfc9562(path: &str) -> Uuid {
    let uuid = Uuid::parse_str(path).unwrap_or_else(|e| panic!("{path} is not a UUID: {e}"));
    assert_eq!(uuid.to_string(), path, "canonical form");
    assert_eq!(uuid.get_variant(), Variant::RFC4122, "{path}: RFC 9562 variant");
    uuid
}

fn class_code(node: &FlatNode) -> Option<&str> {
    node.attributes
        .get("bsi::ifc::class")
        .map(|c| c["code"].as_str().unwrap())
}

fn name(node: &FlatNode) -> Option<&str> {
    node.attributes.get("bsi::ifc::prop::Name").and_then(Value::as_str)
}

/// The parent path of every node that is somebody's child.
fn parents(nodes: &IndexMap<String, FlatNode>) -> HashMap<String, String> {
    let mut parents = HashMap::new();
    for (path, node) in nodes {
        for child in node.children.values().flatten() {
            assert!(
                parents.insert(child.clone(), path.clone()).is_none(),
                "{child} has one parent"
            );
        }
    }
    parents
}

/// The children of every spatial node and element in the model, by kind of
/// child: (parent path, child key, child path).
fn object_children(project: &Project, nodes: &IndexMap<String, FlatNode>) -> Vec<(String, String, String)> {
    let mut parents = vec![
        project.id.to_string(),
        project.site.id.to_string(),
        project.building.id.to_string(),
    ];
    parents.extend(project.storeys.iter().map(|s| s.id.to_string()));
    parents
        .into_iter()
        .flat_map(|parent| {
            nodes[&parent]
                .children
                .iter()
                .map(|(key, child)| (parent.clone(), key.clone(), child.clone().unwrap()))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// IFX-001: the output is a UTF-8 JSON document whose top-level object has
/// exactly the members header, imports, schemas and data, compact and
/// indented. The JSON Schema check of the container is
/// `container_matches_the_ifcx_json_schema` (roundtrip.rs); the `.ifcx`
/// extension of the save dialog is pinned with the app requirements.
#[test]
fn ifx_001_output_is_utf8_json_with_header_imports_schemas_and_data() {
    let mut project = samples::two_storey_shell();
    let mut history = History::default();
    let mut info = project.info.clone();
    info.name = "Haus Müller – Entwurf ½".into();
    execute(&mut project, &mut history, Command::SetInfo(info));
    let geometry = ModelGeometry::evaluate(&project);
    for pretty in [false, true] {
        let (bytes, warnings) = archicraft_ifcx::save(&project, &geometry, &options(), pretty);
        assert!(warnings.is_empty(), "{warnings:?}");
        let text = String::from_utf8(bytes).expect("the output is valid UTF-8");
        assert!(
            text.contains("Haus Müller – Entwurf ½"),
            "non-ASCII text is written as UTF-8"
        );
        let value: Value = serde_json::from_str(&text).unwrap();
        let mut members: Vec<&str> = value.as_object().unwrap().keys().map(String::as_str).collect();
        members.sort_unstable();
        assert_eq!(members, ["data", "header", "imports", "schemas"], "pretty: {pretty}");
        assert!(value["header"].is_object());
        assert!(value["imports"].is_array());
        assert!(value["schemas"].is_object());
        assert!(value["data"].is_array());
    }
}

/// IFX-002: header.id is `archicraft:project/<project UUID>`, ifcxVersion
/// is `ifcx_alpha`, dataVersion and author are the values of WriteOptions
/// and the timestamp is ISO 8601 in UTC. The save counter and the author
/// preference live in the app and are pinned with the app requirements.
#[test]
fn ifx_002_header_carries_id_version_data_version_author_and_utc_timestamp() {
    let project = samples::two_storey_shell();
    let geometry = ModelGeometry::evaluate(&project);
    for written in [
        options(),
        WriteOptions {
            author: "Zoë Ångström".into(),
            timestamp: "2026-10-20T08:15:30.125Z".into(),
            data_version: 42,
        },
    ] {
        let (bytes, _) = archicraft_ifcx::save(&project, &geometry, &written, false);
        let header = parse(&bytes).header;
        assert_eq!(header.id, format!("archicraft:project/{}", project.id.uuid()));
        assert_eq!(header.ifcx_version, "ifcx_alpha");
        assert_eq!(header.data_version, written.data_version.to_string());
        assert_eq!(header.author, written.author);
        assert_eq!(header.timestamp, written.timestamp);
        assert!(is_iso8601_utc(&header.timestamp), "{}", header.timestamp);
    }
    for not_utc in [
        "2026-10-20 08:15:30Z",
        "2026-10-20T08:15:30",
        "2026-10-20T08:15:30+02:00",
    ] {
        assert!(!is_iso8601_utc(not_utc), "the pattern rejects {not_utc}");
    }
}

/// IFX-005: every attribute key in `data` is declared by one of the
/// imported schema files ArchiCraft bundles or in the file's own `schemas`.
#[test]
fn ifx_005_every_data_attribute_key_is_declared() {
    let (mut project, mut history) = small_project();
    let mut info = project.info.clone();
    info.description = "Every optional attribute is present.".into();
    execute(&mut project, &mut history, Command::SetInfo(info));
    let mut checked: HashSet<String> = HashSet::new();
    for project in [project, samples::two_storey_shell()] {
        let file = parse(&save(&project));
        let mut declared: HashSet<&str> = file.schemas.keys().map(String::as_str).collect();
        for import in &file.imports {
            let bundled =
                registry::bundled(&import.uri).unwrap_or_else(|| panic!("{} is a bundled import", import.uri));
            declared.extend(bundled.schemas.keys().map(String::as_str));
        }
        for node in &file.data {
            for key in node.attributes.keys() {
                assert!(declared.contains(key.as_str()), "{key} on {} is declared", node.path);
                checked.insert(key.clone());
            }
        }
    }
    for key in [
        "bsi::ifc::class",
        "bsi::ifc::prop::Name",
        "bsi::ifc::prop::Description",
        "bsi::ifc::prop::IsExternal",
        "bsi::ifc::prop::RefElevation",
        "bsi::ifc::presentation::diffuseColor",
        "bsi::ifc::presentation::opacity",
        "usd::xformop",
        "usd::usdgeom::mesh",
        "usd::usdgeom::basiscurves",
        "archicraft::project",
        "archicraft::storey",
        "archicraft::element",
        "archicraft::wall",
        "archicraft::slab",
    ] {
        assert!(checked.contains(key), "the test saw {key}");
    }
}

/// A project created like the application creates one: the template and
/// the tools' new objects, all with random identities. (The samples use
/// seeded, name-based identities instead.)
fn created_like_the_application() -> Project {
    let mut project = default_metric_project();
    let mut history = History::default();
    let ground = project.storeys.ground();
    let outline = vec![
        DVec2::ZERO,
        DVec2::new(10.0, 0.0),
        DVec2::new(10.0, 8.0),
        DVec2::new(0.0, 8.0),
    ];
    let walls = (0..4)
        .map(|i| project.new_wall(ground, outline[i], outline[(i + 1) % 4], i as u32))
        .collect();
    execute(&mut project, &mut history, Command::AddElements(walls));
    let slab = project.new_slab(ground, outline, Vec::new(), 0);
    execute(&mut project, &mut history, Command::AddElements(vec![slab]));
    project
}

/// IFX-010: every project, site, building, storey, element and material
/// node has an RFC 9562 UUID path, version 4 for the objects created here;
/// Body and Axis sub-nodes have the version 5 UUID of owner and role, which
/// a wall thickness change does not change.
#[test]
fn ifx_010_nodes_have_uuid_paths_and_sub_nodes_name_based_uuids() {
    let mut project = created_like_the_application();
    let nodes = nodes_of(&project);

    let mut objects: Vec<String> = vec![
        project.id.to_string(),
        project.site.id.to_string(),
        project.building.id.to_string(),
    ];
    objects.extend(project.storeys.iter().map(|s| s.id.to_string()));
    objects.extend(project.elements.keys().map(ToString::to_string));
    let mut materials: Vec<String> = nodes
        .values()
        .filter_map(|n| n.inherits.get("material").cloned())
        .collect();
    materials.sort_unstable();
    materials.dedup();
    assert!(materials.len() >= 2, "{materials:?}");
    for material in &materials {
        assert!(
            project.materials.iter().any(|m| m.id.to_string() == *material),
            "{material} is a material of the project"
        );
    }
    objects.extend(materials);
    for path in &objects {
        assert!(nodes.contains_key(path), "{path} is written as a node");
        assert_eq!(rfc9562(path).get_version_num(), 4, "{path}");
    }

    let mut axes = 0;
    for element in project.elements.values() {
        let owner = element.id.uuid();
        let node = &nodes[&element.id.to_string()];
        let body = node.children["Body"].as_ref().unwrap();
        assert_eq!(rfc9562(body).get_version_num(), 5, "{body}");
        assert_eq!(*body, sub_node_id(owner, "Body").to_string());
        assert_eq!(*body, Uuid::new_v5(&owner, b"Body").to_string());
        if let Some(axis) = node.children.get("Axis") {
            let axis = axis.as_ref().unwrap();
            assert_eq!(rfc9562(axis).get_version_num(), 5, "{axis}");
            assert_eq!(*axis, sub_node_id(owner, "Axis").to_string());
            axes += 1;
        }
    }
    assert_eq!(axes, 4, "every wall has an axis");

    let wall = project
        .elements
        .values()
        .find(|e| e.as_wall().is_some())
        .unwrap()
        .clone();
    let body_before = nodes[&wall.id.to_string()].children["Body"].clone();
    let mesh_before = nodes[body_before.as_deref().unwrap()].attributes["usd::usdgeom::mesh"].clone();
    let mut thicker = wall.clone();
    if let ElementKind::Wall(w) = &mut thicker.kind {
        w.thickness += 0.1;
    }
    let mut history = History::default();
    execute(&mut project, &mut history, Command::UpdateElements(vec![thicker]));
    let nodes = nodes_of(&project);
    let body_after = nodes[&wall.id.to_string()].children["Body"].clone();
    assert_eq!(body_after, body_before, "regenerating geometry keeps the identity");
    assert_ne!(
        nodes[body_after.as_deref().unwrap()].attributes["usd::usdgeom::mesh"],
        mesh_before,
        "the body was regenerated"
    );
}

/// IFX-013: the IfcProject node is the only root, and header.id derives from
/// the project UUID: renaming the project and saving the file under another
/// name in another folder keeps it.
#[test]
fn ifx_013_header_id_survives_rename_and_move() {
    let project = samples::two_storey_shell();
    let nodes = nodes_of(&project);
    let roots = validate::roots(&nodes).unwrap();
    assert_eq!(roots, vec![project.id.to_string()], "the project is the only root");
    assert_eq!(class_code(&nodes[&roots[0]]), Some("IfcProject"));

    let scratch = ScratchDir::new("ifx-013");
    let dir = &scratch.0;
    let first = dir.join("house.ifcx");
    std::fs::write(&first, save(&project)).unwrap();
    let opened = archicraft_ifcx::load_path(&first).unwrap().project;
    let expected_id = parse(&std::fs::read(&first).unwrap()).header.id;
    assert_eq!(expected_id, format!("archicraft:project/{}", project.id));

    let mut history = History::default();
    let mut info = opened.info.clone();
    info.name = "Renamed house".into();
    let mut renamed = opened;
    execute(&mut renamed, &mut history, Command::SetInfo(info));
    let moved = dir.join("archive").join("renamed copy.ifcx");
    std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
    std::fs::write(&moved, save(&renamed)).unwrap();
    std::fs::remove_file(&first).unwrap();

    let reopened = archicraft_ifcx::load_path(&moved).unwrap().project;
    let header = parse(&std::fs::read(&moved).unwrap()).header;
    assert_eq!(header.id, expected_id);
    assert_eq!(reopened.id, project.id);
    assert_eq!(reopened.info.name, "Renamed house");
}

/// IFX-014: the children of the project, site, building and storeys have
/// keys `<Kind>_<first 8 hex digits of the UUID>`, unique within their
/// parent and unchanged when the element is renamed. Sub-nodes regenerated
/// with geometry are keyed by their role (`Body`, `Axis`), as Table 5-1 names
/// them.
#[test]
fn ifx_014_child_keys_are_unique_readable_and_survive_rename() {
    let mut project = samples::two_storey_shell();
    let nodes = nodes_of(&project);
    let children = object_children(&project, &nodes);
    assert_eq!(children.len(), 1 + 1 + project.storeys.len() + project.elements.len());
    for (parent, key, child) in &children {
        assert!(is_child_key(key), "{key} under {parent}");
        let uuid = Uuid::parse_str(child).unwrap().simple().to_string();
        assert_eq!(key.split_once('_').unwrap().1, &uuid[..8], "{key} names {child}");
    }
    for storey in project.storeys.iter() {
        let keys: HashSet<&String> = nodes[&storey.id.to_string()].children.keys().collect();
        assert_eq!(
            keys.len(),
            project.elements_on(storey.id).count(),
            "one unique key per element of {}",
            storey.name
        );
    }
    for element in project.elements.values() {
        let mut roles: Vec<&str> = nodes[&element.id.to_string()]
            .children
            .keys()
            .map(String::as_str)
            .collect();
        roles.sort_unstable();
        let expected: &[&str] = if element.as_wall().is_some() {
            &["Axis", "Body"]
        } else {
            &["Body"]
        };
        assert_eq!(roles, expected, "{}", element.name);
    }
    for not_a_key in [
        "wall_93791d5d",
        "Wall_93791D5D",
        "Wall_93791d5",
        "W_93791d5d",
        "Wall93791d5d",
    ] {
        assert!(!is_child_key(not_a_key), "the pattern rejects {not_a_key}");
    }
    assert!(is_child_key("Wall_93791d5d"));

    let mut history = History::default();
    let renamed: Vec<_> = project
        .elements
        .values()
        .map(|e| {
            let mut e = e.clone();
            e.name = format!("Renamed {}", e.name);
            e
        })
        .collect();
    execute(&mut project, &mut history, Command::UpdateElements(renamed));
    let nodes = nodes_of(&project);
    assert_eq!(object_children(&project, &nodes), children, "renaming keeps every key");
    assert!(
        project
            .elements
            .values()
            .all(|e| name(&nodes[&e.id.to_string()]) == Some(e.name.as_str())),
        "the new names are carried by bsi::ifc::prop::Name"
    );
}

/// IFX-020: every spatial node and element carries `bsi::ifc::class` with
/// its Table 5-1 code under its Table 5-1 parent, including the IfcFooting
/// and IfcCovering overrides, and the bSDD IFC 4.3 uri of that code.
/// Compound parts do not exist yet; the owners of IFX-016 extend this test.
#[test]
fn ifx_020_classes_and_parents_follow_table_5_1() {
    let (mut project, mut history) = small_project();
    let mut overrides = Vec::new();
    for element in project.elements.values() {
        let mut e = element.clone();
        if e.as_wall().is_some() && e.position == Position::Interior {
            e.ifc_class = IfcClass::Footing;
            overrides.push(e);
        } else if e.as_slab().is_some() {
            e.ifc_class = IfcClass::Covering;
            overrides.push(e);
        }
    }
    assert_eq!(overrides.len(), 2);
    execute(&mut project, &mut history, Command::UpdateElements(overrides));

    // The pass with the overrides, then the defaults of two_storey_shell.
    for (project, overridden) in [(project, true), (samples::two_storey_shell(), false)] {
        let nodes = nodes_of(&project);
        let parents = parents(&nodes);
        let parent_class = |path: &str| parents.get(path).map(|p| class_code(&nodes[p.as_str()]).unwrap());
        let mut expected: Vec<(String, &str, Option<&str>)> = vec![
            (project.id.to_string(), "IfcProject", None),
            (project.site.id.to_string(), "IfcSite", Some("IfcProject")),
            (project.building.id.to_string(), "IfcBuilding", Some("IfcSite")),
        ];
        for storey in project.storeys.iter() {
            expected.push((storey.id.to_string(), "IfcBuildingStorey", Some("IfcBuilding")));
        }
        for element in project.elements.values() {
            let code = match (element.tag(), element.position, overridden) {
                (KindTag::Wall, Position::Interior, true) => "IfcFooting",
                (KindTag::Wall, _, _) => "IfcWall",
                (KindTag::Slab, _, true) => "IfcCovering",
                (KindTag::Slab, _, false) => "IfcSlab",
                (kind, ..) => panic!("the sample has only walls and slabs, not {kind:?}"),
            };
            assert_eq!(element.ifc_class.code(), code, "{} in the model", element.name);
            expected.push((element.id.to_string(), code, Some("IfcBuildingStorey")));
            assert_eq!(
                parents[&element.id.to_string()],
                element.storey.to_string(),
                "{} sits under its home storey",
                element.name
            );
        }
        let mut classified = 0;
        for (path, code, parent) in &expected {
            let class = &nodes[path].attributes["bsi::ifc::class"];
            assert_eq!(class["code"], *code, "{path}");
            assert_eq!(class["uri"], format!("{CLASS_URI}{code}"), "{path}");
            assert_eq!(parent_class(path), *parent, "parent of {code} {path}");
            classified += 1;
        }
        assert_eq!(
            nodes.values().filter(|n| class_code(n).is_some()).count(),
            classified,
            "only spatial nodes and elements are classified"
        );
        let mut codes: Vec<&str> = nodes.values().filter_map(class_code).collect();
        codes.sort_unstable();
        codes.dedup();
        let expected_codes: &[&str] = if overridden {
            &[
                "IfcBuilding",
                "IfcBuildingStorey",
                "IfcCovering",
                "IfcFooting",
                "IfcProject",
                "IfcSite",
                "IfcWall",
            ]
        } else {
            &[
                "IfcBuilding",
                "IfcBuildingStorey",
                "IfcProject",
                "IfcSite",
                "IfcSlab",
                "IfcWall",
            ]
        };
        assert_eq!(codes, expected_codes);
    }
}

/// IFX-021: the project, site, building, storeys and elements carry their
/// user-visible names as `bsi::ifc::prop::Name`; the project description is
/// written as `bsi::ifc::prop::Description`. Elements have no description in
/// the model yet (gap row).
#[test]
fn ifx_021_names_everywhere_and_the_project_description() {
    let mut project = samples::two_storey_shell();
    let mut history = History::default();
    execute(
        &mut project,
        &mut history,
        Command::SetSpatialNames {
            site: "Grundstück 12".into(),
            building: "Haus A".into(),
        },
    );
    let mut info = project.info.clone();
    info.description = "Zweigeschossiger Rohbau mit Treppenöffnung".into();
    execute(&mut project, &mut history, Command::SetInfo(info));

    let nodes = nodes_of(&project);
    let mut named: Vec<(String, &str)> = vec![
        (project.id.to_string(), project.info.name.as_str()),
        (project.site.id.to_string(), "Grundstück 12"),
        (project.building.id.to_string(), "Haus A"),
    ];
    named.extend(project.storeys.iter().map(|s| (s.id.to_string(), s.name.as_str())));
    named.extend(project.elements.values().map(|e| (e.id.to_string(), e.name.as_str())));
    assert_eq!(named.len(), 3 + 2 + 12);
    for (path, expected) in named {
        assert_eq!(name(&nodes[&path]), Some(expected), "{path}");
    }
    assert_eq!(
        nodes[&project.id.to_string()].attributes["bsi::ifc::prop::Description"],
        "Zweigeschossiger Rohbau mit Treppenöffnung"
    );
}

/// IFX-022: walls carry `bsi::ifc::prop::IsExternal` true when exterior and
/// false when interior; a slab with an undefined position has none.
#[test]
fn ifx_022_is_external_follows_the_known_position() {
    let (project, _) = small_project();
    let nodes = nodes_of(&project);
    let mut seen = HashSet::new();
    for element in project.elements.values() {
        let written = nodes[&element.id.to_string()]
            .attributes
            .get("bsi::ifc::prop::IsExternal");
        let expected = match element.position {
            Position::Exterior => Some(&Value::Bool(true)),
            Position::Interior => Some(&Value::Bool(false)),
            Position::Undefined => None,
        };
        assert_eq!(written, expected, "{} ({:?})", element.name, element.position);
        seen.insert((element.tag(), element.position));
    }
    assert!(seen.contains(&(KindTag::Wall, Position::Exterior)));
    assert!(seen.contains(&(KindTag::Wall, Position::Interior)));
    assert!(seen.contains(&(KindTag::Slab, Position::Undefined)));
}

/// IFX-025: the stair hole of the upper slab of `two_storey_shell` is part
/// of the slab's body mesh, not an IfcOpeningElement and not a Void child.
#[test]
fn ifx_025_slab_holes_are_geometry_not_openings() {
    let project = samples::two_storey_shell();
    let upper_slab = project
        .elements
        .values()
        .find(|e| e.as_slab().is_some_and(|s| !s.holes.is_empty()))
        .unwrap();
    let hole = upper_slab.as_slab().unwrap().holes[0].clone();
    let nodes = nodes_of(&project);
    for (path, node) in &nodes {
        assert_ne!(class_code(node), Some("IfcOpeningElement"), "{path}");
        assert!(!node.children.contains_key("Void"), "{path} has no Void child");
        assert!(
            node.children.keys().all(|k| !k.starts_with("Opening")),
            "{path} has no opening child"
        );
    }
    let slab = &nodes[&upper_slab.id.to_string()];
    assert_eq!(slab.children.len(), 1, "only the body");
    let body = &nodes[slab.children["Body"].as_deref().unwrap()];
    let points: Vec<(f64, f64)> = body.attributes["usd::usdgeom::mesh"]["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p[0].as_f64().unwrap(), p[1].as_f64().unwrap()))
        .collect();
    for corner in hole {
        assert!(
            points
                .iter()
                .any(|&(x, y)| (x - corner.x).abs() < 1e-9 && (y - corner.y).abs() < 1e-9),
            "the hole corner {corner} is a vertex of the slab body"
        );
    }
}

/// Checks that `desc` describes every member present in `value`, through
/// nested objects and arrays, and returns how many members it checked.
fn assert_fully_described(desc: &ValueDescription, value: &Value, at: &str) -> usize {
    match desc.data_type {
        DataType::Object => {
            let restrictions = desc
                .object_restrictions
                .as_ref()
                .unwrap_or_else(|| panic!("{at}: an object schema lists its members"));
            let mut checked = 0;
            for (key, member) in value.as_object().unwrap() {
                let field = restrictions
                    .values
                    .get(key)
                    .unwrap_or_else(|| panic!("{at}.{key} is not described"));
                checked += 1 + assert_fully_described(field, member, &format!("{at}.{key}"));
            }
            checked
        }
        DataType::Array => {
            let item = &desc
                .array_restrictions
                .as_ref()
                .unwrap_or_else(|| panic!("{at}: an array schema describes its items"))
                .value;
            value
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(i, v)| assert_fully_described(item, v, &format!("{at}[{i}]")))
                .sum()
        }
        DataType::Enum => {
            let options = &desc.enum_restrictions.as_ref().unwrap().options;
            assert!(options.iter().any(|o| value == o), "{at}: {value} is an option");
            0
        }
        _ => 0,
    }
}

/// IFX-061: every `archicraft::` attribute in `data` is declared in the
/// file's `schemas` with a value description that covers every member it
/// carries, so the file validates on its own.
#[test]
fn ifx_061_every_archicraft_attribute_has_a_complete_value_description() {
    let (mut project, mut history) = small_project();
    let storey = project.storeys.ground();
    execute(
        &mut project,
        &mut history,
        Command::SetStoreyCutPlane {
            storey,
            height: Some(1.3),
        },
    );
    for project in [project, samples::two_storey_shell()] {
        let file = parse(&save(&project));
        assert!(validate::validate(&file).is_empty());
        let mut used = HashSet::new();
        let mut members = 0;
        for node in &file.data {
            for (key, value) in node.attributes.iter().filter(|(k, _)| k.starts_with("archicraft::")) {
                let declaration = file
                    .schemas
                    .get(key)
                    .unwrap_or_else(|| panic!("{key} is declared in schemas"));
                let entry = SchemaEntry::parse(declaration).unwrap_or_else(|e| panic!("{key}: {e}"));
                members += assert_fully_described(&entry.value, value, key);
                used.insert(key.as_str());
            }
        }
        let mut used: Vec<&str> = used.into_iter().collect();
        used.sort_unstable();
        assert_eq!(
            used,
            [
                "archicraft::element",
                "archicraft::project",
                "archicraft::slab",
                "archicraft::storey",
                "archicraft::wall"
            ]
        );
        assert!(members > 100, "{members}");
    }
}

/// IFX-090: `load` dispatches on ifcxVersion and accepts the version the
/// writer emits, which is the only one: `ifcx_alpha`, compact and indented.
/// Another version is refused with a message naming it.
#[test]
fn ifx_090_one_written_version_and_load_dispatches_on_it() {
    assert_eq!(WRITTEN_VERSION, "ifcx_alpha");
    let project = samples::two_storey_shell();
    let geometry = ModelGeometry::evaluate(&project);
    for pretty in [false, true] {
        let (bytes, _) = archicraft_ifcx::save(&project, &geometry, &options(), pretty);
        assert_eq!(parse(&bytes).header.ifcx_version, WRITTEN_VERSION);
        let loaded = archicraft_ifcx::load(&bytes).unwrap();
        assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    }
    for other in ["ifcx_beta", "ifcx_alpha2", ""] {
        let bytes = edit(&project, |v| v["header"]["ifcxVersion"] = other.into());
        let error = archicraft_ifcx::load(&bytes).unwrap_err();
        assert!(
            error.message.contains(&format!("“{other}” is not supported")),
            "{error:?}"
        );
        assert!(error.message.contains(WRITTEN_VERSION), "{error:?}");
    }
}

/// NFR-PORT-004: the output has no carriage return and no platform path,
/// and it does not depend on where the file is stored: a file opened from a
/// temp folder saves back to the same bytes. Golden files reproducing on
/// Windows with core.autocrlf=true is `golden_files_are_reproduced`.
#[test]
fn nfr_port_004_output_has_no_carriage_return_and_no_platform_path() {
    let project = samples::two_storey_shell();
    let geometry = ModelGeometry::evaluate(&project);
    let scratch = ScratchDir::new("nfr-port-004");
    let dir = &scratch.0;
    let path = dir.join("shell.ifcx");
    let compact = save(&project);
    std::fs::write(&path, &compact).unwrap();
    let reopened = archicraft_ifcx::load_path(&path).unwrap().project;
    assert_eq!(save(&reopened), compact, "the storage location leaves no trace");

    let temp = std::env::temp_dir();
    let mut platform_paths: Vec<String> = vec![
        dir.display().to_string(),
        temp.display().to_string(),
        "C:\\".into(),
        "C:/".into(),
        "/Users/".into(),
        "/home/".into(),
        "/tmp/".into(),
        "file:".into(),
    ];
    platform_paths.extend(std::env::var("HOME").ok().filter(|h| h.len() > 1));
    for pretty in [false, true] {
        let (bytes, _) = archicraft_ifcx::save(&reopened, &geometry, &options(), pretty);
        assert!(!bytes.contains(&b'\r'), "no carriage return (pretty: {pretty})");
        let text = String::from_utf8(bytes).unwrap();
        for platform in &platform_paths {
            assert!(!text.contains(platform.as_str()), "{platform} (pretty: {pretty})");
        }
        assert!(!text.contains('\\'), "no backslash, so no Windows path or escape");
    }
}

fn point(value: &Value) -> DVec3 {
    DVec3::new(
        value[0].as_f64().unwrap(),
        value[1].as_f64().unwrap(),
        value[2].as_f64().unwrap(),
    )
}

/// The `usd::usdgeom::mesh` of a written node as a triangle mesh.
fn mesh_of(node: &FlatNode) -> TriMesh {
    let mesh = &node.attributes["usd::usdgeom::mesh"];
    TriMesh {
        points: mesh["points"].as_array().unwrap().iter().map(point).collect(),
        indices: mesh["faceVertexIndices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| u32::try_from(i.as_u64().unwrap()).unwrap())
            .collect(),
    }
}

/// The written body meshes of an element with their child keys, in child
/// order.
fn bodies(nodes: &IndexMap<String, FlatNode>, element: ElementId) -> Vec<(String, TriMesh)> {
    nodes[&element.to_string()]
        .children
        .iter()
        .filter_map(|(role, child)| {
            let node = &nodes[child.as_deref().unwrap()];
            node.attributes
                .contains_key("usd::usdgeom::mesh")
                .then(|| (role.clone(), mesh_of(node)))
        })
        .collect()
}

/// The written points of an element's `Axis` curve.
fn axis_points(nodes: &IndexMap<String, FlatNode>, element: ElementId) -> Vec<DVec3> {
    let axis = nodes[&element.to_string()].children["Axis"].as_deref().unwrap();
    nodes[axis].attributes["usd::usdgeom::basiscurves"]["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(point)
        .collect()
}

/// The translation of a node's `usd::xformop`, after checking that the
/// matrix is 4×4, row-major in the row-vector convention: identity in the
/// upper 3×3 block, (0, 0, 0, 1) as fourth column, the translation in the
/// fourth row.
fn translation(node: &FlatNode) -> Option<DVec3> {
    let rows = node.attributes.get("usd::xformop")?["transform"].as_array().unwrap();
    let m: Vec<Vec<f64>> = rows
        .iter()
        .map(|row| row.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect())
        .collect();
    assert_eq!(m.len(), 4, "{m:?}");
    for (i, row) in m.iter().enumerate() {
        assert_eq!(row.len(), 4, "{m:?}");
        assert_eq!(row[3], if i == 3 { 1.0 } else { 0.0 }, "fourth column of {m:?}");
        if i < 3 {
            for (j, &v) in row[..3].iter().enumerate() {
                assert_eq!(v, if i == j { 1.0 } else { 0.0 }, "no rotation or scale in {m:?}");
            }
        }
    }
    Some(DVec3::new(m[3][0], m[3][1], m[3][2]))
}

/// The world translation of a node: the transforms of the node and of every
/// ancestor composed, which for pure translations is their sum.
fn world_translation(nodes: &IndexMap<String, FlatNode>, parents: &HashMap<String, String>, path: &str) -> DVec3 {
    let mut offset = DVec3::ZERO;
    let mut at = Some(path);
    while let Some(p) = at {
        offset += translation(&nodes[p]).unwrap_or(DVec3::ZERO);
        at = parents.get(p).map(String::as_str);
    }
    offset
}

/// Removes every `archicraft::` attribute and schema declaration, leaving
/// what a consumer that ignores the ArchiCraft namespace reads.
fn standard_namespaces_only(value: &mut Value) {
    for node in value["data"].as_array_mut().unwrap() {
        if let Some(attributes) = node.get_mut("attributes").and_then(Value::as_object_mut) {
            attributes.retain(|key, _| !key.starts_with("archicraft::"));
        }
    }
    value["schemas"]
        .as_object_mut()
        .unwrap()
        .retain(|key, _| !key.starts_with("archicraft::"));
}

fn standard_content(bytes: &[u8]) -> Value {
    let mut value: Value = serde_json::from_slice(bytes).unwrap();
    standard_namespaces_only(&mut value);
    value
}

/// Per node: class code, Name and number of mesh children.
fn projection(nodes: &IndexMap<String, FlatNode>) -> IndexMap<String, (Option<String>, Option<String>, usize)> {
    nodes
        .iter()
        .map(|(path, node)| {
            let meshes = node
                .children
                .values()
                .flatten()
                .filter(|child| nodes[child.as_str()].attributes.contains_key("usd::usdgeom::mesh"))
                .count();
            (
                path.clone(),
                (
                    class_code(node).map(str::to_owned),
                    name(node).map(str::to_owned),
                    meshes,
                ),
            )
        })
        .collect()
}

fn upper_storey(project: &Project) -> archicraft_model::StoreyId {
    project.storeys.as_slice()[1].id
}

/// A project far from the origin with odd coordinates: the upper storey at
/// 3.0123456789 m with four joined walls around a rectangle whose sides have
/// third and seventh fractions, and a slab with a hole on the ground storey.
fn odd_project() -> Project {
    let mut project = default_metric_project();
    let mut history = History::default();
    let ground = project.storeys.ground();
    let upper = upper_storey(&project);
    execute(
        &mut project,
        &mut history,
        Command::SetStoreyElevation {
            storey: upper,
            elevation: 3.012_345_678_9,
            shift_above: false,
        },
    );
    let origin = DVec2::new(1234.567891, -2345.678912);
    let (w, d) = (7.0 + 1.0 / 3.0, 5.0 + 2.0 / 7.0);
    let corners = [DVec2::ZERO, DVec2::new(w, 0.0), DVec2::new(w, d), DVec2::new(0.0, d)].map(|c| origin + c);
    let mut elements: Vec<_> = (0..4)
        .map(|i| project.new_wall(upper, corners[i], corners[(i + 1) % 4], i as u32))
        .collect();
    let hole = [
        DVec2::new(1.1, 1.3),
        DVec2::new(2.2 + 1.0 / 9.0, 1.3),
        DVec2::new(2.2 + 1.0 / 9.0, 2.7),
        DVec2::new(1.1, 2.7),
    ]
    .map(|c| origin + c);
    elements.push(project.new_slab(ground, corners.to_vec(), vec![hole.to_vec()], 0));
    execute(&mut project, &mut history, Command::AddElements(elements));
    project
}

/// IFX-030: lengths are metres, +Z is up, +Y is project north and the
/// system is right-handed: a 5 m wall along +X, 2.7 m high, is written as a
/// body spanning 5 m in x, its thickness towards +Y (the interior side, left
/// of the drawing direction) and 2.7 m in z, with a positive signed volume
/// and its top face seen counter-clockwise from above. True north and the
/// survey point round-trip in `archicraft::project.location`.
#[test]
fn ifx_030_metres_z_up_right_handed_with_the_location_in_archicraft_project() {
    let mut project = default_metric_project();
    let mut history = History::default();
    let ground = project.storeys.ground();
    let mut wall = project.new_wall(ground, DVec2::ZERO, DVec2::new(5.0, 0.0), 0);
    if let ElementKind::Wall(w) = &mut wall.kind {
        w.top = TopLink::Fixed { height: 2.7 };
    }
    let thickness = wall.as_wall().unwrap().thickness;
    let id = wall.id;
    execute(&mut project, &mut history, Command::AddElements(vec![wall]));
    let location = Location {
        altitude: 312.25,
        survey_point: [2_600_123.456, 1_200_987.654],
        true_north: 0.3,
    };
    execute(&mut project, &mut history, Command::SetLocation(location));

    let bytes = save(&project);
    let nodes = validate::flatten(&parse(&bytes));
    let written = bodies(&nodes, id);
    assert_eq!(written.len(), 1);
    let mesh = &written[0].1;
    let (lo, hi) = mesh.bounds().unwrap();
    let close = |a: DVec3, b: DVec3| (a - b).abs().max_element() < 1e-9;
    assert!(close(lo, DVec3::ZERO), "{lo}");
    assert!(close(hi, DVec3::new(5.0, thickness, 2.7)), "{hi}");
    let volume = 5.0 * thickness * 2.7;
    assert!(
        (mesh.signed_volume() - volume).abs() < 1e-9,
        "outward orientation in a right-handed system: {} vs {volume}",
        mesh.signed_volume()
    );
    let mut top_faces = 0;
    for [a, b, c] in mesh.triangles() {
        if [a, b, c].iter().all(|p| (p.z - 2.7).abs() < 1e-9) {
            assert!((b - a).cross(c - a).z > 0.0, "the top face looks up (+Z)");
            top_faces += 1;
        }
    }
    assert!(top_faces >= 2, "{top_faces}");
    let axis = axis_points(&nodes, id);
    assert!(close(axis[0].with_z(0.0), DVec3::ZERO), "{axis:?}");
    assert!(close(axis[1].with_z(0.0), DVec3::new(5.0, 0.0, 0.0)), "{axis:?}");

    assert_eq!(
        nodes[&project.id.to_string()].attributes["archicraft::project"]["location"],
        json!({
            "altitude": 312.25,
            "surveyPoint": [2_600_123.456, 1_200_987.654],
            "trueNorth": 0.3,
        })
    );
    let reopened = archicraft_ifcx::load(&bytes).unwrap().project;
    assert_eq!(reopened.site.location, location);
}

/// IFX-031: storey placements are `usd::xformop` 4×4 matrices, row-major
/// with the translation in the fourth row, translating by the elevation;
/// no other node is placed, and every written mesh and axis point, composed
/// with the transforms of its ancestors, lies within 0.1 mm of the model's
/// world position.
#[test]
fn ifx_031_storey_transforms_carry_the_elevation_and_world_points_match_within_0_1_mm() {
    let project = odd_project();
    let geometry = ModelGeometry::evaluate(&project);
    let nodes = nodes_of(&project);
    let parents = parents(&nodes);
    for storey in project.storeys.iter() {
        let t = translation(&nodes[&storey.id.to_string()]).unwrap();
        assert!(
            (t - DVec3::new(0.0, 0.0, storey.elevation)).abs().max_element() < 1e-6,
            "{} at {}: {t}",
            storey.name,
            storey.elevation
        );
    }
    let storeys: HashSet<String> = project.storeys.iter().map(|s| s.id.to_string()).collect();
    for (path, node) in &nodes {
        if !storeys.contains(path) {
            assert!(translation(node).is_none(), "only storeys are placed: {path}");
        }
    }
    assert!(project.storeys.iter().any(|s| s.elevation == 3.012_345_678_9));

    let mut checked = 0;
    for element in project.elements.values() {
        let elevation = project.storeys.get(element.storey).unwrap().elevation;
        let model = geometry.get(element.id).unwrap();
        let mut written = bodies(&nodes, element.id);
        assert_eq!(written.len(), model.parts.len());
        let element_node = &nodes[&element.id.to_string()];
        let mut pairs: Vec<(String, Vec<DVec3>, Vec<DVec3>)> = written
            .drain(..)
            .zip(&model.parts)
            .map(|((role, mesh), part)| (role, mesh.points, part.mesh.points.clone()))
            .collect();
        if element_node.children.contains_key("Axis") {
            pairs.push(("Axis".into(), axis_points(&nodes, element.id), model.axis.clone()));
        }
        for (role, written, model) in pairs {
            let child = element_node.children[&role].as_deref().unwrap();
            let offset = world_translation(&nodes, &parents, child);
            assert_eq!(written.len(), model.len(), "{} {role}", element.name);
            for (w, m) in written.iter().zip(&model) {
                let world = *w + offset;
                let expected = *m + DVec3::new(0.0, 0.0, elevation);
                assert!(
                    world.distance(expected) < 1e-4,
                    "{} {role}: {world} vs {expected}",
                    element.name
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 40, "{checked}");
}

/// IFX-043: every body references a material node through `inherits`, and
/// every material node carries `bsi::ifc::prop::Name` with the name of the
/// project material it stands for.
#[test]
fn ifx_043_every_body_references_a_named_material_node() {
    let (small, _) = small_project();
    for project in [small, samples::two_storey_shell()] {
        let geometry = ModelGeometry::evaluate(&project);
        let nodes = nodes_of(&project);
        let mut bodies_seen = 0;
        let mut referenced = HashSet::new();
        for element in project.elements.values() {
            let parts = &geometry.get(element.id).unwrap().parts;
            let node = &nodes[&element.id.to_string()];
            for part in parts {
                let body = &nodes[node.children[&part.name].as_deref().unwrap()];
                assert!(body.attributes.contains_key("usd::usdgeom::mesh"));
                let material = &body.inherits["material"];
                assert_eq!(*material, part.material.to_string(), "{} {}", element.name, part.name);
                let expected = project.materials.iter().find(|m| m.id == part.material).unwrap();
                assert!(!expected.name.is_empty());
                assert_eq!(
                    name(&nodes[material.as_str()]),
                    Some(expected.name.as_str()),
                    "material of {} {}",
                    element.name,
                    part.name
                );
                referenced.insert(material.clone());
                bodies_seen += 1;
            }
        }
        assert!(bodies_seen >= project.elements.len());
        let material_nodes: HashSet<String> = nodes
            .iter()
            .filter(|(_, n)| n.attributes.contains_key("bsi::ifc::presentation::diffuseColor"))
            .map(|(path, n)| {
                assert!(name(n).is_some_and(|s| !s.is_empty()), "{path} is named");
                path.clone()
            })
            .collect();
        assert_eq!(material_nodes, referenced);
        assert!(referenced.len() >= 2, "{referenced:?}");
    }
}

/// IFX-063: a consumer that ignores `archicraft::` sees the same building.
/// A copy of the file with every `archicraft::` attribute and schema removed
/// still validates, keeps every standard attribute, child and inherit, and
/// composes to the same classes, names and mesh counts, which are those of
/// the model.
#[test]
fn ifx_063_without_the_archicraft_namespace_the_file_describes_the_same_building() {
    let project = samples::two_storey_shell();
    let geometry = ModelGeometry::evaluate(&project);
    let bytes = save(&project);
    let stripped_bytes = serde_json::to_vec(&standard_content(&bytes)).unwrap();
    assert!(
        !String::from_utf8(stripped_bytes.clone())
            .unwrap()
            .contains("archicraft::")
    );

    let (full, stripped) = (parse(&bytes), parse(&stripped_bytes));
    for file in [&full, &stripped] {
        let problems = validate::validate(file);
        assert!(problems.is_empty(), "{problems:#?}");
    }
    let (full, stripped) = (validate::flatten(&full), validate::flatten(&stripped));
    assert_eq!(validate::roots(&stripped).unwrap(), vec![project.id.to_string()]);
    assert_eq!(stripped.keys().collect::<Vec<_>>(), full.keys().collect::<Vec<_>>());
    for (path, node) in &full {
        let standard: IndexMap<&String, &Value> = node
            .attributes
            .iter()
            .filter(|(key, _)| !key.starts_with("archicraft::"))
            .collect();
        let kept: IndexMap<&String, &Value> = stripped[path].attributes.iter().collect();
        assert_eq!(kept, standard, "{path}");
        assert_eq!(stripped[path].children, node.children, "{path}");
        assert_eq!(stripped[path].inherits, node.inherits, "{path}");
    }
    let seen = projection(&stripped);
    assert_eq!(seen, projection(&full));

    for element in project.elements.values() {
        let parts = geometry.get(element.id).unwrap().parts.len();
        assert_eq!(
            seen[&element.id.to_string()],
            (
                Some(element.ifc_class.code().to_owned()),
                Some(element.name.clone()),
                parts
            ),
            "{}",
            element.name
        );
    }
    for storey in project.storeys.iter() {
        let node = &stripped[&storey.id.to_string()];
        assert_eq!(name(node), Some(storey.name.as_str()));
        assert!((translation(node).unwrap().z - storey.elevation).abs() < 1e-6);
    }
}

/// IFX-064: after save and reopen every element has its original
/// parameters bit for bit, and the geometry regenerated from them equals the
/// stored meshes within 0.1 mm, on a project far from the origin with odd
/// coordinates, an odd storey elevation, wall junctions and a slab hole.
#[test]
fn ifx_064_reopened_elements_keep_their_parameters_and_regenerate_the_stored_geometry() {
    let project = odd_project();
    let bytes = save(&project);
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    let reopened = loaded.project;
    assert_eq!(reopened.elements.len(), project.elements.len());
    for element in project.elements.values() {
        assert_eq!(reopened.elements[&element.id], *element, "{}", element.name);
    }
    assert_eq!(reopened.storeys, project.storeys);

    let stored = validate::flatten(&parse(&bytes));
    let regenerated = ModelGeometry::evaluate(&reopened);
    let mut checked = 0;
    for element in reopened.elements.values() {
        let parts = &regenerated.get(element.id).unwrap().parts;
        let written = bodies(&stored, element.id);
        assert_eq!(written.len(), parts.len(), "{}", element.name);
        for ((role, mesh), part) in written.iter().zip(parts) {
            assert_eq!(*role, part.name);
            assert_eq!(mesh.indices, part.mesh.indices, "{} {role}", element.name);
            assert_eq!(mesh.points.len(), part.mesh.points.len(), "{} {role}", element.name);
            for (s, r) in mesh.points.iter().zip(&part.mesh.points) {
                assert!(
                    s.distance(*r) < 1e-4,
                    "{} {role}: stored {s}, regenerated {r}",
                    element.name
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 40, "{checked}");
}

/// IFX-067: an unused material and an unused layer are stored in
/// `archicraft::project` on the project node with every other material,
/// layer, view and the tool defaults; only the used material becomes a
/// node, and the composed tree keeps the project as its single root.
#[test]
fn ifx_067_unused_resources_live_in_archicraft_project_and_the_tree_keeps_one_root() {
    let mut project = default_metric_project();
    let mut history = History::default();
    let ground = project.storeys.ground();
    let wall = project.new_wall(ground, DVec2::ZERO, DVec2::new(4.0, 0.0), 0);
    let used = wall.as_wall().unwrap().material;
    execute(&mut project, &mut history, Command::AddElements(vec![wall]));
    let mut unused = project.materials[0].clone();
    unused.id = MaterialId::new();
    unused.name = "Stampflehm (ungenutzt)".into();
    unused.code = "SL".into();
    let unused_layer = Layer::new("Bestand (leer)");
    execute(&mut project, &mut history, Command::AddMaterial(unused.clone()));
    execute(&mut project, &mut history, Command::AddLayer(unused_layer.clone()));
    assert!(project.elements.values().all(|e| e.layer != unused_layer.id));

    let nodes = nodes_of(&project);
    assert_eq!(validate::roots(&nodes).unwrap(), vec![project.id.to_string()]);
    let material_nodes: Vec<&String> = nodes
        .iter()
        .filter(|(_, n)| n.attributes.contains_key("bsi::ifc::presentation::diffuseColor"))
        .map(|(path, _)| path)
        .collect();
    assert_eq!(material_nodes, [&used.to_string()], "only the used material is a node");
    assert!(!nodes.contains_key(&unused.id.to_string()));
    assert!(!nodes.contains_key(&unused_layer.id.to_string()));

    let resources = &nodes[&project.id.to_string()].attributes["archicraft::project"];
    let ids = |list: &str| -> Vec<String> {
        resources[list]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_owned())
            .collect()
    };
    let record = |list: &str, id: String| {
        resources[list]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id.as_str())
            .cloned()
    };
    assert_eq!(
        ids("materials"),
        project.materials.iter().map(|m| m.id.to_string()).collect::<Vec<_>>()
    );
    assert_eq!(
        ids("layers"),
        project.layers.iter().map(|l| l.id.to_string()).collect::<Vec<_>>()
    );
    assert_eq!(
        ids("views"),
        project.views.iter().map(|v| v.id.to_string()).collect::<Vec<_>>()
    );
    assert!(!project.views.is_empty());
    assert_eq!(
        record("materials", unused.id.to_string()).unwrap()["name"],
        "Stampflehm (ungenutzt)"
    );
    assert_eq!(
        record("layers", unused_layer.id.to_string()).unwrap()["name"],
        "Bestand (leer)"
    );
    assert!(resources["toolDefaults"]["wall"].is_object());
    assert!(resources["toolDefaults"]["slab"].is_object());
}

/// IFX-080, XCH-008: a file the system wrote opens from disk without
/// problems, and every element accepts an UpdateElements command changing
/// its name and a parameter; the edits save and reopen, and undo restores
/// the elements as opened.
#[test]
fn ifx_080_xch_008_every_element_of_an_opened_file_accepts_update_elements() {
    let project = samples::two_storey_shell();
    let scratch = ScratchDir::new("ifx-080");
    let path = scratch.0.join("shell.ifcx");
    std::fs::write(&path, save(&project)).unwrap();
    let opened = archicraft_ifcx::load_path(&path).unwrap();
    assert!(opened.problems.is_empty(), "{:#?}", opened.problems);
    let mut reopened = opened.project;
    let as_opened = reopened.elements.clone();
    let mut history = History::default();
    let ids: Vec<ElementId> = reopened.elements.keys().copied().collect();
    assert_eq!(ids.len(), project.elements.len());
    let mut kinds = HashSet::new();
    for id in &ids {
        let mut element = reopened.elements[id].clone();
        element.name = format!("{} (bearbeitet)", element.name);
        match &mut element.kind {
            ElementKind::Wall(w) => w.thickness += 0.01,
            ElementKind::Slab(s) => s.thickness += 0.01,
            other => panic!("the sample has only walls and slabs, not {other:?}"),
        }
        kinds.insert(element.tag());
        execute(
            &mut reopened,
            &mut history,
            Command::UpdateElements(vec![element.clone()]),
        );
        assert_eq!(reopened.elements[id], element);
    }
    assert_eq!(kinds.len(), 2, "walls and slabs");
    assert_eq!(history.undo_len(), ids.len());

    let edited = archicraft_ifcx::load(&save(&reopened)).unwrap();
    assert!(edited.problems.is_empty(), "{:#?}", edited.problems);
    assert_eq!(edited.project.elements, reopened.elements);
    while history.undo(&mut reopened).is_some() {}
    assert_eq!(reopened.elements, as_opened);
}

/// LAY-005: layer visibility and locking live in the ArchiCraft namespace
/// only. A project with a hidden and a locked layer writes standard-namespace
/// content identical to the same project with every layer visible and
/// unlocked; the flags and the layer assignment are in `archicraft::`.
#[test]
fn lay_005_hidden_and_locked_layers_leave_the_standard_content_unchanged() {
    let (visible, _) = small_project();
    let mut flagged = visible.clone();
    let mut history = History::default();
    let walls = flagged.tool_defaults.get(KindTag::Wall).unwrap().layer;
    let slabs = flagged.tool_defaults.get(KindTag::Slab).unwrap().layer;
    assert_ne!(walls, slabs);
    for (layer, count) in [(walls, 2), (slabs, 1)] {
        assert_eq!(flagged.elements.values().filter(|e| e.layer == layer).count(), count);
    }
    let layer = |id| flagged.layers.iter().find(|l| l.id == id).unwrap().clone();
    let mut hidden = layer(walls);
    hidden.visible = false;
    let mut locked = layer(slabs);
    locked.locked = true;
    execute(&mut flagged, &mut history, Command::UpdateLayer(hidden));
    execute(&mut flagged, &mut history, Command::UpdateLayer(locked));

    let (plain, with_flags) = (save(&visible), save(&flagged));
    assert_ne!(plain, with_flags, "the flags are saved");
    assert_eq!(standard_content(&with_flags), standard_content(&plain));

    let nodes = validate::flatten(&parse(&with_flags));
    let layers = nodes[&flagged.id.to_string()].attributes["archicraft::project"]["layers"]
        .as_array()
        .unwrap()
        .clone();
    let flags = |id: archicraft_model::LayerId| {
        let record = layers.iter().find(|l| l["id"] == id.to_string().as_str()).unwrap();
        (record["visible"].clone(), record["locked"].clone())
    };
    assert_eq!(flags(walls), (json!(false), json!(false)));
    assert_eq!(flags(slabs), (json!(true), json!(true)));
    for element in flagged.elements.values() {
        let node = &nodes[&element.id.to_string()];
        assert_eq!(
            node.attributes["archicraft::element"]["layer"],
            element.layer.to_string().as_str()
        );
        assert!(
            !bodies(&nodes, element.id).is_empty(),
            "{} keeps its body",
            element.name
        );
    }
}

/// PRJ-010: SetInfo changes all six project information fields and undo
/// restores them; with non-ASCII text all six round-trip, name and
/// description on the project node as `bsi::ifc::prop::Name` and
/// `bsi::ifc::prop::Description`, the other four in `archicraft::project`.
#[test]
fn prj_010_project_information_is_editable_and_round_trips() {
    let mut project = default_metric_project();
    let mut history = History::default();
    let before = project.info.clone();
    let info = ProjectInfo {
        name: "Wohnhaus Größe ½".into(),
        number: "P-2026/04 № 7".into(),
        description: "Neubau mit Dachgeschoß – Ausführungsplanung".into(),
        client: "Familie Ångström & Søn".into(),
        address: "Hauptstraße 5, 1010 Wien, Österreich".into(),
        author: "Zoë Łukasiewicz".into(),
    };
    let fields = |i: &ProjectInfo| {
        [
            i.name.clone(),
            i.number.clone(),
            i.description.clone(),
            i.client.clone(),
            i.address.clone(),
            i.author.clone(),
        ]
    };
    for (old, new) in fields(&before).iter().zip(fields(&info)) {
        assert_ne!(*old, new);
        assert!(!new.is_ascii(), "{new}");
    }
    execute(&mut project, &mut history, Command::SetInfo(info.clone()));
    assert_eq!(project.info, info);
    assert!(history.undo(&mut project).is_some());
    assert_eq!(project.info, before);
    assert!(history.redo(&mut project).is_some());
    assert_eq!(project.info, info);

    let bytes = save(&project);
    let nodes = validate::flatten(&parse(&bytes));
    let node = &nodes[&project.id.to_string()];
    assert_eq!(name(node), Some(info.name.as_str()));
    assert_eq!(
        node.attributes["bsi::ifc::prop::Description"],
        info.description.as_str()
    );
    assert_eq!(
        node.attributes["archicraft::project"]["info"],
        json!({
            "number": info.number,
            "client": info.client,
            "address": info.address,
            "author": info.author,
        })
    );
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert_eq!(loaded.project.info, info);
}

/// NFR-ACC-001: coordinates are doubles; a wall starting at
/// (9999.99, -9999.99) round-trips its parameters bit for bit, its written
/// mesh and axis points lie within 0.01 mm of the model, and the spacing of
/// doubles at 10 km is far below 0.01 mm.
#[test]
fn nfr_acc_001_a_wall_ten_kilometres_out_round_trips_exactly_and_meshes_within_0_01_mm() {
    let mut project = default_metric_project();
    let mut history = History::default();
    let ground = project.storeys.ground();
    let start = DVec2::new(9999.99, -9999.99);
    let wall = project.new_wall(ground, start, start + DVec2::new(4.0 / 3.0, 2.0 / 7.0), 0);
    let id = wall.id;
    execute(&mut project, &mut history, Command::AddElements(vec![wall]));
    let geometry = ModelGeometry::evaluate(&project);
    let bytes = save(&project);

    let reopened = archicraft_ifcx::load(&bytes).unwrap().project;
    let bits = |w: &Wall| [w.start.x, w.start.y, w.end.x, w.end.y, w.thickness].map(f64::to_bits);
    let (model, read) = (
        project.elements[&id].as_wall().unwrap(),
        reopened.elements[&id].as_wall().unwrap(),
    );
    assert_eq!(bits(read), bits(model));
    assert_eq!(read.start, DVec2::new(9999.99, -9999.99));

    let nodes = validate::flatten(&parse(&bytes));
    let expected = geometry.get(id).unwrap();
    let written = bodies(&nodes, id);
    assert_eq!(written.len(), expected.parts.len());
    let mut pairs: Vec<(Vec<DVec3>, Vec<DVec3>)> = written
        .into_iter()
        .zip(&expected.parts)
        .map(|((_, mesh), part)| (mesh.points, part.mesh.points.clone()))
        .collect();
    pairs.push((axis_points(&nodes, id), expected.axis.clone()));
    let mut checked = 0;
    for (written, model) in pairs {
        assert_eq!(written.len(), model.len());
        for (w, m) in written.iter().zip(&model) {
            assert!(m.x > 9999.0 && m.y < -9998.0, "{m}");
            assert!((*w - *m).abs().max_element() < 1e-5, "{w} vs {m}");
            checked += 1;
        }
    }
    assert!(checked >= 10, "{checked}");
    let spacing = 10_000.0f64.next_up() - 10_000.0;
    assert!(spacing < 1e-8, "representation error at 10 km: {spacing}");
}

/// NFR-ACC-002 (IFCX half): values typed at 0.1 mm and 1/64 inch
/// resolution are written in `archicraft::` with round-trip precision and
/// read back bit for bit; only mesh, curve and placement coordinates are
/// rounded, to 10⁻⁶ m (IFX-007). The input resolution itself is
/// `nfr_acc_002_input_resolves_0_1_mm_and_1_64_inch_at_full_precision`.
#[test]
fn nfr_acc_002_parameters_keep_round_trip_precision_and_only_coordinates_are_rounded() {
    let nf = NumberFormat::default();
    let thickness = parse_length("9 51/64\"", LengthUnit::Meter, &nf).unwrap();
    let x = parse_length("1234567.8", LengthUnit::Millimeter, &nf).unwrap();
    let y = parse_length("-76.5432", LengthUnit::Meter, &nf).unwrap();
    let length = parse_length("17' 3 7/64\"", LengthUnit::Meter, &nf).unwrap();
    let decimals = |v: f64| format_f64(v).split_once('.').map_or(0, |(_, d)| d.len());
    assert_eq!(format_f64(x), "1234.5678", "0.1 mm resolution");
    for value in [thickness, y + length] {
        assert!(decimals(value) > 6, "{value} needs more than 6 decimals");
    }
    let mut project = default_metric_project();
    let mut history = History::default();
    let ground = project.storeys.ground();
    let mut wall = project.new_wall(ground, DVec2::new(x, y), DVec2::new(x, y + length), 0);
    if let ElementKind::Wall(w) = &mut wall.kind {
        w.thickness = thickness;
    }
    let id = wall.id;
    execute(&mut project, &mut history, Command::AddElements(vec![wall]));
    let model = project.elements[&id].as_wall().unwrap().clone();

    let bytes = save(&project);
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.contains(&format!(r#""thickness":{}"#, format_f64(thickness))));
    let nodes = validate::flatten(&parse(&bytes));
    let written = &nodes[&id.to_string()].attributes["archicraft::wall"];
    let f = |v: &Value| v.as_f64().unwrap().to_bits();
    assert_eq!(f(&written["thickness"]), thickness.to_bits());
    assert_eq!(
        [
            &written["start"][0],
            &written["start"][1],
            &written["end"][0],
            &written["end"][1]
        ]
        .map(f),
        [model.start.x, model.start.y, model.end.x, model.end.y].map(f64::to_bits)
    );
    let reopened = archicraft_ifcx::load(&bytes).unwrap().project;
    assert_eq!(*reopened.elements[&id].as_wall().unwrap(), model);

    let geometry = ModelGeometry::evaluate(&project);
    let mut coordinates: Vec<(f64, f64)> = Vec::new();
    for ((_, mesh), part) in bodies(&nodes, id).into_iter().zip(&geometry.get(id).unwrap().parts) {
        for (w, m) in mesh.points.iter().zip(&part.mesh.points) {
            coordinates.extend([(w.x, m.x), (w.y, m.y), (w.z, m.z)]);
        }
    }
    for (w, m) in axis_points(&nodes, id).iter().zip(&geometry.get(id).unwrap().axis) {
        coordinates.extend([(w.x, m.x), (w.y, m.y), (w.z, m.z)]);
    }
    for storey in project.storeys.iter() {
        let t = translation(&nodes[&storey.id.to_string()]).unwrap();
        coordinates.push((t.z, storey.elevation));
    }
    assert!(coordinates.len() > 20);
    let mut rounded = 0;
    for (w, m) in coordinates {
        assert!(decimals(w) <= 6, "{w} is rounded to 10⁻⁶ m");
        assert!((w - m).abs() <= 5e-7 + 1e-9, "{w} vs {m}");
        rounded += usize::from(w != m);
    }
    assert!(rounded > 0, "some coordinates were rounded");
}

/// IFX-083: a composition cycle, a reference to an unknown node and an
/// attribute that violates its schema are each reported with the affected
/// node path, an unresolved import with its URI, and the file still opens
/// with every element.
#[test]
fn ifx_083_composition_and_validation_problems_name_the_node_and_the_file_still_opens() {
    let project = samples::storey_with_walls_and_slab();
    let missing_import = "https://example.com/not-bundled.ifcx.json";
    let mut wall_path = String::new();
    let bytes = common::edit(&project, |v| {
        let i = common::node_index(v, "W-001");
        wall_path = v["data"][i]["path"].as_str().unwrap().to_owned();
        v["data"][i]["children"]["Ghost"] = "no-such-node".into();
        v["data"][i]["attributes"]["bsi::ifc::prop::IsExternal"] = "yes".into();
        let data = v["data"].as_array_mut().unwrap();
        data.push(json!({"path": "cycle-a", "inherits": {"next": "cycle-b"}}));
        data.push(json!({"path": "cycle-b", "inherits": {"next": "cycle-a"}}));
        v["imports"]
            .as_array_mut()
            .unwrap()
            .push(json!({"uri": missing_import}));
    });

    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let problems = &loaded.problems;
    let at = |path: &str, part: &str| {
        problems
            .iter()
            .any(|p| p.path.as_deref() == Some(path) && p.message.contains(part))
    };
    assert!(
        ["cycle-a", "cycle-b"].iter().any(|p| at(p, "composition cycle")),
        "{problems:#?}"
    );
    assert!(at(&wall_path, "unknown node “no-such-node”"), "{problems:#?}");
    assert!(
        at(&wall_path, "“bsi::ifc::prop::IsExternal”: expected a boolean"),
        "{problems:#?}"
    );
    assert!(
        problems.iter().any(|p| p.message.contains(missing_import)),
        "{problems:#?}"
    );

    // Lenient reading: everything that can be read is loaded.
    assert_eq!(loaded.project.elements.len(), project.elements.len());
    for (id, element) in &project.elements {
        assert_eq!(loaded.project.elements[id].name, element.name);
    }
}
