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
            "tool.wall.cycle-reference-line",
            "canvas.cycle-candidates",
            "input.next-field",
            "input.remove-last",
            "input.finish",
            "input.cancel-step",
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
/// Canvas. The plan view's local keys are pinned in
/// `view_keys_are_local_commands`.
#[test]
fn registry_keeps_the_default_shortcuts() {
    let mut bound = 0;
    for spec in Registry::get().specs().iter().filter(|s| !s.local) {
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

/// UI-008: no two commands of the shortcut loop share a key combination,
/// whatever their scope. The local commands of the plan view may share a
/// key with them in a scope that is never active at the same time
/// (`every_command_is_complete`): Backspace deletes the selection, or
/// removes the last point while a tool takes point input.
#[test]
fn shortcuts_are_unique() {
    let mut seen = HashSet::new();
    for (binding, spec) in Registry::get().bindings_in_match_order() {
        if !spec.local {
            assert!(seen.insert(binding), "{} reuses {binding:?}", spec.id);
        }
    }
}

/// HW-003, NFR-A11Y-001, UI-009: the keys the plan view handles itself are
/// registered commands with their default keys, listed in the palette and
/// local to the view: R cycles the wall reference line before and while
/// drawing (Global), Tab cycles the elements under the cursor (Canvas) or
/// moves to the next typed field (PointInput), Backspace removes the last
/// point (PointInput), Enter and Esc work before and while drawing and
/// cannot be rebound. No other command is local.
#[test]
fn view_keys_are_local_commands() {
    use Scope::*;
    let expected = [
        (view_keys::CYCLE_REFERENCE_LINE, Key::R, Global, false),
        (view_keys::CYCLE_CANDIDATES, Key::Tab, Canvas, false),
        (view_keys::NEXT_FIELD, Key::Tab, PointInput, false),
        (view_keys::REMOVE_LAST, Key::Backspace, PointInput, false),
        (view_keys::FINISH, Key::Enter, Global, true),
        (view_keys::CANCEL_STEP, Key::Escape, Global, true),
    ];
    assert_eq!(expected.map(|(id, ..)| id), view_keys::ALL);
    for (id, key, scope, fixed) in expected {
        let spec = spec(id.as_str());
        assert_eq!(spec.default_bindings, [Binding::key(key)], "{id}");
        assert_eq!((spec.scope, spec.fixed), (scope, fixed), "{id}");
        assert!(spec.local && spec.palette_only && spec.menu.is_none(), "{id}");
    }
    let local: Vec<CommandId> = Registry::get()
        .specs()
        .iter()
        .filter(|s| s.local || s.fixed)
        .map(|s| s.id)
        .collect();
    assert_eq!(local, view_keys::ALL);
}

/// UI-009: the keymap gives a command's own bindings where it has them and
/// the registry's defaults otherwise; a fixed command keeps its defaults.
#[test]
fn the_keymap_falls_back_to_the_defaults() {
    let mut keymap = Keymap::default();
    let shift_r = Binding::key(Key::R).shift();
    assert_eq!(
        keymap.effective_bindings(view_keys::CYCLE_REFERENCE_LINE),
        [Binding::key(Key::R)]
    );
    keymap.set(view_keys::CYCLE_REFERENCE_LINE, vec![shift_r]);
    keymap.set(view_keys::FINISH, vec![Binding::key(Key::Space)]);
    assert_eq!(keymap.effective_bindings(view_keys::CYCLE_REFERENCE_LINE), [shift_r]);
    assert_eq!(keymap.effective_bindings(view_keys::FINISH), [Binding::key(Key::Enter)]);
    assert_eq!(keymap.effective_bindings(CommandId::new("no.such-command")), []);
}

/// HW-003, UI-009: a key press runs the local command the keymap binds to
/// it: Shift+R only a command bound to Shift+R, R only one bound to R, and
/// with both bound, each its own; Cmd or Ctrl held matches neither.
#[test]
fn the_keymap_matches_key_presses_to_local_commands() {
    let ids = [view_keys::CYCLE_REFERENCE_LINE, view_keys::REMOVE_LAST];
    let mut keymap = Keymap::default();
    let shift = Modifiers::SHIFT;
    let r = |keymap: &Keymap, modifiers| keymap.matching(&ids, Key::R, modifiers);
    assert_eq!(r(&keymap, Modifiers::NONE), Some(view_keys::CYCLE_REFERENCE_LINE));
    assert_eq!(r(&keymap, Modifiers::COMMAND), None);
    keymap.set(view_keys::CYCLE_REFERENCE_LINE, vec![Binding::key(Key::R).shift()]);
    assert_eq!(r(&keymap, Modifiers::NONE), None);
    assert_eq!(r(&keymap, shift), Some(view_keys::CYCLE_REFERENCE_LINE));
    keymap.set(view_keys::REMOVE_LAST, vec![Binding::key(Key::R)]);
    assert_eq!(r(&keymap, Modifiers::NONE), Some(view_keys::REMOVE_LAST));
    assert_eq!(r(&keymap, shift), Some(view_keys::CYCLE_REFERENCE_LINE));
    assert_eq!(keymap.matching(&ids, Key::Backspace, Modifiers::NONE), None);
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
/// PointInput are never active at the same time, so they do not overlap.
#[test]
fn scopes_overlap_as_documented() {
    use Scope::*;
    for a in [Global, TextSafe, Canvas, PointInput] {
        assert!(Global.overlaps(a) && a.overlaps(Global));
        assert!(TextSafe.overlaps(a) && a.overlaps(TextSafe));
        assert!(a.overlaps(a));
    }
    assert!(!Canvas.overlaps(PointInput) && !PointInput.overlaps(Canvas));
    for (typing, drawing) in [(false, false), (false, true), (true, false), (true, true)] {
        assert!(!(Canvas.is_active(typing, drawing) && PointInput.is_active(typing, drawing)));
    }
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

/// The commands in the menu `path` and its submenus, depth first, with
/// `|` for a separator and `path/` for a submenu.
fn menu_tree(specs: &[CommandSpec], path: &str) -> Vec<String> {
    let mut out = Vec::new();
    for entry in menu_entries(specs, path) {
        match entry {
            MenuEntry::Command(spec) => out.push(spec.id.as_str().to_owned()),
            MenuEntry::Separator => out.push("|".to_owned()),
            MenuEntry::Submenu(sub) => {
                out.push(format!("{sub}/"));
                out.extend(menu_tree(specs, sub));
            }
        }
    }
    out
}

/// UI-017: the menu bar is built from the menu places, so every command with
/// a menu place appears in its menu, exactly once, and nothing else does.
#[test]
fn every_command_with_a_menu_place_appears_in_its_menu() {
    let specs = Registry::get().specs();
    let mut shown = Vec::new();
    for top in MENUS {
        for entry in menu_tree(specs, top) {
            if entry != "|" && !entry.ends_with('/') {
                let place = spec(&entry).menu.expect("a menu place");
                assert!(
                    place.path == top || place.path.starts_with(&format!("{top}/")),
                    "{entry} is in {top}"
                );
                shown.push(entry);
            }
        }
    }
    let mut placed: Vec<String> = specs
        .iter()
        .filter(|s| s.menu.is_some())
        .map(|s| s.id.as_str().to_owned())
        .collect();
    shown.sort();
    placed.sort();
    assert_eq!(shown, placed);
}

/// UI-011, UI-017: the generated menus keep the entries, order and separators
/// of the former hand-written menus.
#[test]
fn menus_follow_group_and_order() {
    let specs = Registry::get().specs();
    let menu = |path| menu_tree(specs, path).join(" ");
    assert_eq!(
        menu("file"),
        "file.new file.open | file.save file.save-as file.close | file.preferences | file.quit"
    );
    assert_eq!(
        menu("edit"),
        "edit.undo edit.redo | edit.delete edit.select-all | edit.settings"
    );
    assert_eq!(
        menu("view"),
        "view.fit-window view.zoom-selection view.previous-zoom view.next-zoom | view.switch-plan-3d \
         view.next-tab view.previous-tab view.orbit | view.reference-lines view.problems | \
         view.place-user-origin view.reset-user-origin"
    );
    assert_eq!(menu("design"), "tool.select tool.wall tool.slab");
    assert_eq!(
        menu("project"),
        "project.storeys project.layers project.materials project.settings"
    );
    assert_eq!(menu("help"), "help.shortcuts help.about");
}

/// UI-017: a command placed in `menu/submenu` appears in a submenu that sits
/// at the place of its first command; ties keep registration order.
#[test]
fn submenus_sit_at_the_place_of_their_first_command() {
    let noop: Handler = |_, _, _| {};
    let specs = [
        CommandSpec::new("view.a", Category::View, noop).menu("view", 1, 0),
        CommandSpec::new("view.zoom.out", Category::View, noop).menu("view/zoom", 0, 1),
        CommandSpec::new("view.zoom.in", Category::View, noop).menu("view/zoom", 0, 0),
        CommandSpec::new("view.b", Category::View, noop).menu("view", 0, 5),
        CommandSpec::new("view.zoom.deep", Category::View, noop).menu("view/zoom/deep", 0, 0),
        CommandSpec::new("view.c", Category::View, noop).menu("view", 0, 5),
        CommandSpec::new("view.zoom.late", Category::View, noop).menu("view/zoom", 0, 2),
        CommandSpec::new("viewer.x", Category::View, noop).menu("viewer", 0, 0),
    ];
    // view/zoom sits at group 0, order 0 of view.zoom.in, before view.b and
    // view.c; view/zoom/deep sits at the same place within view/zoom, after
    // view.zoom.in, which was registered first.
    assert_eq!(
        menu_tree(&specs, "view").join(" "),
        "view/zoom/ view.zoom.in view/zoom/deep/ view.zoom.deep view.zoom.out view.zoom.late \
         view.b view.c | view.a"
    );
}

/// UI-011, NFR-L10N-001: every menu and submenu has a title in the
/// catalogue.
#[test]
fn every_menu_has_a_title() {
    let english = std::rc::Rc::new(Localizer::english());
    let mut paths: HashSet<&str> = MENUS.into_iter().collect();
    for spec in Registry::get().specs() {
        if let Some(place) = spec.menu {
            for (i, _) in place.path.match_indices('/') {
                paths.insert(&place.path[..i]);
            }
            paths.insert(place.path);
        }
    }
    for path in paths {
        let title = crate::i18n::with(english.clone(), || menu_title(path));
        let title = title.unwrap_or_else(|| panic!("menu {path} has no title"));
        assert!(
            !title.trim().is_empty() && !title.starts_with("menu-"),
            "{path}: {title}"
        );
    }
}

/// UI-001, UI-011: the commands the toolbar, the toolbox and the status bar
/// invoke are registered, and each tool button selects its own tool.
#[test]
fn toolbar_commands_are_registered() {
    for id in [
        file::OPEN,
        edit::UNDO,
        edit::REDO,
        view::FIT_WINDOW,
        view::PREVIOUS_ZOOM,
        view::NEXT_ZOOM,
        view::SWITCH_PLAN_3D,
        view::ORBIT,
        view::PROBLEMS,
    ] {
        assert!(Registry::get().spec(id.as_str()).is_some(), "{id}");
    }
    let mut ids = HashSet::new();
    for kind in crate::tools::ToolKind::ALL {
        let id = tools::command(kind);
        assert_eq!(spec(id.as_str()).category, Category::Design, "{kind:?}");
        assert!(ids.insert(id), "{kind:?} shares {id}");
    }
}

/// UI-001: every button of the toolbar and the toolbox shows an icon of its
/// own. The commands are the ones `ui/shell.rs` names in `toolbar` and
/// `toolbox`, so a new button there fails this test until its command is
/// listed here and has an icon.
#[test]
fn toolbar_and_toolbox_buttons_have_their_own_icons() {
    let source = include_str!("../ui/shell.rs").replace("\r\n", "\n");
    let commands = |path: &str| -> Vec<CommandId> {
        match path {
            "edit::UNDO" => vec![edit::UNDO],
            "edit::REDO" => vec![edit::REDO],
            "view::FIT_WINDOW" => vec![view::FIT_WINDOW],
            "view::ORBIT" => vec![view::ORBIT],
            "view::SWITCH_PLAN_3D" => vec![view::SWITCH_PLAN_3D],
            "tools::command" => crate::tools::ToolKind::ALL.map(tools::command).to_vec(),
            _ => panic!("ui/shell.rs shows {path} in a toolbar: list it here"),
        }
    };
    let mut ids = Vec::new();
    for name in ["toolbar", "toolbox"] {
        for id in command_paths(function_body(&source, name))
            .iter()
            .flat_map(|p| commands(p))
        {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    assert_eq!(ids.len(), 8, "{ids:?}");
    let mut icons = HashSet::new();
    for id in ids {
        let icon = spec(id.as_str())
            .icon
            .as_ref()
            .unwrap_or_else(|| panic!("{id} has no icon"));
        let uri = icon.uri().unwrap_or_else(|| panic!("{id}: {icon:?} has no uri"));
        assert!(icons.insert(uri), "{id} shares the icon {uri}");
    }
}

/// The body of the top-level function `name` in `source`, from its
/// signature to the closing brace in the first column.
fn function_body<'a>(source: &'a str, name: &str) -> &'a str {
    let start = source
        .find(&format!("\npub fn {name}("))
        .unwrap_or_else(|| panic!("no fn {name}"));
    let end = source[start..].find("\n}\n").map_or(source.len(), |e| start + e + 3);
    &source[start..end]
}

/// The commands `source` names through the command modules, such as
/// `view::ORBIT` or `tools::command`: paths that start with one of the
/// modules (not `crate::tools::`, the drawing tools).
fn command_paths(source: &str) -> Vec<String> {
    let modules = ["file::", "edit::", "view::", "tools::", "project::", "help::"];
    let mut paths = Vec::new();
    for (i, _) in source.match_indices("::") {
        let before = &source[..i + 2];
        let Some(module) = modules.iter().find(|m| before.ends_with(*m)) else {
            continue;
        };
        let start = i + 2 - module.len();
        let preceded = source[..start]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == ':');
        if preceded {
            continue;
        }
        let name: String = source[i + 2..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        paths.push(format!("{module}{name}"));
    }
    paths
}

/// UI-017: `ui/shell.rs` builds the menus from the registry and names no
/// command: no command id in a string literal and no `CommandId::new`
/// anywhere, and outside the toolbar, the toolbox and the status bar no
/// command at all except File > Open…, after which the Open recent hook
/// follows. The toolbar, the toolbox and the status bar invoke their
/// buttons' commands by id (UI-001, UI-011), through constants of the
/// command modules; the toolbar names Undo and Redo, in that order, and
/// nothing else. The scan holds on a CRLF copy of the source too, as a
/// Windows checkout with `core.autocrlf` has it.
#[test]
fn shell_names_no_command() {
    let source = include_str!("../ui/shell.rs");
    let crlf = source.replace("\r\n", "\n").replace('\n', "\r\n");
    assert_shell_names_no_command(source);
    assert_shell_names_no_command(&crlf);
}

/// The checks of `shell_names_no_command` on `source`, with its line ends
/// normalised to LF first, which `function_body` slices on.
fn assert_shell_names_no_command(source: &str) {
    let source = &source.replace("\r\n", "\n");
    let ids: HashSet<&str> = Registry::get().specs().iter().map(|s| s.id.as_str()).collect();
    // The odd pieces between quotes are the contents of string literals.
    for literal in source.split('"').skip(1).step_by(2) {
        assert!(!ids.contains(literal), "ui/shell.rs names {literal}");
    }
    assert!(!source.contains("CommandId::new"), "ui/shell.rs makes a command id");
    assert!(
        source.contains("Registry::get().menu("),
        "the menus come from the registry"
    );
    let toolbar = function_body(source, "toolbar");
    let toolbox = function_body(source, "toolbox");
    let status_bar = function_body(source, "status_bar");
    let rest = source.replace(toolbar, "").replace(toolbox, "").replace(status_bar, "");
    assert_eq!(command_paths(&rest), ["file::OPEN"]);
    assert_eq!(command_paths(toolbar), ["edit::UNDO", "edit::REDO"]);
    assert!(command_paths(toolbox).contains(&"tools::command".to_owned()));
    assert!(command_paths(status_bar).contains(&"view::PROBLEMS".to_owned()));
    // The scan finds a hand-written menu entry.
    assert_eq!(
        command_paths("item(ui, cx, spec(view::ORBIT), out); crate::tools::ToolKind::Wall"),
        ["view::ORBIT"]
    );
}
