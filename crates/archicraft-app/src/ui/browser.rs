//! The project browser (VIEW-003, STR-003, UI-005).

use archicraft_model::units::format_length;
use archicraft_model::{Command, StoreyId, ViewId};
use egui::text::{CCursor, CCursorRange};
use egui::{
    Atoms, Button, Context, Id, Image, ImageSource, Key, Response, RichText, TextEdit, TextStyle, Ui, Vec2, WidgetText,
};

use super::command_widgets::{menu_item, menu_item_for};
use super::icons;
use crate::commands::project::{DELETE_STOREY, EDIT_STOREY, INSERT_STOREY_ABOVE, INSERT_STOREY_BELOW};
use crate::commands::view3d::{DELETE_VIEW, RENAME_VIEW, SAVE_CAMERA, UPDATE_CAMERA};
use crate::commands::{CommandCtx, Enabled, Origin, Registry, Target};
use crate::tr;
use crate::workspace::{Pane, Request, Workspace};

/// A browser row (VIEW-003): `icon` before `text`, selected while its view
/// is the active pane. The icon is as high as a line of the row's text, so
/// the row is as high as one without an icon, and it takes the text's
/// colour (`Button::image_tint_follows_text_color`): selected, hovered and
/// in either theme. The row stays one line: in a browser narrower than its
/// text the text ends in an ellipsis, and AccessKit still names the row by
/// all of it. Icon and text are one button, so a click anywhere on the row
/// selects and opens it.
fn row(ui: &mut Ui, selected: bool, icon: ImageSource<'static>, text: impl Into<WidgetText>) -> Response {
    let side = ui.text_style_height(&TextStyle::Button);
    let mut atoms = Atoms::new(text.into());
    atoms.push_left(Image::new(icon).fit_to_exact_size(Vec2::splat(side)));
    ui.add(
        Button::selectable(selected, atoms)
            .image_tint_follows_text_color(true)
            .truncate(),
    )
}

/// Opens the 3D view `view` in a tab (VIEW-003). An open camera view
/// returns to its saved viewpoint (VIEW-004), as a closed one opens there.
fn open_view(ws: &mut Workspace, view: ViewId) {
    if let (Some(saved), Some(state)) = (ws.doc.project.view(view), ws.views_3d.get_mut(&view)) {
        state.show_saved(saved);
    }
    ws.requests.push(Request::OpenPane(Pane::View3d(view)));
}

/// The context menu of the 3D view `view` (VIEW-003, VIEW-004): Rename,
/// Update camera, which stores the viewpoint of the view's open tab and is
/// greyed out while it has none, and Delete. They act on `view`, not on the
/// active view.
fn view_menu(ui: &mut Ui, ws: &mut Workspace, cx: &CommandCtx, view: ViewId) {
    let open = Enabled::when(ws.views_3d.contains_key(&view), "cmd-reason-view-not-open");
    for (command, enabled) in [
        (RENAME_VIEW, Enabled::Yes),
        (UPDATE_CAMERA, open),
        (DELETE_VIEW, Enabled::Yes),
    ] {
        if menu_item_for(ui, cx, Registry::get().command(command), enabled) {
            let target = Target::view(ws, view);
            ws.requests.push(Request::Invoke(command, target, Origin::ContextMenu));
        }
    }
}

/// The name of a 3D view being renamed in its row (VIEW-003), kept in
/// egui's memory between frames.
#[derive(Clone, Debug)]
struct Rename {
    view: ViewId,
    text: String,
    /// Whether the field takes the keyboard focus, with its text selected,
    /// this frame.
    focus: bool,
}

fn rename_id() -> Id {
    Id::new("browser-rename-view")
}

/// Starts renaming the 3D view `view` in its browser row (VIEW-003): the
/// row turns into a field with the view's name selected.
pub fn start_rename(ctx: &Context, ws: &Workspace, view: ViewId) {
    if let Some(v) = ws.doc.project.view(view) {
        let rename = Rename {
            view,
            text: v.name.clone(),
            focus: true,
        };
        ctx.data_mut(|d| d.insert_temp(rename_id(), rename));
    }
}

/// The name field of `view` while it is being renamed, in place of its
/// row; returns whether it showed. Enter or a click elsewhere renames the
/// view, one undo step; Escape keeps the name. A name the project rejects
/// (an empty one) is reported in the status line.
fn rename_ui(ui: &mut Ui, ws: &mut Workspace, view: ViewId) -> bool {
    let Some(mut rename) = ui
        .data(|d| d.get_temp::<Rename>(rename_id()))
        .filter(|r| r.view == view)
    else {
        return false;
    };
    let mut output = TextEdit::singleline(&mut rename.text)
        .id(rename_id().with(view))
        .desired_width(f32::INFINITY)
        .show(ui);
    if std::mem::take(&mut rename.focus) {
        output.response.request_focus();
        let all = CCursorRange::two(CCursor::new(0), CCursor::new(rename.text.chars().count()));
        output.state.cursor.set_char_range(Some(all));
        output.state.store(ui.ctx(), output.response.id);
    }
    if output.response.lost_focus() {
        ui.data_mut(|d| d.remove::<Rename>(rename_id()));
        if !ui.input(|i| i.key_pressed(Key::Escape)) {
            ws.execute(Command::RenameView {
                view,
                name: rename.text,
            });
        }
    } else {
        ui.data_mut(|d| d.insert_temp(rename_id(), rename));
    }
    true
}

/// The browser (VIEW-003): the storeys, the 3D views and the project. The
/// context menus of the storeys, the 3D views and the "3D views" header
/// run commands (UI-011, UI-017); Layers, Materials and the project
/// settings are in the toolbar and the Project menu. Each storey shows a
/// plan sheet, each 3D view, saved cameras included (VIEW-004), the icon of
/// the projection it shows ([`icons::view_icon`]).
pub fn ui(ui: &mut Ui, ws: &mut Workspace, cx: &CommandCtx) {
    egui::CollapsingHeader::new(RichText::new(tr!("browser-storeys")).strong())
        .default_open(true)
        .show(ui, |ui| {
            let units = ws.doc.project.settings.units.clone();
            let nf = ws.prefs.number_format();
            let storeys: Vec<(StoreyId, i32, String, f64)> = ws
                .doc
                .project
                .storeys
                .highest_first()
                .map(|s| {
                    (
                        s.id,
                        ws.doc.project.storeys.number(s.id).unwrap_or(0),
                        s.name.clone(),
                        s.elevation,
                    )
                })
                .collect();
            for (id, number, name, elevation) in storeys {
                let active = ws.active_pane == Some(Pane::Plan(id));
                let text = tr!(
                    "browser-storey",
                    number = format!("{number:>3}"),
                    name = name.as_str(),
                    elevation = format_length(elevation, &units, &nf, true)
                );
                let response = row(ui, active, icons::PLAN_SHEET, text);
                if response.double_clicked() || response.clicked() {
                    ws.requests.push(Request::OpenPane(Pane::Plan(id)));
                }
                response.context_menu(|ui| {
                    if ui.button(tr!("browser-open-plan")).clicked() {
                        ws.requests.push(Request::OpenPane(Pane::Plan(id)));
                        ui.close();
                    }
                    let target = Target::storey(id);
                    for command in [INSERT_STOREY_ABOVE, INSERT_STOREY_BELOW, EDIT_STOREY, DELETE_STOREY] {
                        if menu_item(ui, cx, Registry::get().command(command)) {
                            ws.requests.push(Request::Invoke(command, target, Origin::ContextMenu));
                        }
                    }
                });
            }
        });
    let views = egui::CollapsingHeader::new(RichText::new(tr!("browser-views")).strong())
        .default_open(true)
        .show(ui, |ui| {
            // An open view shows its camera's projection, which the
            // view-control bar switches (VIEW-020); a closed one the
            // projection it opens with.
            let views: Vec<_> = ws
                .doc
                .project
                .views
                .iter()
                .map(|v| {
                    let projection = ws.views_3d.get(&v.id).map_or(v.projection, |s| s.camera.projection);
                    (v.id, v.name.clone(), projection)
                })
                .collect();
            for (id, name, projection) in views {
                if rename_ui(ui, ws, id) {
                    continue;
                }
                let active = ws.active_pane == Some(Pane::View3d(id));
                let response = row(ui, active, icons::view_icon(projection), name);
                if response.clicked() {
                    open_view(ws, id);
                }
                response.context_menu(|ui| view_menu(ui, ws, cx, id));
            }
        });
    // Save camera saves the active 3D view's viewpoint (VIEW-004).
    views.header_response.context_menu(|ui| {
        if menu_item(ui, cx, Registry::get().command(SAVE_CAMERA)) {
            let target = Target::active(ws);
            ws.requests
                .push(Request::Invoke(SAVE_CAMERA, target, Origin::ContextMenu));
        }
    });
    egui::CollapsingHeader::new(RichText::new(tr!("browser-project")).strong())
        .default_open(true)
        .show(ui, |ui| {
            let project = &ws.doc.project;
            let count = project.elements.len();
            ui.label(tr!("browser-site", name = project.site.name.as_str()));
            ui.label(tr!("browser-building", name = project.building.name.as_str()));
            ui.label(tr!(
                "browser-elements",
                count = count,
                count_text = ws.count_text(count)
            ));
        });
}
