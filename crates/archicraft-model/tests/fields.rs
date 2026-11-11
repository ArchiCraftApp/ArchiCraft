//! Declared element parameters (fields.rs): every kind lists its fields in
//! its descriptor, and the envelope's are COMMON_FIELDS (UI-002, UI-003,
//! ELM-001, ELM-017, ELM-012, ELM-020).

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::{HashMap, HashSet};

use archicraft_model::fields::{COMMON_FIELDS, FieldSpec, FieldType, FieldValue, OptionLabel, Section, fields_of};
use archicraft_model::glam::DVec2;
use archicraft_model::kinds::{Dummy, KINDS, Note};
use archicraft_model::message::ALL_KEYS;
use archicraft_model::template::default_metric_project;
use archicraft_model::validate::{MAX_COORDINATE, validate_element};
use archicraft_model::{
    AnchorPos, Element, ElementId, KindTag, Position, Project, ReferenceLine, SlabReference, StoreyId,
    StructuralFunction, TopLink,
};
use common::{ground, wall};

/// Every declared field: each kind's, then the common ones.
fn all_specs() -> impl Iterator<Item = &'static FieldSpec> {
    KINDS.iter().flat_map(|d| d.fields).chain(COMMON_FIELDS)
}

fn spec(element: &Element, key: &str) -> &'static FieldSpec {
    fields_of(element)
        .find(|s| s.key == key)
        .unwrap_or_else(|| panic!("{:?} has no field {key}", element.tag()))
}

fn get(p: &Project, element: &Element, key: &str) -> Option<FieldValue> {
    (spec(element, key).get)(element, p)
}

fn set(p: &Project, element: &mut Element, key: &str, value: FieldValue) {
    (spec(element, key).set)(element, &value, p);
}

/// One element of every kind, on the ground storey.
fn samples(p: &Project) -> Vec<Element> {
    let g = ground(p);
    let square = vec![DVec2::ZERO, DVec2::X, DVec2::ONE, DVec2::Y];
    let material = p.materials[0].id;
    vec![
        wall(p, (0.0, 0.0), (5.0, 0.0)),
        p.new_slab(g, square.clone(), Vec::new(), 0),
        p.new_space(g, square, Vec::new(), 0),
        p.new_text(g, DVec2::ZERO, "Hello", 0),
        p.new_column(g, DVec2::ONE, 0.0, 0),
        Dummy::new(DVec2::ZERO, DVec2::ONE, material).into_element(p, g, 0),
        Note::new(g, DVec2::ZERO, "Note").into_element(p, g, 0),
    ]
}

/// The samples and, for every choice of each, the sample with that option
/// picked, so that fields hidden in one mode (a fixed top's height, a custom
/// reference line's offset) are visible in another.
fn variants(p: &Project) -> Vec<Element> {
    let mut out = Vec::new();
    for sample in samples(p) {
        for spec in fields_of(&sample).filter(|s| s.field_type == FieldType::Choice) {
            if (spec.get)(&sample, p).is_none() {
                continue;
            }
            for (value, _) in (spec.options.unwrap())(p, &sample) {
                let mut e = sample.clone();
                (spec.set)(&mut e, &value, p);
                out.push(e);
            }
        }
        out.push(sample);
    }
    out
}

/// The samples cover every kind, so the tests below reach every
/// declared field.
#[test]
fn the_samples_cover_every_kind() {
    let p = default_metric_project();
    let tags: HashSet<KindTag> = samples(&p).iter().map(Element::tag).collect();
    assert_eq!(tags, KindTag::ALL.iter().copied().collect());
}

/// Asserts that `covered` holds every field of every kind, and every common
/// field for some kind, whose type `is_tested` accepts.
fn assert_every_key_covered(covered: &HashSet<(KindTag, &str)>, is_tested: impl Fn(&FieldSpec) -> bool) {
    for (tag, d) in KindTag::ALL.iter().zip(KINDS) {
        for f in d.fields.iter().filter(|f| is_tested(f)) {
            assert!(covered.contains(&(*tag, f.key)), "{} {} was not tested", d.code, f.key);
        }
    }
    for f in COMMON_FIELDS.iter().filter(|f| is_tested(f)) {
        assert!(covered.iter().any(|(_, k)| *k == f.key), "{} was not tested", f.key);
    }
}

/// UI-003: every kind shows something in 'Geometry and positioning'.
#[test]
fn every_kind_declares_a_geometry_field() {
    for d in KINDS {
        assert!(
            d.fields.iter().any(|f| f.section == Section::Geometry),
            "{} has no geometry field",
            d.code
        );
    }
}

#[test]
fn field_keys_are_unique_per_kind() {
    for d in KINDS {
        let mut keys = HashSet::new();
        for f in d.fields.iter().chain(COMMON_FIELDS) {
            assert!(keys.insert(f.key), "{}: key {} twice", d.code, f.key);
        }
    }
}

/// A key names one parameter: kinds that share it, and the common fields,
/// give it the same type and label.
#[test]
fn shared_keys_have_one_type_and_label() {
    let mut seen: HashMap<&str, (FieldType, &str)> = HashMap::new();
    for f in all_specs() {
        let first = *seen.entry(f.key).or_insert((f.field_type, f.label));
        assert_eq!(first, (f.field_type, f.label), "key {}", f.key);
    }
}

/// ELM-017, NFR-REL-005: a ranged field is numeric (a choice of counts,
/// like the linked storey, included) and names the message
/// the validator rejects other values with; labels are model message keys
/// (NFR-L10N-001).
#[test]
fn ranged_fields_explain_their_range_and_labels_are_message_keys() {
    let numeric = [
        FieldType::Length,
        FieldType::Elevation,
        FieldType::Angle,
        FieldType::Count,
        FieldType::Choice,
    ];
    for f in all_specs() {
        assert!(ALL_KEYS.contains(&f.label), "{}: label {}", f.key, f.label);
        if let Some(label) = f.defaults_label {
            assert!(ALL_KEYS.contains(&label), "{}: label {label}", f.key);
        }
        if let Some(range) = f.range {
            assert!(numeric.contains(&f.field_type), "{}: ranged {:?}", f.key, f.field_type);
            assert!(!range.message.is_empty(), "{}: no range message", f.key);
            assert!(ALL_KEYS.contains(&range.message), "{}: {}", f.key, range.message);
            assert!(range.min < range.max, "{}: empty range", f.key);
            assert_eq!(f.range_error().unwrap().key(), range.message);
        }
    }
}

/// Every choice has options, other fields none; every option's label is a
/// message key or a name, and picking it reads it back.
#[test]
fn choice_fields_have_options_that_round_trip() {
    for f in all_specs() {
        assert_eq!(f.options.is_some(), f.field_type == FieldType::Choice, "{}", f.key);
    }
    let p = default_metric_project();
    let mut covered = HashSet::new();
    for e in variants(&p) {
        for f in fields_of(&e).filter(|f| f.field_type == FieldType::Choice) {
            if (f.get)(&e, &p).is_none() {
                continue;
            }
            covered.insert((e.tag(), f.key));
            let options = (f.options.unwrap())(&p, &e);
            assert!(!options.is_empty(), "{:?} {}: no options", e.tag(), f.key);
            for (value, label) in options {
                if let OptionLabel::Message(m) = &label {
                    assert!(ALL_KEYS.contains(&m.key()), "{}: option {}", f.key, m.key());
                }
                let mut changed = e.clone();
                (f.set)(&mut changed, &value, &p);
                assert_eq!((f.get)(&changed, &p), Some(value.clone()), "{:?} {}", e.tag(), f.key);
            }
        }
    }
    assert_every_key_covered(&covered, |f| f.field_type == FieldType::Choice);
}

/// A value of the field's type inside its range, different from `current`.
fn other_value(f: &FieldSpec, current: &FieldValue) -> FieldValue {
    let pick = |v: f64| match f.range {
        Some(r) => r.min + (r.max - r.min) * 0.375,
        None => v + 0.5,
    };
    match current {
        FieldValue::Length(v) => FieldValue::Length(pick(*v)),
        FieldValue::Angle(v) => FieldValue::Angle(pick(*v)),
        FieldValue::Text(t) => FieldValue::Text(format!("{t} changed")),
        FieldValue::OptionalSize(None) => FieldValue::OptionalSize(Some(3.5)),
        FieldValue::OptionalSize(Some(_)) => FieldValue::OptionalSize(None),
        FieldValue::Bool(b) => FieldValue::Bool(!b),
        other => panic!("{}: no test value for {other:?}", f.key),
    }
}

/// UI-002: what a setter writes, the getter reads back, for every visible
/// non-choice field of every kind.
#[test]
fn setters_write_what_getters_read() {
    let p = default_metric_project();
    let mut covered = HashSet::new();
    for e in variants(&p) {
        for f in fields_of(&e).filter(|f| f.field_type != FieldType::Choice) {
            let Some(current) = (f.get)(&e, &p) else { continue };
            covered.insert((e.tag(), f.key));
            let value = other_value(f, &current);
            assert_ne!(value, current, "{}", f.key);
            let mut changed = e.clone();
            (f.set)(&mut changed, &value, &p);
            assert_eq!((f.get)(&changed, &p), Some(value), "{:?} {}", e.tag(), f.key);
        }
    }
    assert_every_key_covered(&covered, |f| f.field_type != FieldType::Choice);
}

/// A getter returning `None` hides the field: the height only for a fixed
/// top, the top storey and offset only for linked tops, the offset from
/// the exterior face only for a custom reference line.
#[test]
fn getters_hide_fields_that_do_not_apply() {
    let p = default_metric_project();
    let mut w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    let shown = |w: &Element| -> Vec<&str> {
        [
            "height",
            "top_storey",
            "linked_storey",
            "top_offset",
            "reference_offset",
        ]
        .into_iter()
        .filter(|k| get(&p, w, k).is_some())
        .collect()
    };
    // The template's wall: exterior face, top at the home storey + 1.
    assert_eq!(shown(&w), ["linked_storey", "top_offset"]);
    set(&p, &mut w, "top_mode", FieldValue::Code("fixed"));
    assert_eq!(shown(&w), ["height"]);
    set(&p, &mut w, "top_mode", FieldValue::Code("storey"));
    assert_eq!(shown(&w), ["top_storey", "top_offset"]);
    set(&p, &mut w, "reference_line", FieldValue::Code("custom"));
    assert_eq!(shown(&w), ["top_storey", "top_offset", "reference_offset"]);
    // A slab has no top or reference line, a wall no slab fields.
    let slab = &samples(&p)[1];
    assert!(fields_of(slab).all(|f| f.key != "height"));
    assert!(fields_of(&w).all(|f| f.key != "elevation_offset"));
}

/// ELM-006: a new top mode keeps the top where it is; a storey link goes to
/// the storey above the home storey, or the storey at the top if there is
/// none; a relative link is home + 1.
#[test]
fn the_top_mode_keeps_the_top_elevation() {
    let p = default_metric_project();
    let storeys: Vec<_> = p.storeys.iter().map(|s| s.id).collect();
    let top_of = |e: &Element| e.as_wall().unwrap().top;
    // On the ground storey, linked to home + 1 (01 at 3 m).
    let w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    let mut fixed = w.clone();
    set(&p, &mut fixed, "top_mode", FieldValue::Code("fixed"));
    assert_eq!(top_of(&fixed), TopLink::Fixed { height: 3.0 });
    let mut storey = w.clone();
    set(&p, &mut storey, "top_mode", FieldValue::Code("storey"));
    assert_eq!(
        top_of(&storey),
        TopLink::Storey {
            storey: storeys[1],
            offset: 0.0
        }
    );
    // On the top storey with a fixed height of 2.5 m: no storey above, so
    // the link goes to the storey at the top, 01, with the height as offset.
    let mut high = p.new_wall(storeys[1], DVec2::ZERO, DVec2::X, 0);
    set(&p, &mut high, "top_mode", FieldValue::Code("fixed"));
    set(&p, &mut high, "height", FieldValue::Length(2.5));
    set(&p, &mut high, "top_mode", FieldValue::Code("storey"));
    assert_eq!(
        top_of(&high),
        TopLink::Storey {
            storey: storeys[1],
            offset: 2.5
        }
    );
    set(&p, &mut high, "top_mode", FieldValue::Code("relative"));
    assert_eq!(
        top_of(&high),
        TopLink::Relative {
            storeys: 1,
            offset: 0.0
        }
    );
}

/// WAL-003: picking a custom reference line keeps the line where it is,
/// so the wall's faces do not move.
#[test]
fn a_custom_reference_line_keeps_the_current_offset() {
    let p = default_metric_project();
    let mut w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    set(&p, &mut w, "thickness", FieldValue::Length(0.3));
    set(&p, &mut w, "reference_line", FieldValue::Code("center"));
    let faces = w.as_wall().unwrap().reference_line.face_offsets(0.3);
    set(&p, &mut w, "reference_line", FieldValue::Code("custom"));
    let line = w.as_wall().unwrap().reference_line;
    assert_eq!(line, ReferenceLine::Custom(0.15));
    assert_eq!(line.face_offsets(0.3), faces);
}

/// ELM-017: each declared range is the one the kind's validator enforces.
/// A value at a limit passes the range check (the element may still fail a
/// later check, such as a top below its base); one just outside fails with
/// the field's range message.
#[test]
fn declared_ranges_are_the_validators_limits() {
    let p = default_metric_project();
    let mut checked = HashSet::new();
    for e in variants(&p) {
        let key = |e: &Element| validate_element(&p, e).err().map(|m| m.key());
        // Some picked options make invalid elements, such as a top linked
        // to the home storey itself.
        if key(&e).is_some() {
            continue;
        }
        for f in fields_of(&e) {
            let (Some(range), Some(current)) = (f.range, (f.get)(&e, &p)) else {
                continue;
            };
            let value = |v: f64| match current {
                FieldValue::Count(_) => FieldValue::Count(v as i64),
                _ => FieldValue::Length(v),
            };
            let step = match current {
                FieldValue::Count(_) => 1.0,
                _ => 1e-6 * range.max.abs().max(1.0),
            };
            for (limit, outside) in [(range.min, range.min - step), (range.max, range.max + step)] {
                let mut at = e.clone();
                (f.set)(&mut at, &value(limit), &p);
                assert_ne!(key(&at), Some(range.message), "{:?} {} at {limit}", e.tag(), f.key);
                assert!(f.in_range(limit));
                let mut beyond = e.clone();
                (f.set)(&mut beyond, &value(outside), &p);
                assert_eq!(
                    key(&beyond),
                    Some(range.message),
                    "{:?} {} at {outside}",
                    e.tag(),
                    f.key
                );
                assert!(!f.in_range(outside));
            }
            checked.insert((e.tag(), f.key));
        }
    }
    for expected in [
        (KindTag::Wall, "thickness"),
        (KindTag::Wall, "reference_offset"),
        (KindTag::Wall, "base_offset"),
        (KindTag::Wall, "height"),
        (KindTag::Wall, "linked_storey"),
        (KindTag::Wall, "top_offset"),
        (KindTag::Slab, "thickness"),
        (KindTag::Slab, "elevation_offset"),
        (KindTag::Dummy, "height"),
        (KindTag::Space, "base_offset"),
        (KindTag::Space, "height"),
        (KindTag::Space, "linked_storey"),
        (KindTag::Space, "top_offset"),
        (KindTag::Note, "x"),
        (KindTag::Note, "y"),
    ] {
        assert!(checked.contains(&expected), "{expected:?} was not checked");
    }
}

/// The dummy test kind declares its base and top as geometry fields, so the
/// application renders it without code of its own (UI-002, UI-003).
#[test]
fn the_dummy_kind_declares_its_base_and_top() {
    let geometry: Vec<&str> = KindTag::Dummy
        .descriptor()
        .fields
        .iter()
        .filter(|f| f.section == Section::Geometry)
        .map(|f| f.key)
        .collect();
    assert!(geometry.contains(&"base_offset") && geometry.contains(&"height"));
}

/// ELM-012, ELM-020: position and structural function are choices whose
/// codes are the envelope's codes, for every kind.
#[test]
fn position_and_structural_function_are_coded_choices() {
    let p = default_metric_project();
    let e = &samples(&p)[0];
    let codes = |key: &str| -> Vec<FieldValue> {
        let f = spec(e, key);
        assert_eq!(f.field_type, FieldType::Choice);
        (f.options.unwrap())(&p, e).into_iter().map(|(v, _)| v).collect()
    };
    assert_eq!(
        codes("position"),
        Position::ALL.map(|v| FieldValue::Code(v.code())).to_vec()
    );
    assert_eq!(
        codes("structural_function"),
        StructuralFunction::ALL.map(|v| FieldValue::Code(v.code())).to_vec()
    );
    for e in samples(&p) {
        assert_eq!(get(&p, &e, "position"), Some(FieldValue::Code(e.position.code())));
        assert_eq!(
            get(&p, &e, "structural_function"),
            Some(FieldValue::Code(e.structural.code()))
        );
    }
}

/// ELM-012, ELM-020: generic matching through the getters alone, as display
/// override criteria and preset selection will use it, finds the exterior
/// load-bearing walls and slabs among a project's elements.
#[test]
fn the_getters_select_exterior_load_bearing_elements() {
    let mut p = default_metric_project();
    let base = samples(&p);
    for (i, name, position, structural) in [
        (0, "wall A", Position::Exterior, StructuralFunction::LoadBearing),
        (0, "wall B", Position::Exterior, StructuralFunction::NonLoadBearing),
        (0, "wall C", Position::Interior, StructuralFunction::LoadBearing),
        (1, "slab D", Position::Exterior, StructuralFunction::LoadBearing),
        (1, "slab E", Position::Undefined, StructuralFunction::Undefined),
    ] {
        let e = Element {
            id: ElementId::new(),
            name: name.into(),
            position,
            structural,
            ..base[i].clone()
        };
        p.elements.insert(e.id, e);
    }
    let matches = |e: &Element, criteria: &[(&str, FieldValue)]| {
        criteria
            .iter()
            .all(|(key, value)| fields_of(e).any(|f| f.key == *key && (f.get)(e, &p).as_ref() == Some(value)))
    };
    let criteria = [
        ("position", FieldValue::Code("exterior")),
        ("structural_function", FieldValue::Code("load-bearing")),
    ];
    let selected: Vec<&str> = p
        .elements
        .values()
        .filter(|e| matches(e, &criteria))
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(selected, ["wall A", "slab D"]);
}

/// SPC-001: a space's room name is labelled 'Default name' while the space
/// tool's defaults are edited; the number and the shared base and top
/// fields follow it.
#[test]
fn a_space_declares_its_name_number_and_heights() {
    let keys: Vec<&str> = KindTag::Space.descriptor().fields.iter().map(|f| f.key).collect();
    assert_eq!(
        keys,
        [
            "long_name",
            "number",
            "base_offset",
            "top_mode",
            "height",
            "top_storey",
            "linked_storey",
            "top_offset"
        ]
    );
    let name = &KindTag::Space.descriptor().fields[0];
    assert_eq!(
        (name.section, name.label, name.defaults_label),
        (Section::Kind, "model-field-room-name", Some("model-field-default-name"))
    );
    assert!(KindTag::Space.descriptor().fields.iter().all(|f| !f.placed_only));
    let height = KindTag::Space
        .descriptor()
        .fields
        .iter()
        .find(|f| f.key == "height")
        .unwrap();
    assert_eq!(height.range.unwrap().message, "model-error-space-height-range");
}

/// ANN-007: a text's content is several lines and shown for placed texts
/// only; the size is optional; the anchor position offers the nine
/// positions with their names; anchor and angle are geometry.
#[test]
fn a_text_declares_content_style_size_anchor_and_angle() {
    let p = default_metric_project();
    let text = &samples(&p)[3];
    let fields = KindTag::Text.descriptor().fields;
    let f = |key: &str| fields.iter().find(|f| f.key == key).unwrap();
    assert_eq!(
        fields.iter().map(|f| f.key).collect::<Vec<_>>(),
        ["content", "text_style", "size", "anchor_position", "angle"]
    );
    assert_eq!(f("content").field_type, FieldType::MultilineText);
    assert!(f("content").placed_only);
    assert!(fields.iter().filter(|f| f.key != "content").all(|f| !f.placed_only));
    assert_eq!(f("size").field_type, FieldType::OptionalSize);
    assert_eq!(f("anchor_position").section, Section::Geometry);
    assert_eq!(
        (f("angle").section, f("angle").field_type),
        (Section::Geometry, FieldType::Angle)
    );
    let anchors = (f("anchor_position").options.unwrap())(&p, text);
    let expected: Vec<(FieldValue, &str)> = AnchorPos::ALL
        .iter()
        .map(|a| (FieldValue::Code(a.code()), a.label_key()))
        .collect();
    let actual: Vec<(FieldValue, &str)> = anchors
        .into_iter()
        .map(|(v, l)| match l {
            OptionLabel::Message(m) => (v, m.key()),
            OptionLabel::Name(n) => panic!("anchor named {n}"),
        })
        .collect();
    assert_eq!(actual, expected);
    let styles = (f("text_style").options.unwrap())(&p, text);
    assert_eq!(styles.len(), p.text_styles.len());
}

/// ELM-017: the note test kind's point is ranged as the validator checks
/// coordinates.
#[test]
fn the_note_kind_declares_its_point() {
    for f in KindTag::Note.descriptor().fields {
        assert_eq!(f.section, Section::Geometry);
        let range = f.range.unwrap();
        assert_eq!((range.min, range.max), (-MAX_COORDINATE, MAX_COORDINATE));
    }
}

/// WAL-003, ELM-006, SLB-001: the enumerations' codes read back through
/// their `from_code`, and a custom reference line takes the given offset.
#[test]
fn enumeration_codes_read_back() {
    for line in ReferenceLine::ALL {
        let expected = match line {
            ReferenceLine::Custom(_) => ReferenceLine::Custom(0.25),
            other => other,
        };
        assert_eq!(ReferenceLine::from_code(line.code(), 0.25), Some(expected));
    }
    assert_eq!(ReferenceLine::from_code("diagonal", 0.0), None);
    for r in SlabReference::ALL {
        assert_eq!(SlabReference::from_code(r.code()), Some(r));
    }
    let tops = [
        TopLink::Fixed { height: 3.0 },
        TopLink::Storey {
            storey: StoreyId::new(),
            offset: 0.0,
        },
        TopLink::Relative {
            storeys: 1,
            offset: 0.0,
        },
    ];
    assert_eq!(tops.map(TopLink::code), TopLink::CODES);
}
