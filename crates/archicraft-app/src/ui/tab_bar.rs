//! The widgets of the view tab row (UI-001, UI-004): the toggle of the
//! project browser at the right end of the tab bar in the top right corner
//! of the views.

use egui::{Atoms, Button, Image, Rect, Ui, Vec2, WidgetInfo, WidgetType};

use crate::commands::{CommandCtx, CommandSpec};
use crate::ui::command_widgets::titled_tooltip;

/// The side of an icon in the tab row, in points: the tab row is lower
/// than the toolbar, so its icons are drawn smaller than the toolbar's
/// (`command_widgets::ICON`), at about the height of the tab titles.
pub const TAB_ICON: f32 = 16.0;

/// Whether the tab bar `bar` is the one in the top right corner of the
/// views `views`, the one that carries the browser toggle: with split views
/// exactly one tab bar touches both the views' top and right edges (UI-004).
pub fn is_top_right(bar: Rect, views: Rect) -> bool {
    (bar.right() - views.right()).abs() < 1.0 && (bar.top() - views.top()).abs() < 1.0
}

/// A square icon button of `spec` as high as the tab row in `ui`: its icon
/// of side [`TAB_ICON`] tinted with the text colour, selected while the
/// command is on, the command's label as accessible name and its titled
/// tooltip (UI-011). Returns true when clicked.
pub fn icon_button(ui: &mut Ui, cx: &CommandCtx, spec: &CommandSpec) -> bool {
    let selected = spec.checked_in(cx).unwrap_or(false);
    let enabled = spec.enabled_in(cx).is_yes();
    let mut atoms = Atoms::default();
    if let Some(icon) = &spec.icon {
        atoms.push_right(Image::new(icon.clone()).fit_to_exact_size(Vec2::splat(TAB_ICON)));
    }
    let button = Button::selectable(selected, atoms).image_tint_follows_text_color(true);
    let side = ui.available_height();
    let response = ui.add_enabled(enabled, |ui: &mut Ui| ui.add_sized(Vec2::splat(side), button));
    let name = spec.label();
    response.widget_info(|| WidgetInfo::selected(WidgetType::Button, enabled, selected, &name));
    let title = spec.label_in(cx);
    response
        .on_hover_ui(|ui| titled_tooltip(ui, cx, spec, &title))
        .clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UI-004: of a plan beside a 3D view, and of two views above each
    /// other, only the tab bar in the top right corner carries the toggle.
    #[test]
    fn only_the_top_right_tab_bar_carries_the_toggle() {
        let views = Rect::from_min_max(egui::pos2(60.0, 100.0), egui::pos2(1150.0, 850.0));
        let bar = |x0: f32, y0: f32, x1: f32| Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y0 + 24.0));
        assert!(is_top_right(bar(60.0, 100.0, 1150.0), views), "one tab bar");
        assert!(!is_top_right(bar(60.0, 100.0, 600.0), views), "the left one");
        assert!(is_top_right(bar(605.0, 100.0, 1150.0), views), "the right one");
        assert!(!is_top_right(bar(60.0, 480.0, 1150.0), views), "the lower one");
    }
}
