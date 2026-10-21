//! Input widgets that edit model values with units (INP-005, ELM-017).

use archicraft_model::units::{NumberFormat, UnitSettings, format_angle, format_length, parse_angle, parse_length};
use egui::{Id, Response, TextEdit, Ui};

/// The result of an edit widget.
#[derive(Clone, Debug, PartialEq)]
pub enum Edited<T> {
    Unchanged,
    Changed(T),
    Invalid(String),
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
    let mut text: String = ui
        .data_mut(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| shown.clone().unwrap_or_default());
    let response = ui.add(
        TextEdit::singleline(&mut text)
            .id(id)
            .desired_width(width)
            .hint_text(if shown.is_none() { "Mixed" } else { "" }),
    );
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
        if response.lost_focus() && Some(&text) != shown.as_ref() && !text.trim().is_empty() {
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

/// Edits a single line of text, committed on Enter or focus loss.
pub fn text(ui: &mut Ui, id: Id, value: Option<&str>, width: f32) -> Edited<String> {
    let (_, result) = committed_text(ui, id, value.map(str::to_owned), width, |t| Ok(t.trim().to_owned()));
    result
}

/// A combo box over `options`; `current` is `None` for a mixed value.
pub fn choice<T: Clone + PartialEq>(
    ui: &mut Ui,
    id: Id,
    current: Option<&T>,
    options: &[(T, String)],
    width: f32,
) -> Option<T> {
    let selected_text = current
        .and_then(|c| options.iter().find(|(v, _)| v == c))
        .map_or_else(|| "Mixed".to_owned(), |(_, label)| label.clone());
    let mut picked = None;
    egui::ComboBox::from_id_salt(id)
        .selected_text(selected_text)
        .width(width)
        .show_ui(ui, |ui| {
            for (value, label) in options {
                if ui.selectable_label(current == Some(value), label).clicked() {
                    picked = Some(value.clone());
                }
            }
        });
    picked.filter(|p| current != Some(p))
}

/// The common value of all items, or `None` if they differ or there are
/// none.
pub fn common<T: PartialEq>(mut values: impl Iterator<Item = T>) -> Option<T> {
    let first = values.next()?;
    values.all(|v| v == first).then_some(first)
}
