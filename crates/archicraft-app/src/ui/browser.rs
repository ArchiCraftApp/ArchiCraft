//! The project browser (VIEW-003, STR-003, UI-005).

use archicraft_model::StoreyId;
use archicraft_model::units::format_length;
use egui::{Atoms, Button, Image, ImageSource, Response, RichText, TextStyle, Ui, Vec2, WidgetText};

use super::command_widgets::{bar_button, menu_item};
use super::icons;
use crate::commands::project::{DELETE_STOREY, EDIT_STOREY, INSERT_STOREY_ABOVE, INSERT_STOREY_BELOW};
use crate::commands::{CommandCtx, Origin, Registry, Target, project};
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

/// The browser (VIEW-003): the storeys, the 3D views and the project. The
/// storeys' context menus and the project's buttons run commands (UI-011,
/// UI-017). Each storey shows a plan sheet, each 3D view the icon of the
/// projection it shows ([`icons::view_icon`]).
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
    egui::CollapsingHeader::new(RichText::new(tr!("browser-views")).strong())
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
                let active = ws.active_pane == Some(Pane::View3d(id));
                if row(ui, active, icons::view_icon(projection), name).clicked() {
                    ws.requests.push(Request::OpenPane(Pane::View3d(id)));
                }
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
            ui.horizontal_wrapped(|ui| {
                for (command, label) in [
                    (project::LAYERS, tr!("browser-layers")),
                    (project::MATERIALS, tr!("browser-materials")),
                    (project::SETTINGS, tr!("browser-settings")),
                ] {
                    if bar_button(ui, cx, Registry::get().command(command), &label) {
                        let target = Target::active(ws);
                        ws.requests.push(Request::Invoke(command, target, Origin::Panel));
                    }
                }
            });
        });
}
