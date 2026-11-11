//! Input widgets that edit model values with units (INP-005, ELM-017).

use archicraft_model::units::{
    AngleUnit, AreaUnit, LengthUnit, NumberFormat, SlopeFormat, UnitSettings, VolumeUnit, format_angle, format_length,
    format_slope, parse_angle, parse_length, parse_slope,
};
use egui::emath::Numeric;
use egui::{DragValue, Id, Response, Slider, TextEdit, Ui};
use std::ops::RangeInclusive;

use crate::tr;

/// The result of an edit widget.
#[derive(Clone, Debug, PartialEq)]
pub enum Edited<T> {
    Unchanged,
    Changed(T),
    Invalid(String),
}

/// How a [`committed`] field behaves beyond the defaults of
/// [`committed_text`].
#[derive(Clone, Copy, Default)]
struct FieldKind<'a> {
    /// Several lines: Enter starts a new line, Ctrl/Cmd+Enter commits.
    multiline: bool,
    /// Clearing the field commits the empty text instead of keeping the
    /// value.
    empty_commits: bool,
    /// Shown in an empty field that is not a mixed value.
    hint: &'a str,
}

/// A text field that keeps the user's text while it has focus and commits
/// it when focus is lost or Enter is pressed. `None` shows a mixed value
/// (UI-002).
fn committed_text<T>(
    ui: &mut Ui,
    id: Id,
    shown: Option<String>,
    width: f32,
    parse: impl FnOnce(&str) -> Result<T, String>,
) -> (Response, Edited<T>) {
    committed(ui, id, shown, width, FieldKind::default(), parse)
}

/// [`committed_text`] as `kind` says.
fn committed<T>(
    ui: &mut Ui,
    id: Id,
    shown: Option<String>,
    width: f32,
    kind: FieldKind<'_>,
    parse: impl FnOnce(&str) -> Result<T, String>,
) -> (Response, Edited<T>) {
    let mut text: String = ui
        .data_mut(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| shown.clone().unwrap_or_default());
    // Ctrl/Cmd+Enter ends a multi-line edit; taken before the field sees it.
    let commit_key = kind.multiline
        && ui.memory(|m| m.has_focus(id))
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter));
    let edit = if kind.multiline {
        TextEdit::multiline(&mut text).desired_rows(1)
    } else {
        TextEdit::singleline(&mut text)
    };
    let response =
        ui.add(
            edit.id(id)
                .desired_width(width)
                .hint_text(if shown.is_none() { "Mixed" } else { kind.hint }),
        );
    if commit_key {
        response.surrender_focus();
    }
    if response.gained_focus() {
        // Select the whole value, so that typing replaces it.
        let mut state = TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
        state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::default(),
            egui::text::CCursor::new(text.chars().count()),
        )));
        state.store(ui.ctx(), id);
    }
    let mut result = Edited::Unchanged;
    if response.has_focus() {
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            ui.data_mut(|d| d.remove::<String>(id));
            response.surrender_focus();
        } else {
            ui.data_mut(|d| d.insert_temp(id, text.clone()));
        }
    } else {
        ui.data_mut(|d| d.remove::<String>(id));
        if (response.lost_focus() || commit_key)
            && Some(&text) != shown.as_ref()
            // A mixed field left empty keeps the values, also where clearing
            // commits.
            && ((kind.empty_commits && shown.is_some()) || !text.trim().is_empty())
        {
            result = match parse(&text) {
                Ok(v) => Edited::Changed(v),
                Err(e) => Edited::Invalid(e),
            };
        }
    }
    (response, result)
}

/// Edits a length in the project's units.
pub fn length(
    ui: &mut Ui,
    id: Id,
    value: Option<f64>,
    units: &UnitSettings,
    nf: &NumberFormat,
    width: f32,
) -> Edited<f64> {
    let shown = value.map(|v| format_length(v, units, nf, false));
    let (response, result) = committed_text(ui, id, shown, width, |text| {
        parse_length(text, units.length, nf).map_err(|e| crate::i18n::render(&e.message(), units, nf))
    });
    response.on_hover_text("Type a length; units such as mm, cm, m, ft and in are accepted");
    result
}

/// Edits an angle in the project's units.
pub fn angle(
    ui: &mut Ui,
    id: Id,
    value: Option<f64>,
    units: &UnitSettings,
    nf: &NumberFormat,
    width: f32,
) -> Edited<f64> {
    let shown = value.map(|v| format_angle(v, units, nf, false));
    let (_, result) = committed_text(ui, id, shown, width, |text| {
        parse_angle(text, units.angle, nf).map_err(|e| crate::i18n::render(&e.message(), units, nf))
    });
    result
}

/// Edits a slope, a pitch angle in radians, in the project's slope format
/// (STR-009, NFR-L10N-003). The value is shown with its unit, so that it reads
/// back in the format; any slope notation can be typed ([`parse_slope`]).
pub fn slope(
    ui: &mut Ui,
    id: Id,
    value: Option<f64>,
    units: &UnitSettings,
    nf: &NumberFormat,
    width: f32,
) -> Edited<f64> {
    let shown = value.map(|v| format_slope(v, units, nf, true));
    let (_, result) = committed_text(ui, id, shown, width, |text| {
        parse_slope(text, units.slope, nf).map_err(|e| crate::i18n::render(&e.message(), units, nf))
    });
    result
}

/// The pitch of the live sample next to the slope format (STR-009).
const SAMPLE_PITCH_DEGREES: f64 = 30.0;

/// The unit rows of a unit settings editor (STR-009): length unit and
/// decimals, angle unit and decimals, slope format with a live sample of a
/// 30° pitch, area unit and volume unit. Adds label and value rows to the
/// caller's two-column [`egui::Grid`], so that the Project settings and the
/// default units of new projects share it.
pub fn unit_settings_editor(ui: &mut Ui, units: &mut UnitSettings, nf: &NumberFormat) {
    fn options<T: Copy>(all: &[T], key: impl Fn(T) -> &'static str) -> Vec<(T, String)> {
        all.iter().map(|u| (*u, tr!(key(*u)))).collect()
    }
    ui.label(tr!("dlg-units-length-unit"));
    if let Some(u) = choice(
        ui,
        Id::new("length-unit"),
        Some(&units.length),
        &options(&LengthUnit::ALL, LengthUnit::label_key),
        160.0,
    ) {
        units.length = u;
    }
    ui.end_row();
    ui.label(tr!("dlg-units-length-decimals"));
    integer(ui, &mut units.length_decimals, 0..=6, nf);
    ui.end_row();
    ui.label(tr!("dlg-units-angle-unit"));
    if let Some(u) = choice(
        ui,
        Id::new("angle-unit"),
        Some(&units.angle),
        &options(&AngleUnit::ALL, AngleUnit::label_key),
        160.0,
    ) {
        units.angle = u;
    }
    ui.end_row();
    ui.label(tr!("dlg-units-angle-decimals"));
    integer(ui, &mut units.angle_decimals, 0..=6, nf);
    ui.end_row();
    ui.label(tr!("dlg-units-slope-format"));
    ui.horizontal(|ui| {
        if let Some(f) = choice(
            ui,
            Id::new("slope-format"),
            Some(&units.slope),
            &options(&SlopeFormat::ALL, SlopeFormat::label_key),
            160.0,
        ) {
            units.slope = f;
        }
        let sample = format_slope(SAMPLE_PITCH_DEGREES.to_radians(), units, nf, true);
        ui.weak(tr!("dlg-units-slope-sample", sample = sample));
    });
    ui.end_row();
    ui.label(tr!("dlg-units-area-unit"));
    if let Some(u) = choice(
        ui,
        Id::new("area-unit"),
        Some(&units.area),
        &options(&AreaUnit::ALL, AreaUnit::label_key),
        160.0,
    ) {
        units.area = u;
    }
    ui.end_row();
    ui.label(tr!("dlg-units-volume-unit"));
    if let Some(u) = choice(
        ui,
        Id::new("volume-unit"),
        Some(&units.volume),
        &options(&VolumeUnit::ALL, VolumeUnit::label_key),
        160.0,
    ) {
        units.volume = u;
    }
    ui.end_row();
}

/// A draggable whole number in `range`, shown and typed with `nf`
/// (NFR-L10N-004, STR-010). Typed text with a fraction is rejected and keeps
/// the old value.
pub fn integer<T: Numeric>(ui: &mut Ui, value: &mut T, range: RangeInclusive<T>, nf: &NumberFormat) -> Response {
    ui.add(
        DragValue::new(value)
            .range(range)
            .custom_formatter(|v, _| nf.format_decimal(v, 0))
            .custom_parser(|text| nf.parse_decimal(text).filter(|v| v.fract() == 0.0)),
    )
}

/// A draggable decimal number in `range` with at most `decimals` decimals,
/// shown and typed with `nf` (NFR-L10N-004, STR-010). `speed` is the change
/// per dragged point.
pub fn decimal<T: Numeric>(
    ui: &mut Ui,
    value: &mut T,
    range: RangeInclusive<T>,
    decimals: usize,
    speed: f64,
    nf: &NumberFormat,
) -> Response {
    ui.add(
        DragValue::new(value)
            .range(range)
            .speed(speed)
            .max_decimals(decimals)
            .custom_formatter(|v, _| format_trimmed(nf, v, decimals))
            .custom_parser(|text| nf.parse_decimal(text)),
    )
}

/// A slider over `range` whose value is shown and typed with `nf` with at
/// most `decimals` decimals (NFR-L10N-004).
pub fn slider(
    ui: &mut Ui,
    value: &mut f64,
    range: RangeInclusive<f64>,
    decimals: usize,
    nf: &NumberFormat,
) -> Response {
    ui.add(
        Slider::new(value, range)
            .max_decimals(decimals)
            .custom_formatter(|v, _| format_trimmed(nf, v, decimals))
            .custom_parser(|text| nf.parse_decimal(text)),
    )
}

/// `value` with `decimals` decimals, without trailing zeros in the fraction.
pub fn format_trimmed(nf: &NumberFormat, value: f64, decimals: usize) -> String {
    let text = nf.format_decimal(value, decimals);
    if decimals == 0 {
        return text;
    }
    text.trim_end_matches('0')
        .trim_end_matches(nf.decimal_separator)
        .to_owned()
}

/// Edits a single line of text, committed on Enter or focus loss.
pub fn text(ui: &mut Ui, id: Id, value: Option<&str>, width: f32) -> Edited<String> {
    let (_, result) = committed_text(ui, id, value.map(str::to_owned), width, |t| Ok(t.trim().to_owned()));
    result
}

/// [`text`] showing `hint` while the value is empty, for example the
/// suggested number of a space without one (SPC-001).
pub fn hinted_text(ui: &mut Ui, id: Id, value: Option<&str>, hint: &str, width: f32) -> Edited<String> {
    let kind = FieldKind {
        hint,
        ..FieldKind::default()
    };
    let (_, result) = committed(ui, id, value.map(str::to_owned), width, kind, |t| {
        Ok(t.trim().to_owned())
    });
    result
}

/// Edits text of several lines (ANN-007): Enter starts a new line, and the
/// text is committed, without trailing white space, on Ctrl/Cmd+Enter or
/// when the field loses focus. `None` shows a mixed value.
pub fn multiline_text(ui: &mut Ui, id: Id, value: Option<&str>, width: f32) -> (Response, Edited<String>) {
    let kind = FieldKind {
        multiline: true,
        ..FieldKind::default()
    };
    committed(ui, id, value.map(str::to_owned), width, kind, |t| {
        Ok(t.trim_end().to_owned())
    })
}

/// Edits an optional positive number typed with `nf` (NFR-L10N-004), such
/// as a size that overrides a style's (ANN-007): `Some(None)` shows an empty
/// field with `hint`, and clearing the field commits `None`; `None` shows a
/// mixed value.
pub fn optional_positive(
    ui: &mut Ui,
    id: Id,
    value: Option<Option<f64>>,
    hint: &str,
    nf: &NumberFormat,
    width: f32,
) -> (Response, Edited<Option<f64>>) {
    let shown = value.map(|v| v.map_or_else(String::new, |v| format_trimmed(nf, v, 3)));
    let kind = FieldKind {
        empty_commits: true,
        hint,
        ..FieldKind::default()
    };
    committed(ui, id, shown, width, kind, |text| {
        if text.trim().is_empty() {
            return Ok(None);
        }
        nf.parse_decimal(text)
            .filter(|v| *v > 0.0)
            .map(Some)
            .ok_or_else(|| tr!("dlg-properties-error-positive-number"))
    })
}

/// A combo box over `options`; `current` is `None` for a mixed value.
pub fn choice<T: Clone + PartialEq>(
    ui: &mut Ui,
    id: Id,
    current: Option<&T>,
    options: &[(T, String)],
    width: f32,
) -> Option<T> {
    choice_response(ui, id, current, options, width).1
}

/// [`choice`] with the combo box's response, for labelling it.
pub fn choice_response<T: Clone + PartialEq>(
    ui: &mut Ui,
    id: Id,
    current: Option<&T>,
    options: &[(T, String)],
    width: f32,
) -> (Response, Option<T>) {
    let selected_text = current
        .and_then(|c| options.iter().find(|(v, _)| v == c))
        .map_or_else(|| "Mixed".to_owned(), |(_, label)| label.clone());
    let mut picked = None;
    let response = egui::ComboBox::from_id_salt(id)
        .selected_text(selected_text)
        .width(width)
        .show_ui(ui, |ui| {
            for (value, label) in options {
                if ui.selectable_label(current == Some(value), label).clicked() {
                    picked = Some(value.clone());
                }
            }
        })
        .response;
    (response, picked.filter(|p| current != Some(p)))
}

/// The anchor grid (COL-001): the first nine `options`, the points of a
/// section's bounding box row by row from the top left, as a 3 × 3 grid of
/// small toggle buttons, then the others, such as 'Profile origin', as
/// toggle buttons with their label. Each button's tooltip and accessible
/// name is its option's label; `current` is `None` for a mixed value, which
/// checks none. Returns the option picked, if it differs from `current`.
/// The grid is the one beams will use (BEA-001).
pub fn anchor_grid<T: Clone + PartialEq>(
    ui: &mut Ui,
    id: Id,
    current: Option<&T>,
    options: &[(T, String)],
) -> Option<T> {
    let mut picked = None;
    let mut toggle = |ui: &mut Ui, (value, label): &(T, String), text: &str| {
        let selected = current == Some(value);
        let response = ui
            .add(
                egui::Button::selectable(selected, text)
                    .small()
                    .min_size(egui::vec2(12.0, 12.0)),
            )
            .on_hover_text(label);
        response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, label));
        if response.clicked() && !selected {
            picked = Some(value.clone());
        }
    };
    let (points, others) = options.split_at(options.len().min(9));
    ui.horizontal(|ui| {
        egui::Grid::new(id)
            .spacing([1.0, 1.0])
            .min_col_width(0.0)
            .min_row_height(0.0)
            .show(ui, |ui| {
                for (n, option) in points.iter().enumerate() {
                    toggle(ui, option, "");
                    if n % 3 == 2 {
                        ui.end_row();
                    }
                }
            });
        for option in others {
            toggle(ui, option, &option.1);
        }
    });
    picked
}

/// The common value of all items, or `None` if they differ or there are
/// none.
pub fn common<T: PartialEq>(mut values: impl Iterator<Item = T>) -> Option<T> {
    let first = values.next()?;
    values.all(|v| v == first).then_some(first)
}
