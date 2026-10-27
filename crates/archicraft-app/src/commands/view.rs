//! Commands of the View menu (VIEW-014, UI-006).

use archicraft_model::project::Projection;
use egui::Key;

use super::{Binding, Category, CommandId, CommandSpec, Enabled, Handler, Scope, Target};
use crate::ArchiCraftApp;
use crate::ui::icons;
use crate::views::view3d::View3dState;
use crate::workspace::Pane;

// The view commands the toolbox and the status bar invoke (UI-001).
pub const FIT_WINDOW: CommandId = CommandId::new("view.fit-window");
pub const PREVIOUS_ZOOM: CommandId = CommandId::new("view.previous-zoom");
pub const NEXT_ZOOM: CommandId = CommandId::new("view.next-zoom");
pub const SWITCH_PLAN_3D: CommandId = CommandId::new("view.switch-plan-3d");
pub const ORBIT: CommandId = CommandId::new("view.orbit");
pub const PROBLEMS: CommandId = CommandId::new("view.problems");

pub(super) fn specs() -> Vec<CommandSpec> {
    let view = |id, handler: Handler| CommandSpec::new(id, Category::View, handler);
    vec![
        view(FIT_WINDOW.as_str(), |app, _, target| app.zoom_pane(target.pane, None))
            .menu("view", 0, 0)
            .bind(Binding::key(Key::F))
            .scope(Scope::Canvas)
            .icon(icons::FIT),
        view("view.zoom-selection", |app, _, target| {
            let ids = app.ws.selected_ids();
            if !ids.is_empty() {
                app.zoom_pane(target.pane, Some(ids));
            }
        })
        .menu("view", 0, 1)
        .bind(Binding::key(Key::F).shift())
        .enabled_when(|cx| Enabled::when(!cx.selection_empty, "cmd-reason-nothing-selected")),
        view(PREVIOUS_ZOOM.as_str(), |app, _, target| {
            if let Some(Pane::Plan(storey)) = target.pane
                && let Some(state) = app.ws.plan_views.get_mut(&storey)
            {
                state.previous_zoom();
            }
        })
        .menu("view", 0, 2)
        .bind(Binding::primary(Key::OpenBracket)),
        view(NEXT_ZOOM.as_str(), |app, _, target| {
            if let Some(Pane::Plan(storey)) = target.pane
                && let Some(state) = app.ws.plan_views.get_mut(&storey)
            {
                state.next_zoom();
            }
        })
        .menu("view", 0, 3)
        .bind(Binding::primary(Key::CloseBracket)),
        view(SWITCH_PLAN_3D.as_str(), |app, _, target| {
            let pane = match target.pane {
                Some(Pane::Plan(_)) => app.ws.doc.project.views.first().map(|v| Pane::View3d(v.id)),
                _ => Some(Pane::Plan(target.storey)),
            };
            if let Some(pane) = pane {
                app.open_pane(pane);
            }
        })
        .menu("view", 1, 0)
        .bind(Binding::key(Key::F3))
        // F3 is a media key on a Mac keyboard and needs Fn (docs/keymap.md).
        .bind(Binding::primary(Key::Num3))
        .scope(Scope::Canvas)
        .icon(icons::PLAN_3D),
        // Ctrl+Tab stays off while a tool takes point input, where Tab moves
        // between the typed fields.
        view("view.next-tab", |app, _, _| app.cycle_tab(1))
            .menu("view", 1, 1)
            .bind(Binding::ctrl(Key::Tab))
            .scope(Scope::Canvas),
        view("view.previous-tab", |app, _, _| app.cycle_tab(-1))
            .menu("view", 1, 2)
            .bind(Binding::ctrl(Key::Tab).shift()),
        view(ORBIT.as_str(), toggle_orbit)
            .menu("view", 1, 3)
            .bind(Binding::key(Key::O))
            .scope(Scope::Canvas)
            .icon(icons::ORBIT),
        view("view.reference-lines", |app, _, _| {
            app.ws.prefs.show_reference_lines = !app.ws.prefs.show_reference_lines;
            app.save_prefs();
        })
        .menu("view", 2, 0)
        .checked_when(|cx| cx.reference_lines_shown),
        view(PROBLEMS.as_str(), |app, _, _| {
            app.dialogs.problems = !app.dialogs.problems
        })
        .menu("view", 2, 1)
        .bind(Binding::primary(Key::M).shift())
        .scope(Scope::TextSafe)
        .checked_when(|cx| cx.problems_open),
        // INP-014: the user origin of the plan views.
        view("view.place-user-origin", |app, _, _| app.ws.place_user_origin()).menu("view", 3, 0),
        view("view.reset-user-origin", |app, _, _| app.ws.reset_user_origin())
            .menu("view", 3, 1)
            .enabled_when(|cx| Enabled::when(cx.user_origin_placed, "cmd-reason-no-user-origin")),
    ]
}

/// Turns orbit mode of the target's 3D view on or off, opening the first 3D
/// view when the target is a plan (VIEW-014).
fn toggle_orbit(app: &mut ArchiCraftApp, _: &egui::Context, target: Target) {
    let view = match target.pane {
        Some(Pane::View3d(v)) => Some(v),
        _ => app.ws.doc.project.views.first().map(|v| v.id),
    };
    let Some(view) = view else {
        return;
    };
    app.open_pane(Pane::View3d(view));
    let projection = app
        .ws
        .doc
        .project
        .view(view)
        .map_or(Projection::Perspective, |v| v.projection);
    let state = app
        .ws
        .views_3d
        .entry(view)
        .or_insert_with(|| View3dState::new(projection));
    state.orbit = !state.orbit;
}
