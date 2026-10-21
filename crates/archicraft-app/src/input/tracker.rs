//! The tracker next to the cursor (INP-002): one line per field of the
//! active field set, with the live value or the typed text.

use archicraft_model::glam::DVec2;
use archicraft_model::units::{format_angle, format_length};

use super::InputContext;
use super::typed::{Field, Reference, TypedInput};

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

/// The tracker lines for `point`, measured from `base` (INP-002, INP-004):
/// polar fields show the distance and the angle from `base_angle`,
/// Cartesian fields the coordinate differences, or the coordinates in
/// absolute input. Z is the elevation of the working plane at the point;
/// ΔZ is 0 in plan views.
pub fn lines(typed: &TypedInput, base: DVec2, base_angle: f64, point: DVec2, ctx: &InputContext) -> Vec<TrackerLine> {
    let offset = point - base;
    let length = |v: f64| format_length(v, ctx.units, ctx.number_format, true);
    typed
        .set
        .fields()
        .iter()
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
                }
            };
            TrackerLine {
                field,
                label: field.label(typed.reference),
                value,
                active: typed.field == field,
            }
        })
        .collect()
}

/// The tracker text: one line per field, the active field marked with ▸.
pub fn text(lines: &[TrackerLine]) -> String {
    lines
        .iter()
        .map(|l| format!("{}{} {}", if l.active { "▸" } else { " " }, l.label, l.value))
        .collect::<Vec<_>>()
        .join("\n")
}
