//! A row of units that never wraps, with an overflow menu (UI-001, UI-002):
//! the layout of the properties strip.
//!
//! The row shows its units left to right in their order, a separator
//! between neighbouring groups. A unit, such as a label and its widget, is
//! shown or moved as a whole. When the units do not fit, the trailing ones
//! that do not fit are hidden, and a chevron button at the right edge opens
//! a popup that lists them in the same order, drawn by the same function, so
//! that edits there go through the same commands.
//!
//! The split follows the units' widths measured in an earlier frame, not the
//! widths of this frame, so it is the same in every frame at the same window
//! width and does not flicker while the window is resized. When the units
//! change (another selection, another tool), the row draws all of them once
//! to measure them, those past the window's edge invisibly, and asks egui
//! to discard that pass. A pass that carries a press or release of a
//! pointer button is kept, so that a click or double-click reaches every
//! widget of the frame as egui saw it; the next frame then shows the split.

use std::hash::{DefaultHasher, Hash, Hasher};

use egui::{Align, Id, Layout, Rect, Ui};

/// Where a unit is drawn: in the row, or in the overflow popup, one unit
/// per line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Row,
    Overflow,
}

/// What the row knows of a unit before drawing it: a name for it and what
/// its width depends on, such as `field:thickness` (a unit whose name
/// changes is measured anew), and its group (neighbours of different
/// groups have a separator between them).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UnitKey {
    pub name: String,
    pub group: u8,
}

/// The row as laid out in the last frame, for tests: the names of all
/// units in row order, how many leading ones the row shows (the others are
/// in the popup), their rectangles, the separators' and the chevron's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RowLayout {
    pub names: Vec<String>,
    pub shown: usize,
    pub rects: Vec<Rect>,
    pub separators: Vec<Rect>,
    pub button: Option<Rect>,
}

impl RowLayout {
    /// The layout of the row `id` in the last frame.
    pub fn load(ctx: &egui::Context, id: Id) -> Option<Self> {
        ctx.data(|d| d.get_temp::<Self>(id.with("layout")))
    }

    /// The names of the units in the overflow popup.
    pub fn hidden(&self) -> &[String] {
        &self.names[self.shown..]
    }
}

/// The units' advances in the row, measured for the units whose names
/// hash to `signature`.
#[derive(Clone, Debug)]
struct Widths {
    signature: u64,
    advances: Vec<f32>,
}

/// The number of leading units the row shows: all when they fit into
/// `available`, otherwise as many as fit beside the chevron button of width
/// `button`. An advance is a unit's width with the separator before it and
/// the `spacing` after it.
pub fn split(advances: &[f32], spacing: f32, available: f32, button: f32) -> usize {
    let total: f32 = advances.iter().sum();
    if total - spacing <= available {
        return advances.len();
    }
    let mut used = 0.0;
    for (i, advance) in advances.iter().enumerate() {
        used += advance;
        if used + button > available {
            return i;
        }
    }
    advances.len()
}

/// The id of the overflow popup of the row `id`, which also keeps whether
/// it is open.
pub fn overflow_id(id: Id) -> Id {
    id.with("overflow")
}

/// The margin above and below the text of a single-line text field, egui's
/// default.
const TEXT_EDIT_MARGIN: f32 = 2.0;

/// The height of the row: one line of widgets, the highest of which is a
/// single-line text field, rounded up to whole points. The row keeps it
/// whatever it shows, so that the views below do not move, not even by a
/// fraction of a pixel, when the tool or the selection changes.
pub fn row_height(ui: &Ui) -> f32 {
    let text_field = ui.text_style_height(&egui::TextStyle::Body) + 2.0 * TEXT_EDIT_MARGIN;
    ui.spacing().interact_size.y.max(text_field).ceil()
}

/// Draws the row `id` of `units`, `draw` drawing the unit of an index at
/// its place; `more` names the chevron button.
pub fn show(ui: &mut Ui, id: Id, units: &[UnitKey], more: &str, draw: &mut dyn FnMut(&mut Ui, usize, Place)) {
    let signature = {
        let mut hasher = DefaultHasher::new();
        units.hash(&mut hasher);
        hasher.finish()
    };
    let cached = ui
        .data(|d| d.get_temp::<Widths>(id))
        .filter(|w| w.signature == signature && w.advances.len() == units.len());
    let measuring = cached.is_none();
    let spacing = ui.spacing().item_spacing.x;
    let button_size = ui.spacing().interact_size.y;
    let available = ui.available_width();
    let shown = cached
        .as_ref()
        .map_or(units.len(), |w| split(&w.advances, spacing, available, button_size));
    let mut advances = cached.map_or_else(|| vec![0.0; units.len()], |w| w.advances);
    let mut layout = RowLayout {
        names: units.iter().map(|u| u.name.clone()).collect(),
        shown,
        ..RowLayout::default()
    };
    let was_open = ui.data(|d| d.get_temp::<bool>(overflow_id(id))).unwrap_or(false);
    // The pass that measures shows every unit in the row: the popup stays
    // as it was.
    let mut open = was_open && (measuring || shown < units.len());

    let height = row_height(ui);
    let button = ui
        .allocate_ui_with_layout(
            egui::vec2(available, height),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.set_min_height(height);
                let edge = ui.max_rect().right() - button_size;
                for (i, unit) in units.iter().enumerate().take(shown) {
                    let start = ui.cursor().min.x;
                    if i > 0 && unit.group != units[i - 1].group {
                        layout.separators.push(ui.separator().rect);
                    }
                    // While measuring, the units that start past the edge
                    // are measured but not shown.
                    let builder = if measuring && start > edge {
                        egui::UiBuilder::new().invisible()
                    } else {
                        egui::UiBuilder::new()
                    };
                    let rect = ui.scope_builder(builder, |ui| draw(ui, i, Place::Row)).response.rect;
                    layout.rects.push(rect);
                    advances[i] = ui.cursor().min.x - start;
                }
                (shown < units.len()).then(|| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let response = ui
                            .add_sized([button_size, button_size], egui::Button::new("»").selected(open))
                            .on_hover_text(more);
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), open, more)
                        });
                        response
                    })
                    .inner
                })
            },
        )
        .inner;

    if measuring {
        let ctx = ui.ctx();
        let clicked = ctx.input(|i| i.pointer.any_pressed() || i.pointer.any_released());
        if ctx.current_pass_index() == 0 && !clicked {
            ctx.request_discard("the properties strip measured its units");
        } else {
            ctx.request_repaint();
        }
    }
    if let Some(button) = &button {
        layout.button = Some(button.rect);
        overflow(ui, id, units, shown, button, &mut open, draw);
    }
    ui.data_mut(|d| {
        d.insert_temp(id, Widths { signature, advances });
        d.insert_temp(overflow_id(id), open);
        d.insert_temp(id.with("layout"), layout);
    });
}

/// The overflow popup below the chevron `button`: the units from `shown`
/// on, a separator between groups. It closes on the button, Escape or a
/// click outside it, but not on a click into a combo box it opened.
fn overflow(
    ui: &Ui,
    id: Id,
    units: &[UnitKey],
    shown: usize,
    button: &egui::Response,
    open: &mut bool,
    draw: &mut dyn FnMut(&mut Ui, usize, Place),
) {
    if button.clicked() {
        *open = !*open;
    }
    // A combo box of the popup has its own popup; a click there is not
    // outside.
    let other_popup = egui::Popup::is_any_open(ui.ctx());
    let response = egui::Popup::new(overflow_id(id), ui.ctx().clone(), button, ui.layer_id())
        .open_bool(open)
        .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
        .align(egui::RectAlign::BOTTOM_END)
        .show(|ui| {
            for (i, unit) in units.iter().enumerate().skip(shown) {
                if i > shown && unit.group != units[i - 1].group {
                    ui.separator();
                }
                ui.horizontal(|ui| draw(ui, i, Place::Overflow));
            }
        });
    let Some(response) = response else {
        return;
    };
    let clicked_outside = ui.ctx().input(|i| {
        i.pointer.any_click()
            && i.pointer
                .interact_pos()
                .is_some_and(|pos| !response.response.rect.contains(pos) && !button.rect.contains(pos))
    });
    if clicked_outside && !other_popup {
        *open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UI-002: all units fit when their advances without the last spacing
    /// fit; otherwise the row keeps room for the button.
    #[test]
    fn split_keeps_room_for_the_button() {
        let advances = [50.0, 50.0, 50.0];
        assert_eq!(split(&advances, 8.0, 142.0, 18.0), 3);
        assert_eq!(split(&advances, 8.0, 141.0, 18.0), 2);
        assert_eq!(split(&advances, 8.0, 118.0, 18.0), 2);
        assert_eq!(split(&advances, 8.0, 117.0, 18.0), 1);
        assert_eq!(split(&advances, 8.0, 10.0, 18.0), 0);
        assert_eq!(split(&[], 8.0, 0.0, 18.0), 0);
    }
}
