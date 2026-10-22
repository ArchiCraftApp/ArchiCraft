//! Deterministic, diffable IFCX output (IFX-007): rounded placements,
//! full-precision parameters (NFR-ACC-001, NFR-ACC-002), node and key order,
//! the indented option and the insta snapshots reviewed in pull requests.

#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};

use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::WriteOptions;
use archicraft_ifcx::alpha::writer::{sub_node_id, to_bytes, write};
use archicraft_ifcx::json::{self, format_f64, round_coordinate};
use archicraft_model::glam::DVec2;
use archicraft_model::units::{LengthUnit, NumberFormat, parse_length};
use archicraft_model::{
    Command, ElementId, ElementKind, History, Project, Storey, StoreyId, samples, template::default_metric_project,
};
use serde_json::{Map, Value, json};

use common::{options, save};

fn execute(project: &mut Project, history: &mut History, command: Command) {
    history.execute(project, command).unwrap();
}

fn upper(project: &Project) -> StoreyId {
    project.storeys.as_slice()[1].id
}

/// Adds a wall and returns its id.
fn add_wall(project: &mut Project, history: &mut History, storey: StoreyId, start: DVec2, end: DVec2) -> ElementId {
    let wall = project.new_wall(storey, start, end, project.elements.len() as u32);
    let id = wall.id;
    execute(project, history, Command::AddElements(vec![wall]));
    id
}

/// The default project with the ground storey at 1e-7 m, the upper storey at
/// 2.9999999999 m and a wall at odd coordinates on the upper storey.
fn odd_elevations() -> (Project, ElementId) {
    let mut project = default_metric_project();
    let mut history = History::default();
    let ground = project.storeys.ground();
    let upper = upper(&project);
    for (storey, elevation) in [(ground, 1e-7), (upper, 2.9999999999)] {
        execute(
            &mut project,
            &mut history,
            Command::SetStoreyElevation {
                storey,
                elevation,
                shift_above: false,
            },
        );
    }
    let wall = add_wall(
        &mut project,
        &mut history,
        upper,
        DVec2::new(1.0 / 3.0, 2.0 / 7.0),
        DVec2::new(4.0 + 1.0 / 3.0, 3.0 + 2.0 / 7.0),
    );
    (project, wall)
}

fn parse(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap()
}

fn data(value: &Value) -> &Vec<Value> {
    value["data"].as_array().unwrap()
}

fn node<'a>(value: &'a Value, path: &str) -> &'a Value {
    data(value).iter().find(|n| n["path"] == path).unwrap()
}

/// The fourth row of a storey's `usd::xformop` matrix: its translation.
fn translation_row(storey: &Value) -> &Value {
    &storey["attributes"]["usd::xformop"]["transform"][3]
}

/// True if the bytes outside string literals contain an exponent.
fn has_exponent(bytes: &[u8]) -> bool {
    let mut outside = Vec::with_capacity(bytes.len());
    let (mut in_string, mut escaped) = (false, false);
    for &c in bytes {
        match (in_string, escaped, c) {
            (true, true, _) => escaped = false,
            (true, false, b'\\') => escaped = true,
            (true, false, b'"') => in_string = false,
            (true, false, _) => {}
            (false, _, b'"') => in_string = true,
            (false, _, c) => outside.push(c),
        }
    }
    outside
        .windows(2)
        .any(|w| w[0].is_ascii_digit() && (w[1] == b'e' || w[1] == b'E'))
}

/// Number of decimals of a number as written (`3.25` has 2).
fn decimals(text: &str) -> usize {
    text.split_once('.').map_or(0, |(_, f)| f.len())
}

#[test]
fn storey_placements_are_rounded_and_storey_parameters_are_not() {
    // IFX-007: placements are coordinates rounded to 10⁻⁶ m; NFR-ACC-002:
    // the archicraft:: parameter keeps its round-trip precision.
    let (project, _) = odd_elevations();
    let value = parse(&save(&project));
    let ground = node(&value, &project.storeys.ground().to_string());
    let upper = node(&value, &upper(&project).to_string());
    assert_eq!(translation_row(upper), &json!([0, 0, 3, 1]));
    assert_eq!(
        upper["attributes"]["archicraft::storey"]["elevation"],
        json!(2.9999999999)
    );
    assert_eq!(translation_row(ground), &json!([0, 0, 0, 1]));
    assert_eq!(ground["attributes"]["archicraft::storey"]["elevation"], json!(1e-7));
    let text = String::from_utf8(save(&project)).unwrap();
    assert!(text.contains(r#""elevation":2.9999999999"#), "{text}");
    assert!(text.contains(r#""elevation":0.0000001"#), "{text}");
}

#[test]
fn exported_points_stay_within_a_tenth_of_a_millimetre_in_world_coordinates() {
    // IFX-031: the storey transform is rounded, the mesh points are rounded,
    // and their sum stays within 0.1 mm of the model's world position.
    let (project, wall) = odd_elevations();
    let geometry = ModelGeometry::evaluate(&project);
    let value = parse(&save(&project));
    let storey = project.storeys.get(upper(&project)).unwrap();
    let row = translation_row(node(&value, &storey.id.to_string()));
    let offset: Vec<f64> = (0..3).map(|i| row[i].as_f64().unwrap()).collect();
    let mut checked = 0;
    for part in &geometry.get(wall).unwrap().parts {
        let body = node(&value, &sub_node_id(wall.uuid(), &part.name).to_string());
        let written = body["attributes"]["usd::usdgeom::mesh"]["points"].as_array().unwrap();
        assert_eq!(written.len(), part.mesh.points.len());
        for (w, model) in written.iter().zip(&part.mesh.points) {
            let model = [model.x, model.y, model.z + storey.elevation];
            for axis in 0..3 {
                let world = w[axis].as_f64().unwrap() + offset[axis];
                assert!(
                    (world - model[axis]).abs() < 1e-4,
                    "{world} vs {} on axis {axis}",
                    model[axis]
                );
            }
            checked += 1;
        }
    }
    assert!(checked >= 8);
}

#[test]
fn saves_differ_only_in_timestamp_and_data_version() {
    // IFX-007: an unchanged model saved twice differs only in the header's
    // timestamp and data version.
    let project = samples::two_storey_shell();
    let geometry = ModelGeometry::evaluate(&project);
    let first = archicraft_ifcx::save(&project, &geometry, &options(), false).0;
    let later = WriteOptions {
        timestamp: "2027-01-02T03:04:05Z".into(),
        data_version: 8,
        ..options()
    };
    let second = archicraft_ifcx::save(&project, &geometry, &later, false).0;
    assert_ne!(first, second);
    let first = String::from_utf8(first)
        .unwrap()
        .replace(
            r#""timestamp":"2026-10-16T12:00:00Z""#,
            r#""timestamp":"2027-01-02T03:04:05Z""#,
        )
        .replace(r#""dataVersion":"7""#, r#""dataVersion":"8""#);
    assert_eq!(first, String::from_utf8(second).unwrap());
}

/// A project whose parameters hold values without a short decimal form.
fn awkward_floats() -> Project {
    let mut project = default_metric_project();
    let mut history = History::default();
    let ground = project.storeys.ground();
    execute(
        &mut project,
        &mut history,
        Command::SetStoreyElevation {
            storey: ground,
            elevation: 1e-7,
            shift_above: false,
        },
    );
    let id = add_wall(
        &mut project,
        &mut history,
        ground,
        DVec2::new(0.1 + 0.2, 1.0 / 3.0),
        DVec2::new(12345.678901234, 1.0 / 3.0),
    );
    let mut wall = project.elements[&id].clone();
    if let ElementKind::Wall(w) = &mut wall.kind {
        w.thickness = 1.0 / 3.0;
        w.base_offset = 1e-7;
    }
    execute(&mut project, &mut history, Command::UpdateElements(vec![wall]));
    project
}

fn save_with(project: &Project, pretty: bool) -> Vec<u8> {
    let geometry = ModelGeometry::evaluate(project);
    let (bytes, warnings) = archicraft_ifcx::save(project, &geometry, &options(), pretty);
    assert!(warnings.is_empty(), "{warnings:?}");
    bytes
}

#[test]
fn awkward_floats_save_load_save_byte_identically_without_exponents() {
    // IFX-007: shortest round-trip digits, never an exponent, compact or
    // indented.
    let project = awkward_floats();
    for pretty in [false, true] {
        let first = save_with(&project, pretty);
        assert!(!has_exponent(&first), "pretty = {pretty}");
        let loaded = archicraft_ifcx::load(&first).unwrap();
        assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
        let second = save_with(&loaded.project, pretty);
        assert!(first == second, "pretty = {pretty}: save-load-save changed the bytes");
    }
    let text = String::from_utf8(save_with(&project, false)).unwrap();
    for number in [
        format_f64(0.1 + 0.2),
        format_f64(1.0 / 3.0),
        "0.0000001".to_owned(),
        "12345.678901234".to_owned(),
    ] {
        assert!(text.contains(&number), "{number} is missing");
    }
}

/// Inserts a basement below the ground storey with a wall on it.
fn with_basement() -> Project {
    let mut project = samples::two_storey_shell();
    let mut history = History::default();
    let ground = project.storeys.ground();
    let basement = Storey::new("-01 Basement", 0.0, 2.6);
    let basement_id = basement.id;
    execute(
        &mut project,
        &mut history,
        Command::InsertStorey {
            reference: ground,
            above: false,
            storey: basement,
        },
    );
    add_wall(
        &mut project,
        &mut history,
        basement_id,
        DVec2::new(1.0, 1.0),
        DVec2::new(6.0, 1.0),
    );
    project
}

fn class(node: &Value) -> Option<&str> {
    node["attributes"]["bsi::ifc::class"]["code"].as_str()
}

fn children(node: &Value) -> Vec<String> {
    node["children"]
        .as_object()
        .map(|c| c.values().filter_map(|v| v.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

#[test]
fn node_order_follows_the_spatial_tree() {
    // IFX-007: project, site, building, then each storey in elevation order
    // followed by its elements and their sub-nodes, then the materials.
    let project = with_basement();
    let value = parse(&save(&project));
    let data = data(&value);
    let mut expected = vec![project.id.to_string()];
    let site = children(&data[0]).remove(0);
    let building = children(node(&value, &site)).remove(0);
    expected.extend([site, building.clone()]);
    let storeys = children(node(&value, &building));
    assert_eq!(storeys.len(), 3);
    let mut elevations = Vec::new();
    for storey in &storeys {
        expected.push(storey.clone());
        let storey = node(&value, storey);
        elevations.push(
            storey["attributes"]["archicraft::storey"]["elevation"]
                .as_f64()
                .unwrap(),
        );
        for element in children(storey) {
            expected.push(element.clone());
            expected.extend(children(node(&value, &element)));
        }
    }
    assert!(elevations.windows(2).all(|w| w[0] < w[1]), "{elevations:?}");
    assert_eq!(elevations[0], -2.6, "the basement comes first");
    let paths: Vec<String> = data.iter().map(|n| n["path"].as_str().unwrap().to_owned()).collect();
    assert_eq!(paths[..expected.len()], expected[..]);
    let materials = &data[expected.len()..];
    assert!(!materials.is_empty());
    for material in materials {
        assert_eq!(class(material), None);
        assert!(material["attributes"]["bsi::ifc::presentation::diffuseColor"].is_array());
    }
    assert_eq!(class(&data[0]), Some("IfcProject"));
    assert_eq!(class(&data[3]), Some("IfcBuildingStorey"));
}

/// Every object's keys in document order, with their JSON pointer.
fn key_paths(value: &Value, at: &str, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, item) in map {
                let path = format!("{at}/{key}");
                out.push(path.clone());
                key_paths(item, &path, out);
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                key_paths(item, &format!("{at}/{i}"), out);
            }
        }
        _ => {}
    }
}

fn keys(bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    key_paths(&parse(bytes), "", &mut out);
    out
}

#[test]
fn attribute_key_order_is_stable_across_saves_and_reloads() {
    // IFX-007: attribute order is stable.
    let project = with_basement();
    let first = save(&project);
    let reloaded = archicraft_ifcx::load(&first).unwrap().project;
    let keys_first = keys(&first);
    assert!(keys_first.len() > 500);
    assert_eq!(keys_first, keys(&save(&project)));
    assert_eq!(keys_first, keys(&save(&reloaded)));
}

#[test]
fn wall_axis_points_are_multiples_of_a_micrometre() {
    // IFX-007: standard-namespace plan coordinates are rounded to 10⁻⁶ m;
    // the archicraft:: wall keeps the drawn coordinates (NFR-ACC-002).
    let (project, wall) = odd_elevations();
    let bytes = save(&project);
    let value = parse(&bytes);
    let axis = node(&value, &sub_node_id(wall.uuid(), "Axis").to_string());
    let points = axis["attributes"]["usd::usdgeom::basiscurves"]["points"]
        .as_array()
        .unwrap();
    assert_eq!(points.len(), 2);
    let text = String::from_utf8(bytes).unwrap();
    for point in points {
        for c in point.as_array().unwrap() {
            let c = c.as_f64().unwrap();
            assert_eq!(round_coordinate(c), c);
            let written = format_f64(c);
            assert!(decimals(&written) <= 6, "{written}");
            assert!(text.contains(&written));
        }
    }
    assert_eq!(points[0], json!([0.333333, 0.285714, 0]));
    let start = &node(&value, &wall.to_string())["attributes"]["archicraft::wall"]["start"];
    assert_eq!(start, &json!([1.0 / 3.0, 2.0 / 7.0]));
}

fn golden_files() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "ifcx"))
        .collect();
    files.sort();
    assert!(files.len() >= 2);
    files
}

/// Saves a loaded golden file again with its own header values.
fn resave(golden: &[u8], project: &Project, pretty: bool) -> Vec<u8> {
    let header = archicraft_ifcx::validate::parse(golden).unwrap().header;
    let options = WriteOptions {
        author: header.author,
        timestamp: header.timestamp,
        data_version: project.data_version,
    };
    archicraft_ifcx::save(project, &ModelGeometry::evaluate(project), &options, pretty).0
}

#[test]
fn indented_output_round_trips_to_the_golden_bytes() {
    // IFX-008 with IFX-007: the indented option loads to the same project
    // and saving that compact reproduces the golden file.
    for path in golden_files() {
        let golden = std::fs::read(&path).unwrap();
        let compact = archicraft_ifcx::load(&golden).unwrap().project;
        let indented = resave(&golden, &compact, true);
        assert!(!has_exponent(&indented), "{}", path.display());
        assert!(indented.iter().filter(|&&b| b == b'\n').count() > 100);
        let loaded = archicraft_ifcx::load(&indented).unwrap();
        assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
        assert_eq!(loaded.project, compact, "{}", path.display());
        assert!(
            resave(&golden, &loaded.project, false) == golden,
            "{}: indented, loaded and saved compact differs from the golden file",
            path.display()
        );
    }
}

#[test]
fn saving_node_by_node_writes_the_bytes_of_the_whole_document() {
    // IFX-007, IFX-008, NFR-SEC-003: `save` formats the document one node at
    // a time; its bytes are those of the whole document formatted at once,
    // compact and indented, and so are those of `to_bytes`.
    for project in common::samples() {
        let geometry = ModelGeometry::evaluate(&project);
        let document = write(&project, &geometry, &options()).file;
        let whole = serde_json::to_value(&document).unwrap();
        for pretty in [false, true] {
            let (saved, _) = archicraft_ifcx::save(&project, &geometry, &options(), pretty);
            let name = &project.info.name;
            assert!(
                saved == json::to_string(&whole, pretty).as_bytes(),
                "{name} (indented: {pretty}): the saved bytes differ from the whole document's"
            );
            assert!(
                to_bytes(&document, pretty) == saved,
                "{name} (indented: {pretty}): to_bytes differs from save"
            );
        }
    }
}

#[test]
fn each_element_node_comes_right_before_its_sub_nodes_in_child_order() {
    // NFR-SEC-003: `save` emits an element node first and then each body and
    // the axis node as it builds them, so an element's sub-nodes follow it
    // directly, in the order of its children.
    for project in common::samples() {
        let geometry = ModelGeometry::evaluate(&project);
        let data = write(&project, &geometry, &options()).file.data;
        let elements: std::collections::HashSet<String> = project.elements.keys().map(ToString::to_string).collect();
        let mut checked = 0;
        for (index, node) in data.iter().enumerate() {
            if !elements.contains(&node.path) {
                continue;
            }
            let children: Vec<&str> = node.children.values().flatten().map(String::as_str).collect();
            let following: Vec<&str> = data[index + 1..]
                .iter()
                .take(children.len())
                .map(|n| n.path.as_str())
                .collect();
            assert_eq!(following, children, "{}: {}", project.info.name, node.path);
            checked += 1;
        }
        assert_eq!(checked, project.elements.len(), "{}", project.info.name);
    }
}

fn wall_on(project: &mut Project, start: DVec2, end: DVec2, thickness: f64) -> ElementId {
    let mut history = History::default();
    let ground = project.storeys.ground();
    let id = add_wall(project, &mut history, ground, start, end);
    let mut wall = project.elements[&id].clone();
    if let ElementKind::Wall(w) = &mut wall.kind {
        w.thickness = thickness;
    }
    execute(project, &mut history, Command::UpdateElements(vec![wall]));
    id
}

fn wall_of(project: &Project, id: ElementId) -> &archicraft_model::Wall {
    project.elements[&id].as_wall().unwrap()
}

#[test]
fn imperial_and_far_parameters_round_trip_bit_exactly() {
    // NFR-ACC-002: a thickness typed with 1/64 inch keeps every digit;
    // NFR-ACC-001: a point 9,999.99 m from the origin round-trips exactly,
    // and doubles resolve far below 0.01 mm at 10 km.
    let thickness = parse_length("11 1/64\"", LengthUnit::Meter, &NumberFormat::default()).unwrap();
    assert!(decimals(&format_f64(thickness)) > 6, "{thickness}");
    let mut project = default_metric_project();
    let imperial = wall_on(&mut project, DVec2::ZERO, DVec2::new(5.0, 0.0), thickness);
    let far_start = DVec2::new(-9999.99, 0.0);
    let far = wall_on(
        &mut project,
        far_start,
        far_start + DVec2::new(4.0 / 3.0, 2.0 / 7.0),
        0.3,
    );

    let bytes = save(&project);
    let value = parse(&bytes);
    let written = &node(&value, &imperial.to_string())["attributes"]["archicraft::wall"]["thickness"];
    assert_eq!(written.as_f64().unwrap().to_bits(), thickness.to_bits());
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.contains(&format!(r#""thickness":{}"#, format_f64(thickness))));

    let loaded = archicraft_ifcx::load(&bytes).unwrap().project;
    assert_eq!(wall_of(&loaded, imperial).thickness.to_bits(), thickness.to_bits());
    let (a, b) = (wall_of(&project, far), wall_of(&loaded, far));
    for (x, y) in [(a.start, b.start), (a.end, b.end)] {
        assert_eq!((x.x.to_bits(), x.y.to_bits()), (y.x.to_bits(), y.y.to_bits()));
    }
    assert_eq!(b.start.length(), 9999.99);
    let spacing = 10_000.0f64.next_up() - 10_000.0;
    assert!(spacing < 1e-5, "representation error at 10 km: {spacing}");
}

/// 64-bit FNV-1a of a byte string.
fn fnv1a(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// A long list replaced by its length and the FNV-1a hash of its written
/// (rounded) values.
fn summary(items: &Value) -> Value {
    let text = archicraft_ifcx::json::to_string(items, false);
    json!({"count": items.as_array().map_or(0, Vec::len), "fnv": fnv1a(text.as_bytes())})
}

/// The review view of a file: the document, indented, with mesh and curve
/// point lists replaced by their [`summary`] so that a pull request diff
/// shows which bodies changed and every parameter change line by line.
fn review_view(bytes: &[u8]) -> String {
    let mut value = parse(bytes);
    for node in value["data"].as_array_mut().unwrap() {
        let Some(attributes) = node["attributes"].as_object_mut() else {
            continue;
        };
        for (key, lists) in [
            ("usd::usdgeom::mesh", &["points", "faceVertexIndices"][..]),
            ("usd::usdgeom::basiscurves", &["points"][..]),
        ] {
            if let Some(Value::Object(attribute)) = attributes.get_mut(key) {
                summarise(attribute, lists);
            }
        }
    }
    archicraft_ifcx::json::to_string(&value, true)
}

fn summarise(attribute: &mut Map<String, Value>, lists: &[&str]) {
    for list in lists {
        if let Some(items) = attribute.get_mut(*list) {
            *items = summary(items);
        }
    }
}

/// The golden file of a sample, saved again by this build.
fn golden_resaved(name: &str) -> (Vec<u8>, Project) {
    let golden = std::fs::read(golden_files().into_iter().find(|p| p.ends_with(name)).unwrap()).unwrap();
    let project = archicraft_ifcx::load(&golden).unwrap().project;
    (resave(&golden, &project, false), project)
}

#[test]
fn review_view_snapshots() {
    // IFX-007: the samples' output reviewed as JSON diffs (docs/testing.md).
    for name in ["walls-and-slab", "two-storey-shell"] {
        let (bytes, _) = golden_resaved(&format!("{name}.ifcx"));
        insta::assert_snapshot!(name, review_view(&bytes));
    }
}

#[test]
fn a_thickness_change_is_a_readable_hunk_in_the_review_view() {
    let (bytes, mut project) = golden_resaved("walls-and-slab.ifcx");
    let before = review_view(&bytes);
    let id = *project
        .elements
        .iter()
        .find(|(_, e)| matches!(e.kind, ElementKind::Wall(_)))
        .unwrap()
        .0;
    let mut wall = project.elements[&id].clone();
    if let ElementKind::Wall(w) = &mut wall.kind {
        w.thickness = 0.3;
    }
    execute(
        &mut project,
        &mut History::default(),
        Command::UpdateElements(vec![wall]),
    );
    let golden = std::fs::read(
        golden_files()
            .into_iter()
            .find(|p| p.ends_with("walls-and-slab.ifcx"))
            .unwrap(),
    )
    .unwrap();
    let after = review_view(&resave(&golden, &project, false));
    let (before, after): (Vec<&str>, Vec<&str>) = (before.lines().collect(), after.lines().collect());
    assert_eq!(before.len(), after.len(), "the change keeps the view's structure");
    let changed: Vec<(&str, &str)> = before
        .iter()
        .zip(&after)
        .filter(|(a, b)| a != b)
        .map(|(a, b)| (*a, *b))
        .collect();
    assert!(
        changed.iter().any(|(_, b)| b.trim() == r#""thickness": 0.3,"#),
        "{changed:#?}"
    );
    assert!(changed.len() <= 12, "a small hunk, not a rewrite: {changed:#?}");
    assert!(before.len() > 200);
}
