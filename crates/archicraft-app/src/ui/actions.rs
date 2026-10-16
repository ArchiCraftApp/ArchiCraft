//! Application commands with their menu labels and default shortcuts
//! (UI-008, UI-011, UI-017, Appendix D.8).

use egui::{Key, KeyboardShortcut, Modifiers};

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
    pub const ALL: [Action; 31] = [
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
}
