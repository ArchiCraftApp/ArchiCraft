//! The command registry (UI-017, UI-008, HW-003).
//!
//! Every command the user can invoke is one [`CommandSpec`]: a stable id, its
//! place in the menus, its default key bindings and the scope they are active
//! in. Each area registers its commands in its own file (`file.rs`,
//! `edit.rs`, …), so adding a command means adding one spec there plus its
//! label and description in `locales/en/commands.ftl`.

pub mod binding;
mod edit;
mod file;
mod help;
mod project;
mod tools;
mod view;

use std::sync::LazyLock;

pub use binding::Binding;

/// The stable id of a command, lower-case and dotted such as `file.save`.
/// Ids are a public contract: keymaps and exported preferences store them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommandId(&'static str);

impl CommandId {
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    pub fn as_str(self) -> &'static str {
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
    /// Canvas and PointInput count as overlapping, so that a key keeps one
    /// meaning in the views.
    pub fn overlaps(self, other: Scope) -> bool {
        use Scope::*;
        matches!(
            (self, other),
            (Global | TextSafe, _) | (_, Global | TextSafe) | (Canvas | PointInput, Canvas | PointInput)
        )
    }
}

/// The description of one command.
#[derive(Clone, Debug, PartialEq)]
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
}

impl CommandSpec {
    /// A command without menu entry and bindings, in the [`Scope::Global`]
    /// scope.
    pub fn new(id: &'static str, category: Category) -> Self {
        Self {
            id: CommandId::new(id),
            category,
            menu: None,
            palette_only: false,
            context_only: false,
            default_bindings: Vec::new(),
            scope: Scope::Global,
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

    /// The command's label in the current language.
    pub fn label(&self) -> String {
        crate::tr!(&self.id.label_key())
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
        ]
        .into_iter()
        .flatten()
        .collect();
        Self { specs }
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
}

#[cfg(test)]
mod tests;
