//! The tracker next to the cursor (INP-002): one line per field of the
//! active field set, with the live value or the typed text, and on demand
//! the absolute X, Y and Z.

use archicraft_model::glam::{DVec2, DVec3};
use archicraft_model::units::{format_angle, format_length};

use super::InputContext;
use super::surface::Frame2d;
use super::typed::{Field, FieldSet, Origin, Reference, TypedInput};

/// One tracker line.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackerLine {
    pub field: Field,
    pub label: String,
    /// The typed text followed by `_`, or the live value in working units.
    pub value: String,
    /// The keyboard edits this field.
    pub active: bool,
}

/// The tracker lines for `point`, measured in the frame `base` (INP-002,
/// INP-004): polar fields show the distance and the angle from `base_angle`,
/// Cartesian fields the coordinate differences, or the coordinates in
/// absolute input, along the user origin's axes when measured from it
/// (INP-014). Z is the elevation of the working plane at the point;
/// ΔZ is 0 in plan views. A stretch shows its active field only, the total
/// length by default and the length change on demand, with the point
/// projected onto the stretch direction. With the X/Y/Z preference on, the
/// absolute X, Y and Z follow, from the user origin once placed, otherwise
/// from the project origin, unless the fields already are those.
pub fn lines(typed: &TypedInput, base: Frame2d, base_angle: f64, point: DVec2, ctx: &InputContext) -> Vec<TrackerLine> {
    let offset = base.to_local(point);
    let length = |v: f64| format_length(v, ctx.units, ctx.number_format, true);
    let total = || {
        ctx.tracker
            .stretch()
            .map_or(offset.length(), |(base, dir, _)| (point - base).dot(dir))
    };
    let stretch = matches!(typed.set, FieldSet::Stretch { .. });
    let mut lines: Vec<TrackerLine> = typed
        .set
        .fields()
        .iter()
        .filter(|&&field| !stretch || field == typed.field)
        .map(|&field| {
            let text = typed.text(field);
            let value = if !text.is_empty() {
                format!("{text}_")
            } else {
                match field {
                    Field::Distance => length(offset.length()),
                    Field::Angle => {
                        let a = offset.to_angle() - base_angle;
                        format_angle(a.sin().atan2(a.cos()), ctx.units, ctx.number_format, true)
                    }
                    Field::X => length(offset.x),
                    Field::Y => length(offset.y),
                    Field::Z if typed.reference == Reference::Relative => length(0.0),
                    Field::Z => length(ctx.frame.to_model(point).z),
                    Field::Total => length(total()),
                    Field::Change => length(total() - ctx.tracker.stretch().map_or(0.0, |(_, _, l)| l)),
                }
            };
            TrackerLine {
                field,
                label: field.label(typed.reference),
                value,
                active: typed.field == field,
            }
        })
        .collect();
    let absolute = Reference::Absolute(ctx.absolute());
    let shown = matches!(typed.set, FieldSet::Cartesian { .. }) && typed.reference == absolute;
    if ctx.tracker_xyz && !shown {
        let xy = match ctx.absolute() {
            Origin::User => ctx.user_origin.to_local(point),
            _ => point - ctx.frame.to_plane(DVec3::ZERO),
        };
        let z = ctx.frame.to_model(point).z;
        lines.extend(
            [(Field::X, xy.x), (Field::Y, xy.y), (Field::Z, z)].map(|(field, v)| TrackerLine {
                field,
                label: field.label(absolute),
                value: length(v),
                active: false,
            }),
        );
    }
    lines
}

/// The tracker text: one line per field, the active field marked with ▸.
pub fn text(lines: &[TrackerLine]) -> String {
    lines
        .iter()
        .map(|l| format!("{}{} {}", if l.active { "▸" } else { " " }, l.label, l.value))
        .collect::<Vec<_>>()
        .join("\n")
}
