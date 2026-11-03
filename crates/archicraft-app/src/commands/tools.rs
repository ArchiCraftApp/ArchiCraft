//! The tools of the Design menu (UI-001, WAL-001, SLB-001, MEA-001).

use egui::Key;

use super::{Binding, Category, CommandCtx, CommandId, CommandSpec, Handler, Scope};
use crate::tools::ToolKind;
use crate::ui::icons;

pub const SELECT: CommandId = CommandId::new("tool.select");
pub const WALL: CommandId = CommandId::new("tool.wall");
pub const SLAB: CommandId = CommandId::new("tool.slab");
/// The measure tool (MEA-001), also a toolbar button (UI-011).
pub const MEASURE: CommandId = CommandId::new("tool.measure");

/// The command that selects the tool `kind`, which the toolbox buttons
/// invoke (UI-001).
pub fn command(kind: ToolKind) -> CommandId {
    match kind {
        ToolKind::Select => SELECT,
        ToolKind::Wall => WALL,
        ToolKind::Slab => SLAB,
        ToolKind::Measure => MEASURE,
    }
}

pub(super) fn specs() -> Vec<CommandSpec> {
    // Checked while the tool is active, as the toolbox shows it.
    let tool = |id: CommandId, order, key, icon, handler: Handler, active: fn(&CommandCtx) -> bool| {
        CommandSpec::new(id.as_str(), Category::Design, handler)
            .menu("design", 0, order)
            .bind(Binding::key(key))
            .scope(Scope::Canvas)
            .icon(icon)
            .checked_when(active)
    };
    vec![
        tool(
            SELECT,
            0,
            Key::V,
            icons::SELECT,
            |app, _, _| app.ws.select_tool(ToolKind::Select),
            |cx| cx.tool == ToolKind::Select,
        ),
        tool(
            WALL,
            1,
            Key::W,
            icons::WALL,
            |app, _, _| app.ws.select_tool(ToolKind::Wall),
            |cx| cx.tool == ToolKind::Wall,
        ),
        tool(
            SLAB,
            2,
            Key::S,
            icons::SLAB,
            |app, _, _| app.ws.select_tool(ToolKind::Slab),
            |cx| cx.tool == ToolKind::Slab,
        ),
        tool(
            MEASURE,
            3,
            Key::M,
            icons::MEASURE,
            |app, _, _| app.ws.select_tool(ToolKind::Measure),
            |cx| cx.tool == ToolKind::Measure,
        ),
    ]
}
