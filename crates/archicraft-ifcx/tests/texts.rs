//! Texts and text styles in IFCX (ANN-007, ANN-010, IFX-060). A text is not
//! an IFCX node: it is an entry of its storey's `archicraft::annotations`
//! with its view, anchor, anchor position, angle, content, style and size
//! override under `text`, so the standard content does not depend on it
//! (IFX-068). The project's text styles are `textStyles` of
//! `archicraft::project`, and the text tool's defaults slot and counter are
//! stored like those of every tool. Reading is lenient (PRJ-008): a damaged
//! style is repaired or dropped with a report, and a text or the tool's
//! defaults whose style is missing fall back to the project's first style
//! with a warning.

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::HashSet;

use archicraft_ifcx::Problem;
use archicraft_ifcx::validate::{self, Severity};
use archicraft_model::template::template_text_styles;
use archicraft_model::{Element, KindTag, Project, TextStyleId, samples};
use serde_json::{Value, json};

use common::{edit, options, save};

const ANNOTATIONS: &str = "archicraft::annotations";

/// What a loaded copy of `project` equals: the revision is not stored and
/// the data version is the save counter of [`options`].
fn as_loaded(project: &Project) -> Project {
    let mut expected = project.clone();
    expected.revision = 0;
    expected.data_version = options().data_version;
    expected
}

fn named<'p>(project: &'p Project, name: &str) -> &'p Element {
    project.elements.values().find(|e| e.name == name).unwrap()
}

/// The index of the data node at `path`.
fn node_at(file: &Value, path: &str) -> usize {
    file["data"]
        .as_array()
        .unwrap()
        .iter()
        .position(|n| n["path"] == path)
        .unwrap()
}

/// The annotation entry of `element` in `file`.
fn entry_of<'f>(file: &'f mut Value, element: &Element) -> &'f mut Value {
    let i = node_at(file, &element.storey.to_string());
    file["data"][i]["attributes"][ANNOTATIONS]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|e| e["id"] == element.id.to_string().as_str())
        .unwrap()
}

/// `archicraft::project` of the file.
fn project_attribute(file: &mut Value) -> &mut Value {
    &mut file["data"][0]["attributes"]["archicraft::project"]
}

/// `value` with every number as a float: the file writes whole numbers
/// without a fraction (IFX-007), so `4.0` reads back as the integer `4`.
fn floats(value: &Value) -> Value {
    match value {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(items) => items.iter().map(floats).collect(),
        Value::Object(map) => map.iter().map(|(k, v)| (k.clone(), floats(v))).collect(),
        other => other.clone(),
    }
}

/// The reader's own reports (skipped and repaired records) in their key
/// form, without what the validator says about the same damage.
fn reader_reports(problems: &[Problem]) -> Vec<(Severity, String)> {
    problems
        .iter()
        .filter(|p| ["ifcx-skipped", "ifcx-repaired"].contains(&p.message.key()))
        .map(|p| (p.severity, p.message.to_string()))
        .collect()
}

/// IFX-060, IFX-064, ANN-007: the annotation sample loads back equal and
/// without problems: its three texts with every member at full precision,
/// the project's text styles, the text tool's defaults and its counter.
#[test]
fn the_annotation_sample_round_trips_with_its_texts_and_styles() {
    let project = samples::annotation_sample();
    let loaded = archicraft_ifcx::load(&save(&project)).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert_eq!(loaded.project, as_loaded(&project));
    let texts = loaded
        .project
        .elements
        .values()
        .filter(|e| e.as_text().is_some())
        .count();
    assert_eq!(texts, 3);
    assert_eq!(loaded.project.text_styles, template_text_styles());
    assert_eq!(loaded.project.counters.get(KindTag::Text), 3);
}

/// ANN-010, IFX-060: texts are entries of their storey's
/// `archicraft::annotations`, never nodes: no node has a text's id or the
/// kind `text`, and the file declares no `archicraft::text`. Each entry
/// holds the text's members under `text`, in the order view, anchor,
/// anchor position (its snake-case code), angle, content and style, with
/// the size override only when the text has one; the file validates.
#[test]
fn texts_are_annotation_entries_with_their_members() {
    let project = samples::annotation_sample();
    let bytes = save(&project);
    let mut file: Value = serde_json::from_slice(&bytes).unwrap();
    let ids: HashSet<String> = project
        .elements
        .values()
        .filter(|e| e.as_text().is_some())
        .map(|e| e.id.to_string())
        .collect();
    for node in file["data"].as_array().unwrap() {
        assert!(!ids.contains(node["path"].as_str().unwrap()), "{node}");
        assert_ne!(node["attributes"]["archicraft::element"]["kind"], "text", "{node}");
        assert!(node["attributes"].get("archicraft::text").is_none(), "{node}");
    }
    let schemas = file["schemas"].as_object().unwrap();
    assert!(schemas.contains_key(ANNOTATIONS));
    assert!(!schemas.contains_key("archicraft::text"));

    let ground = project.storeys.ground();
    let i = node_at(&file, &ground.to_string());
    let on_ground: Vec<&str> = file["data"][i]["attributes"][ANNOTATIONS]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(on_ground, ["TX-001", "TX-002"], "model order");

    let style = |name: &str| {
        project
            .text_styles
            .iter()
            .find(|s| s.name == name)
            .unwrap()
            .id
            .to_string()
    };
    let living = named(&project, "TX-001").clone();
    assert_eq!(
        floats(&entry_of(&mut file, &living)["text"]),
        json!({
            "view": {"type": "plan", "storey": ground.to_string()},
            "anchor": [2.5, 4.0],
            "anchorPos": "bottom_left",
            "angle": 0.0,
            "content": "Living room",
            "style": style("Standard 2.5 mm"),
        })
    );
    let kitchen = named(&project, "TX-002").clone();
    let members = &entry_of(&mut file, &kitchen)["text"];
    let keys: Vec<&String> = members.as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        ["view", "anchor", "anchorPos", "angle", "content", "style", "sizeMm"]
    );
    assert_eq!(members["anchorPos"], "center");
    assert_eq!(members["content"], "Kitchen\n12.5 m²\nTiled floor");
    assert_eq!(members["sizeMm"], 3.5);
    let landing = named(&project, "TX-003").clone();
    let members = &entry_of(&mut file, &landing)["text"];
    assert_eq!(members["anchorPos"], "middle_left");
    assert_eq!(members["angle"].as_f64(), Some(std::f64::consts::FRAC_PI_6));
    assert_eq!(members["style"], style("Heading 5.0 mm").as_str());
    assert_eq!(members["view"]["storey"], landing.storey.to_string().as_str());
    assert_eq!(
        entry_of(&mut file, &landing)["ifcClass"],
        "IfcAnnotation",
        "the envelope of every annotation entry"
    );

    let parsed = validate::parse(&bytes).unwrap();
    let problems = validate::validate(&parsed);
    assert!(problems.is_empty(), "{problems:#?}");
}

/// IFX-060, IFX-061, ANN-007: the project's text styles are `textStyles` of
/// `archicraft::project`, in list order, each with its identity, name, size
/// in paper millimetres, alignment code and line spacing, and the
/// declaration describes them.
#[test]
fn text_styles_are_stored_in_the_project_attribute() {
    let project = samples::annotation_sample();
    let mut file: Value = serde_json::from_slice(&save(&project)).unwrap();
    let written = project_attribute(&mut file)["textStyles"].clone();
    let expected: Vec<Value> = project
        .text_styles
        .iter()
        .map(|s| {
            json!({
                "id": s.id.to_string(),
                "name": s.name,
                "sizeMm": s.size_mm,
                "align": s.align.code(),
                "lineSpacing": s.line_spacing,
            })
        })
        .collect();
    assert_eq!(floats(&written), Value::Array(expected));
    assert_eq!(written[3]["name"], "Heading 5.0 mm");
    assert_eq!(written[0]["align"], "left");

    let declared = &file["schemas"]["archicraft::project"]["value"]["objectRestrictions"]["values"]["textStyles"];
    assert_eq!(declared["dataType"], "Array");
    assert!(declared.get("optional").is_none(), "every file has text styles");
    let record = &declared["arrayRestrictions"]["value"]["objectRestrictions"]["values"];
    let members: Vec<&String> = record.as_object().unwrap().keys().collect();
    assert_eq!(members, ["id", "name", "sizeMm", "align", "lineSpacing"]);
    assert_eq!(
        record["align"]["enumRestrictions"]["options"],
        json!(["left", "center", "right"])
    );
}

/// ELM-001, ELM-019: the text tool's defaults slot holds the common members
/// and, apart from them, its parameters without a material: the anchor
/// position, angle and style, and the size override only when set; its
/// placeholder view, anchor and empty content are not written. The text counter is written like every
/// tool's. A size override in the slot round-trips.
#[test]
fn the_text_tools_defaults_and_counter_are_stored() {
    let mut project = samples::annotation_sample();
    let mut file: Value = serde_json::from_slice(&save(&project)).unwrap();
    let attribute = project_attribute(&mut file);
    let slot = attribute["toolDefaults"]["text"].as_object().unwrap();
    let keys: Vec<&String> = slot.keys().collect();
    assert_eq!(keys, ["layer", "position", "ifcClass", "parameters"]);
    let parameters = slot["parameters"].as_object().unwrap();
    let keys: Vec<&String> = parameters.keys().collect();
    assert_eq!(keys, ["anchorPos", "angle", "style"]);
    assert_eq!(parameters["anchorPos"], "bottom_left");
    assert_eq!(parameters["style"], project.text_styles[0].id.to_string().as_str());
    assert_eq!(attribute["counters"]["text"], 3);

    let mut defaults = project.tool_defaults.clone();
    if let archicraft_model::ElementKind::Text(text) = &mut defaults.get_mut(KindTag::Text).unwrap().kind {
        text.size_mm = Some(7.0);
    }
    archicraft_model::History::default()
        .execute(&mut project, archicraft_model::Command::SetToolDefaults(defaults))
        .unwrap();
    let bytes = save(&project);
    let mut file: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        project_attribute(&mut file)["toolDefaults"]["text"]["parameters"]["sizeMm"],
        7.0
    );
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    assert_eq!(loaded.project.tool_defaults, project.tool_defaults);
}

/// Removes every `archicraft::` attribute and schema declaration, leaving
/// what a consumer that ignores the ArchiCraft namespace reads, as bytes.
fn standard_content(bytes: &[u8]) -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(bytes).unwrap();
    for node in value["data"].as_array_mut().unwrap() {
        if let Some(attributes) = node.get_mut("attributes").and_then(Value::as_object_mut) {
            attributes.retain(|key, _| !key.starts_with("archicraft::"));
        }
    }
    value["schemas"]
        .as_object_mut()
        .unwrap()
        .retain(|key, _| !key.starts_with("archicraft::"));
    serde_json::to_vec(&value).unwrap()
}

/// IFX-068, IFX-063, ANN-010: the standard content does not depend on
/// texts. The annotation sample written with and without its texts gives
/// byte-identical files once every `archicraft::` attribute and schema
/// declaration is stripped, while the files themselves differ.
#[test]
fn ifx_068_texts_leave_the_standard_content_unchanged() {
    let with_texts = samples::annotation_sample();
    let mut without = with_texts.clone();
    without.elements.retain(|_, e| e.as_text().is_none());
    let (with_bytes, without_bytes) = (save(&with_texts), save(&without));
    assert_ne!(with_bytes, without_bytes);
    assert_eq!(standard_content(&with_bytes), standard_content(&without_bytes));
}

/// IFX-007, IFX-064: saving the annotation sample twice, with other header
/// values, gives the same bytes apart from the header, and saving the
/// loaded project gives the same bytes again.
#[test]
fn saving_twice_gives_the_same_bytes_apart_from_the_header() {
    let project = samples::annotation_sample();
    let geometry = archicraft_geometry::ModelGeometry::evaluate(&project);
    let later = archicraft_ifcx::WriteOptions {
        author: "Someone else".into(),
        timestamp: "2026-11-10T08:30:00Z".into(),
        data_version: 8,
    };
    let (first, _) = archicraft_ifcx::save(&project, &geometry, &options(), false);
    let (second, warnings) = archicraft_ifcx::save(&project, &geometry, &later, false);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_ne!(first, second);
    let without_header = |bytes: &[u8]| {
        let mut value: Value = serde_json::from_slice(bytes).unwrap();
        value.as_object_mut().unwrap().remove("header");
        serde_json::to_vec(&value).unwrap()
    };
    assert_eq!(without_header(&first), without_header(&second));
    let loaded = archicraft_ifcx::load(&first).unwrap();
    assert_eq!(save(&loaded.project), first);
}

/// PRJ-008, NFR-REL-004, ANN-007: a text whose style names no style of the
/// project, or is no identity at all, loads with the project's first style
/// and one warning that names the text and the style it got; the other
/// texts keep theirs and the project has no integrity problem.
#[test]
fn a_text_whose_style_is_missing_is_repaired_to_the_first_style() {
    let project = samples::annotation_sample();
    let landing = named(&project, "TX-003").clone();
    for style in [json!(TextStyleId::new().to_string()), json!("heading")] {
        let bytes = edit(&project, |v| entry_of(v, &landing)["text"]["style"] = style.clone());
        let loaded = archicraft_ifcx::load(&bytes).unwrap();
        let read = loaded.project.element(landing.id).unwrap().as_text().unwrap();
        assert_eq!(read.style, project.text_styles[0].id, "{style}");
        let reports = reader_reports(&loaded.problems);
        assert_eq!(reports.len(), 1, "{style}: {:#?}", loaded.problems);
        let (severity, message) = &reports[0];
        assert_eq!(*severity, Severity::Warning);
        assert!(message.contains(r#"ifcx-record-element(name="TX-003")"#), "{message}");
        assert!(
            message.contains(r#"ifcx-text-style-missing(name="Standard 2.5 mm")"#),
            "{message}"
        );
        let kitchen = named(&loaded.project, "TX-002").as_text().unwrap();
        assert_eq!(kitchen.style, named(&project, "TX-002").as_text().unwrap().style);
        assert!(loaded.project.integrity_problems().is_empty());
    }
}

/// PRJ-008, ANN-007: the reader keeps no invalid or duplicate style. A
/// style that is not an object, has no identity or no name, a size or a
/// line spacing that is not positive, or repeats the name or the identity
/// of an earlier style is dropped with a report that names the record and
/// says why; a member of a kept style that is unreadable is repaired to the
/// first preset's value with a report. A text whose style was dropped gets
/// the first style with a warning, and the styles left pass the model's
/// check.
#[test]
fn invalid_or_duplicate_text_styles_are_dropped_or_repaired_with_a_report() {
    let project = samples::annotation_sample();
    let standard = project.text_styles[0].clone();
    let notes = TextStyleId::new();
    let bytes = edit(&project, |v| {
        let styles = project_attribute(v)["textStyles"].as_array_mut().unwrap();
        styles[1]["name"] = json!(standard.name);
        styles[2]["id"] = json!(standard.id.to_string());
        styles[3]["sizeMm"] = json!(-1.0);
        styles.push(json!("Notes"));
        styles.push(json!({"name": "Notes 3.0 mm", "sizeMm": 3.0, "align": "left", "lineSpacing": 1.2}));
        styles.push(json!({"id": TextStyleId::new().to_string(), "sizeMm": 3.0, "align": "left", "lineSpacing": 1.2}));
        styles.push(json!({"id": TextStyleId::new().to_string(), "name": "Wide", "sizeMm": 3.0, "align": "left", "lineSpacing": 0.0}));
        styles.push(json!({
            "id": notes.to_string(),
            "name": "Notes 3.0 mm",
            "sizeMm": 3.0,
            "align": "justified",
            "lineSpacing": "wide",
        }));
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let styles = &loaded.project.text_styles;
    let names: Vec<&str> = styles.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Standard 2.5 mm", "Notes 3.0 mm"]);
    assert_eq!(styles[0], standard);
    assert_eq!((styles[1].id, styles[1].size_mm), (notes, 3.0));
    assert_eq!(styles[1].align, standard.align, "repaired to the first preset's");
    assert_eq!(styles[1].line_spacing, standard.line_spacing);
    assert_eq!(archicraft_model::text::check_text_styles(styles), Ok(()));

    let reports = reader_reports(&loaded.problems);
    let skipped = |index: usize, reason: &str| {
        (
            Severity::Error,
            format!("ifcx-skipped(record=ifcx-record-text-style(index={index}), reason={reason})"),
        )
    };
    let expected_skips = [
        skipped(1, r#"model-error-text-style-duplicate-name(name="Standard 2.5 mm")"#),
        skipped(2, r#"model-error-duplicate-text-style-id(name="Small 1.8 mm")"#),
        skipped(3, r#"model-error-text-style-size(name="Heading 5.0 mm")"#),
        skipped(4, "ifcx-not-object"),
        skipped(5, r#"ifcx-field-missing(key="id")"#),
        skipped(6, r#"ifcx-field-missing(key="name")"#),
        skipped(7, r#"model-error-text-style-line-spacing(name="Wide")"#),
    ];
    for skip in &expected_skips {
        assert!(reports.contains(skip), "{skip:?} in {reports:#?}");
    }
    let repaired: Vec<&String> = reports
        .iter()
        .filter(|(severity, _)| *severity == Severity::Warning)
        .map(|(_, message)| message)
        .collect();
    assert_eq!(repaired.len(), 3, "{repaired:#?}");
    assert!(repaired[0].contains("ifcx-record-text-style(index=8)") && repaired[0].contains(r#"key="align""#));
    assert!(repaired[1].contains("ifcx-record-text-style(index=8)") && repaired[1].contains(r#"key="lineSpacing""#));
    assert!(
        repaired[2].contains(r#"ifcx-record-element(name="TX-003")"#)
            && repaired[2].contains(r#"ifcx-text-style-missing(name="Standard 2.5 mm")"#),
        "{}",
        repaired[2]
    );
    assert_eq!(reports.len(), expected_skips.len() + repaired.len(), "{reports:#?}");
    let landing = named(&loaded.project, "TX-003").as_text().unwrap();
    assert_eq!(landing.style, standard.id);
    assert!(loaded.project.integrity_problems().is_empty());
}

/// PRJ-008, ANN-007: a file without readable text styles, or with none,
/// gets the template's presets with one warning, and its texts keep their
/// styles, which are presets.
#[test]
fn a_file_without_text_styles_gets_the_templates_with_a_warning() {
    let project = samples::annotation_sample();
    for (styles, reason) in [
        (None, r#"ifcx-field-missing(key="textStyles")"#),
        (Some(json!("Standard")), r#"ifcx-field-not-list(key="textStyles")"#),
        (Some(json!([])), "ifcx-text-styles-none"),
        (Some(json!(["Standard"])), "ifcx-text-styles-none"),
    ] {
        let bytes = edit(&project, |v| {
            let attribute = project_attribute(v).as_object_mut().unwrap();
            match &styles {
                Some(styles) => attribute.insert("textStyles".into(), styles.clone()),
                None => attribute.remove("textStyles"),
            };
        });
        let loaded = archicraft_ifcx::load(&bytes).unwrap();
        assert_eq!(loaded.project.text_styles, template_text_styles(), "{styles:?}");
        let reports: Vec<String> = reader_reports(&loaded.problems)
            .into_iter()
            .filter(|(severity, _)| *severity == Severity::Warning)
            .map(|(_, message)| message)
            .collect();
        assert_eq!(
            reports,
            [format!(
                "ifcx-repaired(record=ifcx-record-project, reason=ifcx-text-styles-template-used(reason={reason}))"
            )],
            "{styles:?}"
        );
        let landing = named(&loaded.project, "TX-003");
        assert_eq!(landing, named(&project, "TX-003"));
    }
}

/// PRJ-008, ELM-001: the text tool's defaults whose style names no style of
/// the project get the project's first style with one warning, and the
/// file loads with the other defaults as written.
#[test]
fn the_text_tools_default_style_is_repaired_when_missing() {
    let project = samples::annotation_sample();
    let bytes = edit(&project, |v| {
        project_attribute(v)["toolDefaults"]["text"]["parameters"]["style"] = json!(TextStyleId::new().to_string());
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let slot = loaded.project.tool_defaults.get(KindTag::Text).unwrap();
    assert_eq!(slot.kind.text_style(), Some(project.text_styles[0].id));
    assert_eq!(
        reader_reports(&loaded.problems),
        [(
            Severity::Warning,
            r#"ifcx-repaired(record=ifcx-record-tool-defaults, reason=ifcx-tool-text-style-missing(name="Standard 2.5 mm"))"#
                .to_owned()
        )]
    );
    assert_eq!(loaded.project.tool_defaults, project.tool_defaults);
}

/// PRJ-008, IFX-083, ANN-007: a text without content, view or anchor is
/// skipped with the reason; an unreadable anchor position or angle is
/// repaired to the default with a report, and an unreadable or non-positive
/// size override is left out with a report. The other texts load.
#[test]
fn damaged_text_members_are_repaired_or_the_text_skipped() {
    let project = samples::annotation_sample();
    let living = named(&project, "TX-001").clone();
    let kitchen = named(&project, "TX-002").clone();
    let landing = named(&project, "TX-003").clone();
    for (member, reason) in [
        ("content", r#"ifcx-field-missing(key="content")"#),
        ("view", r#"ifcx-field-missing(key="view")"#),
        ("anchor", r#"ifcx-field-missing(key="anchor")"#),
    ] {
        let bytes = edit(&project, |v| {
            entry_of(v, &living)["text"].as_object_mut().unwrap().remove(member);
        });
        let loaded = archicraft_ifcx::load(&bytes).unwrap();
        assert!(loaded.project.element(living.id).is_none(), "{member}");
        assert_eq!(loaded.project.elements.len(), project.elements.len() - 1);
        assert_eq!(
            reader_reports(&loaded.problems),
            [(
                Severity::Error,
                format!(r#"ifcx-skipped(record=ifcx-record-element(name="TX-001"), reason={reason})"#)
            )],
            "{member}"
        );
    }

    let bytes = edit(&project, |v| {
        let text = &mut entry_of(v, &kitchen)["text"];
        text["anchorPos"] = json!("middle");
        text["angle"] = json!("steep");
        text["sizeMm"] = json!(0.0);
        entry_of(v, &landing)["text"]["sizeMm"] = json!("large");
    });
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    let read = loaded.project.element(kitchen.id).unwrap().as_text().unwrap();
    assert_eq!(read.anchor_pos, archicraft_model::AnchorPos::BottomLeft);
    assert_eq!(read.angle, 0.0);
    assert_eq!(read.size_mm, None);
    assert_eq!(read.content, kitchen.as_text().unwrap().content);
    assert_eq!(
        loaded.project.element(landing.id).unwrap().as_text().unwrap().size_mm,
        None
    );
    let reports: Vec<String> = reader_reports(&loaded.problems).into_iter().map(|(_, m)| m).collect();
    assert_eq!(reports.len(), 4, "{reports:#?}");
    for (report, (name, reason)) in reports.iter().zip([
        ("TX-002", r#"ifcx-field-unknown-value(key="anchorPos", value="middle")"#),
        ("TX-002", r#"ifcx-field-not-number(key="angle")"#),
        ("TX-002", r#"ifcx-field-out-of-range(key="sizeMm")"#),
        ("TX-003", r#"ifcx-field-not-number(key="sizeMm")"#),
    ]) {
        assert!(
            report.contains(&format!(r#"ifcx-record-element(name="{name}")"#)),
            "{report}"
        );
        assert!(report.contains(reason), "{report}");
    }
    assert_eq!(loaded.project.elements.len(), project.elements.len());
}
