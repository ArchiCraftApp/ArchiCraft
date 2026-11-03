//! The tools of the Design menu and their geometry methods (UI-001, UI-017,
//! WAL-001, SLB-001, MEA-001).

use egui::Key;

use super::{Binding, Category, CommandId, CommandSpec, Registry, Scope, ToolGroup};
use crate::ArchiCraftApp;
use crate::tools::{SlabMode, ToolKind, WallMode};
use crate::ui::icons;

pub const SELECT: CommandId = CommandId::new("tool.select");
pub const WALL: CommandId = CommandId::new("tool.wall");
pub const SLAB: CommandId = CommandId::new("tool.slab");
/// The measure tool (MEA-001), also a toolbar button (UI-011).
pub const MEASURE: CommandId = CommandId::new("tool.measure");

/// The family of the wall tool's geometry methods, one command per
/// [`WallMode`]: `tool.wall.mode.<code>`.
pub const WALL_MODE: &str = "tool.wall.mode";
/// The family of the slab tool's geometry methods, one command per
/// [`SlabMode`]: `tool.slab.mode.<code>`.
pub const SLAB_MODE: &str = "tool.slab.mode";

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

/// The command that switches the wall tool to `mode` (WAL-001).
pub fn wall_mode(mode: WallMode) -> CommandId {
    Registry::get().member(WALL_MODE, mode.code())
}

/// The command that switches the slab tool to `mode` (SLB-001).
pub fn slab_mode(mode: SlabMode) -> CommandId {
    Registry::get().member(SLAB_MODE, mode.code())
}

pub(super) fn specs() -> Vec<CommandSpec> {
    let tool = |kind: ToolKind, order, key, icon, (group, place, label)| {
        CommandSpec::with(command(kind), Category::Design, move |app, _, _| {
            app.ws.select_tool(kind)
        })
        .menu("design", 0, order)
        .bind(Binding::key(key))
        .scope(Scope::Canvas)
        .icon(icon)
        .toolbox(group, place, label)
        .checked_when(move |cx| cx.tool == kind)
    };
    let mut specs = vec![
        tool(
            ToolKind::Select,
            0,
            Key::V,
            icons::SELECT,
            (ToolGroup::Selection, 0, "toolbox-select"),
        ),
        tool(
            ToolKind::Wall,
            1,
            Key::W,
            icons::WALL,
            (ToolGroup::Modelling, 0, "toolbox-wall"),
        ),
        tool(
            ToolKind::Slab,
            2,
            Key::S,
            icons::SLAB,
            (ToolGroup::Modelling, 1, "toolbox-slab"),
        ),
        // The measure tool sits with the view tools: it changes nothing.
        tool(
            ToolKind::Measure,
            3,
            Key::M,
            icons::MEASURE,
            (ToolGroup::View, 3, "toolbox-measure"),
        ),
    ];
    // The geometry methods are checkable, so the properties strip, the menus
    // and the palette show the same state.
    specs.extend((0..).zip(WallMode::ALL).map(|(order, mode)| {
        CommandSpec::with(
            CommandId::member(WALL_MODE, mode.code()),
            Category::Design,
            move |app, _, _| set_mode(app, ToolKind::Wall, |tool| tool.wall_mode = mode),
        )
        .menu("design/wall-mode", 1, order)
        .checked_when(move |cx| cx.wall_mode == mode)
    }));
    specs.extend((0..).zip(SlabMode::ALL).map(|(order, mode)| {
        CommandSpec::with(
            CommandId::member(SLAB_MODE, mode.code()),
            Category::Design,
            move |app, _, _| set_mode(app, ToolKind::Slab, |tool| tool.slab_mode = mode),
        )
        .menu("design/slab-mode", 1, order)
        .checked_when(move |cx| cx.slab_mode == mode)
    }));
    specs
}

/// Selects the tool `kind` if it is not active and switches its geometry
/// method with `set`, discarding the points of the old method. A pending
/// wall chain is created first (UND-003); if the project rejects it, the
/// method stays.
fn set_mode(app: &mut ArchiCraftApp, kind: ToolKind, set: impl FnOnce(&mut crate::tools::ToolState)) {
    let ws = &mut app.ws;
    if ws.tool.kind != kind {
        ws.select_tool(kind);
    }
    if ws.tool.kind == kind && ws.end_chain() {
        set(&mut ws.tool);
        ws.reset_input();
    }
}
