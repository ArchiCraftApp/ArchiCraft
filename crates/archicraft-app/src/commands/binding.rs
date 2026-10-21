//! Platform-neutral key bindings (UI-008, HW-003).

use egui::os::OperatingSystem;
use egui::{Key, KeyboardShortcut, ModifierNames, Modifiers};

/// A key with modifiers, the same on every platform: `primary` is Cmd on
/// macOS and Ctrl elsewhere (UI-008), `ctrl` is the Control key everywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Binding {
    pub primary: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: Key,
}

impl Binding {
    /// `key` alone.
    pub const fn key(key: Key) -> Self {
        Self {
            primary: false,
            ctrl: false,
            alt: false,
            shift: false,
            key,
        }
    }

    /// Primary (Cmd on macOS, Ctrl elsewhere) + `key`.
    pub const fn primary(key: Key) -> Self {
        Self {
            primary: true,
            ..Self::key(key)
        }
    }

    /// Ctrl + `key` on every platform, also on macOS.
    pub const fn ctrl(key: Key) -> Self {
        Self {
            ctrl: true,
            ..Self::key(key)
        }
    }

    /// The same binding with Shift held.
    pub const fn shift(self) -> Self {
        Self { shift: true, ..self }
    }

    /// The modifiers as egui matches them logically: `primary` is
    /// [`Modifiers::COMMAND`], which egui sets for Cmd on macOS and for Ctrl
    /// elsewhere.
    pub fn modifiers(self) -> Modifiers {
        Modifiers {
            alt: self.alt,
            ctrl: self.ctrl,
            shift: self.shift,
            mac_cmd: false,
            command: self.primary,
        }
    }

    /// The shortcut to match key events against, on any platform.
    pub fn shortcut(self) -> KeyboardShortcut {
        KeyboardShortcut::new(self.modifiers(), self.key)
    }

    /// The physical keys on `os`: Cmd ([`Modifiers::MAC_CMD`]) on macOS, Ctrl
    /// on Windows and Linux (UI-008).
    pub fn for_os(self, os: OperatingSystem) -> KeyboardShortcut {
        let mut modifiers = self.modifiers();
        modifiers.command = false;
        if self.primary {
            if os.is_mac() {
                modifiers.mac_cmd = true;
            } else {
                modifiers.ctrl = true;
            }
        }
        KeyboardShortcut::new(modifiers, self.key)
    }

    /// The text shown for the binding on `os`, such as `Cmd+S` on macOS and
    /// `Ctrl+S` elsewhere.
    pub fn display(self, os: OperatingSystem) -> String {
        self.for_os(os).format(&ModifierNames::NAMES, os.is_mac())
    }

    /// The number of modifier keys. Bindings with more modifiers are matched
    /// first, because egui ignores an extra Shift: otherwise Primary+Z would
    /// swallow Primary+Shift+Z.
    pub fn modifier_count(self) -> usize {
        [self.primary, self.ctrl, self.alt, self.shift]
            .into_iter()
            .filter(|&b| b)
            .count()
    }

    /// A key without modifiers. Such bindings must not fire while text is
    /// typed (UI-008).
    pub fn is_single_key(self) -> bool {
        self.modifier_count() == 0
    }
}
