//! The tools of the Design menu, their geometry methods and contour
//! detection (UI-001, UI-017, WAL-001, SLB-001, SPC-001, ANN-007, MEA-001,
//! INP-018, EDT-020).

use egui::Key;

use super::{Binding, Category, CommandId, CommandSpec, Registry, Scope, ToolGroup};
use crate::ArchiCraftApp;
use crate::tools::{SlabMode, SpaceMode, ToolKind, WallMode};
use crate::ui::icons;

pub const SELECT: CommandId = CommandId::new("tool.select");
pub const WALL: CommandId = CommandId::new("tool.wall");
pub const SLAB: CommandId = CommandId::new("tool.slab");
/// The space tool (SPC-001).
pub const SPACE: CommandId = CommandId::new("tool.space");
/// The text tool (ANN-007).
pub const TEXT: CommandId = CommandId::new("tool.text");
/// The measure tool (MEA-001), also a toolbar button (UI-011).
pub const MEASURE: CommandId = CommandId::new("tool.measure");

/// Switches contour detection of the wall and slab tools on or off: the
/// explicit command of the key held for it (INP-018, EDT-020).
pub const CONTOUR_DETECTION: CommandId = CommandId::new("tool.contour-detection");

/// The family of the wall tool's geometry methods, one command per
/// [`WallMode`]: `tool.wall.mode.<code>`.
pub const WALL_MODE: &str = "tool.wall.mode";
/// The family of the slab tool's geometry methods, one command per
/// [`SlabMode`]: `tool.slab.mode.<code>`.
pub const SLAB_MODE: &str = "tool.slab.mode";
/// The family of the space tool's geometry methods, one command per
/// [`SpaceMode`]: `tool.space.mode.<code>`.
pub const SPACE_MODE: &str = "tool.space.mode";

/// The command that selects the tool `kind`, which the toolbox buttons
/// invoke (UI-001).
pub fn command(kind: ToolKind) -> CommandId {
    match kind {
        ToolKind::Select => SELECT,
        ToolKind::Wall => WALL,
        ToolKind::Slab => SLAB,
        ToolKind::Space => SPACE,
        ToolKind::Text => TEXT,
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

/// The command that switches the space tool to `mode` (SPC-001).
pub fn space_mode(mode: SpaceMode) -> CommandId {
    Registry::get().member(SPACE_MODE, mode.code())
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
        // A for the area of a room: R cycles the wall reference line.
        tool(
            ToolKind::Space,
            3,
            Key::A,
            icons::SPACE,
            (ToolGroup::Modelling, 2, "toolbox-space"),
        ),
        // T for text, the first tool of the 2D and annotation group.
        tool(
            ToolKind::Text,
            4,
            Key::T,
            icons::TEXT,
            (ToolGroup::Annotation2d, 0, "toolbox-text"),
        ),
        // The measure tool sits with the view tools: it changes nothing.
        tool(
            ToolKind::Measure,
            5,
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
    specs.extend((0..).zip(SpaceMode::ALL).map(|(order, mode)| {
        CommandSpec::with(
            CommandId::member(SPACE_MODE, mode.code()),
            Category::Design,
            move |app, _, _| set_mode(app, ToolKind::Space, |tool| tool.space_mode = mode),
        )
        .menu("design/space-mode", 1, order)
        .checked_when(move |cx| cx.space_mode == mode)
    }));
    specs.push(
        CommandSpec::with(CONTOUR_DETECTION, Category::Design, |app, _, _| {
            app.ws.tool.detection.toggle()
        })
        .menu("design", 2, 0)
        .checked_when(|cx| cx.contour_detection),
    );
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
