//! Regression net for the done IFCX requirements about the container, the
//! header, namespaces, identities, classes and names (IFX-001, IFX-002,
//! IFX-005, IFX-010, IFX-013, IFX-014, IFX-020, IFX-021, IFX-022, IFX-025,
//! IFX-061, IFX-090, NFR-PORT-004).
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

use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::alpha::writer::sub_node_id;
use archicraft_ifcx::container::{DataType, SchemaEntry, ValueDescription};
use archicraft_ifcx::validate::{self, FlatNode};
use archicraft_ifcx::{IfcxFile, WRITTEN_VERSION, WriteOptions, registry};
use archicraft_model::glam::DVec2;
use archicraft_model::template::default_metric_project;
use archicraft_model::{Command, ElementKind, History, IfcClass, KindTag, Position, Project, samples};
use indexmap::IndexMap;
use serde_json::Value;
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

/// IFX-010: every project, site, building, storey, element and material
/// node has an RFC 9562 UUID path, version 4 for the objects created here;
/// Body and Axis sub-nodes have the version 5 UUID of owner and role, which
/// a wall thickness change does not change.
#[test]
fn ifx_010_nodes_have_uuid_paths_and_sub_nodes_name_based_uuids() {
    let mut project = samples::two_storey_shell();
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
    assert_eq!(axes, 10, "every wall has an axis");

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
