//! Transfer of settings between elements and tool defaults (ELM-003,
//! ELM-002).
//!
//! The [`FieldSpec`]s are the transfer channel: a target takes every field
//! of its kind whose key and [`FieldType`] the source's kind also declares,
//! whose value the source shows and whose value the target accepts. So a
//! wall gives a slab its thickness, material, layer and position but not its
//! top link, and a kind registered later transfers correctly without a table
//! per pair of kinds (the key vocabulary is in [`crate::fields`]). Fields
//! marked [`Transfer::Never`], an element's own identity and placement, stay.
//!
//! The commands [`Command::InjectSettings`](crate::Command::InjectSettings)
//! and [`Command::PickUpSettings`](crate::Command::PickUpSettings) run the
//! transfer into elements and into a tool's defaults slot.

use serde::{Deserialize, Serialize};

use crate::element::{Element, KindTag};
use crate::fields::{FieldSpec, FieldType, FieldValue, Transfer, fields_of};
use crate::ids::ElementId;
use crate::kinds::VERTICAL_FIELD_KEYS;
use crate::message::Message;
use crate::project::Project;
use crate::validate;

/// Where transferred settings come from (ELM-003, ELM-002).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingsSource {
    /// A placed element.
    Element(ElementId),
    /// The defaults slot of a kind's tool (ELM-001).
    Defaults(KindTag),
}

/// Why a field of the source was not transferred.
#[derive(Clone, Debug, PartialEq)]
pub enum SkipReason {
    /// The target's kind has no field with the key and type.
    Absent,
    /// The target has the field but not in its state: the field qualifies
    /// another one that was not transferred ([`Transfer::With`]), the target
    /// hides it (the height of a top it keeps linked to a storey) or has no
    /// value for it (the material of a kind without one).
    Inapplicable,
    /// The target does not offer the value, such as `IfcWall` for a slab.
    NotOffered,
    /// The target's range rejects the value, or its validator the result
    /// with the value (for a base or top field, with the other base and top
    /// fields), with the reason (NFR-REL-005).
    Invalid(Message),
}

/// What [`transfer_settings`] did, by field key: the fields it applied, in
/// the target's field order, and the fields the source shows that it did not
/// apply, with the reason.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TransferReport {
    pub applied: Vec<&'static str>,
    pub skipped: Vec<(&'static str, SkipReason)>,
}

/// Whether `a` and `b` are the same parameter: same key, same type
/// (ELM-003).
fn same_parameter(a: &FieldSpec, b: &FieldSpec) -> bool {
    a.key == b.key && a.field_type == b.field_type
}

/// The number a numeric field value stands for, for its range.
fn number(value: &FieldValue) -> Option<f64> {
    match value {
        FieldValue::Length(v) | FieldValue::Angle(v) => Some(*v),
        FieldValue::Count(n) => Some(*n as f64),
        FieldValue::OptionalSize(v) => *v,
        _ => None,
    }
}

/// Copies the compatible settings of `source` into `target` (ELM-003):
/// in the target's field order, every transferable field whose key and type
/// the source's kind also has, that the source shows, and whose value the
/// target accepts. A value the target rejects is skipped and reported,
/// instead of failing the whole transfer, so that a mixed set of targets
/// gets everything that fits.
///
/// Each field is applied through the target's setter, which sees the
/// target's storey context (a top mode keeps the target's top until its
/// height or storey follows), after checking it against the target's
/// options and range. The target's validator runs once, at the end, because
/// the fields depend on each other (a base offset is valid only with the
/// top that follows it): as a placed element
/// ([`validate::validate_element`]) if `target` is in `project`, else as a
/// tool's prototype ([`validate::validate_prototype`]). When it rejects the
/// result, the base and top fields, which together set the vertical extent,
/// stay as the target had them and are reported with the reason; when it
/// still rejects that, `target` stays as it was and every field is reported.
/// `target` ends normalised by the validator. Its identity, home storey and
/// geometry, which no transferable field holds, stay unchanged.
pub fn transfer_settings(source: &Element, target: &mut Element, project: &Project) -> TransferReport {
    let check = |e: &Element| {
        if project.element(e.id).is_some() {
            validate::validate_element(project, e)
        } else {
            validate::validate_prototype(project, e)
        }
    };
    let (element, report) = apply_fields(source, target, project, &[], None);
    let reason = match check(&element) {
        Ok(element) => {
            *target = element;
            return report;
        }
        Err(reason) => reason,
    };
    let (element, kept) = apply_fields(source, target, project, &VERTICAL_FIELD_KEYS, Some(&reason));
    if let Ok(element) = check(&element) {
        *target = element;
        return kept;
    }
    // Nothing fits together: the target stays as it was.
    let mut skipped: Vec<_> = report
        .applied
        .iter()
        .map(|key| (*key, SkipReason::Invalid(reason.clone())))
        .collect();
    skipped.extend(report.skipped);
    TransferReport {
        applied: Vec::new(),
        skipped,
    }
}

/// `target` with the compatible fields of `source` applied, without the
/// target's validator, and the report. The fields `kept` stay as the target
/// has them and are reported as skipped for `reason`.
fn apply_fields(
    source: &Element,
    target: &Element,
    project: &Project,
    kept: &[&str],
    reason: Option<&Message>,
) -> (Element, TransferReport) {
    let transferable = |spec: &&FieldSpec| spec.transfer != Transfer::Never;
    let source_fields: Vec<&FieldSpec> = fields_of(source).filter(transferable).collect();
    let target_fields: Vec<&FieldSpec> = fields_of(target).filter(transferable).collect();
    let mut element = target.clone();
    let mut report = TransferReport::default();
    for spec in &target_fields {
        let Some(from) = source_fields.iter().find(|s| same_parameter(s, spec)) else {
            continue;
        };
        // A field the source hides does not apply to it.
        let Some(value) = (from.get)(source, project) else {
            continue;
        };
        if let Some(reason) = reason
            && kept.contains(&spec.key)
        {
            report.skipped.push((spec.key, SkipReason::Invalid(reason.clone())));
            continue;
        }
        if let Transfer::With(key) = spec.transfer
            && !report.applied.contains(&key)
        {
            report.skipped.push((spec.key, SkipReason::Inapplicable));
            continue;
        }
        match accept(spec, &value, &element, project) {
            Ok(changed) => {
                element = changed;
                report.applied.push(spec.key);
            }
            Err(reason) => report.skipped.push((spec.key, reason)),
        }
    }
    for spec in &source_fields {
        if !target_fields.iter().any(|t| same_parameter(t, spec)) && (spec.get)(source, project).is_some() {
            report.skipped.push((spec.key, SkipReason::Absent));
        }
    }
    (element, report)
}

/// `target` with the field `spec` set to `value`, or why the target does
/// not accept it: the field does not apply to it, its options or range
/// exclude the value, or its setter cannot hold it.
fn accept(spec: &FieldSpec, value: &FieldValue, target: &Element, project: &Project) -> Result<Element, SkipReason> {
    if (spec.get)(target, project).is_none() {
        return Err(SkipReason::Inapplicable);
    }
    if let Some(options) = spec.options
        && !options(project, target).iter().any(|(option, _)| option == value)
    {
        return Err(SkipReason::NotOffered);
    }
    if let Some(n) = number(value)
        && !spec.in_range(n)
        && let Some(reason) = spec.range_error()
    {
        return Err(SkipReason::Invalid(reason));
    }
    let mut candidate = target.clone();
    (spec.set)(&mut candidate, value, project);
    // A setter that cannot hold the value leaves another one.
    if (spec.get)(&candidate, project).as_ref() != Some(value) {
        return Err(if spec.field_type == FieldType::Choice {
            SkipReason::NotOffered
        } else {
            SkipReason::Inapplicable
        });
    }
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use glam::DVec2;

    use super::*;
    use crate::element::{IfcClass, Position, PredefinedType, StructuralFunction, TopLink};
    use crate::kinds::ReferenceLine;
    use crate::template::default_metric_project;

    /// A project with a 0.30 m centre-lined footing wall on the interior
    /// walls layer, of the second material, whose base is 0.10 m up and
    /// whose top is linked to the first floor + 0.20 m, and a default wall
    /// and a default slab to transfer into.
    fn project() -> (Project, Element, Element, Element) {
        let mut p = default_metric_project();
        let ground = p.storeys.ground();
        let first = p.storeys.relative(ground, 1).unwrap().id;
        let mut source = p.new_wall(ground, DVec2::ZERO, DVec2::new(4.0, 0.0), 0);
        let wall = source.as_wall_mut().unwrap();
        wall.thickness = 0.30;
        wall.reference_line = ReferenceLine::Center;
        wall.base_offset = 0.10;
        wall.top = TopLink::Storey {
            storey: first,
            offset: 0.20,
        };
        wall.material = p.materials[1].id;
        source.layer = p.layers.iter().find(|l| l.name == "Interior walls").unwrap().id;
        source.position = Position::Interior;
        source.structural = StructuralFunction::LoadBearing;
        source.ifc_class = IfcClass::Footing;
        source.predefined_type = Some(PredefinedType::StripFooting);
        let target = p.new_wall(ground, DVec2::new(0.0, 3.0), DVec2::new(2.0, 5.0), 1);
        let slab = p.new_slab(ground, vec![DVec2::ZERO, DVec2::X, DVec2::ONE, DVec2::Y], Vec::new(), 0);
        for e in [&source, &target, &slab] {
            p.elements.insert(e.id, e.clone());
        }
        (p, source, target, slab)
    }

    /// ELM-003: wall to wall copies thickness, reference line location,
    /// base offset, top link and offset, material, layer, position,
    /// structural function and IFC class with its predefined type, and
    /// keeps the target's reference line points, name and home storey.
    #[test]
    fn wall_to_wall_copies_every_setting_but_identity_and_geometry() {
        let (p, source, before, _) = project();
        let mut target = before.clone();
        let report = transfer_settings(&source, &mut target, &p);
        assert_eq!(
            report.applied,
            [
                "thickness",
                "reference_line",
                "base_offset",
                "top_mode",
                "top_storey",
                "top_offset",
                "material",
                "layer",
                "ifc_class",
                "predefined_type",
                "position",
                "structural_function",
            ]
        );
        assert_eq!(report.skipped, []);
        let (s, t) = (source.as_wall().unwrap(), target.as_wall().unwrap());
        assert_eq!(t.thickness, s.thickness);
        assert_eq!(t.reference_line, s.reference_line);
        assert_eq!(t.base_offset, s.base_offset);
        assert_eq!(t.top, s.top);
        assert_eq!(t.material, s.material);
        assert_eq!(
            (
                target.layer,
                target.position,
                target.structural,
                target.ifc_class,
                target.predefined_type
            ),
            (
                source.layer,
                source.position,
                source.structural,
                source.ifc_class,
                source.predefined_type
            )
        );
        let b = before.as_wall().unwrap();
        assert_eq!((t.start, t.end), (b.start, b.end));
        assert_eq!(
            (&target.name, target.storey, target.id),
            (&before.name, before.storey, before.id)
        );
    }

    /// ELM-003: a fixed height and a custom reference line offset follow the
    /// top mode and the location they belong to.
    #[test]
    fn wall_to_wall_copies_a_fixed_height_and_a_custom_offset() {
        let (p, mut source, mut target, _) = project();
        let wall = source.as_wall_mut().unwrap();
        wall.top = TopLink::Fixed { height: 2.4 };
        wall.reference_line = ReferenceLine::Custom(0.05);
        let report = transfer_settings(&source, &mut target, &p);
        for key in ["reference_line", "reference_offset", "top_mode", "height"] {
            assert!(report.applied.contains(&key), "{key} in {report:?}");
        }
        let t = target.as_wall().unwrap();
        assert_eq!(t.top, TopLink::Fixed { height: 2.4 });
        assert_eq!(t.reference_line, ReferenceLine::Custom(0.05));
    }

    /// ELM-003: wall to slab copies only the compatible fields; the slab
    /// does not offer IfcWall, so the class is skipped, and the wall-only
    /// fields are absent from the slab. IfcWall has no predefined types, so
    /// the wall shows none to transfer.
    #[test]
    fn wall_to_slab_copies_only_the_compatible_fields() {
        let (p, mut source, _, before) = project();
        source.ifc_class = IfcClass::Wall;
        source.predefined_type = None;
        let mut slab = before.clone();
        let report = transfer_settings(&source, &mut slab, &p);
        assert_eq!(
            report.applied,
            ["thickness", "material", "layer", "position", "structural_function"]
        );
        assert_eq!(
            report.skipped,
            [
                ("ifc_class", SkipReason::NotOffered),
                ("reference_line", SkipReason::Absent),
                ("base_offset", SkipReason::Absent),
                ("top_mode", SkipReason::Absent),
                ("top_storey", SkipReason::Absent),
                ("top_offset", SkipReason::Absent),
            ]
        );
        let s = slab.as_slab().unwrap();
        assert_eq!(s.thickness, 0.30);
        assert_eq!(s.material, p.materials[1].id);
        assert_eq!(s.outline, before.as_slab().unwrap().outline);
        assert_eq!((slab.layer, slab.position), (source.layer, Position::Interior));
        assert_eq!(slab.ifc_class, IfcClass::Slab);
    }

    /// ELM-003: a class both kinds offer transfers between kinds, with the
    /// predefined type that qualifies it.
    #[test]
    fn a_shared_class_transfers_between_kinds() {
        let (p, source, _, mut slab) = project();
        let report = transfer_settings(&source, &mut slab, &p);
        assert!(report.applied.contains(&"ifc_class"));
        assert!(report.applied.contains(&"predefined_type"));
        assert_eq!(
            (slab.ifc_class, slab.predefined_type),
            (IfcClass::Footing, Some(PredefinedType::StripFooting))
        );
    }

    /// ELM-003: a footing wall gives a space its base and top, which both
    /// kinds share, but not its class, which spaces do not offer, nor the
    /// predefined type that qualifies the class, nor a material, which
    /// spaces lack.
    #[test]
    fn wall_to_space_copies_the_shared_base_and_top() {
        let (p, source, _, _) = project();
        let mut space = p.new_element(KindTag::Space, p.storeys.ground(), 0).unwrap();
        let report = transfer_settings(&source, &mut space, &p);
        for key in [
            "base_offset",
            "top_mode",
            "top_storey",
            "top_offset",
            "layer",
            "position",
        ] {
            assert!(report.applied.contains(&key), "{key} in {report:?}");
        }
        for (key, reason) in [
            ("material", SkipReason::Inapplicable),
            ("ifc_class", SkipReason::NotOffered),
            ("predefined_type", SkipReason::Inapplicable),
            ("thickness", SkipReason::Absent),
        ] {
            assert!(
                report.skipped.contains(&(key, reason.clone())),
                "{key} {reason:?} in {report:?}"
            );
        }
        assert_eq!(space.as_space().unwrap().link, source.as_wall().unwrap().link());
        assert_eq!(space.ifc_class, IfcClass::Space);
    }

    /// ELM-003: a storey the target cannot link to is not offered, and the
    /// rest still transfers.
    #[test]
    fn a_rejected_value_is_skipped_and_the_rest_transfers() {
        let (mut p, source, mut target, _) = project();
        // The source's top storey is gone: the target cannot link to it.
        let first = p.storeys.relative(p.storeys.ground(), 1).unwrap().id;
        p.storeys = p.storeys.remove(first).unwrap();
        let report = transfer_settings(&source, &mut target, &p);
        assert!(
            report.skipped.contains(&("top_storey", SkipReason::NotOffered)),
            "{report:?}"
        );
        assert!(report.applied.contains(&"thickness"));
        assert_eq!(target.as_wall().unwrap().thickness, 0.30);
    }

    /// The default target wall of [`project`] after taking the source's
    /// base offset `base` and top `top`, and the report.
    fn transfer_vertical(base: f64, top: TopLink) -> (Element, TransferReport) {
        let (p, mut source, mut target, _) = project();
        let wall = source.as_wall_mut().unwrap();
        (wall.base_offset, wall.top) = (base, top);
        let report = transfer_settings(&source, &mut target, &p);
        (target, report)
    }

    /// ELM-003: the base and top are validated together, after all of them
    /// are applied, so a top on the home storey and a base above the
    /// target's top transfer although the steps between are invalid.
    #[test]
    fn the_base_and_top_transfer_together() {
        let top = TopLink::Relative {
            storeys: 0,
            offset: 1.0,
        };
        let (target, report) = transfer_vertical(0.0, top);
        assert_eq!(target.as_wall().unwrap().top, top, "{report:?}");
        assert!(report.applied.contains(&"linked_storey"));

        let (target, report) = transfer_vertical(3.0, TopLink::Fixed { height: 1.0 });
        let wall = target.as_wall().unwrap();
        assert_eq!(
            (wall.base_offset, wall.top),
            (3.0, TopLink::Fixed { height: 1.0 }),
            "{report:?}"
        );
        assert_eq!(report.skipped, []);
    }

    /// ELM-003, NFR-REL-005: a top the placed target cannot have, linked to
    /// a storey below its home storey, is skipped with the validator's reason
    /// together with the base, and the other settings still transfer.
    #[test]
    fn a_top_below_the_targets_base_is_skipped_with_the_reason() {
        let (p, mut source, _, _) = project();
        let ground = p.storeys.ground();
        source.as_wall_mut().unwrap().top = TopLink::Storey {
            storey: ground,
            offset: 1.0,
        };
        let first = p.storeys.relative(ground, 1).unwrap().id;
        let mut p = p;
        let upstairs = p.new_wall(first, DVec2::ZERO, DVec2::X, 2);
        p.elements.insert(upstairs.id, upstairs.clone());
        let mut target = upstairs.clone();
        let report = transfer_settings(&source, &mut target, &p);
        let reason = SkipReason::Invalid(Message::new("model-error-top-below-base"));
        for key in ["base_offset", "top_mode", "top_storey", "top_offset"] {
            assert!(report.skipped.contains(&(key, reason.clone())), "{key} in {report:?}");
        }
        assert!(report.applied.contains(&"thickness"));
        let (t, u) = (target.as_wall().unwrap(), upstairs.as_wall().unwrap());
        assert_eq!((t.base_offset, t.top), (u.base_offset, u.top));
        assert_eq!(t.thickness, 0.30);
    }
}
