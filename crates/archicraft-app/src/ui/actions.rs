//! Application commands with their menu labels and default shortcuts
//! (UI-008, UI-011, UI-017, Appendix D.8).

use std::cmp::Reverse;

use egui::{InputState, Key, KeyboardShortcut, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    New,
    Open,
    Save,
    SaveAs,
    Close,
    Quit,
    Undo,
    Redo,
    Delete,
    SelectAll,
    ToolSelect,
    ToolWall,
    ToolSlab,
    Settings,
    FitWindow,
    ZoomSelection,
    PreviousZoom,
    NextZoom,
    NextTab,
    PreviousTab,
    SwitchPlan3d,
    OrbitMode,
    ToggleReferenceLines,
    PlaceUserOrigin,
    ResetUserOrigin,
    Storeys,
    Layers,
    Materials,
    ProjectSettings,
    Preferences,
    Problems,
    Shortcuts,
    About,
}

impl Action {
    pub const ALL: [Action; 33] = [
        Action::New,
        Action::Open,
        Action::Save,
        Action::SaveAs,
        Action::Close,
        Action::Quit,
        Action::Undo,
        Action::Redo,
        Action::Delete,
        Action::SelectAll,
        Action::ToolSelect,
        Action::ToolWall,
        Action::ToolSlab,
        Action::Settings,
        Action::FitWindow,
        Action::ZoomSelection,
        Action::PreviousZoom,
        Action::NextZoom,
        Action::NextTab,
        Action::PreviousTab,
        Action::SwitchPlan3d,
        Action::OrbitMode,
        Action::ToggleReferenceLines,
        Action::PlaceUserOrigin,
        Action::ResetUserOrigin,
        Action::Storeys,
        Action::Layers,
        Action::Materials,
        Action::ProjectSettings,
        Action::Preferences,
        Action::Problems,
        Action::Shortcuts,
        Action::About,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Action::New => "New project",
            Action::Open => "Open…",
            Action::Save => "Save",
            Action::SaveAs => "Save as…",
            Action::Close => "Close project",
            Action::Quit => "Quit",
            Action::Undo => "Undo",
            Action::Redo => "Redo",
            Action::Delete => "Delete",
            Action::SelectAll => "Select all",
            Action::ToolSelect => "Selection tool",
            Action::ToolWall => "Wall tool",
            Action::ToolSlab => "Slab tool",
            Action::Settings => "Settings of tool or selection…",
            Action::FitWindow => "Fit to window",
            Action::ZoomSelection => "Zoom to selection",
            Action::PreviousZoom => "Previous zoom",
            Action::NextZoom => "Next zoom",
            Action::NextTab => "Next tab",
            Action::PreviousTab => "Previous tab",
            Action::SwitchPlan3d => "Switch between plan and 3D",
            Action::OrbitMode => "Orbit mode",
            Action::ToggleReferenceLines => "Show wall reference lines",
            Action::PlaceUserOrigin => "Place user origin",
            Action::ResetUserOrigin => "Reset user origin",
            Action::Storeys => "Storeys…",
            Action::Layers => "Layers…",
            Action::Materials => "Materials…",
            Action::ProjectSettings => "Project settings…",
            Action::Preferences => "Preferences…",
            Action::Problems => "Problems",
            Action::Shortcuts => "Keyboard shortcuts",
            Action::About => "About ArchiCraft",
        }
    }

    /// The default shortcut. `COMMAND` is Cmd on macOS and Ctrl elsewhere;
    /// combinations reserved by the operating systems are avoided (UI-008).
    pub fn shortcut(self) -> Option<KeyboardShortcut> {
        let cmd = Modifiers::COMMAND;
        let cmd_shift = Modifiers::COMMAND.plus(Modifiers::SHIFT);
        let none = Modifiers::NONE;
        let (modifiers, key) = match self {
            Action::New => (cmd, Key::N),
            Action::Open => (cmd, Key::O),
            Action::Save => (cmd, Key::S),
            Action::SaveAs => (cmd_shift, Key::S),
            Action::Close => (cmd, Key::W),
            Action::Quit => (cmd, Key::Q),
            Action::Undo => (cmd, Key::Z),
            Action::Redo => (cmd_shift, Key::Z),
            Action::Delete => (none, Key::Delete),
            Action::SelectAll => (cmd, Key::A),
            Action::ToolSelect => (none, Key::V),
            Action::ToolWall => (none, Key::W),
            Action::ToolSlab => (none, Key::S),
            Action::Settings => (cmd, Key::T),
            Action::FitWindow => (none, Key::F),
            Action::ZoomSelection => (Modifiers::SHIFT, Key::F),
            Action::PreviousZoom => (cmd, Key::OpenBracket),
            Action::NextZoom => (cmd, Key::CloseBracket),
            Action::NextTab => (Modifiers::CTRL, Key::Tab),
            Action::PreviousTab => (Modifiers::CTRL.plus(Modifiers::SHIFT), Key::Tab),
            Action::SwitchPlan3d => (none, Key::F3),
            Action::OrbitMode => (none, Key::O),
            Action::Problems => (cmd_shift, Key::P),
            _ => return None,
        };
        Some(KeyboardShortcut::new(modifiers, key))
    }

    /// The actions in the order their shortcuts are matched. egui ignores an
    /// extra Shift or Alt when matching a shortcut, so the ones with more
    /// modifiers go first: Ctrl+Shift+Tab is the previous tab, not the next
    /// one (UI-006), Cmd+Shift+Z redo, not undo, Cmd+Shift+S save as, not
    /// save (UI-008), and Shift+F zooms to the selection, not to the window
    /// (VIEW-014).
    pub fn in_match_order() -> [Action; Self::ALL.len()] {
        let mut actions = Self::ALL;
        actions.sort_by_key(|a| Reverse(a.shortcut().map_or(0, |s| modifier_count(s.modifiers))));
        actions
    }

    /// Consumes the shortcuts pressed this frame in match order and returns
    /// their actions. Actions for which `enabled` is false are skipped and
    /// leave their key press to the others.
    pub fn consume_shortcuts(input: &mut InputState, enabled: impl Fn(Action) -> bool) -> Vec<Action> {
        Self::in_match_order()
            .into_iter()
            .filter(|&action| enabled(action))
            .filter(|action| action.shortcut().is_some_and(|s| input.consume_shortcut(&s)))
            .collect()
    }

    /// Shortcuts without modifier keys must not fire while typing (UI-008).
    pub fn is_single_key(self) -> bool {
        self.shortcut().is_some_and(|s| s.modifiers.is_none())
    }

    /// Shortcuts that stay active while a text field has focus. The others
    /// (select all, undo, delete, tab switching…) belong to the text field
    /// then.
    pub fn works_while_typing(self) -> bool {
        matches!(
            self,
            Action::New
                | Action::Open
                | Action::Save
                | Action::SaveAs
                | Action::Close
                | Action::Quit
                | Action::Settings
                | Action::Problems
        )
    }
}

/// The number of modifier keys a shortcut needs.
fn modifier_count(m: Modifiers) -> usize {
    [m.alt, m.ctrl, m.shift, m.mac_cmd, m.command]
        .into_iter()
        .filter(|&b| b)
        .count()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn shortcuts_are_unique() {
        let mut seen = HashSet::new();
        for action in Action::ALL {
            if let Some(s) = action.shortcut() {
                assert!(seen.insert((s.modifiers, s.logical_key)), "{action:?}");
            }
        }
    }

    #[test]
    fn appendix_d8_commands_have_shortcuts() {
        for action in [
            Action::Save,
            Action::Undo,
            Action::Redo,
            Action::Delete,
            Action::SelectAll,
            Action::ToolWall,
            Action::ToolSlab,
            Action::SwitchPlan3d,
            Action::NextTab,
            Action::PreviousTab,
            Action::FitWindow,
            Action::ZoomSelection,
            Action::OrbitMode,
        ] {
            assert!(action.shortcut().is_some(), "{action:?}");
        }
    }

    /// UI-008: a shortcut is matched before every other shortcut on the same
    /// key whose modifiers it contains, because egui would let the one with
    /// fewer modifiers take the key press.
    #[test]
    fn shortcuts_with_more_modifiers_are_matched_first() {
        let order = Action::in_match_order();
        let position = |a: Action| order.iter().position(|&b| b == a);
        for a in Action::ALL {
            for b in Action::ALL {
                let (Some(sa), Some(sb)) = (a.shortcut(), b.shortcut()) else {
                    continue;
                };
                if a != b && sa.logical_key == sb.logical_key && sa.modifiers.contains(sb.modifiers) {
                    assert!(position(a) < position(b), "{a:?} must be matched before {b:?}");
                }
            }
        }
    }

    /// The actions a key press with `modifiers` triggers.
    fn pressed(modifiers: Modifiers, key: Key) -> Vec<Action> {
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
            triggered = ui.ctx().input_mut(|i| Action::consume_shortcuts(i, |_| true));
        });
        output.drop_without_applying_deltas();
        triggered
    }

    /// UI-008, UI-006, VIEW-014: Cmd+Shift+Z redoes and Cmd+Shift+S saves
    /// as, on macOS and elsewhere; the shortcuts without Shift on the same
    /// keys keep their own action.
    #[test]
    fn a_shortcut_with_shift_runs_its_own_action() {
        let mac_cmd = Modifiers::MAC_CMD.plus(Modifiers::COMMAND);
        let ctrl = Modifiers::CTRL.plus(Modifiers::COMMAND);
        let shift = Modifiers::SHIFT;
        for cmd in [mac_cmd, ctrl] {
            for (modifiers, key, expected) in [
                (cmd.plus(shift), Key::Z, Action::Redo),
                (cmd, Key::Z, Action::Undo),
                (cmd.plus(shift), Key::S, Action::SaveAs),
                (cmd, Key::S, Action::Save),
            ] {
                assert_eq!(pressed(modifiers, key), [expected], "{modifiers:?}+{key:?}");
            }
        }
        for (modifiers, key, expected) in [
            (Modifiers::CTRL.plus(shift), Key::Tab, Action::PreviousTab),
            (Modifiers::CTRL, Key::Tab, Action::NextTab),
            (shift, Key::F, Action::ZoomSelection),
            (Modifiers::NONE, Key::F, Action::FitWindow),
        ] {
            assert_eq!(pressed(modifiers, key), [expected], "{modifiers:?}+{key:?}");
        }
    }
}
