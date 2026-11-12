//! Widgets that run commands (UI-011, UI-017): menu items, toolbox and
//! status-bar buttons and the buttons of panels and dialogs. Their label,
//! enabled and checked state come from the registry in the state of the
//! frame, and their tooltip is the command's description with its shortcut
//! from the effective keymap, so every place a command appears shows the
//! same text and state.

use egui::{Atom, Atoms, Button, Image, Response, RichText, Ui, Vec2, WidgetInfo, WidgetType};

use crate::commands::{Binding, CommandCtx, CommandSpec, Enabled, Keymap};
use crate::input::Modifiers as Held;
use crate::tr;

/// The side of the toolbox's icons, in points.
const TOOLBOX_ICON: f32 = 20.0;
/// The side of a toolbox button, which is square, in points.
pub const TOOLBOX_BUTTON: f32 = 32.0;

/// How a platform writes keys: a binding, and the modifiers held for a
/// click gesture.
pub trait KeyNames {
    fn binding(&self, binding: Binding) -> String;
    fn modifiers(&self, held: Held) -> String;
}

/// The text of the first binding that runs `spec` under `keymap`, written by
/// `keys`, if it has one; otherwise of the click gesture of its parameter
/// transfer, such as "Alt-click" (ELM-003, EDT-020).
pub fn shortcut_text(spec: &CommandSpec, keymap: &Keymap, keys: &impl KeyNames) -> Option<String> {
    match keymap.effective_bindings(spec.id).first() {
        Some(&binding) => Some(keys.binding(binding)),
        None => spec.gesture.map(|mode| {
            tr!(
                "cmd-gesture-click",
                modifiers = keys.modifiers(keymap.transfer_gesture(mode))
            )
        }),
    }
}

/// The tooltip of a command (UI-011): its description and, when a key runs
/// it, that key in brackets, such as "Measure distances along picked
/// points and angles in working units, without changing the model (M)".
pub fn tooltip_text(spec: &CommandSpec, keymap: &Keymap, keys: &impl KeyNames) -> String {
    let description = spec.description();
    match shortcut_text(spec, keymap, keys) {
        Some(shortcut) => format!("{} ({shortcut})", description.trim_end_matches('.')),
        None => description,
    }
}

/// The tooltip of a toolbar or toolbox button (UI-011): `title` in bold
/// with the command's shortcut, then its description and, while it cannot
/// run, the reason from the catalogue.
pub fn titled_tooltip(ui: &mut Ui, cx: &CommandCtx, spec: &CommandSpec, title: &str) {
    ui.set_max_width(ui.spacing().tooltip_width);
    ui.horizontal(|ui| {
        ui.strong(title);
        if let Some(keys) = shortcut_text(spec, &cx.keymap, &UiKeys(ui)) {
            ui.weak(keys);
        }
    });
    ui.label(spec.description());
    if let Enabled::No(reason) = spec.enabled_in(cx) {
        ui.colored_label(ui.visuals().warn_fg_color, tr!(reason));
    }
}

/// The keys as egui shows them on this platform.
struct UiKeys<'a>(&'a Ui);

impl KeyNames for UiKeys<'_> {
    fn binding(&self, binding: Binding) -> String {
        self.0.ctx().format_shortcut(&binding.shortcut())
    }

    fn modifiers(&self, held: Held) -> String {
        let modifiers = egui::Modifiers {
            alt: held.alt,
            shift: held.shift,
            command: held.command,
            ..egui::Modifiers::NONE
        };
        self.0.ctx().format_modifiers(modifiers)
    }
}

/// Adds the command's tooltip to `response`; while the command cannot run,
/// the tooltip also gives the reason from the catalogue.
fn with_tooltip(response: Response, ui: &Ui, cx: &CommandCtx, spec: &CommandSpec) -> Response {
    let text = tooltip_text(spec, &cx.keymap, &UiKeys(ui));
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
    let shortcut = shortcut_text(spec, &cx.keymap, &UiKeys(ui));
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

/// A toolbox button of `spec` (UI-001): a square button with its icon and
/// no text, selected while the command is on. The short `label` is its
/// accessible name and the bold title of its tooltip, which adds the
/// shortcut and the command's description (UI-011). Returns true when
/// clicked.
pub fn tool_button(ui: &mut Ui, cx: &CommandCtx, spec: &CommandSpec, label: &str) -> bool {
    let selected = spec.checked_in(cx).unwrap_or(false);
    let enabled = spec.enabled_in(cx).is_yes();
    let mut atoms = Atoms::default();
    if let Some(icon) = &spec.icon {
        atoms.push_right(Image::new(icon.clone()).fit_to_exact_size(Vec2::splat(TOOLBOX_ICON)));
    }
    let button = Button::selectable(selected, atoms).image_tint_follows_text_color(true);
    let response = ui.add_enabled(enabled, |ui: &mut Ui| ui.add_sized(Vec2::splat(TOOLBOX_BUTTON), button));
    response.widget_info(|| WidgetInfo::selected(WidgetType::Button, enabled, selected, label));
    response
        .on_hover_ui(|ui| titled_tooltip(ui, cx, spec, label))
        .on_disabled_hover_ui(|ui| titled_tooltip(ui, cx, spec, label))
        .clicked()
}

/// A bar button of `spec` with the short `label`, for the status line, the
/// view-control bar and the project browser: a button, or a selectable
/// button for an on/off command, both in the one text size of the bars
/// (UI-001, `ui::style`), so they are equally high. Returns true when
/// clicked.
pub fn bar_button(ui: &mut Ui, cx: &CommandCtx, spec: &CommandSpec, label: &str) -> bool {
    let enabled = spec.enabled_in(cx).is_yes();
    let response = match spec.checked_in(cx) {
        Some(on) => ui.add_enabled(enabled, Button::selectable(on, label)),
        None => ui.add_enabled(enabled, Button::new(label)),
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

    /// Keys written as on macOS, the modifiers of a gesture by their
    /// debug form.
    struct MacKeys;

    impl KeyNames for MacKeys {
        fn binding(&self, binding: Binding) -> String {
            binding.display(OperatingSystem::Mac)
        }

        fn modifiers(&self, held: Held) -> String {
            format!("{held:?}")
        }
    }

    /// UI-011: the tooltip of every command contains its English description
    /// and, when a key runs it, that key's text, or else the click gesture of
    /// its parameter transfer (ELM-003); a rebound key or gesture shows
    /// instead of the default.
    #[test]
    fn every_tooltip_has_the_description_and_the_effective_shortcut() {
        let english = std::rc::Rc::new(Localizer::english());
        let os = OperatingSystem::Mac;
        let keys = &MacKeys;
        let defaults = Keymap::default();
        i18n::with(english, || {
            for spec in Registry::get().specs() {
                let tip = tooltip_text(spec, &defaults, keys);
                let description = spec.description();
                assert!(tip.contains(description.trim_end_matches('.')), "{}: {tip}", spec.id);
                match (defaults.effective_bindings(spec.id).first(), spec.gesture) {
                    (Some(binding), _) => assert!(tip.contains(&binding.display(os)), "{}: {tip}", spec.id),
                    (None, Some(mode)) => {
                        let gesture = format!("{:?}-click", defaults.transfer_gesture(mode));
                        assert!(tip.ends_with(&format!("({gesture})")), "{}: {tip}", spec.id);
                    }
                    (None, None) => assert_eq!(tip, description, "{}", spec.id),
                }
            }
            let pick_up = Registry::get().command(crate::commands::edit::PICK_UP_PARAMETERS);
            let mut remapped = Keymap::default();
            let shift_alt = Held {
                shift: true,
                command: false,
                alt: true,
            };
            remapped.set_transfer_gesture(crate::views::transfer::TransferMode::PickUp, shift_alt);
            assert!(
                tooltip_text(pick_up, &remapped, keys).ends_with(&format!("({shift_alt:?}-click)")),
                "a remapped gesture shows"
            );
            let wall = Registry::get().command(tools::WALL);
            assert_eq!(
                tooltip_text(wall, &defaults, keys),
                "Draw walls along their reference line in a length you choose. Thickness, height and material can be changed later (W)"
            );
            let mut rebound = Keymap::default();
            rebound.set(tools::WALL, vec![Binding::key(Key::W).shift()]);
            assert_eq!(
                tooltip_text(wall, &rebound, keys),
                "Draw walls along their reference line in a length you choose. Thickness, height and material can be changed later (Shift+W)"
            );
        });
    }
}
