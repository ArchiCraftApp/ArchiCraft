//! The tools of the Design menu (UI-001, WAL-001, SLB-001).

use egui::Key;

use super::{Binding, Category, CommandSpec, Handler, Scope};
use crate::tools::ToolKind;

pub(super) fn specs() -> Vec<CommandSpec> {
    let tool = |id, order, key, handler: Handler| {
        CommandSpec::new(id, Category::Design, handler)
            .menu("design", 0, order)
            .bind(Binding::key(key))
            .scope(Scope::Canvas)
    };
    vec![
        tool("tool.select", 0, Key::V, |app, _, _| {
            app.ws.select_tool(ToolKind::Select)
        }),
        tool("tool.wall", 1, Key::W, |app, _, _| app.ws.select_tool(ToolKind::Wall)),
        tool("tool.slab", 2, Key::S, |app, _, _| app.ws.select_tool(ToolKind::Slab)),
    ]
}
