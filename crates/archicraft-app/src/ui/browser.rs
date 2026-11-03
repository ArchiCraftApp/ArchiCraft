//! The project browser (VIEW-003, STR-003, UI-005).

use archicraft_model::StoreyId;
use archicraft_model::units::format_length;
use egui::{RichText, Ui};

use super::command_widgets::{bar_button, menu_item};
use crate::commands::project::{DELETE_STOREY, EDIT_STOREY, INSERT_STOREY_ABOVE, INSERT_STOREY_BELOW};
use crate::commands::{CommandCtx, Origin, Registry, Target, project};
use crate::tr;
use crate::workspace::{Pane, Request, Workspace};

/// The browser (VIEW-003): the storeys, the 3D views and the project. The
/// storeys' context menus and the project's buttons run commands (UI-011,
/// UI-017).
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
                let response = ui.selectable_label(active, text);
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
            let views: Vec<_> = ws.doc.project.views.iter().map(|v| (v.id, v.name.clone())).collect();
            for (id, name) in views {
                let active = ws.active_pane == Some(Pane::View3d(id));
                if ui.selectable_label(active, name).clicked() {
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
