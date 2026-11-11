//! Frozen format fixtures: files written by earlier ArchiCraft format
//! versions, which every later release must keep reading (IFX-090, IFX-091,
//! IFX-062).
//!
//! The files under tests/formats/ are never regenerated: unlike the golden
//! files under tests/golden/, `ARCHICRAFT_BLESS` does not touch them, and a
//! hash test fails when their bytes change. `format-1.ifcx` is a copy of the
//! two-storey-shell golden as written at FORMAT_VERSION 1 in `ifcx_alpha`,
//! `format-2.ifcx` one of the two-storey-shell-with-spaces golden as written
//! at FORMAT_VERSION 2, the format that added spaces (SPC-001),
//! `format-3.ifcx` one of the annotations golden as written at
//! FORMAT_VERSION 3, the format that added texts and text styles (ANN-007),
//! `format-4.ifcx` one of the walls-and-slab golden as written at
//! FORMAT_VERSION 4, the format that keeps each tool's parameters apart from
//! the common members of its defaults slot (ELM-001), `format-5.ifcx` the
//! profile sample of tests/common as written at FORMAT_VERSION 5, the format
//! that stores profiles (PRF-004), and `format-6.ifcx` one of the
//! two-storey-shell-with-columns golden as written at FORMAT_VERSION 6, the
//! format that stores columns (COL-001).

#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};

use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::alpha::schema::FORMAT_VERSION;
use archicraft_ifcx::{MigrationReport, Severity, WRITTEN_VERSION, WriteOptions, validate};
use archicraft_model::glam::DVec2;
use archicraft_model::kinds::{Anchor, ColumnCut, CrossSection};
use archicraft_model::template::{default_metric_project, template_text_styles};
use archicraft_model::{AnchorPos, BoundarySource, ElementKind, IfcClass, KindTag, Position, TopLink};

use common::options;

fn formats_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/formats")
}

fn format_1() -> Vec<u8> {
    std::fs::read(formats_dir().join("format-1.ifcx")).unwrap()
}

/// 64-bit FNV-1a, enough to notice any change of a fixture's bytes.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The frozen fixtures keep their bytes forever, also when the golden files
/// are blessed: a migration test is only worth something against the bytes
/// an earlier release really wrote.
#[test]
fn frozen_format_fixtures_never_change() {
    let frozen: [(&str, usize, u64); 6] = [
        ("format-1.ifcx", 36_786, 0x4b1c_2083_91de_01b8),
        ("format-2.ifcx", 43_980, 0xf858_2581_17c9_fc49),
        ("format-3.ifcx", 44_960, 0xbe04_c6db_2e23_1a7d),
        ("format-4.ifcx", 34_275, 0x2161_21be_a3d6_c392),
        ("format-5.ifcx", 33_049, 0xd060_e3f6_b35e_83b9),
        ("format-6.ifcx", 72_561, 0x236c_5a9f_cc7a_30dc),
    ];
    let mut found: Vec<String> = std::fs::read_dir(formats_dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    let mut expected: Vec<String> = frozen.iter().map(|(name, ..)| (*name).to_owned()).collect();
    expected.sort();
    assert_eq!(found, expected, "every frozen format fixture is listed here");
    for (name, len, hash) in frozen {
        let bytes = std::fs::read(formats_dir().join(name)).unwrap();
        assert!(
            bytes.len() == len && fnv1a(&bytes) == hash,
            "frozen format fixture changed: {name} ({} bytes, hash {:#x}); restore it from git, never regenerate it",
            bytes.len(),
            fnv1a(&bytes)
        );
    }
}

/// IFX-091, IFX-062, ANN-007: a file written at format 1 is migrated to
/// the current format and loads into the project it holds, with no problem
/// but one info per migration step, the steps that add spaces (SPC-001),
/// texts, profiles and columns among them, gains the template's text styles
/// and reports the IFCX version it was read as.
#[test]
fn format_1_migrates_and_loads_into_the_expected_project() {
    let loaded = archicraft_ifcx::load(&format_1()).unwrap();
    let steps: Vec<String> = loaded.problems.iter().map(|p| p.message.to_string()).collect();
    assert!(
        loaded.problems.iter().all(|p| p.severity == Severity::Info),
        "{:#?}",
        loaded.problems
    );
    assert!(steps.iter().all(|s| s.starts_with("ifcx-migrated(")), "{steps:#?}");
    for change in [
        "ifcx-migration-adds-spaces",
        "ifcx-migration-adds-texts",
        "ifcx-migration-nests-tool-parameters",
        "ifcx-migration-adds-profiles",
        "ifcx-migration-adds-columns",
    ] {
        assert!(
            steps.iter().any(|s| s.ends_with(&format!("change={change})"))),
            "{steps:#?}"
        );
    }
    assert_eq!(loaded.project.text_styles, template_text_styles());
    // The text tool's defaults are the template's, on the file's layer.
    let slot = loaded.project.tool_defaults.get(KindTag::Text).unwrap();
    let template = default_metric_project();
    assert_eq!(slot.kind, template.tool_defaults.get(KindTag::Text).unwrap().kind);
    assert_eq!(loaded.project.layer(slot.layer).unwrap().name, "2D construction");
    assert!(!loaded.newer_format);
    assert_eq!(loaded.source_version, "ifcx_alpha");
    assert_eq!(loaded.migrated.as_ref().map(|m| m.from), Some(1));

    let project = &loaded.project;
    assert_eq!(project.info.name, "Two-storey shell");
    let storeys: Vec<(&str, f64)> = project
        .storeys
        .as_slice()
        .iter()
        .map(|s| (s.name.as_str(), s.elevation))
        .collect();
    assert_eq!(storeys, [("00 Ground floor", 0.0), ("01 First floor", 3.0)]);
    for (storey, names) in project.storeys.as_slice().iter().zip([
        ["SL-001", "W-001", "W-002", "W-003", "W-004", "W-005"],
        ["SL-002", "W-006", "W-007", "W-008", "W-009", "W-010"],
    ]) {
        let mut found: Vec<&str> = project.elements_on(storey.id).map(|e| e.name.as_str()).collect();
        found.sort_unstable();
        assert_eq!(found, names, "elements on {}", storey.name);
    }
    assert!(project.integrity_problems().is_empty());
}

/// IFX-091, IFX-062, SPC-001: a file written at format 2 loads into the
/// project it holds with no problem but the reports of later migration
/// steps: its two spaces with their room names, numbers and boundary
/// sources, and the space tool's default name.
#[test]
fn format_2_loads_its_spaces() {
    let bytes = std::fs::read(formats_dir().join("format-2.ifcx")).unwrap();
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(
        loaded.problems.iter().all(|p| p.severity == Severity::Info),
        "{:#?}",
        loaded.problems
    );
    let project = &loaded.project;
    let mut spaces: Vec<(&str, &str, &str, BoundarySource)> = project
        .elements
        .values()
        .filter_map(|e| {
            let s = e.as_space()?;
            Some((e.name.as_str(), s.long_name.as_str(), s.number.as_str(), s.source))
        })
        .collect();
    spaces.sort_by_key(|s| s.0);
    assert_eq!(
        spaces,
        [
            (
                "SP-001",
                "Living room",
                "0.01",
                BoundarySource::Detected {
                    seed: DVec2::new(2.5, 4.0),
                    auto_update: true
                }
            ),
            ("SP-002", "Landing", "1.01", BoundarySource::Drawn),
        ]
    );
    let slot = project.tool_defaults.get(KindTag::Space).unwrap();
    assert!(matches!(&slot.kind, ElementKind::Space(s) if s.long_name == "Raum {n}"));
    assert_eq!(project.text_styles, template_text_styles());
    assert_eq!(loaded.migrated.as_ref().map(|m| m.from), Some(2));
    assert!(project.integrity_problems().is_empty());
}

/// A text's name, content, anchor position, angle, size override and style
/// name.
type TextSummary<'a> = (&'a str, &'a str, AnchorPos, f64, Option<f64>, &'a str);

/// IFX-091, IFX-062, ANN-007: a file written at format 3 loads into the
/// project it holds with no problem but the reports of later migration
/// steps: its three texts with their contents, anchor positions, angles,
/// size overrides and styles, and the text styles.
#[test]
fn format_3_loads_its_texts_and_styles() {
    let bytes = std::fs::read(formats_dir().join("format-3.ifcx")).unwrap();
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(
        loaded.problems.iter().all(|p| p.severity == Severity::Info),
        "{:#?}",
        loaded.problems
    );
    let project = &loaded.project;
    let texts: Vec<TextSummary<'_>> = project
        .elements
        .values()
        .filter_map(|e| {
            let t = e.as_text()?;
            let style = project.text_style(t.style)?;
            Some((
                e.name.as_str(),
                t.content.as_str(),
                t.anchor_pos,
                t.angle,
                t.size_mm,
                style.name.as_str(),
            ))
        })
        .collect();
    assert_eq!(
        texts,
        [
            (
                "TX-001",
                "Living room",
                AnchorPos::BottomLeft,
                0.0,
                None,
                "Standard 2.5 mm"
            ),
            (
                "TX-002",
                "Kitchen\n12.5 m²\nTiled floor",
                AnchorPos::Center,
                0.0,
                Some(3.5),
                "Standard 2.5 mm"
            ),
            (
                "TX-003",
                "Landing",
                AnchorPos::MiddleLeft,
                std::f64::consts::FRAC_PI_6,
                None,
                "Heading 5.0 mm"
            ),
        ]
    );
    assert_eq!(project.text_styles, template_text_styles());
    assert_eq!(project.counters.get(KindTag::Text), 3);
    assert!(project.integrity_problems().is_empty());
}

/// The golden file `name` as this release writes it.
fn golden(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/golden/{name}.ifcx"));
    std::fs::read(path).unwrap()
}

/// `project` saved with the header options of the golden file `golden`.
fn saved_like(project: &archicraft_model::Project, golden: &[u8]) -> Vec<u8> {
    let header = validate::parse(golden).unwrap().header;
    let options = WriteOptions {
        author: header.author,
        timestamp: header.timestamp,
        data_version: project.data_version,
    };
    let geometry = ModelGeometry::evaluate(project);
    let (bytes, warnings) = archicraft_ifcx::save(project, &geometry, &options, false);
    assert!(warnings.is_empty(), "{warnings:?}");
    bytes
}

/// The `archicraft::project` attribute and the `archicraft::` declarations
/// of a file.
fn project_and_declarations(bytes: &[u8]) -> (serde_json::Value, Vec<(String, serde_json::Value)>) {
    let file = validate::parse(bytes).unwrap();
    let project = file
        .data
        .iter()
        .find_map(|n| n.attributes.get("archicraft::project"))
        .unwrap()
        .clone();
    let declarations = file
        .schemas
        .into_iter()
        .filter(|(id, _)| id.starts_with("archicraft::"))
        .collect();
    (project, declarations)
}

/// The info problems of a migration from `from`, one per step of `report`.
fn step_infos(report: &MigrationReport) -> Vec<String> {
    report
        .steps
        .iter()
        .zip(report.from..)
        .map(|(change, from)| format!("ifcx-migrated(from={from}, to={}, change={change})", from + 1))
        .collect()
}

/// IFX-062, IFX-091: the frozen format 1 file migrates with a report from
/// format 1 to [`FORMAT_VERSION`] that names every step, the one that keeps
/// the tools' parameters apart (format 4) once among them, one info problem
/// per step and no warning or error. Saved, it has the current
/// `formatVersion`, the tools' parameters nested in their slots and the
/// declarations of the current golden file of its sample, the two-storey
/// shell. (The file holds an earlier edition of that sample, with other
/// identities and without predefined types or snap flags, so the equality
/// with the golden file is checked on the format 4 file below.)
#[test]
fn format_v1_files_migrate_with_a_report() {
    let loaded = archicraft_ifcx::load(&format_1()).unwrap();
    let report = loaded.migrated.clone().unwrap();
    assert_eq!((report.from, report.to), (1, FORMAT_VERSION));
    assert_eq!(report.steps.len(), usize::try_from(FORMAT_VERSION - 1).unwrap());
    assert_eq!(
        report
            .steps
            .iter()
            .filter(|s| **s == "ifcx-migration-nests-tool-parameters")
            .count(),
        1,
        "{report:?}"
    );
    let infos: Vec<String> = loaded.problems.iter().map(|p| p.message.to_string()).collect();
    assert_eq!(infos, step_infos(&report));
    assert!(loaded.problems.iter().all(|p| p.severity == Severity::Info));
    assert!(!loaded.newer_format);

    let golden_bytes = golden("two-storey-shell");
    let bytes = saved_like(&loaded.project, &golden_bytes);
    let (project, declarations) = project_and_declarations(&bytes);
    assert_eq!(project["formatVersion"], FORMAT_VERSION.to_string().as_str());
    let wall = &project["toolDefaults"]["wall"];
    assert!(wall.get("thickness").is_none(), "{wall}");
    assert_eq!(wall["parameters"]["thickness"], 0.25, "{wall}");
    assert_eq!(declarations, project_and_declarations(&golden_bytes).1);
    let reloaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(reloaded.problems.is_empty(), "{:#?}", reloaded.problems);
    assert_eq!(reloaded.migrated, None);
}

/// IFX-062: the frozen file of format 4, the walls-and-slab sample, loads
/// into the project the current golden file of that sample holds, with a
/// report of the steps to [`FORMAT_VERSION`] that add profiles and columns,
/// their info problems and nothing else; saved with the golden file's header
/// options, it is the golden file byte for byte.
#[test]
fn the_format_4_file_migrates_to_the_golden_file() {
    let bytes = std::fs::read(formats_dir().join("format-4.ifcx")).unwrap();
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let golden_bytes = golden("walls-and-slab");
    let current = archicraft_ifcx::load(&golden_bytes).unwrap();
    assert!(current.problems.is_empty(), "{:#?}", current.problems);
    assert_eq!(current.migrated, None);
    assert_eq!(loaded.project, current.project);
    assert_eq!(
        loaded.migrated,
        Some(MigrationReport {
            from: 4,
            to: FORMAT_VERSION,
            steps: vec!["ifcx-migration-adds-profiles", "ifcx-migration-adds-columns"],
        })
    );
    let infos: Vec<String> = loaded.problems.iter().map(|p| p.message.to_string()).collect();
    assert_eq!(infos, step_infos(loaded.migrated.as_ref().unwrap()));
    assert!(loaded.problems.iter().all(|p| p.severity == Severity::Info));
    assert!(
        saved_like(&loaded.project, &golden_bytes) == golden_bytes,
        "not the golden file"
    );
}

/// IFX-062, COL-001: the frozen file of the format before this release's,
/// the profile sample at format 5, loads into the profile sample, with the
/// column tool's template defaults and no column counted, with a report of
/// the one step to [`FORMAT_VERSION`] that adds columns, its one info
/// problem and nothing else; saved with the file's header options, it is
/// the file this release writes for the sample.
#[test]
fn the_previous_format_migrates_to_the_project_it_holds() {
    let bytes = std::fs::read(formats_dir().join("format-5.ifcx")).unwrap();
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let mut sample = common::profile_sample();
    sample.revision = 0;
    sample.data_version = 7;
    // The sample's column slot is the template's, as the reader gives it.
    assert_eq!(loaded.project, sample);
    assert_eq!(loaded.project.counters.get(KindTag::Column), 0);
    assert_eq!(
        loaded.migrated,
        Some(MigrationReport {
            from: 5,
            to: FORMAT_VERSION,
            steps: vec!["ifcx-migration-adds-columns"],
        })
    );
    let infos: Vec<String> = loaded.problems.iter().map(|p| p.message.to_string()).collect();
    assert_eq!(infos, step_infos(loaded.migrated.as_ref().unwrap()));
    assert!(loaded.problems.iter().all(|p| p.severity == Severity::Info));
    assert!(
        saved_like(&loaded.project, &bytes) == saved_like(&sample, &bytes),
        "not the file this release writes"
    );
}

/// IFX-091, IFX-062, ELM-001: a file written at format 4 loads into the
/// project it holds with no problem but the reports of later migration
/// steps: the tools' parameters from the slots' `parameters`, next to their
/// common members, the walls and the slab.
#[test]
fn format_4_loads_its_tool_parameters() {
    let bytes = std::fs::read(formats_dir().join("format-4.ifcx")).unwrap();
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(
        loaded.problems.iter().all(|p| p.severity == Severity::Info),
        "{:#?}",
        loaded.problems
    );
    let project = &loaded.project;
    let wall = project.tool_defaults.get(KindTag::Wall).unwrap();
    let ElementKind::Wall(parameters) = &wall.kind else {
        panic!("{wall:?}");
    };
    assert_eq!(parameters.thickness, 0.25);
    assert_eq!(project.material(parameters.material).unwrap().name, "Brick");
    assert_eq!(project.layer(wall.layer).unwrap().name, "Exterior walls");
    assert_eq!(wall.position, Position::Exterior);
    let slab = project.tool_defaults.get(KindTag::Slab).unwrap();
    let ElementKind::Slab(parameters) = &slab.kind else {
        panic!("{slab:?}");
    };
    assert_eq!(parameters.thickness, 0.2);
    assert_eq!(
        project.material(parameters.material).unwrap().name,
        "Reinforced concrete"
    );
    let mut names: Vec<&str> = project.elements.values().map(|e| e.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["SL-001", "W-001", "W-002", "W-003", "W-004"]);
    assert!(project.integrity_problems().is_empty());
}

/// IFX-091, IFX-062, PRF-004, PRF-005: a file written at format 5 loads
/// into the project it holds with no problem but the reports of later
/// migration steps: its profiles in list order, one of each parametric
/// shape at 1.1 times the shape's typical dimensions, the I shape with its
/// origin off the centre and the Z shape with its usage flags, the custom
/// profile's two regions with their materials and hole, and the catalogue
/// profile's designation.
#[test]
fn format_5_loads_its_profiles() {
    use archicraft_model::{ParametricShape, ProfileShape, ProfileUsage};

    let bytes = std::fs::read(formats_dir().join("format-5.ifcx")).unwrap();
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(
        loaded.problems.iter().all(|p| p.severity == Severity::Info),
        "{:#?}",
        loaded.problems
    );
    let project = &loaded.project;
    let names: Vec<&str> = project.profiles.iter().map(|p| p.name.as_str()).collect();
    let mut expected: Vec<String> = ParametricShape::ALL
        .iter()
        .map(|s| format!("Parametric {}", s.code()))
        .collect();
    expected.extend(["Composite".to_owned(), "IPE 200".to_owned()]);
    assert_eq!(names, expected);
    let material = |id| project.material(id).unwrap().name.as_str();
    for (profile, typical) in project.profiles.iter().zip(ParametricShape::ALL) {
        let ProfileShape::Parametric { shape, material: m } = &profile.shape else {
            panic!("{profile:?}");
        };
        let scaled: Vec<f64> = typical.dimensions().iter().map(|(_, v)| v * 1.1).collect();
        let read: Vec<f64> = shape.dimensions().iter().map(|(_, v)| *v).collect();
        assert_eq!(read, scaled, "{}", profile.name);
        assert_eq!(material(*m), "Steel");
    }
    assert_eq!(project.profiles[4].origin, DVec2::new(0.1 + 0.2, -1.0 / 3.0));
    assert_eq!(
        project.profiles[8].usage,
        ProfileUsage {
            columns: false,
            beams: true,
            walls: true,
            railings: false
        }
    );
    let ProfileShape::Custom { regions } = &project.profiles[9].shape else {
        panic!("{:?}", project.profiles[9]);
    };
    let regions: Vec<(&str, usize, usize)> = regions
        .iter()
        .map(|r| (material(r.material), r.outer.len(), r.holes.len()))
        .collect();
    assert_eq!(regions, [("Reinforced concrete", 4, 1), ("Steel", 4, 0)]);
    assert_eq!(project.profiles[10].catalogue.as_deref(), Some("IPE 200"));
    assert!(project.profiles[..10].iter().all(|p| p.catalogue.is_none()));
    assert!(project.integrity_problems().is_empty());
}

/// IFX-091, IFX-062, COL-001, COL-003: a file written at format 6 loads
/// into the project it holds with no problem but the reports of later
/// migration steps: its six columns with their sections, anchors,
/// rotations, tops, cuts and classes, the profiles of the two profile
/// columns, and the column counter.
#[test]
fn format_6_loads_its_columns() {
    let bytes = std::fs::read(formats_dir().join("format-6.ifcx")).unwrap();
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(
        loaded.problems.iter().all(|p| p.severity == Severity::Info),
        "{:#?}",
        loaded.problems
    );
    let project = &loaded.project;
    let mut columns: Vec<_> = project
        .elements
        .values()
        .filter_map(|e| Some((e.name.as_str(), e.ifc_class, e.as_column()?)))
        .collect();
    columns.sort_by_key(|(name, ..)| *name);
    let names: Vec<&str> = columns.iter().map(|(name, ..)| *name).collect();
    assert_eq!(names, ["C-001", "C-002", "C-003", "C-004", "C-005", "C-006"]);
    let template = CrossSection::Rectangular {
        width: 0.3,
        height: 0.3,
    };
    let (_, class, first) = columns[0];
    assert_eq!(
        (class, first.section, first.anchor),
        (IfcClass::Column, template, Anchor::Centre)
    );
    assert_eq!(
        first.link.top,
        TopLink::Relative {
            storeys: 1,
            offset: 0.0
        }
    );
    let (_, _, round) = columns[1];
    assert_eq!(round.section, CrossSection::Circular { diameter: 0.35 });
    assert_eq!(round.rotation, 0.5);
    assert!(matches!(round.link.top, TopLink::Storey { offset, .. } if offset == -0.25));
    let (_, class, member) = columns[2];
    assert_eq!(class, IfcClass::Member);
    assert_eq!(
        member.section,
        CrossSection::Rectangular {
            width: 0.1 + 0.2,
            height: 0.2
        }
    );
    assert_eq!(member.anchor, Anchor::TopRight);
    assert_eq!(member.rotation, std::f64::consts::FRAC_PI_6);
    assert_eq!(
        (member.link.base_offset, member.link.top),
        (0.15, TopLink::Fixed { height: 2.6 })
    );
    assert_eq!(
        (member.top_cut, member.bottom_cut),
        (ColumnCut::Angle(0.3), ColumnCut::Angle(-0.2))
    );
    assert_eq!(project.material(member.material).unwrap().name, "Steel");
    let profile_name = |section: CrossSection| project.profile(section.profile().unwrap()).unwrap().name.as_str();
    let (_, _, i_200) = columns[4];
    assert_eq!(profile_name(i_200.section), "I 200");
    assert_eq!(i_200.anchor, Anchor::ProfileOrigin);
    assert_eq!(i_200.rotation, std::f64::consts::FRAC_PI_2);
    let (_, _, composite) = columns[5];
    assert_eq!(profile_name(composite.section), "Composite");
    assert_eq!(composite.anchor, Anchor::Centre);
    assert_eq!(project.counters.get(KindTag::Column), 6);
    assert!(project.integrity_problems().is_empty());
}

/// IFX-090: a file read from an earlier format is saved in the one version
/// this release writes, `ifcx_alpha`, and reads back to the same project.
#[test]
fn format_1_is_saved_in_the_written_version() {
    let loaded = archicraft_ifcx::load(&format_1()).unwrap();
    let geometry = ModelGeometry::evaluate(&loaded.project);
    let (bytes, warnings) = archicraft_ifcx::save(&loaded.project, &geometry, &options(), false);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(validate::parse(&bytes).unwrap().header.ifcx_version, WRITTEN_VERSION);
    assert_eq!(WRITTEN_VERSION, "ifcx_alpha");
    let reloaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(reloaded.problems.is_empty(), "{:#?}", reloaded.problems);
    assert_eq!(reloaded.project.elements.len(), loaded.project.elements.len());
}

/// IFX-090: `load` names every version this release reads when it refuses an
/// unknown one.
#[test]
fn an_unknown_version_names_the_supported_versions() {
    let mut value: serde_json::Value = serde_json::from_slice(&format_1()).unwrap();
    value["header"]["ifcxVersion"] = "ifcx_omega".into();
    let error = archicraft_ifcx::load(&serde_json::to_vec(&value).unwrap()).unwrap_err();
    assert!(
        error
            .message
            .to_string()
            .contains(r#"ifcx-version-unsupported(version="ifcx_omega""#),
        "{error:?}"
    );
    for adapter in archicraft_ifcx::ADAPTERS {
        assert!(error.message.to_string().contains(adapter.version), "{error:?}");
    }
}
