//! Columns in IFCX (COL-001, COL-003, IFX-035, IFX-060, PRF-004): a column
//! is an `IfcColumn` node, or an `IfcMember` when chosen, with its parameters
//! in `archicraft::column`, a `Body` child of its material, or one
//! `Region_<n>` child per region of its custom profile, and an `Axis` child,
//! the vertical line it stands on as `usd::usdgeom::basiscurves`. The column
//! tool's defaults slot holds the same parameters without the position. A
//! damaged member is repaired with a warning (PRJ-008).

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;

use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::alpha::writer::sub_node_id;
use archicraft_ifcx::validate::{self, Severity};
use archicraft_model::kinds::{Anchor, Column, ColumnCut, CrossSection};
use archicraft_model::template::default_metric_project;
use archicraft_model::{
    Command, ElementKind, History, IfcClass, KindTag, ProfileId, ProfileShape, Project, TopLink, samples,
};
use serde_json::{Value, json};

use common::{edit, node_index, save};

/// The reference model with columns: rectangular, circular, parametric and
/// custom profile sections, both cut kinds, a relative, a storey-linked and
/// a fixed top, and an `IfcMember`.
fn sample() -> Project {
    samples::two_storey_shell_with_columns()
}

/// The columns of `project` with their names, in name order.
fn columns(project: &Project) -> Vec<(&str, &Column)> {
    let mut columns: Vec<(&str, &Column)> = project
        .elements
        .values()
        .filter_map(|e| Some((e.name.as_str(), e.as_column()?)))
        .collect();
    columns.sort_by_key(|(name, _)| *name);
    columns
}

/// The column named `name`.
fn column<'p>(project: &'p Project, name: &str) -> &'p Column {
    columns(project)
        .into_iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("no column {name}"))
        .1
}

/// A column with every float in its shortest round-trip form, which tells
/// any two finite values apart bit for bit, -0.0 and 0.0 included
/// (NFR-ACC-002).
fn exact(column: &Column) -> String {
    format!("{column:?}")
}

/// The column tool's section in the template (Appendix D.2).
fn template_section() -> CrossSection {
    let template = default_metric_project();
    let ElementKind::Column(column) = &template.tool_defaults.get(KindTag::Column).unwrap().kind else {
        panic!("the template has a column tool");
    };
    column.section
}

/// The `archicraft::column` attribute of the node of the element `name`.
fn column_attribute<'v>(value: &'v mut Value, name: &str) -> &'v mut Value {
    let index = node_index(value, name);
    &mut value["data"][index]["attributes"]["archicraft::column"]
}

/// The `parameters` of the column tool's defaults slot in a saved file.
fn slot_parameters(value: &mut Value) -> &mut Value {
    let project = value["data"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find_map(|n| n["attributes"].get_mut("archicraft::project"))
        .unwrap();
    &mut project["toolDefaults"]["column"]["parameters"]
}

/// The problems the reader reports for `loaded`, without validation's report
/// of a value that breaks its declaration (IFX-061, IFX-083), which an
/// unknown option or a member of the wrong type also gets, at most once.
fn reader_problems(loaded: &archicraft_ifcx::ReadOutcome) -> Vec<(Severity, String)> {
    let (validation, reader): (Vec<_>, Vec<_>) = loaded
        .problems
        .iter()
        .partition(|p| p.message.to_string().starts_with("ifcx-attribute-invalid("));
    assert!(validation.len() <= 1, "{validation:#?}");
    reader.iter().map(|p| (p.severity, p.message.to_string())).collect()
}

/// The one problem the reader reports for `loaded`, its severity and its
/// message in key form.
fn only_problem(loaded: &archicraft_ifcx::ReadOutcome) -> (Severity, String) {
    let problems = reader_problems(loaded);
    assert_eq!(problems.len(), 1, "{:#?}", loaded.problems);
    problems.into_iter().next().unwrap()
}

/// COL-001, COL-003, PRF-004, IFX-060, NFR-ACC-002: the sample's columns
/// save and load equal, every field and every float bit for bit:
/// rectangular, circular and profile sections, the anchor, the profile's
/// insertion origin among them, the rotation, the base offset, a relative,
/// a storey-linked and a fixed top, horizontal and angled cuts and the
/// material, with their names, classes and the column counter.
#[test]
fn columns_round_trip_exactly() {
    let project = sample();
    let written = columns(&project);
    assert_eq!(written.len(), 6);
    let sections: BTreeSet<&str> = written
        .iter()
        .map(|(_, c)| match c.section {
            CrossSection::Rectangular { .. } => "rectangular",
            CrossSection::Circular { .. } => "circular",
            CrossSection::Profile(_) => "profile",
        })
        .collect();
    assert_eq!(sections.len(), 3, "every section kind");
    assert!(written.iter().any(|(_, c)| c.anchor == Anchor::ProfileOrigin));
    assert!(written.iter().any(|(_, c)| matches!(c.top_cut, ColumnCut::Angle(_))));
    assert!(written.iter().any(|(_, c)| c.top_cut == ColumnCut::Horizontal));
    let tops: BTreeSet<&str> = written.iter().map(|(_, c)| c.link.top.code()).collect();
    assert_eq!(tops.len(), 3, "relative, storey-linked and fixed tops: {tops:?}");
    assert!(
        written
            .iter()
            .any(|(_, c)| c.anchor != Anchor::Centre && c.rotation != 0.0)
    );

    let loaded = archicraft_ifcx::load(&save(&project)).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    let read = columns(&loaded.project);
    assert_eq!(read.len(), written.len());
    for ((name, w), (read_name, r)) in written.iter().zip(&read) {
        assert_eq!(name, read_name);
        assert_eq!(exact(r), exact(w), "{name}");
    }
    assert_eq!(loaded.project.counters.get(KindTag::Column), 6);
    let mut expected = project.clone();
    expected.revision = 0;
    expected.data_version = 7;
    assert_eq!(loaded.project, expected);
}

/// COL-001, PRF-004, IFX-035, IFX-010, IFX-043, IFX-006: a column's node
/// carries `IfcColumn`, or `IfcMember` when chosen, and as children its
/// parts and its `Axis`: one `Body` of the column's material, of a
/// parametric profile's material for a profile column, or `Region_1`,
/// `Region_2`, … of the regions' materials for a custom profile, each
/// inheriting its material; the `Axis` with the id `sub_node_id(owner,
/// "Axis")` and two `usd::usdgeom::basiscurves` points on the column's
/// position, at the bottom and the top of its geometry axis. The file
/// validates.
#[test]
fn a_column_node_is_an_ifc_column_with_its_parts_and_an_axis() {
    let project = sample();
    let geometry = ModelGeometry::evaluate(&project);
    assert!(geometry.problems.is_empty(), "{:?}", geometry.problems);
    let bytes = save(&project);
    let parsed = validate::parse(&bytes).unwrap();
    assert!(
        validate::validate(&parsed).is_empty(),
        "{:#?}",
        validate::validate(&parsed)
    );
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    let nodes = value["data"].as_array().unwrap();
    let node = |path: &str| nodes.iter().find(|n| n["path"] == path).unwrap();

    let (mut classes, mut part_names) = (BTreeSet::new(), BTreeSet::new());
    for element in project.elements.values().filter(|e| e.tag() == KindTag::Column) {
        let column = element.as_column().unwrap();
        let owner = element.id.uuid();
        let n = node(&element.id.to_string());
        let class = n["attributes"]["bsi::ifc::class"]["code"].as_str().unwrap();
        assert_eq!(class, element.ifc_class.code(), "{}", element.name);
        classes.insert(class.to_owned());
        let parts = &geometry.get(element.id).unwrap().parts;
        let names: Vec<&str> = parts.iter().map(|p| p.name.as_str()).collect();
        part_names.insert(names.join(", "));
        let mut children = serde_json::Map::new();
        for part in parts {
            children.insert(part.name.clone(), json!(sub_node_id(owner, &part.name).to_string()));
        }
        let axis_id = sub_node_id(owner, "Axis").to_string();
        children.insert("Axis".into(), json!(axis_id));
        assert_eq!(n["children"], Value::Object(children), "{}", element.name);
        let own = [column.material];
        let materials: Vec<_> = match column.profile(&project).map(|p| &p.shape) {
            None => own.to_vec(),
            Some(ProfileShape::Parametric { material, .. }) => vec![*material],
            Some(ProfileShape::Custom { regions }) => regions.iter().map(|r| r.material).collect(),
        };
        assert_eq!(parts.len(), materials.len(), "{}", element.name);
        for (part, material) in parts.iter().zip(materials) {
            assert_eq!(part.material, Some(material), "{}", element.name);
            let body = node(&sub_node_id(owner, &part.name).to_string());
            assert_eq!(body["inherits"], json!({"material": material.to_string()}));
            assert!(
                !body["attributes"]["usd::usdgeom::mesh"]["points"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
        }

        let points = node(&axis_id)["attributes"]["usd::usdgeom::basiscurves"]["points"]
            .as_array()
            .unwrap();
        let model = &geometry.get(element.id).unwrap().axis;
        assert_eq!((points.len(), model.len()), (2, 2), "{}", element.name);
        for (point, expected) in points.iter().zip(model) {
            let point: Vec<f64> = point.as_array().unwrap().iter().map(|c| c.as_f64().unwrap()).collect();
            assert_eq!(point.len(), 3);
            for (c, e) in point.iter().zip([expected.x, expected.y, expected.z]) {
                assert!((c - e).abs() <= 5e-7, "{}: {point:?} vs {expected}", element.name);
            }
            assert!((point[0] - column.position.x).abs() <= 5e-7);
            assert!((point[1] - column.position.y).abs() <= 5e-7);
        }
        assert!(model[1].z > model[0].z, "{}: the axis runs upwards", element.name);
    }
    assert_eq!(
        classes,
        [IfcClass::Column.code(), IfcClass::Member.code()]
            .map(str::to_owned)
            .into()
    );
    assert_eq!(part_names, ["Body", "Region_1, Region_2"].map(str::to_owned).into());
}

/// ELM-001, COL-001, COL-003: the column tool's defaults slot saves and
/// loads equal, its parameters bit for bit: a circular section, an anchor,
/// a rotation, a base offset, a fixed top, two angled cuts and a material,
/// with the slot's class.
#[test]
fn the_column_tool_defaults_round_trip() {
    let mut project = sample();
    let steel = project.materials.iter().find(|m| m.name == "Steel").unwrap().id;
    let mut defaults = project.tool_defaults.clone();
    let mut slot = defaults.get(KindTag::Column).unwrap().clone();
    slot.ifc_class = IfcClass::Member;
    let ElementKind::Column(column) = &mut slot.kind else {
        panic!("{slot:?}");
    };
    column.section = CrossSection::Circular { diameter: 0.1 + 0.15 };
    column.anchor = Anchor::BottomRight;
    column.rotation = 1.0 / 3.0;
    column.material = steel;
    column.link.base_offset = 0.05;
    column.link.top = TopLink::Fixed { height: 2.8 };
    column.top_cut = ColumnCut::Angle(0.1);
    column.bottom_cut = ColumnCut::Angle(-0.4);
    let column = column.clone();
    defaults.insert(slot.clone());
    History::default()
        .execute(&mut project, Command::SetToolDefaults(defaults))
        .unwrap();

    let loaded = archicraft_ifcx::load(&save(&project)).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    let read = loaded.project.tool_defaults.get(KindTag::Column).unwrap();
    assert_eq!(read, &slot);
    let ElementKind::Column(read_column) = &read.kind else {
        panic!("{read:?}");
    };
    assert_eq!(exact(read_column), exact(&column));
}

/// COL-003, PRJ-008: a cut whose angle is not below the limit loads as a
/// horizontal cut with one warning naming the member, and so does a cut that
/// cannot be read, with the reason; the column and its other cut load as
/// written.
#[test]
fn an_invalid_cut_angle_loads_as_a_horizontal_cut_with_one_warning() {
    let project = sample();
    let written = column(&project, "C-003").clone();
    assert_eq!(written.top_cut, ColumnCut::Angle(0.3));
    let cases: [(&str, Value, &str); 4] = [
        (
            "topCut",
            json!({"kind": "angle", "angle": 1.2}),
            "ifcx-column-cut-horizontal",
        ),
        (
            "topCut",
            json!({"kind": "angle", "angle": -1.1}),
            "ifcx-column-cut-horizontal",
        ),
        ("topCut", json!({"kind": "angle"}), "ifcx-default-used"),
        ("topCut", json!({"kind": "oblique"}), "ifcx-default-used"),
    ];
    for (member, cut, reason) in cases {
        let bytes = edit(&project, |value| column_attribute(value, "C-003")[member] = cut.clone());
        let loaded = archicraft_ifcx::load(&bytes).unwrap();
        let (severity, message) = only_problem(&loaded);
        assert_eq!(severity, Severity::Warning, "{cut}");
        assert!(
            message.contains("ifcx-repaired") && message.contains(reason),
            "{cut}: {message}"
        );
        if reason == "ifcx-column-cut-horizontal" {
            assert!(message.contains("topCut"), "{cut}: {message}");
        }
        let read = column(&loaded.project, "C-003");
        assert_eq!(read.top_cut, ColumnCut::Horizontal, "{cut}");
        assert_eq!(
            exact(&Column {
                top_cut: ColumnCut::Horizontal,
                ..written.clone()
            }),
            exact(read)
        );
    }
}

/// COL-001, PRJ-008: a section that cannot be read loads as the column
/// tool's template section, 0.30 × 0.30 m (Appendix D.2), with one warning;
/// a column without a position is skipped with one error.
#[test]
fn a_damaged_section_or_position_is_repaired_or_skipped_with_one_report() {
    let project = sample();
    let written = column(&project, "C-002").clone();
    for section in [
        json!({"kind": "hexagonal"}),
        json!({"kind": "circular"}),
        json!("round"),
    ] {
        let bytes = edit(&project, |value| {
            column_attribute(value, "C-002")["section"] = section.clone();
        });
        let loaded = archicraft_ifcx::load(&bytes).unwrap();
        let (severity, message) = only_problem(&loaded);
        assert_eq!(severity, Severity::Warning, "{section}");
        assert!(message.contains("ifcx-repaired"), "{section}: {message}");
        let read = column(&loaded.project, "C-002");
        assert_eq!(read.section, template_section(), "{section}");
        assert_eq!(
            exact(&Column {
                section: template_section(),
                ..written.clone()
            }),
            exact(read)
        );
    }

    let bytes = edit(&project, |value| {
        column_attribute(value, "C-002")
            .as_object_mut()
            .unwrap()
            .remove("position");
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let (severity, message) = only_problem(&loaded);
    assert_eq!(severity, Severity::Error);
    assert!(message.contains("ifcx-skipped"), "{message}");
    assert_eq!(columns(&loaded.project).len(), columns(&project).len() - 1);
}

/// ELM-001, PRJ-008: the column tool's defaults slot is read leniently: an
/// unreadable section gives the template's, and a cut angle beyond the
/// limit a horizontal cut, each with one warning; the other members load
/// as written.
#[test]
fn the_column_tool_defaults_are_read_leniently() {
    let project = sample();
    let bytes = edit(&project, |value| {
        let parameters = slot_parameters(value);
        parameters["section"] = json!("square");
        parameters["anchor"] = json!("top-left");
        parameters["topCut"] = json!({"kind": "angle", "angle": 2.0});
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let problems = reader_problems(&loaded);
    assert_eq!(problems.len(), 2, "{:#?}", loaded.problems);
    assert!(problems.iter().all(|(severity, _)| *severity == Severity::Warning));
    let messages: Vec<&String> = problems.iter().map(|(_, message)| message).collect();
    assert!(messages[0].contains("ifcx-default-used"), "{messages:#?}");
    assert!(messages[1].contains("ifcx-column-cut-horizontal"), "{messages:#?}");
    let slot = loaded.project.tool_defaults.get(KindTag::Column).unwrap();
    let ElementKind::Column(column) = &slot.kind else {
        panic!("{slot:?}");
    };
    assert_eq!(column.section, template_section());
    assert_eq!(column.anchor, Anchor::TopLeft);
    assert_eq!(column.top_cut, ColumnCut::Horizontal);
}

/// The profile column of the sample anchored at its profile's insertion
/// origin, `C-005`.
const PROFILE_COLUMN: &str = "C-005";

/// PRF-004, COL-001, PRJ-008: a column whose profile the project does not
/// have loads with the column tool's template section, anchored at its
/// centre, with one warning; its other members load as written.
#[test]
fn an_unknown_profile_loads_as_the_template_section_with_one_warning() {
    let project = sample();
    let written = column(&project, PROFILE_COLUMN).clone();
    assert!(written.section.profile().is_some());
    assert_eq!(written.anchor, Anchor::ProfileOrigin);
    let unknown = ProfileId::new();
    let bytes = edit(&project, |value| {
        column_attribute(value, PROFILE_COLUMN)["section"]["profile"] = json!(unknown.to_string());
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let (severity, message) = only_problem(&loaded);
    assert_eq!(severity, Severity::Warning);
    assert!(
        message.contains("ifcx-repaired") && message.contains("ifcx-column-profile-missing"),
        "{message}"
    );
    assert!(message.contains(PROFILE_COLUMN), "{message}");
    let read = column(&loaded.project, PROFILE_COLUMN);
    assert_eq!(
        exact(read),
        exact(&Column {
            section: template_section(),
            anchor: Anchor::Centre,
            ..written
        })
    );
}

/// ELM-001, PRF-004: the column tool's defaults slot with a profile section
/// anchored at the profile's insertion origin round-trips; with a profile
/// the project does not have, the slot loads the template section with one
/// warning.
#[test]
fn a_profile_in_the_column_tool_defaults_round_trips_or_is_repaired() {
    let mut project = sample();
    let profile = project.profiles[0].id;
    let mut defaults = project.tool_defaults.clone();
    let mut slot = defaults.get(KindTag::Column).unwrap().clone();
    let ElementKind::Column(column) = &mut slot.kind else {
        panic!("{slot:?}");
    };
    column.section = CrossSection::Profile(profile);
    column.anchor = Anchor::ProfileOrigin;
    defaults.insert(slot.clone());
    History::default()
        .execute(&mut project, Command::SetToolDefaults(defaults))
        .unwrap();

    let loaded = archicraft_ifcx::load(&save(&project)).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert_eq!(loaded.project.tool_defaults.get(KindTag::Column), Some(&slot));

    let bytes = edit(&project, |value| {
        slot_parameters(value)["section"]["profile"] = json!(ProfileId::new().to_string());
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let (severity, message) = only_problem(&loaded);
    assert_eq!(severity, Severity::Warning);
    assert!(message.contains("ifcx-column-profile-missing"), "{message}");
    let read = loaded.project.tool_defaults.get(KindTag::Column).unwrap();
    let ElementKind::Column(column) = &read.kind else {
        panic!("{read:?}");
    };
    assert_eq!(column.section, template_section());
}

/// `project` with a copy of its first profile that only beams may use, and
/// that profile's identity.
fn with_a_beam_only_profile(project: &mut Project) -> ProfileId {
    let mut profile = project.profiles[0].clone();
    profile.id = ProfileId::new();
    profile.name = "Beams only".into();
    profile.usage.columns = false;
    assert!(profile.usage.beams);
    let id = profile.id;
    History::default()
        .execute(project, Command::AddProfile(profile))
        .unwrap();
    id
}

/// PRF-004, COL-001, PRJ-008: a column whose profile exists but does not
/// allow columns loads with the column tool's template section, anchored at
/// its centre, with one warning, instead of being skipped; its other
/// members load as written.
#[test]
fn a_profile_without_the_columns_flag_loads_as_the_template_section_with_one_warning() {
    let mut project = sample();
    let beam_only = with_a_beam_only_profile(&mut project);
    let written = column(&project, PROFILE_COLUMN).clone();
    assert_eq!(written.anchor, Anchor::ProfileOrigin);
    let bytes = edit(&project, |value| {
        column_attribute(value, PROFILE_COLUMN)["section"]["profile"] = json!(beam_only.to_string());
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let (severity, message) = only_problem(&loaded);
    assert_eq!(severity, Severity::Warning);
    assert!(
        message.contains("ifcx-repaired") && message.contains("ifcx-column-profile-not-for-columns"),
        "{message}"
    );
    assert!(message.contains(PROFILE_COLUMN), "{message}");
    assert!(loaded.project.profile(beam_only).is_some());
    let read = column(&loaded.project, PROFILE_COLUMN);
    assert_eq!(
        exact(read),
        exact(&Column {
            section: template_section(),
            anchor: Anchor::Centre,
            ..written
        })
    );
}

/// ELM-001, PRF-004, PRJ-008: the column tool's defaults slot naming a
/// profile that exists but does not allow columns loads the template section
/// with one warning, so later placements with the tool validate.
#[test]
fn a_profile_without_the_columns_flag_in_the_column_tool_defaults_is_repaired() {
    let mut project = sample();
    let beam_only = with_a_beam_only_profile(&mut project);
    let bytes = edit(&project, |value| {
        slot_parameters(value)["section"] = json!({"kind": "profile", "profile": beam_only.to_string()});
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let (severity, message) = only_problem(&loaded);
    assert_eq!(severity, Severity::Warning);
    assert!(message.contains("ifcx-column-profile-not-for-columns"), "{message}");
    let read = loaded.project.tool_defaults.get(KindTag::Column).unwrap();
    let ElementKind::Column(column) = &read.kind else {
        panic!("{read:?}");
    };
    assert_eq!(column.section, template_section());
}
