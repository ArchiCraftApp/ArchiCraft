//! Writes the hand-crafted seeds of the IFCX fuzz corpora into a directory
//! (NFR-REL-007, NFR-SEC-003; docs/testing.md, Fuzzing, Corpus).
//!
//! Random mutation rarely reaches the inputs that found the first campaign's
//! time-outs and memory problems (archicraft-lnow), nor the reader branches
//! that need a field left out or a large structure, so `scripts/fuzz-corpus.sh`
//! seeds them through this example. Every seed is the golden file
//! `walls-and-slab.ifcx`, changed:
//!
//! - `stress-*`: large structures — a slab outline within the point limit and
//!   far above it, a storey or a slab listed thousands of times, thousands of
//!   storeys, many walls meeting at one point or crossing;
//! - `extreme-<node>-<attribute>-<value>`: every number of one ArchiCraft
//!   attribute of one node replaced by one extreme value;
//! - `coverage-*`: damaged or unusual records that reach the reader branches
//!   the other seeds missed in the coverage check of archicraft-4ehi.
//!
//! Each seed is smaller than the fuzz runs' `-max_len` and opens within their
//! time and memory limits. Every run writes the same bytes.
//!
//! Usage: `cargo run -p archicraft-ifcx --example fuzz_seeds -- <dir>`

use std::path::PathBuf;

use serde_json::{Value, json};
use uuid::Uuid;

/// The golden file every seed is built from.
const WALLS_AND_SLAB: &[u8] = include_bytes!("../tests/golden/walls-and-slab.ifcx");

/// The numbers each `extreme-*` seed puts into an attribute, with the name
/// they give the seed: zero and its negative, a negative, a length below the
/// model tolerance, a huge length, the largest `u32` (the counters' limit),
/// an integer beyond `f64`'s exact range, the smallest `i64`, the largest and
/// smallest finite `f64`, and the smallest positive one.
const EXTREMES: &[(&str, &str)] = &[
    ("zero", "0"),
    ("negative-zero", "-0.0"),
    ("minus-one", "-1"),
    ("tiny", "1e-12"),
    ("huge", "1e15"),
    ("u32-max", "4294967295"),
    ("beyond-f64-integers", "9007199254740993"),
    ("i64-min", "-9223372036854775808"),
    ("f64-max", "1.7976931348623157e308"),
    ("f64-lowest", "-1.7976931348623157e308"),
    ("subnormal", "5e-324"),
];

/// A number no golden file holds, replaced by the extreme value as text.
const MARKER: f64 = 271_828_182_845.904_5;

/// Walls in the `walls-at-one-point` seed.
const WALLS_AT_ONE_POINT: usize = 500;
/// Walls in each direction of the `crossing-walls` grid.
const CROSSING: usize = 100;

/// The golden file as a JSON tree.
fn golden() -> Value {
    serde_json::from_slice(WALLS_AND_SLAB).expect("the golden file is JSON")
}

fn data(file: &mut Value) -> &mut Vec<Value> {
    file["data"].as_array_mut().expect("a data list")
}

/// The index of the node named `name`.
fn named(file: &Value, name: &str) -> usize {
    file["data"]
        .as_array()
        .expect("a data list")
        .iter()
        .position(|n| n["attributes"]["bsi::ifc::prop::Name"] == name)
        .unwrap_or_else(|| panic!("no node {name}"))
}

/// The node named `name`.
fn node<'f>(file: &'f mut Value, name: &str) -> &'f mut Value {
    let index = named(file, name);
    &mut data(file)[index]
}

/// A path for the `k`-th added node, distinct from the golden file's.
fn path(k: usize) -> String {
    Uuid::from_u128(0x4e41_0000_0000_0000 + k as u128).to_string()
}

/// A copy of the node named `template` with a new path and without its
/// geometry children, added to the data list and listed among the children
/// of the node named `parent` under `key`.
fn add_copy(file: &mut Value, template: &str, parent: &str, key: &str, k: usize) -> usize {
    let template = named(file, template);
    let mut copy = data(file)[template].clone();
    let path = path(k);
    copy["path"] = json!(path);
    copy.as_object_mut().expect("a node").remove("children");
    node(file, parent)["children"][key] = json!(path);
    data(file).push(copy);
    data(file).len() - 1
}

/// A comb-shaped outline of `points` points (a multiple of four, plus
/// three): valid, without self-intersections.
fn comb(points: usize) -> Value {
    let teeth = (points - 3) / 4;
    let mut outline: Vec<Value> = (0..teeth)
        .flat_map(|i| {
            let x = 2 * i;
            [json!([x, 0]), json!([x, 1]), json!([x + 1, 1]), json!([x + 1, 0])]
        })
        .collect();
    let end = 2 * teeth;
    outline.extend([json!([end, 0]), json!([end, -1]), json!([0, -1])]);
    Value::Array(outline)
}

/// Lists the node named `child` `times` times among the children of the
/// node named `parent`.
fn list_repeatedly(file: &mut Value, parent: &str, child: &str, times: usize) {
    let child_path = node(file, child)["path"].clone();
    let children = node(file, parent)["children"].as_object_mut().expect("children");
    for k in 0..times {
        children.insert(format!("Again_{k:05}"), child_path.clone());
    }
}

/// Adds `walls` walls on the ground storey, from `ends(k)`.
fn add_walls(file: &mut Value, walls: usize, ends: impl Fn(usize) -> ([f64; 2], [f64; 2])) {
    for k in 0..walls {
        let index = add_copy(file, "W-001", "00 Ground floor", &format!("Wall_{k:05}"), k);
        let wall = &mut data(file)[index];
        let (start, end) = ends(k);
        wall["attributes"]["bsi::ifc::prop::Name"] = json!(format!("W-{:04}", k + 100));
        wall["attributes"]["archicraft::wall"]["start"] = json!(start);
        wall["attributes"]["archicraft::wall"]["end"] = json!(end);
    }
}

/// The `stress-*` seeds: the shapes of the first campaign's hand-crafted
/// inputs, at sizes that stay within the fuzz runs' limits since its fixes.
fn stress() -> Vec<(String, Value)> {
    let mut seeds = Vec::new();
    let mut seed = |name: &str, edit: &dyn Fn(&mut Value)| {
        let mut file = golden();
        edit(&mut file);
        seeds.push((format!("stress-{name}"), file));
    };
    // MAX_SLAB_POINTS: within the limit (1,000 at the campaign, 10,000 since
    // archicraft-91jj), every check runs on the outline; far above it
    // (finding 1, a 16 s time-out), the slab is skipped before.
    seed("slab-outline-999-points", &|f| {
        node(f, "SL-001")["attributes"]["archicraft::slab"]["outline"] = comb(999);
    });
    seed("slab-outline-32003-points", &|f| {
        node(f, "SL-001")["attributes"]["archicraft::slab"]["outline"] = comb(32_003);
    });
    // Findings 2 and 3: each further listing is reported, not read again.
    seed("storey-listed-5000-times", &|f| {
        list_repeatedly(f, "Building", "00 Ground floor", 5_000);
    });
    seed("slab-999-points-listed-20000-times", &|f| {
        node(f, "SL-001")["attributes"]["archicraft::slab"]["outline"] = comb(999);
        list_repeatedly(f, "00 Ground floor", "SL-001", 20_000);
    });
    seed("2000-storeys", &|f| {
        for k in 0..2_000 {
            let index = add_copy(f, "01 First floor", "Building", &format!("Storey_{k:05}"), k);
            let storey = &mut data(f)[index];
            storey["attributes"]["bsi::ifc::prop::Name"] = json!(format!("{:04} Storey", k + 2));
            storey["attributes"]["archicraft::storey"]["elevation"] = json!(3 * (k + 2));
        }
    });
    seed(&format!("{WALLS_AT_ONE_POINT}-walls-at-one-point"), &|f| {
        add_walls(f, WALLS_AT_ONE_POINT, |k| {
            let angle = std::f64::consts::TAU * k as f64 / WALLS_AT_ONE_POINT as f64;
            ([5.0, 4.0], [5.0 + 4.0 * angle.cos(), 4.0 + 4.0 * angle.sin()])
        });
    });
    seed(&format!("{CROSSING}-by-{CROSSING}-crossing-walls"), &|f| {
        add_walls(f, 2 * CROSSING, |k| {
            let at = (k % CROSSING) as f64 * 10.0 / CROSSING as f64;
            if k < CROSSING {
                ([at, 0.0], [at, 10.0])
            } else {
                ([0.0, at], [10.0, at])
            }
        });
    });
    seeds
}

/// The `archicraft::project` attribute.
fn project(file: &mut Value) -> &mut Value {
    &mut node(file, "Walls and slab")["attributes"]["archicraft::project"]
}

/// The attribute `key` of the node named `name`.
fn attribute<'f>(file: &'f mut Value, name: &str, key: &str) -> &'f mut Value {
    &mut node(file, name)["attributes"][key]
}

/// Removes the member `key` of `object`.
fn remove(object: &mut Value, key: &str) {
    object.as_object_mut().expect("an object").remove(key);
}

/// The path of the node named `name`.
fn path_of(file: &mut Value, name: &str) -> Value {
    node(file, name)["path"].clone()
}

/// The `coverage-*` seeds: the reader branches (src/alpha/reader.rs) that
/// the other seeds missed in the coverage check of archicraft-4ehi
/// (docs/testing.md, Fuzzing, Coverage check). The reader is lenient, so one seed
/// damages several records that are read independently.
fn coverage() -> Vec<(String, Value)> {
    let mut seeds = Vec::new();
    let mut seed = |name: &str, edit: &dyn Fn(&mut Value)| {
        let mut file = golden();
        edit(&mut file);
        seeds.push((format!("coverage-{name}"), file));
    };
    // The optional fields the writer writes only when set, all set.
    seed("optional-fields-present", &|f| {
        let project = project(f);
        project["materials"][0]["cutFill"]["background"] = json!([0.1, 0.2, 0.3]);
        project["layers"][0]["snapExcluded"] = json!(true);
        attribute(f, "00 Ground floor", "archicraft::storey")["cutPlaneHeight"] = json!(1.2);
        attribute(f, "W-001", "archicraft::element")["snapExcluded"] = json!(true);
    });
    // The optional plan grid and ground storey, both left out.
    seed("optional-fields-absent", &|f| {
        let project = project(f);
        remove(&mut project["settings"], "planGrid");
        remove(project, "groundStorey");
    });
    // Every top link and reference line, a wrong one of each, a point of
    // three coordinates.
    seed("wall-variants", &|f| {
        let first_floor = path_of(f, "01 First floor");
        let wall = attribute(f, "W-001", "archicraft::wall");
        wall["top"] = json!({"link": "fixed", "value": 2.5});
        wall["referenceLine"] = json!("inside");
        let wall = attribute(f, "W-002", "archicraft::wall");
        wall["top"] = json!({"link": "storey", "value": 0.5, "storey": first_floor});
        wall["referenceLine"] = json!("custom");
        wall["referenceOffset"] = json!(0.1);
        let wall = attribute(f, "W-003", "archicraft::wall");
        wall["top"] = json!({"link": "sideways", "value": 0});
        wall["referenceLine"] = json!("outer");
        attribute(f, "W-004", "archicraft::wall")["start"] = json!([0, 8, 0]);
    });
    // Damaged project-level records: materials, layers, the grid, the
    // location, the tool defaults.
    seed("damaged-project-records", &|f| {
        let project = project(f);
        let materials = project["materials"].as_array_mut().expect("materials");
        materials[0]["name"] = json!(" ");
        materials.extend([json!(5), json!({"id": "not a UUID"})]);
        project["layers"].as_array_mut().expect("layers").push(json!({"id": 5}));
        project["settings"]["planGrid"]["auxiliarySpacing"] = json!([2, 2]);
        project["location"] = json!(5);
        let defaults = &mut project["toolDefaults"];
        defaults["door"] = json!({});
        defaults["slab"] = json!(5);
        defaults["wall"]["material"] = json!(path(1));
        defaults["wall"]["ifcClass"] = json!("IfcSlab");
    });
    // Damaged elements, each read on its own: no ArchiCraft data object, an
    // unknown kind, no name, a class and a material that do not fit, holes
    // that are no list, missing or unreadable parameters; a child without
    // ArchiCraft data, a child that does not exist, and an element whose path
    // differs from another's only in case, so that both have the same
    // identity.
    seed("damaged-elements", &|f| {
        let wall = attribute(f, "W-001", "archicraft::element").clone();
        let slab = attribute(f, "SL-001", "archicraft::element").clone();
        *attribute(f, "W-001", "archicraft::element") = json!(5);
        attribute(f, "W-002", "archicraft::element")["kind"] = json!("door");
        remove(&mut node(f, "W-003")["attributes"], "bsi::ifc::prop::Name");
        attribute(f, "W-004", "bsi::ifc::class")["code"] = json!("IfcSlab");
        attribute(f, "W-004", "archicraft::wall")["material"] = json!(path(1));
        attribute(f, "SL-001", "archicraft::slab")["holes"] = json!(5);
        let edits: [(&str, &Value, &str, &str, Value); 4] = [
            ("W-001", &wall, "archicraft::wall", "", Value::Null),
            ("W-001", &wall, "archicraft::wall", "start", json!("here")),
            ("SL-001", &slab, "archicraft::slab", "", Value::Null),
            ("SL-001", &slab, "archicraft::slab", "outline", json!(5)),
        ];
        for (k, (template, element, key, field, value)) in edits.into_iter().enumerate() {
            let index = add_copy(f, template, "00 Ground floor", &format!("Damaged_{k}"), k + 2);
            let attributes = &mut data(f)[index]["attributes"];
            attributes["bsi::ifc::prop::Name"] = json!(format!("Damaged {k}"));
            attributes["archicraft::element"] = element.clone();
            if field.is_empty() {
                remove(attributes, key);
            } else {
                attributes[key][field] = value;
            }
        }
        let brick = path_of(f, "Brick");
        node(f, "00 Ground floor")["children"]["Material"] = brick;
        node(f, "00 Ground floor")["children"]["Missing"] = json!(path(99));
        let index = add_copy(f, "SL-001", "00 Ground floor", "Same_identity", 10);
        let slab_path = path_of(f, "SL-001");
        let upper = slab_path.as_str().expect("a path").to_uppercase();
        data(f)[index]["path"] = json!(upper);
        node(f, "00 Ground floor")["children"]["Same_identity"] = json!(upper);
    });
    // A storey without a name and without ArchiCraft data.
    seed("damaged-storey", &|f| {
        let attributes = &mut node(f, "01 First floor")["attributes"];
        remove(attributes, "bsi::ifc::prop::Name");
        remove(attributes, "archicraft::storey");
    });
    // A storey list the model rejects (a cut plane out of range): one
    // storey is used, and the elements of the lost storeys are skipped; their
    // other children are not reported.
    seed("storeys-unreadable", &|f| {
        attribute(f, "00 Ground floor", "archicraft::storey")["cutPlaneHeight"] = json!(1e9);
        let brick = path_of(f, "Brick");
        node(f, "00 Ground floor")["children"]["Material"] = brick;
    });
    seed("no-storeys", &|f| {
        node(f, "Building")["children"] = json!({});
    });
    // A project without site, building, views, materials or layers, and a
    // header whose data version is no number.
    seed("bare-project", &|f| {
        remove(node(f, "Walls and slab"), "children");
        let project = project(f);
        for list in ["views", "materials", "layers"] {
            project[list] = json!([]);
        }
        f["header"]["dataVersion"] = json!("one");
    });
    // Format versions: newer than this build's, unreadable, and older
    // without a known migration.
    for (name, version) in [("newer", "2"), ("unreadable", "one"), ("older", "0")] {
        seed(&format!("format-version-{name}"), &|f| {
            project(f)["formatVersion"] = json!(version);
        });
    }
    seed("project-not-object", &|f| {
        *project(f) = json!(5);
    });
    seeds
}

/// The `extreme-*` seeds: for each ArchiCraft attribute of the golden file
/// that holds numbers and each value of [`EXTREMES`], one seed with every
/// number of the attribute replaced by the value. The reader reads each field on its own,
/// so one seed reaches the checks of all the attribute's numbers.
fn extreme() -> Vec<(String, String)> {
    let file = golden();
    let mut seeds = Vec::new();
    for (index, node) in file["data"].as_array().expect("a data list").iter().enumerate() {
        let name = node["attributes"]["bsi::ifc::prop::Name"].as_str().unwrap_or("node");
        let Some(attributes) = node["attributes"].as_object() else {
            continue;
        };
        for key in attributes.keys().filter(|k| k.starts_with("archicraft::")) {
            let mut edited = file.clone();
            if replace_numbers(&mut edited["data"][index]["attributes"][key], MARKER) == 0 {
                continue;
            }
            // The value goes in as text, so that integers beyond `f64` and
            // `-0.0` reach the reader as they are written.
            let text = serde_json::to_string(&edited).expect("JSON serialises");
            let short = key.trim_start_matches("archicraft::");
            for (value_name, value) in EXTREMES {
                seeds.push((
                    format!("extreme-{}-{short}-{value_name}", slug(name)),
                    text.replace(&MARKER.to_string(), value),
                ));
            }
        }
    }
    seeds
}

/// Replaces every number in `value` by `with`, and returns how many there
/// were.
fn replace_numbers(value: &mut Value, with: f64) -> usize {
    match value {
        Value::Number(n) => {
            *n = serde_json::Number::from_f64(with).expect("finite");
            1
        }
        Value::Array(items) => items.iter_mut().map(|v| replace_numbers(v, with)).sum(),
        Value::Object(map) => map.values_mut().map(|v| replace_numbers(v, with)).sum(),
        _ => 0,
    }
}

/// A node name as part of a file name: lower case, words joined by `-`.
fn slug(name: &str) -> String {
    name.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join("-")
}

/// Every seed, by file name (without the `.ifcx` extension), as the bytes
/// written.
fn seeds() -> Vec<(String, Vec<u8>)> {
    let tree = stress().into_iter().chain(coverage()).map(|(name, file)| {
        let bytes = serde_json::to_vec(&file).expect("JSON serialises");
        (name, bytes)
    });
    let text = extreme().into_iter().map(|(name, text)| (name, text.into_bytes()));
    tree.chain(text).collect()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(std::env::args().nth(1).ok_or("usage: fuzz_seeds <output directory>")?);
    std::fs::create_dir_all(&dir)?;
    for (name, bytes) in seeds() {
        let path = dir.join(format!("{name}.ifcx"));
        std::fs::write(&path, bytes)?;
        println!("{}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::time::{Duration, Instant};

    use archicraft_geometry::ModelGeometry;
    use archicraft_ifcx::{ReadOutcome, WriteOptions};
    use archicraft_model::{PlanGrid, ReferenceLine, TopLink};
    use serde_json::Value;

    use super::{CROSSING, EXTREMES, MARKER, WALLS_AT_ONE_POINT, golden, seeds};

    /// The fuzz runs' `-max_len` (`scripts/fuzz-run.sh`): libFuzzer
    /// truncates a longer seed.
    const MAX_LEN: usize = 4_194_304;

    /// The time-out of the fuzz runs (`scripts/fuzz-run.sh`).
    const TIME_LIMIT: Duration = Duration::from_secs(10);

    /// The open path of the `ifcx_load` fuzz target: load, then evaluate,
    /// save and load again. A rejected file carries a message.
    fn open(bytes: &[u8]) {
        match archicraft_ifcx::load(bytes) {
            Ok(outcome) => {
                let geometry = ModelGeometry::evaluate(&outcome.project);
                let options = WriteOptions {
                    author: "Fuzz".into(),
                    timestamp: "2026-10-20T00:00:00Z".into(),
                    data_version: 1,
                };
                let (saved, _warnings) = archicraft_ifcx::save(&outcome.project, &geometry, &options, false);
                let _ = archicraft_ifcx::load(&saved);
            }
            Err(problem) => assert!(!problem.message.key().is_empty(), "rejected without a message"),
        }
    }

    fn seed(name: &str) -> Vec<u8> {
        seeds()
            .into_iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("no seed {name}"))
            .1
    }

    fn load(name: &str) -> ReadOutcome {
        archicraft_ifcx::load(&seed(name)).unwrap_or_else(|p| panic!("{name}: {}", p.message))
    }

    /// The problems of a loaded seed that contain `text`.
    fn count(outcome: &ReadOutcome, text: &str) -> usize {
        outcome
            .problems
            .iter()
            .filter(|p| p.message.to_string().contains(text))
            .count()
    }

    /// NFR-REL-007: libFuzzer truncates a seed longer than `-max_len`, and
    /// `scripts/fuzz-corpus.sh` writes each seed under its name, so every
    /// seed is shorter and has a name of its own.
    #[test]
    fn every_seed_is_shorter_than_max_len_and_named_once() {
        let seeds = seeds();
        let mut names = HashSet::new();
        for (name, bytes) in &seeds {
            assert!(bytes.len() < MAX_LEN, "{name}: {} bytes", bytes.len());
            let marker = MARKER.to_string();
            assert!(
                !String::from_utf8_lossy(bytes).contains(&marker),
                "{name} holds {marker}"
            );
            assert!(names.insert(name), "{name} twice");
            let group = ["stress-", "extreme-", "coverage-"];
            assert!(group.iter().any(|g| name.starts_with(g)), "{name}");
        }
        assert!(seeds.len() > 100, "{} seeds", seeds.len());
    }

    /// NFR-REL-007: every `extreme-*` seed holds its value, so none is a copy
    /// of the golden file under a name it does not live up to.
    #[test]
    fn every_extreme_seed_holds_its_value() {
        let mut count = 0;
        for (name, bytes) in seeds().iter().filter(|(n, _)| n.starts_with("extreme-")) {
            let (_, value) = EXTREMES
                .iter()
                .find(|(value_name, _)| name.ends_with(&format!("-{value_name}")))
                .unwrap_or_else(|| panic!("{name} names no value"));
            let text = String::from_utf8_lossy(bytes);
            assert!(
                text.contains(&format!(":{value}")) || text.contains(&format!("[{value}")),
                "{name}"
            );
            let tree: Value = serde_json::from_slice(bytes).expect("JSON");
            assert_ne!(tree, golden(), "{name} is the golden file");
            count += 1;
        }
        assert_eq!(count, 8 * EXTREMES.len(), "the 8 ArchiCraft attributes with numbers");
    }

    /// NFR-REL-007, NFR-SEC-003: every seed goes through the open path of
    /// the `ifcx_load` fuzz target without panicking, within the fuzz runs'
    /// time-out, so the corpus starts without a finding.
    #[test]
    fn every_seed_opens_along_the_fuzz_target_path() {
        for (name, bytes) in seeds() {
            let start = Instant::now();
            let outcome = std::panic::catch_unwind(|| open(&bytes));
            let elapsed = start.elapsed();
            assert!(outcome.is_ok(), "{name} panicked");
            assert!(elapsed < TIME_LIMIT, "{name} took {elapsed:?}");
        }
    }

    /// NFR-SEC-003: the stress seeds open as the first campaign's fixes
    /// intend (docs/testing.md, Fuzzing): a slab within the point limit is
    /// loaded and one beyond it skipped, every further listing of a storey
    /// or an element is reported instead of read, and the many storeys and
    /// walls are all loaded.
    #[test]
    fn the_stress_seeds_open_as_the_campaign_fixes_intend() {
        let at_limit = load("stress-slab-outline-999-points");
        assert_eq!(at_limit.project.elements.len(), 5, "{:#?}", at_limit.problems);
        let beyond = load("stress-slab-outline-32003-points");
        assert_eq!(beyond.project.elements.len(), 4);
        assert_eq!(
            count(
                &beyond,
                r#"ifcx-skipped(record=ifcx-record-element(name="SL-001"), reason=model-error-slab-points"#
            ),
            1
        );

        let storey = load("stress-storey-listed-5000-times");
        assert_eq!(count(&storey, "ifcx-storey-listed-twice"), 5_000);
        assert_eq!(storey.project.storeys.len(), 2);
        let slab = load("stress-slab-999-points-listed-20000-times");
        assert_eq!(count(&slab, r#"ifcx-duplicate(name="SL-001")"#), 20_000);
        assert_eq!(slab.project.elements.len(), 5);

        assert_eq!(load("stress-2000-storeys").project.storeys.len(), 2_002);
        let star = load(&format!("stress-{WALLS_AT_ONE_POINT}-walls-at-one-point"));
        assert_eq!(
            star.project.elements.len(),
            WALLS_AT_ONE_POINT + 5,
            "{:#?}",
            star.problems
        );
        let grid = load(&format!("stress-{CROSSING}-by-{CROSSING}-crossing-walls"));
        assert_eq!(grid.project.elements.len(), 2 * CROSSING + 5, "{:#?}", grid.problems);
    }

    /// NFR-REL-007: each coverage seed reaches the reader branches it was
    /// added for (docs/testing.md, Fuzzing, Corpus), shown by the problem
    /// each branch reports.
    #[test]
    fn each_coverage_seed_reaches_its_reader_branches() {
        let expected: &[(&str, &[&str])] = &[
            ("optional-fields-absent", &["ifcx-ground-storey-undefined"]),
            (
                "wall-variants",
                &[
                    r#"ifcx-repaired(record=ifcx-record-element(name="W-003"), reason=ifcx-default-used(reason=ifcx-field-unknown-value(key="referenceLine", value="outer")))"#,
                    r#"ifcx-repaired(record=ifcx-record-element(name="W-003"), reason=ifcx-default-used(reason=ifcx-field-unknown-value(key="link", value="sideways")))"#,
                    r#"ifcx-skipped(record=ifcx-record-element(name="W-004"), reason=ifcx-point-coordinates)"#,
                ],
            ),
            (
                "damaged-project-records",
                &[
                    r#"ifcx-repaired(record=ifcx-record-project, reason=ifcx-default-used(reason=ifcx-field-not-object(key="location")))"#,
                    "ifcx-grid-spacings",
                    r#"ifcx-repaired(record=ifcx-record-material(index=0), reason=ifcx-material-corrected(name=" ", reason=model-error-material-name))"#,
                    "ifcx-skipped(record=ifcx-record-material(index=25), reason=ifcx-not-object)",
                    r#"ifcx-skipped(record=ifcx-record-material(index=26), reason=ifcx-field-not-uuid(key="id"))"#,
                    r#"ifcx-skipped(record=ifcx-record-layer(index=11), reason=ifcx-field-not-text(key="id"))"#,
                    r#"ifcx-repaired(record=ifcx-record-tool-defaults, reason=ifcx-tool-unknown(key="door"))"#,
                    "ifcx-repaired(record=ifcx-record-tool-defaults, reason=ifcx-tool-material-missing)",
                    r#"ifcx-repaired(record=ifcx-record-tool-defaults, reason=ifcx-tool-class-not-allowed(class="IfcSlab", kind=wall"#,
                    r#"ifcx-repaired(record=ifcx-record-tool-defaults, reason=ifcx-tool-template-used(reason=ifcx-field-not-object(key="slab")))"#,
                ],
            ),
            (
                "damaged-elements",
                &[
                    r#"ifcx-skipped(record=ifcx-record-element(name="W-001"), reason=ifcx-element-data-not-object)"#,
                    r#"ifcx-skipped(record=ifcx-record-element(name="W-002"), reason=ifcx-field-unknown-value(key="kind", value="door"))"#,
                    r#"ifcx-no-name(name="W-?")"#,
                    r#"ifcx-repaired(record=ifcx-record-element(name="W-004"), reason=ifcx-element-class-unsupported"#,
                    r#"ifcx-repaired(record=ifcx-record-element(name="W-004"), reason=ifcx-element-material-missing"#,
                    r#"ifcx-repaired(record=ifcx-record-element(name="SL-001"), reason=ifcx-holes-left-out"#,
                    r#"ifcx-skipped(record=ifcx-record-element(name="Damaged 0"), reason=ifcx-parameters-missing(kind=wall))"#,
                    r#"ifcx-skipped(record=ifcx-record-element(name="Damaged 1"), reason=ifcx-point-not-list)"#,
                    r#"ifcx-skipped(record=ifcx-record-element(name="Damaged 2"), reason=ifcx-parameters-missing(kind=slab))"#,
                    r#"ifcx-skipped(record=ifcx-record-element(name="Damaged 3"), reason=ifcx-points-not-list)"#,
                    r#"ifcx-foreign-node(name="Brick")"#,
                    r#"ifcx-unknown-node(target="00000000-0000-0000-4e41-000000000063")"#,
                    r#"ifcx-duplicate(name="SL-001")"#,
                ],
            ),
            (
                "damaged-storey",
                &[
                    r#"ifcx-no-name(name="Storey")"#,
                    "ifcx-storey-no-data(elevation=0m, height=3m)",
                ],
            ),
            (
                "storeys-unreadable",
                &[
                    "ifcx-storeys-unreadable",
                    r#"ifcx-skipped(record=ifcx-record-element(name="W-001"), reason=ifcx-storey-lost(name="00 Ground floor"))"#,
                ],
            ),
            ("no-storeys", &["ifcx-no-storeys"]),
            (
                "bare-project",
                &[
                    "ifcx-no-site",
                    "ifcx-no-building",
                    "ifcx-no-views",
                    "ifcx-no-materials",
                    "ifcx-no-layers",
                    "ifcx-data-version-invalid",
                ],
            ),
            ("format-version-newer", &["ifcx-newer-format(version=2)"]),
            ("format-version-unreadable", &["ifcx-format-version-invalid"]),
            ("format-version-older", &["ifcx-migration-unknown(version=0)"]),
        ];
        for (name, messages) in expected {
            let outcome = load(&format!("coverage-{name}"));
            for message in *messages {
                assert!(
                    count(&outcome, message) > 0,
                    "{name}: no “{message}” in {:#?}",
                    outcome.problems
                );
            }
        }

        // The top links and reference lines that report no problem.
        let variants = load("coverage-wall-variants");
        let wall = |name: &str| {
            let element = variants.project.elements.values().find(|e| e.name == name);
            element.and_then(|e| e.as_wall()).cloned().expect(name)
        };
        let first_floor = variants.project.storeys.iter().find(|s| s.name == "01 First floor");
        assert_eq!(wall("W-001").top, TopLink::Fixed { height: 2.5 });
        assert_eq!(wall("W-001").reference_line, ReferenceLine::Inside);
        let storey = first_floor.expect("the first floor").id;
        assert_eq!(wall("W-002").top, TopLink::Storey { storey, offset: 0.5 });
        assert_eq!(wall("W-002").reference_line, ReferenceLine::Custom(0.1));
        // Without a plan grid, the default grid.
        let absent = load("coverage-optional-fields-absent");
        assert_eq!(absent.project.settings.plan_grid, PlanGrid::default());

        // The optional fields, all present, load without a problem.
        let present = load("coverage-optional-fields-present");
        assert!(present.problems.is_empty(), "{:#?}", present.problems);
        let project = &present.project;
        assert!(project.materials[0].cut_fill.background.is_some());
        assert!(project.layers[0].snap_excluded);
        assert!(project.storeys.iter().any(|s| s.cut_plane_height == Some(1.2)));
        assert!(project.elements.values().any(|e| e.snap_excluded));

        // The children of a lost storey that are no elements are not reported.
        assert_eq!(count(&load("coverage-storeys-unreadable"), "Brick"), 0);

        let rejected = archicraft_ifcx::load(&seed("coverage-project-not-object")).unwrap_err();
        assert_eq!(rejected.message.to_string(), "ifcx-project-not-object");
    }
}
