use std::collections::HashSet;

use egui::os::OperatingSystem;
use egui::{Key, KeyboardShortcut, Modifiers};

use super::*;
use crate::i18n::Localizer;
use crate::ui::actions::Action;

fn spec(id: &str) -> &'static CommandSpec {
    Registry::get().spec(id).unwrap_or_else(|| panic!("no command {id}"))
}

/// UI-017: every command has an id, a label, a description and a place where
/// the user finds it, and its default keys do not collide (UI-008).
#[test]
fn every_command_is_complete() {
    let english = Localizer::english();
    let specs = Registry::get().specs();
    let mut ids = HashSet::new();
    let mut keys = HashSet::new();
    for spec in specs {
        let id = spec.id.as_str();
        assert!(ids.insert(id), "duplicate id {id}");
        let segments: Vec<&str> = id.split('.').collect();
        assert!(
            segments.len() >= 2
                && segments.iter().all(|s| {
                    !s.is_empty()
                        && s.chars()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                }),
            "{id} is not a lower-case dotted id"
        );
        for key in [spec.id.label_key(), spec.id.description_key()] {
            assert!(
                keys.insert(key.clone()),
                "{id}: key {key} is shared with another command"
            );
            assert!(english.has(&key), "{id}: no English text for {key}");
            assert!(!english.format(&key, None).trim().is_empty(), "{id}: {key} is empty");
        }
        assert!(
            spec.menu.is_some() || spec.palette_only || spec.context_only,
            "{id} has no menu place and is neither palette_only nor context_only"
        );
        if let Some(place) = spec.menu {
            let top = place.path.split('/').next().unwrap_or_default();
            assert!(
                MENUS.contains(&top),
                "{id}: menu path {} is not under a top-level menu",
                place.path
            );
        }
    }
    for (i, a) in specs.iter().enumerate() {
        for b in &specs[i + 1..] {
            if !a.scope.overlaps(b.scope) {
                continue;
            }
            for binding in &a.default_bindings {
                assert!(
                    !b.default_bindings.contains(binding),
                    "{} and {} share the binding {binding:?}",
                    a.id,
                    b.id
                );
            }
        }
        let own: HashSet<_> = a.default_bindings.iter().collect();
        assert_eq!(own.len(), a.default_bindings.len(), "{} repeats a binding", a.id);
    }
}

/// Command ids are a public contract (keymaps and exported preferences store
/// them), so renaming one must be deliberate: change this list with it.
#[test]
fn command_ids_are_pinned() {
    let ids: Vec<&str> = Registry::get().specs().iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "file.new",
            "file.open",
            "file.save",
            "file.save-as",
            "file.close",
            "file.preferences",
            "file.quit",
            "edit.undo",
            "edit.redo",
            "edit.delete",
            "edit.select-all",
            "edit.settings",
            "view.fit-window",
            "view.zoom-selection",
            "view.previous-zoom",
            "view.next-zoom",
            "view.switch-plan-3d",
            "view.next-tab",
            "view.previous-tab",
            "view.orbit",
            "view.reference-lines",
            "view.problems",
            "view.place-user-origin",
            "view.reset-user-origin",
            "tool.select",
            "tool.wall",
            "tool.slab",
            "project.storeys",
            "project.layers",
            "project.materials",
            "project.settings",
            "help.shortcuts",
            "help.about",
        ]
    );
}

/// UI-008: Primary is Cmd on macOS and Ctrl on Windows and Linux, in the keys
/// matched and in the text shown.
#[test]
fn primary_binding_follows_the_platform() {
    let save = Binding::primary(Key::S);
    assert_eq!(
        save.for_os(OperatingSystem::Mac),
        KeyboardShortcut::new(Modifiers::MAC_CMD, Key::S)
    );
    for os in [OperatingSystem::Windows, OperatingSystem::Nix] {
        assert_eq!(
            save.for_os(os),
            KeyboardShortcut::new(Modifiers::CTRL, Key::S),
            "{os:?}"
        );
        assert_eq!(save.display(os), "Ctrl+S", "{os:?}");
    }
    assert_eq!(save.display(OperatingSystem::Mac), "Cmd+S");
    assert_eq!(save.shortcut(), KeyboardShortcut::new(Modifiers::COMMAND, Key::S));
    // Ctrl stays Ctrl on macOS (Ctrl+Tab switches tabs everywhere, UI-006).
    assert_eq!(
        Binding::ctrl(Key::Tab).for_os(OperatingSystem::Mac),
        KeyboardShortcut::new(Modifiers::CTRL, Key::Tab)
    );
}

/// The command each action of `ui/actions.rs` becomes.
fn command_of(action: Action) -> &'static str {
    match action {
        Action::New => "file.new",
        Action::Open => "file.open",
        Action::Save => "file.save",
        Action::SaveAs => "file.save-as",
        Action::Close => "file.close",
        Action::Quit => "file.quit",
        Action::Undo => "edit.undo",
        Action::Redo => "edit.redo",
        Action::Delete => "edit.delete",
        Action::SelectAll => "edit.select-all",
        Action::ToolSelect => "tool.select",
        Action::ToolWall => "tool.wall",
        Action::ToolSlab => "tool.slab",
        Action::Settings => "edit.settings",
        Action::FitWindow => "view.fit-window",
        Action::ZoomSelection => "view.zoom-selection",
        Action::PreviousZoom => "view.previous-zoom",
        Action::NextZoom => "view.next-zoom",
        Action::NextTab => "view.next-tab",
        Action::PreviousTab => "view.previous-tab",
        Action::SwitchPlan3d => "view.switch-plan-3d",
        Action::OrbitMode => "view.orbit",
        Action::ToggleReferenceLines => "view.reference-lines",
        Action::PlaceUserOrigin => "view.place-user-origin",
        Action::ResetUserOrigin => "view.reset-user-origin",
        Action::Storeys => "project.storeys",
        Action::Layers => "project.layers",
        Action::Materials => "project.materials",
        Action::ProjectSettings => "project.settings",
        Action::Preferences => "file.preferences",
        Action::Problems => "view.problems",
        Action::Shortcuts => "help.shortcuts",
        Action::About => "help.about",
    }
}

/// UI-008, HW-003: the registry carries all 33 actions with the 23 default
/// shortcuts they have today as first binding, and the scopes keep today's
/// rules: the commands that work while typing are TextSafe, and the
/// single-key shortcuts and Ctrl+Tab, which stay off while a tool is
/// drawing, are Canvas.
#[test]
fn registry_keeps_the_default_shortcuts_of_the_actions() {
    let mut ids: Vec<&str> = Action::ALL.iter().map(|&a| command_of(a)).collect();
    ids.sort_unstable();
    let mut registered: Vec<&str> = Registry::get().specs().iter().map(|s| s.id.as_str()).collect();
    registered.sort_unstable();
    assert_eq!(ids, registered);
    let mut bound = 0;
    for action in Action::ALL {
        let spec = spec(command_of(action));
        let first = spec.default_bindings.first().map(|b| b.shortcut());
        assert_eq!(first, action.shortcut(), "{action:?}");
        bound += usize::from(first.is_some());
        assert_eq!(spec.scope == Scope::TextSafe, action.works_while_typing(), "{action:?}");
        if first.is_some() {
            let off_while_drawing = action.is_single_key() || action == Action::NextTab;
            assert_eq!(spec.scope == Scope::Canvas, off_while_drawing, "{action:?}");
        }
    }
    assert_eq!(bound, 23);
    // The special cases of the shortcut loop become second bindings.
    assert_eq!(spec("edit.redo").default_bindings[1], Binding::primary(Key::Y));
    assert_eq!(spec("edit.delete").default_bindings[1], Binding::key(Key::Backspace));
}

/// UI-008: the commands of Appendix D.8 that exist have default shortcuts.
#[test]
fn appendix_d8_commands_have_shortcuts() {
    for id in [
        "file.save",
        "edit.undo",
        "edit.redo",
        "edit.delete",
        "edit.select-all",
        "tool.wall",
        "tool.slab",
        "view.switch-plan-3d",
        "view.next-tab",
        "view.previous-tab",
        "view.fit-window",
        "view.zoom-selection",
        "view.orbit",
    ] {
        assert!(!spec(id).default_bindings.is_empty(), "{id}");
    }
}

/// UI-008: shortcuts with more modifiers are matched first.
#[test]
fn modifier_count_orders_bindings() {
    assert_eq!(Binding::key(Key::F).modifier_count(), 0);
    assert!(Binding::key(Key::F).is_single_key());
    assert_eq!(Binding::key(Key::F).shift().modifier_count(), 1);
    assert_eq!(Binding::ctrl(Key::Tab).shift().modifier_count(), 2);
    assert!(!Binding::primary(Key::S).is_single_key());
}

/// Global and TextSafe bindings can be active anywhere; Canvas and
/// PointInput are both active in the views.
#[test]
fn scopes_overlap_as_documented() {
    use Scope::*;
    for a in [Global, TextSafe, Canvas, PointInput] {
        assert!(Global.overlaps(a) && a.overlaps(Global));
        assert!(TextSafe.overlaps(a) && a.overlaps(TextSafe));
    }
    assert!(Canvas.overlaps(PointInput));
}
