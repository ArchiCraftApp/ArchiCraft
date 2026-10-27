//! The key combinations the operating systems keep for themselves (UI-008):
//! a default binding must not be one of them on any platform, because the
//! system takes the key press before the application sees it. The tables
//! hold macOS, Windows and Linux (GNOME and KDE) and are checked on every
//! host, so a Mac build cannot ship a binding that Windows reserves.

use egui::Key;
use egui::os::OperatingSystem;

use super::Binding;

/// The desktop platforms the tables cover, in display order.
pub const PLATFORMS: [OperatingSystem; 3] = [OperatingSystem::Mac, OperatingSystem::Windows, OperatingSystem::Nix];

/// The keys of a reserved combination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keys {
    One(Key),
    /// The four arrow keys.
    Arrows,
    /// Every key.
    Any,
}

impl Keys {
    fn contains(self, key: Key) -> bool {
        match self {
            Keys::One(k) => k == key,
            Keys::Arrows => matches!(key, Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown),
            Keys::Any => true,
        }
    }
}

/// One reserved combination with the physical modifiers held: `logo` is Cmd
/// on macOS, the Windows key on Windows and Super on Linux.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reserved {
    pub logo: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub keys: Keys,
    /// What the system does with it, for docs/keymap.md.
    pub purpose: &'static str,
}

impl Reserved {
    const fn new(keys: Keys, purpose: &'static str) -> Self {
        Self {
            logo: false,
            ctrl: false,
            alt: false,
            shift: false,
            keys,
            purpose,
        }
    }

    const fn logo(self) -> Self {
        Self { logo: true, ..self }
    }

    const fn ctrl(self) -> Self {
        Self { ctrl: true, ..self }
    }

    const fn alt(self) -> Self {
        Self { alt: true, ..self }
    }

    const fn shift(self) -> Self {
        Self { shift: true, ..self }
    }

    /// Whether `binding`, pressed on `os`, is this combination: the same
    /// key with exactly the same physical modifiers.
    pub fn matches(&self, binding: Binding, os: OperatingSystem) -> bool {
        let shortcut = binding.for_os(os);
        let m = shortcut.modifiers;
        (m.mac_cmd, m.ctrl, m.alt, m.shift) == (self.logo, self.ctrl, self.alt, self.shift)
            && self.keys.contains(shortcut.logical_key)
    }

    /// The combination as shown on `os`, in the spelling of
    /// [`Binding::display`]: `Cmd+Shift+3`, `Win+any key`.
    pub fn display(&self, os: OperatingSystem) -> String {
        let mut parts = Vec::new();
        let logo = match os {
            OperatingSystem::Mac => "Cmd",
            OperatingSystem::Windows => "Win",
            _ => "Super",
        };
        if os.is_mac() {
            // The order of egui's ModifierNames on macOS.
            parts.extend([
                (self.ctrl, "Ctrl"),
                (self.shift, "Shift"),
                (self.alt, "Option"),
                (self.logo, logo),
            ]);
        } else {
            parts.extend([
                (self.logo, logo),
                (self.ctrl, "Ctrl"),
                (self.alt, "Alt"),
                (self.shift, "Shift"),
            ]);
        }
        let mut names: Vec<&str> = parts.into_iter().filter(|(held, _)| *held).map(|(_, n)| n).collect();
        names.push(match self.keys {
            Keys::One(key) => key.name(),
            Keys::Arrows => "arrow keys",
            Keys::Any => "any key",
        });
        names.join("+")
    }
}

const fn key(key: Key, purpose: &'static str) -> Reserved {
    Reserved::new(Keys::One(key), purpose)
}

/// macOS: the system-wide shortcuts of the window server, Spotlight, the
/// input sources, screenshots and Mission Control.
const MAC: &[Reserved] = &[
    key(Key::H, "hide the application").logo(),
    key(Key::H, "hide the other applications").logo().alt(),
    key(Key::M, "minimise the window").logo(),
    key(Key::Tab, "switch applications").logo(),
    key(Key::Tab, "switch applications backwards").logo().shift(),
    key(Key::Space, "Spotlight").logo(),
    key(Key::Space, "Finder search").logo().alt(),
    key(Key::Space, "previous input source").ctrl(),
    key(Key::Space, "emoji and symbols").logo().ctrl(),
    key(Key::Backtick, "next window of the application").logo(),
    key(Key::Escape, "force quit").logo().alt(),
    key(Key::Num3, "screenshot of the screen").logo().shift(),
    key(Key::Num4, "screenshot of a selection").logo().shift(),
    key(Key::Num5, "screenshot and recording options").logo().shift(),
    Reserved::new(Keys::Arrows, "Mission Control and spaces").ctrl(),
    key(Key::Q, "lock the screen").logo().ctrl(),
    key(Key::Q, "log out").logo().shift(),
];

/// Windows: the shell's task switching, Start menu, Task Manager and secure
/// attention keys; the Windows key with any key belongs to the shell.
const WINDOWS: &[Reserved] = &[
    key(Key::Tab, "switch windows").alt(),
    key(Key::Tab, "switch windows backwards").alt().shift(),
    key(Key::Space, "window menu").alt(),
    key(Key::F4, "close the window").alt(),
    key(Key::Escape, "Start menu").ctrl(),
    key(Key::Escape, "Task Manager").ctrl().shift(),
    key(Key::Delete, "security options").ctrl().alt(),
    Reserved::new(Keys::Any, "shell shortcuts").logo(),
];

/// Linux: the defaults of GNOME and KDE Plasma together, since both are
/// supported desktops; Super with any key belongs to the desktop.
const LINUX: &[Reserved] = &[
    key(Key::F1, "application menu").alt(),
    key(Key::F2, "run command").alt(),
    key(Key::F4, "close the window").alt(),
    key(Key::F7, "move the window").alt(),
    key(Key::F8, "resize the window").alt(),
    key(Key::Tab, "switch windows").alt(),
    key(Key::Tab, "switch windows backwards").alt().shift(),
    key(Key::Space, "window menu, KRunner").alt(),
    key(Key::T, "open a terminal").ctrl().alt(),
    key(Key::Delete, "log out").ctrl().alt(),
    Reserved::new(Keys::Arrows, "switch workspaces").ctrl().alt(),
    key(Key::F1, "switch to desktop 1 (KDE)").ctrl(),
    key(Key::F2, "switch to desktop 2 (KDE)").ctrl(),
    key(Key::F3, "switch to desktop 3 (KDE)").ctrl(),
    key(Key::F4, "switch to desktop 4 (KDE)").ctrl(),
    key(Key::U, "Unicode input (IBus)").ctrl().shift(),
    Reserved::new(Keys::Any, "desktop shortcuts").logo(),
];

/// The reserved combinations of `os`; none for a platform without a table.
pub fn reserved(os: OperatingSystem) -> &'static [Reserved] {
    match os {
        OperatingSystem::Mac => MAC,
        OperatingSystem::Windows => WINDOWS,
        OperatingSystem::Nix => LINUX,
        _ => &[],
    }
}

/// The first platform of [`PLATFORMS`] that reserves `binding`, with the
/// combination it reserves.
pub fn reserved_by(binding: Binding) -> Option<(OperatingSystem, &'static Reserved)> {
    PLATFORMS
        .into_iter()
        .find_map(|os| reserved(os).iter().find(|r| r.matches(binding, os)).map(|r| (os, r)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UI-008: the OS tables hold the combinations each system reserves, and a
    /// binding matches one only with exactly its physical modifiers.
    #[test]
    fn os_reserved_tables_hold_the_system_shortcuts() {
        use OperatingSystem::{Mac, Nix, Windows};
        let p = Binding::primary;
        let ctrl = Binding::ctrl;
        let alt = |key| Binding::key(key).alt();
        let is_reserved = |b: Binding, os| reserved(os).iter().any(|r| r.matches(b, os));
        for b in [
            p(Key::H),
            p(Key::M),
            p(Key::Tab),
            p(Key::Space),
            ctrl(Key::Space),
            p(Key::Backtick),
            p(Key::Escape).alt(),
            p(Key::Num3).shift(),
            p(Key::Num4).shift(),
            p(Key::Num5).shift(),
            ctrl(Key::ArrowLeft),
            ctrl(Key::ArrowRight),
            ctrl(Key::ArrowUp),
            ctrl(Key::ArrowDown),
        ] {
            assert!(is_reserved(b, Mac), "macOS {}", b.display(Mac));
        }
        for b in [
            alt(Key::Tab),
            alt(Key::Space),
            ctrl(Key::Escape),
            ctrl(Key::Escape).shift(),
        ] {
            assert!(is_reserved(b, Windows), "Windows {}", b.display(Windows));
        }
        for b in [
            alt(Key::F2),
            alt(Key::F7),
            alt(Key::F8),
            ctrl(Key::T).alt(),
            ctrl(Key::ArrowLeft).alt(),
            ctrl(Key::ArrowDown).alt(),
        ] {
            assert!(is_reserved(b, Nix), "Linux {}", b.display(Nix));
        }
        // The Windows and Super keys take every key; egui has no binding for
        // them, so the tables say so instead.
        let logo_any = |os| {
            reserved(os)
                .iter()
                .any(|r| r.logo && !r.ctrl && !r.alt && !r.shift && r.keys == Keys::Any)
        };
        assert!(logo_any(Windows) && logo_any(Nix));
        // Exactly the modifiers: Cmd+3 and Cmd+Shift+H are free on macOS, and
        // Primary+H is Ctrl+H, not reserved, on Windows and Linux.
        assert!(!is_reserved(p(Key::Num3), Mac));
        assert!(!is_reserved(p(Key::H).shift(), Mac));
        assert!(!is_reserved(p(Key::H), Windows) && !is_reserved(p(Key::H), Nix));
        assert_eq!(reserved(Mac)[0].display(Mac), "Cmd+H");
    }
}
