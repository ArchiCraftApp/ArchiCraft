//! The tools of the Design menu (UI-001, WAL-001, SLB-001).

use egui::Key;

use super::{Binding, Category, CommandId, CommandSpec, Handler, Scope};
use crate::tools::ToolKind;

pub const SELECT: CommandId = CommandId::new("tool.select");
pub const WALL: CommandId = CommandId::new("tool.wall");
pub const SLAB: CommandId = CommandId::new("tool.slab");

/// The command that selects the tool `kind`, which the toolbox buttons
/// invoke (UI-001).
pub fn command(kind: ToolKind) -> CommandId {
    match kind {
        ToolKind::Select => SELECT,
        ToolKind::Wall => WALL,
        ToolKind::Slab => SLAB,
    }
}

pub(super) fn specs() -> Vec<CommandSpec> {
    let tool = |id: CommandId, order, key, handler: Handler| {
        CommandSpec::new(id.as_str(), Category::Design, handler)
            .menu("design", 0, order)
            .bind(Binding::key(key))
            .scope(Scope::Canvas)
    };
    vec![
        tool(SELECT, 0, Key::V, |app, _, _| app.ws.select_tool(ToolKind::Select)),
        tool(WALL, 1, Key::W, |app, _, _| app.ws.select_tool(ToolKind::Wall)),
        tool(SLAB, 2, Key::S, |app, _, _| app.ws.select_tool(ToolKind::Slab)),
    ]
}
