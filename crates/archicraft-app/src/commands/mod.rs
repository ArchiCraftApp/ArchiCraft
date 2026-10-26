//! The command registry (UI-017, UI-008, HW-003).
//!
//! Every command the user can invoke is one [`CommandSpec`]: a stable id, its
//! place in the menus, its default key bindings and the scope they are active
//! in. Each area registers its commands in its own file (`file.rs`,
//! `edit.rs`, …), so adding a command means adding one spec there plus its
//! label and description in `locales/en/commands.ftl`.
//!
//! The menu bar is built from the commands' [`MenuPlace`]s
//! ([`Registry::menu`]), so a new command with a menu place appears in its
//! menu without a change to `ui/shell.rs`. Menus, shortcuts, the toolbar,
//! the toolbox and the status bar run commands through one entry,
//! [`ArchiCraftApp::invoke`], which looks the command up here and runs its
//! [`Handler`].

pub mod binding;
pub mod ctx;
pub mod edit;
pub mod file;
mod help;
pub mod keymap;
mod project;
pub mod tools;
pub mod view;
pub mod view_keys;

use std::cmp::Reverse;
use std::sync::LazyLock;

pub use binding::Binding;
pub use ctx::{CommandCtx, Enabled, Invocation, Origin, Target};
use egui::InputState;
pub use keymap::Keymap;

use crate::ArchiCraftApp;

/// The stable id of a command, lower-case and dotted such as `file.save`.
/// Ids are a public contract: keymaps and exported preferences store them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommandId(&'static str);

impl CommandId {
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }

    /// The catalogue key of the command's label, `cmd-<id>-label` with the
    /// dots of the id as hyphens (Fluent keys have no dots).
    pub fn label_key(self) -> String {
        format!("cmd-{}-label", self.0.replace('.', "-"))
    }

    /// The catalogue key of the command's one-sentence description.
    pub fn description_key(self) -> String {
        format!("cmd-{}-desc", self.0.replace('.', "-"))
    }
}

impl std::fmt::Display for CommandId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// The ids of the top-level menus, in menu-bar order (UI-011).
pub const MENUS: [&str; 6] = ["file", "edit", "view", "design", "project", "help"];

/// The title of the menu or submenu `path` in the current language, `None`
/// for a path without a title. A new submenu adds its title here and in
/// `locales/en/app.ftl`.
pub fn menu_title(path: &str) -> Option<String> {
    Some(match path {
        "file" => crate::tr!("menu-file"),
        "edit" => crate::tr!("menu-edit"),
        "view" => crate::tr!("menu-view"),
        "design" => crate::tr!("menu-design"),
        "project" => crate::tr!("menu-project"),
        "help" => crate::tr!("menu-help"),
        _ => return None,
    })
}

/// One entry of a menu built from the registry (UI-017).
#[derive(Clone, Copy, Debug)]
pub enum MenuEntry<'a> {
    Command(&'a CommandSpec),
    /// The submenu with this full path, such as `view/zoom`.
    Submenu(&'a str),
    /// The line between two groups.
    Separator,
}

/// The entries of the menu `path` from the menu places of `specs`: the
/// commands placed in it and its submenus, sorted by group and order (ties
/// in registration order), with a separator between groups. A submenu sits
/// at the place of its first command.
pub fn menu_entries<'a>(specs: &'a [CommandSpec], path: &str) -> Vec<MenuEntry<'a>> {
    let mut placed: Vec<((u16, u16, usize), MenuEntry<'a>)> = Vec::new();
    for (i, spec) in specs.iter().enumerate() {
        let Some(place) = spec.menu else {
            continue;
        };
        let key = (place.group, place.order, i);
        if place.path == path {
            placed.push((key, MenuEntry::Command(spec)));
        } else if let Some(rest) = place.path.strip_prefix(path).and_then(|r| r.strip_prefix('/')) {
            let name = rest.split('/').next().unwrap_or(rest);
            let sub = &place.path[..path.len() + 1 + name.len()];
            match placed
                .iter_mut()
                .find(|(_, e)| matches!(e, MenuEntry::Submenu(s) if *s == sub))
            {
                Some((first, _)) => *first = (*first).min(key),
                None => placed.push((key, MenuEntry::Submenu(sub))),
            }
        }
    }
    placed.sort_by_key(|(key, _)| *key);
    let mut entries = Vec::new();
    let mut group = None;
    for ((g, _, _), entry) in placed {
        if group.is_some_and(|previous| previous != g) {
            entries.push(MenuEntry::Separator);
        }
        group = Some(g);
        entries.push(entry);
    }
    entries
}

/// The area a command belongs to, for grouping in the command palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    File,
    Edit,
    View,
    Design,
    Project,
    Help,
}

/// Where a command appears in the menus: the menu path (a top-level menu of
/// [`MENUS`], optionally followed by `/submenu`), the separator group within
/// the menu and the order within the group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuPlace {
    pub path: &'static str,
    pub group: u16,
    pub order: u16,
}

/// When a binding is active.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
    /// Everywhere except while a text field has focus.
    Global,
    /// Everywhere, also while a text field has focus (UI-008).
    TextSafe,
    /// In the views while no tool is taking point input: single-key
    /// shortcuts, which typed coordinates would otherwise trigger.
    Canvas,
    /// While a tool is taking point input.
    PointInput,
}

impl Scope {
    /// Whether a key can be active in both scopes at once, so two commands
    /// must not share it. Global and TextSafe bindings are active everywhere;
    /// Canvas and PointInput are never active together ([`Scope::is_active`]),
    /// so Backspace can delete the selection in one and remove the last point
    /// in the other. A key the views handle both while and while not drawing
    /// (R cycles the wall reference line already before the first point) is
    /// Global, so no Canvas or PointInput binding can take it.
    pub fn overlaps(self, other: Scope) -> bool {
        use Scope::*;
        matches!((self, other), (Global | TextSafe, _) | (_, Global | TextSafe)) || self == other
    }

    /// Whether bindings of this scope are active while a text field has
    /// focus (`typing`) or a tool takes point input (`drawing`). While
    /// typing only TextSafe bindings work, so single keys reach the text
    /// field (UI-008); while drawing Canvas bindings are off, so typed values
    /// and the tool's own keys (Backspace removes the last point) reach the
    /// tool.
    pub fn is_active(self, typing: bool, drawing: bool) -> bool {
        match self {
            Scope::TextSafe => true,
            Scope::Global => !typing,
            Scope::Canvas => !typing && !drawing,
            Scope::PointInput => !typing && drawing,
        }
    }
}

/// Runs a command on its target.
pub type Handler = fn(&mut ArchiCraftApp, &egui::Context, Target);

/// The description of one command.
#[derive(Clone, Debug)]
pub struct CommandSpec {
    pub id: CommandId,
    pub category: Category,
    /// The main-menu entry, if any.
    pub menu: Option<MenuPlace>,
    /// Listed in the command palette only, without a menu entry.
    pub palette_only: bool,
    /// No main-menu entry, but listed in the palette and reachable by
    /// shortcut against the active target (context-menu commands).
    pub context_only: bool,
    /// The default key bindings; the first is shown in menus.
    pub default_bindings: Vec<Binding>,
    pub scope: Scope,
    /// The view that has the keys matches the command's bindings itself,
    /// against the [`Keymap`] and the state of its tool, so the shortcut loop
    /// leaves them to it (the plan view's local keys, HW-003).
    pub local: bool,
    /// The bindings cannot be changed: Enter and Esc of point input keep
    /// their standard meaning, as in text fields and dialogs (UI-009).
    pub fixed: bool,
    /// Whether the command can run now; menus grey it out otherwise. The
    /// handler still copes with a disabled state, because a shortcut runs it
    /// regardless (Undo reports that there is nothing to undo).
    pub enabled: fn(&CommandCtx) -> Enabled,
    /// For an on/off command, whether it is on; menus show a check box.
    pub checked: Option<fn(&CommandCtx) -> bool>,
    /// A label that depends on the state, such as "Undo Create wall"; `None`
    /// from the function falls back to the catalogue label.
    pub dynamic_label: Option<fn(&CommandCtx) -> Option<String>>,
    pub handler: Handler,
}

impl CommandSpec {
    /// An always enabled command without menu entry and bindings, in the
    /// [`Scope::Global`] scope, that runs `handler`.
    pub fn new(id: &'static str, category: Category, handler: Handler) -> Self {
        Self {
            id: CommandId::new(id),
            category,
            menu: None,
            palette_only: false,
            context_only: false,
            default_bindings: Vec::new(),
            scope: Scope::Global,
            local: false,
            fixed: false,
            enabled: |_| Enabled::Yes,
            checked: None,
            dynamic_label: None,
            handler,
        }
    }

    /// Places the command in the menu `path`, in separator group `group` at
    /// position `order`.
    pub fn menu(mut self, path: &'static str, group: u16, order: u16) -> Self {
        self.menu = Some(MenuPlace { path, group, order });
        self
    }

    /// Adds a default binding.
    pub fn bind(mut self, binding: Binding) -> Self {
        self.default_bindings.push(binding);
        self
    }

    pub fn scope(mut self, scope: Scope) -> Self {
        self.scope = scope;
        self
    }

    /// Lists the command in the palette only, without a menu entry.
    pub fn palette_only(mut self) -> Self {
        self.palette_only = true;
        self
    }

    /// Leaves the command's keys to the view that has them (see
    /// [`CommandSpec::local`]).
    pub fn local(mut self) -> Self {
        self.local = true;
        self
    }

    /// Makes the default bindings the only ones (see [`CommandSpec::fixed`]).
    pub fn fixed(mut self) -> Self {
        self.fixed = true;
        self
    }

    pub fn enabled_when(mut self, enabled: fn(&CommandCtx) -> Enabled) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn checked_when(mut self, checked: fn(&CommandCtx) -> bool) -> Self {
        self.checked = Some(checked);
        self
    }

    pub fn dynamic_label(mut self, label: fn(&CommandCtx) -> Option<String>) -> Self {
        self.dynamic_label = Some(label);
        self
    }

    /// The command's label in the current language.
    pub fn label(&self) -> String {
        crate::tr!(&self.id.label_key())
    }

    /// The label shown in menus in the state `ctx`: the dynamic label if the
    /// command has one, otherwise [`CommandSpec::label`].
    pub fn label_in(&self, ctx: &CommandCtx) -> String {
        self.dynamic_label
            .and_then(|label| label(ctx))
            .unwrap_or_else(|| self.label())
    }

    /// The command's description in the current language.
    pub fn description(&self) -> String {
        crate::tr!(&self.id.description_key())
    }
}

/// All commands, built once at startup from the registration functions of
/// the areas.
#[derive(Debug)]
pub struct Registry {
    specs: Vec<CommandSpec>,
    /// Every default binding with the index of its command, in the order
    /// key presses are matched: more modifiers first, because egui ignores
    /// an extra Shift, so Primary+Z would otherwise swallow Primary+Shift+Z
    /// and F would swallow Shift+F (UI-008).
    match_order: Vec<(Binding, usize)>,
}

impl Registry {
    fn build() -> Self {
        let specs = [
            file::specs(),
            edit::specs(),
            view::specs(),
            tools::specs(),
            project::specs(),
            help::specs(),
            view_keys::specs(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<CommandSpec>>();
        let mut match_order: Vec<(Binding, usize)> = specs
            .iter()
            .enumerate()
            .flat_map(|(i, spec)| spec.default_bindings.iter().map(move |&b| (b, i)))
            .collect();
        match_order.sort_by_key(|(binding, _)| Reverse(binding.modifier_count()));
        Self { specs, match_order }
    }

    /// The commands of the application.
    pub fn get() -> &'static Registry {
        static REGISTRY: LazyLock<Registry> = LazyLock::new(Registry::build);
        &REGISTRY
    }

    /// All commands, in registration order.
    pub fn specs(&self) -> &[CommandSpec] {
        &self.specs
    }

    /// The command with the given id.
    pub fn spec(&self, id: &str) -> Option<&CommandSpec> {
        self.specs.iter().find(|s| s.id.as_str() == id)
    }

    /// The entries of the menu `path` (see [`menu_entries`]).
    pub fn menu(&self, path: &str) -> Vec<MenuEntry<'_>> {
        menu_entries(&self.specs, path)
    }

    /// The default bindings with their commands, in match order.
    pub fn bindings_in_match_order(&self) -> impl Iterator<Item = (Binding, &CommandSpec)> {
        self.match_order.iter().map(|&(binding, i)| (binding, &self.specs[i]))
    }

    /// Consumes the key presses of this frame that match a binding whose
    /// scope is `active`, in match order, and returns their commands
    /// (UI-008, HW-003). Bindings of inactive scopes leave their key press
    /// to the text field or tool that has the keys, and so do the bindings
    /// of [local](CommandSpec::local) commands.
    pub fn consume_shortcuts(&self, input: &mut InputState, active: impl Fn(Scope) -> bool) -> Vec<CommandId> {
        self.bindings_in_match_order()
            .filter(|(_, spec)| !spec.local && active(spec.scope))
            .filter(|(binding, _)| input.consume_shortcut(&binding.shortcut()))
            .map(|(_, spec)| spec.id)
            .collect()
    }
}

#[cfg(test)]
mod tests;
