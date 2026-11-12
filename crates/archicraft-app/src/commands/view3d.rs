//! Commands of the active 3D view: display mode, view presets, projection
//! and its camera views (UI-017, VIEW-020, VIEW-050, VIEW-004). They act on
//! the 3D view of the target and are greyed out while no 3D view is active.

use archicraft_model::project::Projection;
use archicraft_model::{Command, ViewId};

use super::{Category, CommandCtx, CommandId, CommandSpec, Enabled, Handler, Registry, Target};
use crate::ui::{browser, icons};
use crate::views::camera::ViewPreset;
use crate::views::render3d::DisplayMode;
use crate::views::view3d::View3dState;
use crate::workspace::Pane;
use crate::{ArchiCraftApp, tr};

/// The family of the display modes, one command per [`DisplayMode`]:
/// `view3d.display.<code>`.
pub const DISPLAY: &str = "view3d.display";
/// The family of the view presets, one command per [`ViewPreset`]:
/// `view3d.preset.<code>`.
pub const PRESET: &str = "view3d.preset";
/// Switches between perspective and parallel projection (VIEW-020).
pub const TOGGLE_PROJECTION: CommandId = CommandId::new("view3d.toggle-projection");
/// Saves the viewpoint of the target's 3D view as a new camera view
/// (VIEW-004).
pub const SAVE_CAMERA: CommandId = CommandId::new("view3d.save-camera");
/// Stores the viewpoint of the target view's open tab in that view
/// (VIEW-004).
pub const UPDATE_CAMERA: CommandId = CommandId::new("view3d.update-camera");
/// Renames the target's 3D view in the project browser (VIEW-003).
pub const RENAME_VIEW: CommandId = CommandId::new("view3d.rename-view");
/// Deletes the target's 3D view and closes its tab (VIEW-003).
pub const DELETE_VIEW: CommandId = CommandId::new("view3d.delete-view");

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
    specs.push(
        CommandSpec::new(SAVE_CAMERA.as_str(), Category::View, save_camera)
            .menu("view", 4, 21)
            .icon(icons::CAMERA)
            .enabled_when(in_3d),
    );
    // The browser's context menu of a 3D view runs these on that view; the
    // palette on the active one.
    for (id, handler) in [
        (UPDATE_CAMERA, update_camera as Handler),
        (RENAME_VIEW, |app, ctx, target| {
            if let Some(view) = target_view(target) {
                browser::start_rename(ctx, &app.ws, view);
            }
        }),
        (DELETE_VIEW, |app, _, target| {
            if let Some(view) = target_view(target) {
                app.ws.execute(Command::DeleteView { view });
            }
        }),
    ] {
        specs.push(
            CommandSpec::new(id.as_str(), Category::View, handler)
                .palette_only()
                .enabled_when(in_3d),
        );
    }
    specs
}

fn target_view(target: Target) -> Option<ViewId> {
    match target.pane {
        Some(Pane::View3d(view)) => Some(view),
        _ => None,
    }
}

/// Saves the viewpoint and projection of the target's open 3D view as a new
/// view named "Camera n" (VIEW-004), one undo step. The tab stays.
fn save_camera(app: &mut ArchiCraftApp, _: &egui::Context, target: Target) {
    let Some(state) = target_view(target).and_then(|view| app.ws.views_3d.get(&view)) else {
        return;
    };
    let command = Command::SaveCamera {
        view: ViewId::new(),
        base: tr!("view-camera-name"),
        projection: state.camera.projection,
        camera: state.camera.viewpoint(),
    };
    app.ws.execute(command);
}

/// Stores the viewpoint of the target view's open tab in that view
/// (VIEW-004), keeping its identity, name and projection; nothing while it
/// has no open tab.
fn update_camera(app: &mut ArchiCraftApp, _: &egui::Context, target: Target) {
    let Some(view) = target_view(target) else {
        return;
    };
    let Some(state) = app.ws.views_3d.get(&view) else {
        return;
    };
    let camera = state.camera.viewpoint();
    app.ws.execute(Command::SetViewCamera { view, camera });
}
