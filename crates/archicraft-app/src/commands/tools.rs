//! The tools of the Design menu (UI-001, WAL-001, SLB-001).

use egui::Key;

use super::{Binding, Category, CommandSpec, Scope};

pub(super) fn specs() -> Vec<CommandSpec> {
    let tool = |id, order, key| {
        CommandSpec::new(id, Category::Design)
            .menu("design", 0, order)
            .bind(Binding::key(key))
            .scope(Scope::Canvas)
    };
    vec![
        tool("tool.select", 0, Key::V),
        tool("tool.wall", 1, Key::W),
        tool("tool.slab", 2, Key::S),
    ]
}
