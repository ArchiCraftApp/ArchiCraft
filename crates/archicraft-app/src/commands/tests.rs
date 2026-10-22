use std::collections::HashSet;

use egui::os::OperatingSystem;
use egui::{Key, KeyboardShortcut, Modifiers};

use super::*;
use crate::i18n::Localizer;

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

/// The 23 default shortcuts of the former `ui/actions.rs`, each the first
/// binding of its command. Primary is Cmd on macOS and Ctrl elsewhere.
const DEFAULT_SHORTCUTS: [(&str, Binding); 23] = [
    ("file.new", Binding::primary(Key::N)),
    ("file.open", Binding::primary(Key::O)),
    ("file.save", Binding::primary(Key::S)),
    ("file.save-as", Binding::primary(Key::S).shift()),
    ("file.close", Binding::primary(Key::W)),
    ("file.quit", Binding::primary(Key::Q)),
    ("edit.undo", Binding::primary(Key::Z)),
    ("edit.redo", Binding::primary(Key::Z).shift()),
    ("edit.delete", Binding::key(Key::Delete)),
    ("edit.select-all", Binding::primary(Key::A)),
    ("tool.select", Binding::key(Key::V)),
    ("tool.wall", Binding::key(Key::W)),
    ("tool.slab", Binding::key(Key::S)),
    ("edit.settings", Binding::primary(Key::T)),
    ("view.fit-window", Binding::key(Key::F)),
    ("view.zoom-selection", Binding::key(Key::F).shift()),
    ("view.previous-zoom", Binding::primary(Key::OpenBracket)),
    ("view.next-zoom", Binding::primary(Key::CloseBracket)),
    ("view.next-tab", Binding::ctrl(Key::Tab)),
    ("view.previous-tab", Binding::ctrl(Key::Tab).shift()),
    ("view.switch-plan-3d", Binding::key(Key::F3)),
    ("view.orbit", Binding::key(Key::O)),
    ("view.problems", Binding::primary(Key::P).shift()),
];

/// The commands whose shortcuts work while a text field has focus: the file
/// commands, the settings and the problems panel (UI-008).
const WORKS_WHILE_TYPING: [&str; 8] = [
    "file.new",
    "file.open",
    "file.save",
    "file.save-as",
    "file.close",
    "file.quit",
    "edit.settings",
    "view.problems",
];

/// UI-008, HW-003: the registry keeps the 23 default shortcuts as first
/// bindings, the former special cases of the shortcut loop as second
/// bindings (Primary+Y redoes, Backspace deletes) and no others; the
/// commands that work while typing are TextSafe, and the single-key
/// shortcuts and Ctrl+Tab, which stay off while a tool is drawing, are
/// Canvas.
#[test]
fn registry_keeps_the_default_shortcuts() {
    let mut bound = 0;
    for spec in Registry::get().specs() {
        let id = spec.id.as_str();
        let expected = DEFAULT_SHORTCUTS.iter().find(|(i, _)| *i == id).map(|&(_, b)| b);
        assert_eq!(spec.default_bindings.first().copied(), expected, "{id}");
        bound += spec.default_bindings.len();
        assert_eq!(spec.scope == Scope::TextSafe, WORKS_WHILE_TYPING.contains(&id), "{id}");
        if let Some(first) = expected {
            let off_while_drawing = first.is_single_key() || first == Binding::ctrl(Key::Tab);
            assert_eq!(spec.scope == Scope::Canvas, off_while_drawing, "{id}");
        }
    }
    assert_eq!(bound, 25);
    assert_eq!(spec("edit.redo").default_bindings[1], Binding::primary(Key::Y));
    assert_eq!(spec("edit.delete").default_bindings[1], Binding::key(Key::Backspace));
}

/// UI-008: no two commands share a key combination, whatever their scope.
#[test]
fn shortcuts_are_unique() {
    let mut seen = HashSet::new();
    for (binding, spec) in Registry::get().bindings_in_match_order() {
        assert!(seen.insert(binding), "{} reuses {binding:?}", spec.id);
    }
}

/// UI-008: a single key never works while a text field has focus, so no
/// TextSafe command has one.
#[test]
fn text_safe_commands_have_no_single_key() {
    for spec in Registry::get().specs() {
        if spec.scope == Scope::TextSafe {
            assert!(spec.default_bindings.iter().all(|b| !b.is_single_key()), "{}", spec.id);
        }
    }
}

/// UI-008: while typing only TextSafe bindings work; while drawing Canvas
/// bindings are off and PointInput bindings on.
#[test]
fn scopes_are_active_as_documented() {
    use Scope::*;
    for (typing, drawing, active) in [
        (false, false, [true, true, true, false]),
        (false, true, [true, true, false, true]),
        (true, false, [false, true, false, false]),
        (true, true, [false, true, false, false]),
    ] {
        for (scope, expected) in [Global, TextSafe, Canvas, PointInput].into_iter().zip(active) {
            assert_eq!(
                scope.is_active(typing, drawing),
                expected,
                "{scope:?} {typing} {drawing}"
            );
        }
    }
}

/// UI-017: the reasons a command gives for being disabled have English
/// text, in the states that disable them.
#[test]
fn disabled_commands_name_a_reason_from_the_catalogue() {
    let english = Localizer::english();
    let nothing = CommandCtx {
        selection_empty: true,
        ..CommandCtx::default()
    };
    let mut reasons = 0;
    for spec in Registry::get().specs() {
        if let Enabled::No(reason) = (spec.enabled)(&nothing) {
            assert!(reason.starts_with("cmd-reason-"), "{}: {reason}", spec.id);
            assert!(english.has(reason), "{}: no English text for {reason}", spec.id);
            reasons += 1;
        }
    }
    // Undo, Redo, Delete, Zoom to selection and Reset user origin.
    assert_eq!(reasons, 5);
    let everything = CommandCtx {
        can_undo: true,
        can_redo: true,
        user_origin_placed: true,
        ..CommandCtx::default()
    };
    for spec in Registry::get().specs() {
        assert_eq!((spec.enabled)(&everything), Enabled::Yes, "{}", spec.id);
    }
}

/// UND-001: Undo and Redo name the change they take back or repeat, and
/// fall back to their plain label without one.
#[test]
fn undo_and_redo_labels_name_the_change() {
    let english = std::rc::Rc::new(Localizer::english());
    let cx = CommandCtx {
        undo_label: Some("Create wall".to_owned()),
        redo_label: Some("Delete slab".to_owned()),
        ..CommandCtx::default()
    };
    let label = |id, cx: &CommandCtx| crate::i18n::with(english.clone(), || spec(id).label_in(cx));
    assert_eq!(label("edit.undo", &cx), "Undo Create wall");
    assert_eq!(label("edit.redo", &cx), "Redo Delete slab");
    assert_eq!(label("edit.undo", &CommandCtx::default()), "Undo");
    assert_eq!(label("file.save", &cx), "Save");
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

/// UI-008: a binding is matched before every other binding on the same key
/// whose modifiers it contains, because egui would let the one with fewer
/// modifiers take the key press.
#[test]
fn shortcuts_with_more_modifiers_are_matched_first() {
    let order: Vec<Binding> = Registry::get().bindings_in_match_order().map(|(b, _)| b).collect();
    let position = |b: Binding| order.iter().position(|&o| o == b);
    for &a in &order {
        for &b in &order {
            let (ma, mb) = (a.modifiers(), b.modifiers());
            if a != b && a.key == b.key && ma.contains(mb) {
                assert!(position(a) < position(b), "{a:?} must be matched before {b:?}");
            }
        }
    }
}

/// The commands a key press with `modifiers` triggers, with every scope
/// active.
fn pressed(modifiers: Modifiers, key: Key) -> Vec<&'static str> {
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        events: vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }],
        ..Default::default()
    };
    let mut triggered = Vec::new();
    let output = ctx.run_ui(input, |ui| {
        triggered = ui.ctx().input_mut(|i| Registry::get().consume_shortcuts(i, |_| true));
    });
    output.drop_without_applying_deltas();
    triggered.into_iter().map(CommandId::as_str).collect()
}

/// UI-008, UI-006, VIEW-014: Cmd+Shift+Z redoes and Cmd+Shift+S saves as,
/// on macOS and elsewhere; the shortcuts without Shift on the same keys keep
/// their own command.
#[test]
fn a_shortcut_with_shift_runs_its_own_command() {
    let mac_cmd = Modifiers::MAC_CMD.plus(Modifiers::COMMAND);
    let ctrl = Modifiers::CTRL.plus(Modifiers::COMMAND);
    let shift = Modifiers::SHIFT;
    for cmd in [mac_cmd, ctrl] {
        for (modifiers, key, expected) in [
            (cmd.plus(shift), Key::Z, "edit.redo"),
            (cmd, Key::Z, "edit.undo"),
            (cmd, Key::Y, "edit.redo"),
            (cmd.plus(shift), Key::S, "file.save-as"),
            (cmd, Key::S, "file.save"),
        ] {
            assert_eq!(pressed(modifiers, key), [expected], "{modifiers:?}+{key:?}");
        }
    }
    for (modifiers, key, expected) in [
        (Modifiers::CTRL.plus(shift), Key::Tab, "view.previous-tab"),
        (Modifiers::CTRL, Key::Tab, "view.next-tab"),
        (shift, Key::F, "view.zoom-selection"),
        (Modifiers::NONE, Key::F, "view.fit-window"),
        (Modifiers::NONE, Key::Backspace, "edit.delete"),
    ] {
        assert_eq!(pressed(modifiers, key), [expected], "{modifiers:?}+{key:?}");
    }
}

/// The menus, toolbox and status bar look commands up by id and panic on an
/// unknown one, so every command id written in `ui/shell.rs` must be
/// registered, also in menus no test opens.
#[test]
fn shell_names_registered_commands() {
    let source = include_str!("../ui/shell.rs");
    let prefixes = ["file.", "edit.", "view.", "tool.", "project.", "help."];
    let mut named = 0;
    // The odd pieces between quotes are the contents of string literals.
    for literal in source.split('"').skip(1).step_by(2) {
        let looks_like_id = prefixes.iter().any(|p| literal.starts_with(p))
            && literal
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.');
        if looks_like_id {
            assert!(Registry::get().spec(literal).is_some(), "ui/shell.rs names {literal}");
            named += 1;
        }
    }
    assert!(named >= 33, "only {named} command ids found in ui/shell.rs");
}
