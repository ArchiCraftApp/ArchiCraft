//! Widgets that run commands (UI-011, UI-017): menu items, toolbox and
//! status-bar buttons and the buttons of panels and dialogs. Their label,
//! enabled and checked state come from the registry in the state of the
//! frame, and their tooltip is the command's description with its shortcut
//! from the effective keymap, so every place a command appears shows the
//! same text and state.

use egui::{Atom, Atoms, Button, Image, Response, RichText, Ui, Vec2};

use crate::commands::{Binding, CommandCtx, CommandSpec, Enabled, Keymap};
use crate::tr;

/// The side of the toolbox's icons, in points.
const TOOLBOX_ICON: f32 = 20.0;
/// The size of a toolbox button, in points.
const TOOLBOX_BUTTON: [f32; 2] = [88.0, 26.0];

/// The text of the first binding that runs `spec` under `keymap`, written by
/// `keys`, if it has one.
pub fn shortcut_text(spec: &CommandSpec, keymap: &Keymap, keys: impl Fn(Binding) -> String) -> Option<String> {
    keymap.effective_bindings(spec.id).first().map(|&b| keys(b))
}

/// The tooltip of a command (UI-011): its description and, when a key runs
/// it, that key in brackets, such as "Draw walls by their reference line
/// (W)".
pub fn tooltip_text(spec: &CommandSpec, keymap: &Keymap, keys: impl Fn(Binding) -> String) -> String {
    let description = spec.description();
    match shortcut_text(spec, keymap, keys) {
        Some(shortcut) => format!("{} ({shortcut})", description.trim_end_matches('.')),
        None => description,
    }
}

/// The keys of `binding` as egui shows them on this platform.
fn keys(ui: &Ui) -> impl Fn(Binding) -> String + '_ {
    |binding| ui.ctx().format_shortcut(&binding.shortcut())
}

/// Adds the command's tooltip to `response`; while the command cannot run,
/// the tooltip also gives the reason from the catalogue.
fn with_tooltip(response: Response, ui: &Ui, cx: &CommandCtx, spec: &CommandSpec) -> Response {
    let text = tooltip_text(spec, &cx.keymap, keys(ui));
    match spec.enabled_in(cx) {
        Enabled::Yes => response.on_hover_text(text),
        Enabled::No(reason) => response.on_disabled_hover_text(format!("{text}\n{}", tr!(reason))),
    }
}

/// The icon of `spec` (UI-001, `ui/icons.rs`) with a side of `size` points,
/// then `label`. The button that shows them must tint its images with its
/// text colour, so the white icon takes the colour of the label.
pub fn icon_and_label<'a>(spec: &CommandSpec, size: f32, label: impl Into<Atom<'a>>) -> Atoms<'a> {
    let mut atoms = Atoms::new(label);
    if let Some(icon) = &spec.icon {
        atoms.push_left(Image::new(icon.clone()).fit_to_exact_size(Vec2::splat(size)));
    }
    atoms
}

/// The menu item of `spec`: its label in the state `cx`, its shortcut, its
/// enabled state and, for an on/off command, a check box. Closes the menu
/// and returns true when clicked.
pub fn menu_item(ui: &mut Ui, cx: &CommandCtx, spec: &CommandSpec) -> bool {
    let label = spec.label_in(cx);
    let enabled = spec.enabled_in(cx).is_yes();
    let shortcut = shortcut_text(spec, &cx.keymap, keys(ui));
    let response = if let Some(mut on) = spec.checked_in(cx) {
        let mut atoms = Atoms::new(label);
        if let Some(text) = shortcut {
            atoms.push_right(Atom::grow());
            atoms.push_right(RichText::new(text).weak());
        }
        ui.add_enabled(enabled, egui::Checkbox::new(&mut on, atoms))
    } else {
        let mut button = Button::new(label);
        if let Some(text) = shortcut {
            button = button.shortcut_text(text);
        }
        ui.add_enabled(enabled, button)
    };
    let clicked = with_tooltip(response, ui, cx, spec).clicked();
    if clicked {
        ui.close();
    }
    clicked
}

/// A toolbox button of `spec` (UI-001): its icon and the short `label`,
/// both at the button's left so the icons line up in a column, selected
/// while the command is on. Returns true when clicked.
pub fn tool_button(ui: &mut Ui, cx: &CommandCtx, spec: &CommandSpec, label: &str) -> bool {
    let mut atoms = icon_and_label(spec, TOOLBOX_ICON, label);
    atoms.push_right(Atom::grow());
    let button = Button::selectable(spec.checked_in(cx).unwrap_or(false), atoms).image_tint_follows_text_color(true);
    let response = ui.add_enabled_ui(spec.enabled_in(cx).is_yes(), |ui| ui.add_sized(TOOLBOX_BUTTON, button));
    with_tooltip(response.inner, ui, cx, spec).clicked()
}

/// A status-bar button of `spec` with the short `label`: a small button, or
/// a selectable label for an on/off command. Returns true when clicked.
pub fn bar_button(ui: &mut Ui, cx: &CommandCtx, spec: &CommandSpec, label: &str) -> bool {
    let enabled = spec.enabled_in(cx).is_yes();
    let response = match spec.checked_in(cx) {
        Some(on) => ui.add_enabled(enabled, Button::selectable(on, label)),
        None => ui.add_enabled(enabled, Button::new(RichText::new(label).small()).small()),
    };
    with_tooltip(response, ui, cx, spec).clicked()
}

/// A button of `spec` with its label in the state `cx`, for panels and
/// dialogs: a button, or a selectable label for an on/off command. Returns
/// true when clicked.
pub fn command_button(ui: &mut Ui, cx: &CommandCtx, spec: &CommandSpec) -> bool {
    let label = spec.label_in(cx);
    let enabled = spec.enabled_in(cx).is_yes();
    let response = match spec.checked_in(cx) {
        Some(on) => ui.add_enabled(enabled, Button::selectable(on, label)),
        None => ui.add_enabled(enabled, Button::new(label)),
    };
    with_tooltip(response, ui, cx, spec).clicked()
}

#[cfg(test)]
mod tests {
    use egui::Key;
    use egui::os::OperatingSystem;

    use super::*;
    use crate::commands::{Registry, tools};
    use crate::i18n::{self, Localizer};

    /// UI-011: the tooltip of every command contains its English description
    /// and, when a key runs it, that key's text; a rebound key shows instead
    /// of the default.
    #[test]
    fn every_tooltip_has_the_description_and_the_effective_shortcut() {
        let english = std::rc::Rc::new(Localizer::english());
        let os = OperatingSystem::Mac;
        let keys = |b: Binding| b.display(os);
        let defaults = Keymap::default();
        i18n::with(english, || {
            for spec in Registry::get().specs() {
                let tip = tooltip_text(spec, &defaults, keys);
                let description = spec.description();
                assert!(tip.contains(description.trim_end_matches('.')), "{}: {tip}", spec.id);
                match defaults.effective_bindings(spec.id).first() {
                    Some(binding) => assert!(tip.contains(&binding.display(os)), "{}: {tip}", spec.id),
                    None => assert_eq!(tip, description, "{}", spec.id),
                }
            }
            let wall = Registry::get().command(tools::WALL);
            assert_eq!(
                tooltip_text(wall, &defaults, keys),
                "Draw walls by their reference line (W)"
            );
            let mut rebound = Keymap::default();
            rebound.set(tools::WALL, vec![Binding::key(Key::W).shift()]);
            assert_eq!(
                tooltip_text(wall, &rebound, keys),
                "Draw walls by their reference line (Shift+W)"
            );
        });
    }
}
