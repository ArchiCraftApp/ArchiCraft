//! Commands of the active 3D view: display mode, view presets and projection
//! (UI-017, VIEW-020, VIEW-050). They act on the 3D view of the target and
//! are greyed out while no 3D view is active.

use archicraft_model::project::Projection;

use super::{Category, CommandCtx, CommandId, CommandSpec, Enabled, Registry, Target};
use crate::ArchiCraftApp;
use crate::views::camera::ViewPreset;
use crate::views::render3d::DisplayMode;
use crate::views::view3d::View3dState;
use crate::workspace::Pane;

/// The family of the display modes, one command per [`DisplayMode`]:
/// `view3d.display.<code>`.
pub const DISPLAY: &str = "view3d.display";
/// The family of the view presets, one command per [`ViewPreset`]:
/// `view3d.preset.<code>`.
pub const PRESET: &str = "view3d.preset";
/// Switches between perspective and parallel projection (VIEW-020).
pub const TOGGLE_PROJECTION: CommandId = CommandId::new("view3d.toggle-projection");

/// The command that shows the active 3D view in `mode`.
pub fn display(mode: DisplayMode) -> CommandId {
    Registry::get().member(DISPLAY, mode.code())
}

/// The command that turns the camera of the active 3D view to `preset`.
pub fn preset(preset: ViewPreset) -> CommandId {
    Registry::get().member(PRESET, preset.code())
}

fn in_3d(cx: &CommandCtx) -> Enabled {
    Enabled::when(cx.view3d.is_some(), "cmd-reason-no-3d-view")
}

/// Runs `change` on the state of the target's 3D view, if the target is one.
fn with_view(app: &mut ArchiCraftApp, target: Target, change: impl FnOnce(&mut View3dState)) {
    if let Some(Pane::View3d(view)) = target.pane
        && let Some(state) = app.ws.views_3d.get_mut(&view)
    {
        change(state);
    }
}

pub(super) fn specs() -> Vec<CommandSpec> {
    let mut specs: Vec<CommandSpec> = (0..)
        .zip(DisplayMode::ALL)
        .map(|(order, mode)| {
            CommandSpec::with(
                CommandId::member(DISPLAY, mode.code()),
                Category::View,
                move |app, _, target| with_view(app, target, |state| state.mode = mode),
            )
            .menu("view/display-mode", 4, order)
            .enabled_when(in_3d)
            .checked_when(move |cx| cx.view3d.is_some_and(|v| v.mode == mode))
        })
        .collect();
    specs.extend((0..).zip(ViewPreset::ALL).map(|(order, preset)| {
        CommandSpec::with(
            CommandId::member(PRESET, preset.code()),
            Category::View,
            move |app, _, target| with_view(app, target, |state| state.camera.apply_preset(preset)),
        )
        .menu("view/view-preset", 4, 10 + order)
        .enabled_when(in_3d)
    }));
    specs.push(
        CommandSpec::new(TOGGLE_PROJECTION.as_str(), Category::View, |app, _, target| {
            with_view(app, target, |state| {
                state.camera.projection = match state.camera.projection {
                    Projection::Perspective => Projection::Parallel,
                    Projection::Parallel => Projection::Perspective,
                };
            })
        })
        .menu("view", 4, 20)
        .enabled_when(in_3d)
        .checked_when(|cx| cx.view3d.is_some_and(|v| v.projection == Projection::Perspective)),
    );
    specs
}
