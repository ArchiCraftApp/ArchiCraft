//! The defaults of SRS Appendix D, one table row per value (STR-001,
//! STR-008, UND-001, LAY-004, NFR-ACC-003, EDT-019, IFX-034).
//!
//! [`ROWS`] mirrors the D.1, D.2 and D.3 tables completely: every row of
//! those tables in docs/SRS.md has at least one entry whose reference starts
//! with `D.<n> <setting>:` (checked by `every_appendix_d_row_is_in_the_table`).
//! An entry is
//!
//! * `Checked`: a getter on [`default_metric_project`] or a constant, compared
//!   with the Appendix D value;
//! * `Elsewhere`: pinned by the defaults test of another crate, which repeats
//!   the reference in its failure message (the view range in the geometry
//!   crate, the preference defaults in the app crate);
//! * `Pending`: the value does not exist yet; the owner epic converts the
//!   entry into a check when it adds the value (docs/testing.md).
//!
//! The D.4 block maps every template material by name to its material group
//! and checks the group priority. The map is explicit because D.4 groups do
//! not follow `MaterialCategory` (glass and aluminium share a group, render is
//! filed under `Other`).

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::path::{Path, PathBuf};

use archicraft_model::command::DEFAULT_UNDO_LIMIT;
use archicraft_model::template::default_metric_project;
use archicraft_model::units::{AngleUnit, LengthUnit, SlopeFormat};
use archicraft_model::validate::{MIN_CORNER_ANGLE_DEG, MIN_EDGE_LENGTH};
use archicraft_model::{MODEL_TOLERANCE, MaterialCategory, MaterialId, Project, ReferenceLine, SlabReference, TopLink};

/// Floats are compared with this absolute tolerance: the defaults are exact
/// decimal literals, so anything larger is a changed value.
const FLOAT_TOLERANCE: f64 = 1e-12;

type Check = fn(&Project) -> Result<(), String>;

#[derive(Clone, Copy)]
enum Status {
    Checked(Check),
    /// Pinned by the defaults test in this file, relative to `crates/`.
    Elsewhere(&'static str),
    /// Not implemented yet; `owner` is the roadmap epic that adds it.
    Pending {
        owner: &'static str,
    },
}

#[derive(Clone, Copy)]
struct Row {
    /// `D.<n> <setting as in the SRS table>: <value>`, or a requirement ID
    /// for template contents that Appendix D does not list.
    reference: &'static str,
    status: Status,
}

macro_rules! row {
    ($reference:literal, elsewhere $file:expr) => {
        Row {
            reference: $reference,
            status: Status::Elsewhere($file),
        }
    };
    ($reference:literal, pending $owner:expr) => {
        Row {
            reference: $reference,
            status: Status::Pending { owner: $owner },
        }
    };
    ($reference:literal, $check:expr) => {
        Row {
            reference: $reference,
            status: Status::Checked($check),
        }
    };
}

const GEOMETRY: &str = "archicraft-geometry/tests/defaults.rs";
const APP: &str = "archicraft-app/tests/defaults.rs";

const PROFILES: &str = "profiles-columns-beams (archicraft-xvmz)";
const DOORS_WINDOWS: &str = "doors-windows-core (archicraft-5il4)";
const STAIRS: &str = "stair-solver-tool (archicraft-169a)";

#[rustfmt::skip]
const ROWS: &[Row] = &[
    // D.1 Project defaults
    row!("D.1 Length unit: metre", |p| equal(p.settings.units.length, LengthUnit::Meter)),
    row!("D.1 Length unit: 3 decimals", |p| equal(p.settings.units.length_decimals, 3)),
    row!("D.1 Length unit: cm and mm selectable", |_| contains(&LengthUnit::ALL, &[LengthUnit::Centimeter, LengthUnit::Millimeter])),
    row!("D.1 Angle unit; slope: decimal degrees", |p| equal(p.settings.units.angle, AngleUnit::DecimalDegrees)),
    row!("D.1 Angle unit; slope: 2 decimals", |p| equal(p.settings.units.angle_decimals, 2)),
    row!("D.1 Angle unit; slope: roof pitch in degrees", |p| equal(p.settings.units.slope, SlopeFormat::Degrees)),
    row!("D.1 Angle unit; slope: percent selectable", |_| contains(&SlopeFormat::ALL, &[SlopeFormat::Percent])),
    row!("D.1 Storeys: 00 Ground floor and 01 First floor (STR-001)", |p| equal(storey_names(p), vec!["00 Ground floor", "01 First floor"])),
    row!("D.1 Storeys: 00 Ground floor at ±0.000 m", |p| close(storey(p, 0)?.elevation, 0.0)),
    row!("D.1 Storeys: 00 Ground floor 3.00 m high", |p| close(storey(p, 0)?.height, 3.0)),
    row!("D.1 Storeys: 01 First floor at +3.000 m", |p| close(storey(p, 1)?.elevation, 3.0)),
    row!("D.1 Storeys: 01 First floor 3.00 m high", |p| close(storey(p, 1)?.height, 3.0)),
    row!("D.1 Storeys: 00 Ground floor is the ground storey, numbered 0 (STR-001)", |p| equal(storey_numbers(p), vec![Some(0), Some(1)])),
    row!("D.1 Plan cut plane; view range: cut plane 1.10 m above the storey elevation (STR-008)", |p| close(p.settings.cut_plane_height, 1.10)),
    row!("D.1 Plan cut plane; view range: no storey overrides the cut plane (STR-008)", |p| equal(storey_cut_planes(p), vec![None, None])),
    row!("D.1 Plan cut plane; view range: bottom at the storey elevation, top at the next storey", elsewhere GEOMETRY),
    row!("D.1 Drawing scale: 1:100", pending "plan-renderer (archicraft-dbc9)"),
    row!("D.1 Model tolerance; repair thresholds: model tolerance 0.01 mm (NFR-ACC-003)", |_| close(MODEL_TOLERANCE, 0.000_01)),
    row!("D.1 Model tolerance; repair thresholds: edges shorter than 0.1 mm (EDT-019)", |_| close(MIN_EDGE_LENGTH, 0.000_1)),
    row!("D.1 Model tolerance; repair thresholds: sliver angles below 0.1° (EDT-019)", |_| close(MIN_CORNER_ANGLE_DEG, 0.1)),
    row!("D.1 Curve tessellation: 2 mm chord deviation (IFX-034)", |p| close(p.settings.tessellation.chord_deviation, 0.002)),
    row!("D.1 Curve tessellation: at most 10° per segment (IFX-034)", |p| close(p.settings.tessellation.max_angle_deg, 10.0)),
    row!("D.1 Curve tessellation: hence at least 36 segments per circle (IFX-034)", |p| equal(segments_per_circle(p), 36)),
    row!("D.1 Autosave; backups; undo: autosave every 5 minutes (PRJ-004, NFR-REL-002)", elsewhere APP),
    row!("D.1 Autosave; backups; undo: 1 backup version (NFR-REL-003)", elsewhere APP),
    row!("D.1 Autosave; backups; undo: at least 100 undo steps in the model history (UND-001)", |_| at_least(DEFAULT_UNDO_LIMIT, 100)),
    row!("D.1 Autosave; backups; undo: at least 100 undo steps in the preferences (UND-001)", elsewhere APP),
    row!("D.1 Snapping: radius 8 px", elsewhere APP),
    row!("D.1 Snapping: reference dwell 1.0 s (INP-011)", pending "point-input-snapping (archicraft-nimm)"),
    row!("D.1 Snapping: angle increment 45°", elsewhere APP),
    row!("D.1 Snapping: 15° selectable", elsewhere APP),
    // D.2 Element defaults
    row!("D.2 Wall: 0.25 m thick", |p| close(p.tool_defaults.wall.thickness, 0.25)),
    row!("D.2 Wall: masonry (Brick)", |p| material(p, p.tool_defaults.wall.material, "Brick", MaterialCategory::Masonry)),
    row!("D.2 Wall: reference line on the exterior face", |p| equal(p.tool_defaults.wall.reference_line, ReferenceLine::Outside)),
    row!("D.2 Wall: base offset 0", |p| close(p.tool_defaults.wall.base_offset, 0.0)),
    row!("D.2 Wall: top linked to home storey + 1, offset 0", |p| equal(p.tool_defaults.wall.top, TopLink::Relative { storeys: 1, offset: 0.0 })),
    row!("D.2 Slab: 0.20 m thick", |p| close(p.tool_defaults.slab.thickness, 0.20)),
    row!("D.2 Slab: reinforced concrete", |p| material(p, p.tool_defaults.slab.material, "Reinforced concrete", MaterialCategory::Concrete)),
    row!("D.2 Slab: top at the storey elevation (reference)", |p| equal(p.tool_defaults.slab.reference, SlabReference::Top)),
    row!("D.2 Slab: top at the storey elevation (offset 0)", |p| close(p.tool_defaults.slab.offset, 0.0)),
    row!("D.2 Column: 0.30 × 0.30 m reinforced concrete; centre anchor; top linked to home storey + 1", pending PROFILES),
    row!("D.2 Beam: 0.25 × 0.50 m reinforced concrete; top-centre anchor; top at the home storey elevation + 2.80 m", pending PROFILES),
    row!("D.2 Door: 0.90 × 2.10 m wall opening; single-leaf hinged; sill at the storey elevation; centred in the wall", pending DOORS_WINDOWS),
    row!("D.2 Window: 1.20 × 1.40 m wall opening; tilt-and-turn, single sash; sill 0.90 m above the storey elevation; centred in the core", pending DOORS_WINDOWS),
    row!("D.2 Roof: pitch 35°; 0.30 m thick; eaves overhang 0.50 m; pivot elevation at the top of the walls", pending "roofs (archicraft-v7d6)"),
    row!("D.2 Stair: width 1.00 m; rule set D.3; monolithic structure, smooth underside; tread and riser finish 0.025 m", pending STAIRS),
    row!("D.2 Railing: height 1.00 m; inner posts at most 1.20 m apart; top rail 50 × 50 mm; bars with ≤ 0.12 m clear spacing", pending "railings-stairs-adv (archicraft-i39p)"),
    row!("D.2 Curtain wall: grid 1.50 × 3.00 m; frames 0.05 × 0.15 m aluminium; glass panels 0.03 m", pending "curtain-walls (archicraft-1lz)"),
    row!("D.2 Terrain: bottom 5.00 m below the lowest terrain point", pending "terrain-and-site (archicraft-9x87)"),
    row!("D.2 Grid: spacing 6.00 m; axes labelled A, B, C… and 1, 2, 3…", pending "grids (archicraft-7qjj)"),
    // D.3 Stair rule set
    row!("D.3 Riser height R: enabled, 150 mm to 200 mm", pending STAIRS),
    row!("D.3 Going G: enabled, 250 mm to 350 mm", pending STAIRS),
    row!("D.3 Step formula 2R + G: enabled, 600 mm to 650 mm", pending STAIRS),
    row!("D.3 Ratio R / G: disabled, 0.10 to 1.00", pending STAIRS),
    row!("D.3 Sum R + G: disabled, 450 mm to 600 mm", pending STAIRS),
    row!("D.3 Pitch atan(R/G): disabled, 20° to 30°", pending STAIRS),
    row!("D.3 Walking line distance from inner edge: enabled, at least 300 mm", pending STAIRS),
    row!("D.3 Automatic landing length: enabled, at least 1.00 m", pending STAIRS),
    // Template contents outside Appendix D
    row!("LAY-004 Template layers: structure, exterior walls, interior walls, slabs, roof, openings, stairs and railings, terrain, furniture, 2D construction, references", |p| equal(layer_names(p), vec![
        "Structure", "Exterior walls", "Interior walls", "Slabs", "Roof", "Openings", "Stairs and railings", "Terrain", "Furniture", "2D construction", "References",
    ])),
];

/// D.4 material groups with their intersection priority, named exactly as in
/// the SRS table.
const GROUPS: [(&str, u16); 16] = [
    (REINFORCED_CONCRETE, 900),
    (CONCRETE, 880),
    (STRUCTURAL_STEEL, 860),
    (MASONRY, 800),
    (CLT_GLULAM, 760),
    (STRUCTURAL_SOFTWOOD, 740),
    (SCREED, 600),
    (NATURAL_STONE, 560),
    (GLASS_ALUMINIUM, 500),
    (THERMAL_INSULATION, 400),
    (GYPSUM_BOARD, 350),
    (PLASTER_RENDER, 300),
    (TILES, 280),
    (MEMBRANES, 200),
    (AIR_GAP, 100),
    (SOIL_GRAVEL, 50),
];

const REINFORCED_CONCRETE: &str = "Reinforced concrete";
const CONCRETE: &str = "Concrete";
const STRUCTURAL_STEEL: &str = "Structural steel";
const MASONRY: &str = "Masonry (brick, sand-lime brick, aerated concrete, clay block)";
const CLT_GLULAM: &str = "Cross-laminated timber, glulam";
const STRUCTURAL_SOFTWOOD: &str = "Structural softwood";
const SCREED: &str = "Screed";
const NATURAL_STONE: &str = "Natural stone";
const GLASS_ALUMINIUM: &str = "Glass, aluminium";
const THERMAL_INSULATION: &str = "Thermal insulation (mineral wool, EPS/XPS, wood fibre)";
const GYPSUM_BOARD: &str = "Gypsum board";
const PLASTER_RENDER: &str = "Plaster, render";
const TILES: &str = "Tiles";
const MEMBRANES: &str = "Membranes";
const AIR_GAP: &str = "Air gap";
const SOIL_GRAVEL: &str = "Soil, gravel";

/// The D.4 group of every template material, by material name.
const MATERIAL_GROUPS: [(&str, &str); 25] = [
    ("Reinforced concrete", REINFORCED_CONCRETE),
    ("Concrete", CONCRETE),
    ("Brick", MASONRY),
    ("Sand-lime brick", MASONRY),
    ("Aerated concrete", MASONRY),
    ("Clay block", MASONRY),
    ("Softwood", STRUCTURAL_SOFTWOOD),
    ("Glulam", CLT_GLULAM),
    ("Cross-laminated timber", CLT_GLULAM),
    ("Steel", STRUCTURAL_STEEL),
    ("Aluminium", GLASS_ALUMINIUM),
    ("Glass", GLASS_ALUMINIUM),
    ("Mineral wool", THERMAL_INSULATION),
    ("EPS/XPS", THERMAL_INSULATION),
    ("Wood fibre insulation", THERMAL_INSULATION),
    ("Plaster", PLASTER_RENDER),
    ("Render", PLASTER_RENDER),
    ("Gypsum board", GYPSUM_BOARD),
    ("Screed", SCREED),
    ("Natural stone", NATURAL_STONE),
    ("Tiles", TILES),
    ("Soil", SOIL_GRAVEL),
    ("Gravel", SOIL_GRAVEL),
    ("Membrane", MEMBRANES),
    ("Air gap", AIR_GAP),
];

fn equal<T: PartialEq + Debug>(actual: T, expected: T) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!("expected {expected:?}, found {actual:?}"))
    }
}

fn close(actual: f64, expected: f64) -> Result<(), String> {
    if (actual - expected).abs() <= FLOAT_TOLERANCE {
        Ok(())
    } else {
        Err(format!("expected {expected}, found {actual}"))
    }
}

fn at_least<T: PartialOrd + Debug>(actual: T, minimum: T) -> Result<(), String> {
    if actual >= minimum {
        Ok(())
    } else {
        Err(format!("expected at least {minimum:?}, found {actual:?}"))
    }
}

fn contains<T: PartialEq + Debug>(all: &[T], wanted: &[T]) -> Result<(), String> {
    match wanted.iter().find(|w| !all.contains(w)) {
        None => Ok(()),
        Some(missing) => Err(format!("{missing:?} is not selectable, only {all:?}")),
    }
}

fn storey(p: &Project, index: usize) -> Result<&archicraft_model::Storey, String> {
    p.storeys
        .as_slice()
        .get(index)
        .ok_or_else(|| format!("the template has no storey {index}"))
}

fn storey_names(p: &Project) -> Vec<&str> {
    p.storeys.iter().map(|s| s.name.as_str()).collect()
}

fn storey_numbers(p: &Project) -> Vec<Option<i32>> {
    p.storeys.iter().map(|s| p.storeys.number(s.id)).collect()
}

fn storey_cut_planes(p: &Project) -> Vec<Option<f64>> {
    p.storeys.iter().map(|s| s.cut_plane_height).collect()
}

fn layer_names(p: &Project) -> Vec<&str> {
    p.layers.iter().map(|l| l.name.as_str()).collect()
}

/// The fewest segments a full circle needs at the maximum angle per segment.
fn segments_per_circle(p: &Project) -> u32 {
    (360.0 / p.settings.tessellation.max_angle_deg).ceil() as u32
}

fn material(p: &Project, id: MaterialId, name: &str, category: MaterialCategory) -> Result<(), String> {
    let material = p.material(id).ok_or("the default material is not in the template")?;
    equal((material.name.as_str(), material.category), (name, category))
}

/// `crates/`, which the `Elsewhere` paths are relative to.
fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The data rows of the table under `### <section> ` in docs/SRS.md, as cells.
fn srs_table(section: &str) -> Vec<Vec<String>> {
    let path = crates_dir().join("../docs/SRS.md");
    let srs = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let heading = format!("### {section} ");
    let rows: Vec<Vec<String>> = srs
        .lines()
        .skip_while(|line| !line.starts_with(&heading))
        .skip(1)
        .take_while(|line| !line.starts_with("#"))
        .filter(|line| line.starts_with('|') && !line.starts_with("|---"))
        .map(|line| line.trim_matches('|').split('|').map(|c| c.trim().to_owned()).collect())
        .skip(1) // the header
        .collect();
    assert!(!rows.is_empty(), "docs/SRS.md has no table under {heading}");
    rows
}

/// Appendix D (STR-001, STR-008, UND-001, LAY-004, NFR-ACC-003, EDT-019,
/// IFX-034): every checked row holds for a new project from the default
/// metric template. A failure names the reference of each row that changed.
#[test]
fn appendix_d_defaults_hold_in_the_default_metric_template() {
    let p = default_metric_project();
    let mut failures = Vec::new();
    let mut pending = Vec::new();
    for row in ROWS {
        match row.status {
            Status::Checked(check) => {
                if let Err(message) = check(&p) {
                    failures.push(format!("{}: {message}", row.reference));
                }
            }
            Status::Elsewhere(file) => {
                let path = crates_dir().join(file);
                let pinned = std::fs::read_to_string(&path).is_ok_and(|text| text.contains(row.reference));
                if !pinned {
                    failures.push(format!("{}: not pinned by crates/{file}", row.reference));
                }
            }
            Status::Pending { owner } => pending.push(format!("{} (owner: {owner})", row.reference)),
        }
    }
    println!("Appendix D values not implemented yet:\n  {}", pending.join("\n  "));
    assert!(
        failures.is_empty(),
        "Appendix D defaults changed:\n  {}",
        failures.join("\n  ")
    );
}

/// Appendix D.1 to D.3: the table has an entry for every row of the SRS
/// tables, so a value added to Appendix D cannot go unpinned.
#[test]
fn every_appendix_d_row_is_in_the_table() {
    let mut missing = Vec::new();
    for section in ["D.1", "D.2", "D.3"] {
        for cells in srs_table(section) {
            let prefix = format!("{section} {}:", cells[0]);
            if !ROWS.iter().any(|row| row.reference.starts_with(&prefix)) {
                missing.push(prefix);
            }
        }
    }
    assert!(
        missing.is_empty(),
        "Appendix D rows without a table entry:\n  {}",
        missing.join("\n  ")
    );
}

/// Appendix D.4 (WAL-023): every template material has the intersection
/// priority of its material group; a template material without a group fails.
#[test]
fn d4_every_template_material_has_the_priority_of_its_group() {
    let groups: BTreeMap<&str, u16> = GROUPS.into_iter().collect();
    let material_groups: BTreeMap<&str, &str> = MATERIAL_GROUPS.into_iter().collect();
    let p = default_metric_project();
    let mut failures = Vec::new();
    for material in &p.materials {
        let Some(group) = material_groups.get(material.name.as_str()) else {
            failures.push(format!(
                "D.4: the template material {} has no material group",
                material.name
            ));
            continue;
        };
        let expected = groups[group];
        if material.priority != expected {
            failures.push(format!(
                "D.4 {group}: {} has priority {}, expected {expected}",
                material.name, material.priority
            ));
        }
    }
    for (name, group) in MATERIAL_GROUPS {
        if !p.materials.iter().any(|m| m.name == name) {
            failures.push(format!("D.4 {group}: the template has no material {name}"));
        }
    }
    assert!(
        failures.is_empty(),
        "Appendix D.4 priorities changed:\n  {}",
        failures.join("\n  ")
    );
}

/// Appendix D.4: the group table of this test is the table of docs/SRS.md.
#[test]
fn d4_groups_match_the_srs_table() {
    let srs: Vec<(String, String)> = srs_table("D.4")
        .into_iter()
        .map(|cells| (cells[0].clone(), cells[1].clone()))
        .collect();
    let ours: Vec<(String, String)> = GROUPS
        .iter()
        .map(|(group, priority)| (group.to_string(), priority.to_string()))
        .collect();
    assert_eq!(ours, srs, "GROUPS differs from Appendix D.4");
}
