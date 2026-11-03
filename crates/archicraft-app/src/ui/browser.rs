//! The project browser (VIEW-003, STR-003, UI-005).

use archicraft_model::units::format_length;
use archicraft_model::{Command, Storey, StoreyId};
use egui::{RichText, Ui};

use super::command_widgets::bar_button;
use super::dialogs::Dialogs;
use crate::commands::{CommandCtx, Origin, Registry, Target, project};
use crate::tr;
use crate::workspace::{Pane, Request, Workspace};

/// A new storey for inserting next to `reference`, named by its future
/// number in the user's language.
pub fn new_storey(ws: &Workspace, reference: StoreyId, above: bool) -> Storey {
    let project = &ws.doc.project;
    let number = project.storeys.number(reference).unwrap_or(0) + if above { 1 } else { 0 };
    let height = project.storeys.get(reference).map_or(3.0, |s| s.height);
    let name = if number >= 0 {
        tr!("browser-new-storey", number = format!("{number:02}"))
    } else {
        tr!("browser-new-basement", number = format!("{:02}", -number))
    };
    Storey::new(name, 0.0, height)
}

/// The browser: the storeys, the 3D views and the project, whose buttons run
/// commands (UI-011, UI-017).
pub fn ui(ui: &mut Ui, ws: &mut Workspace, dialogs: &mut Dialogs, cx: &CommandCtx) {
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
                    if ui.button(tr!("browser-insert-above")).clicked() {
                        let storey = new_storey(ws, id, true);
                        ws.execute(Command::InsertStorey {
                            reference: id,
                            above: true,
                            storey,
                        });
                        ui.close();
                    }
                    if ui.button(tr!("browser-insert-below")).clicked() {
                        let storey = new_storey(ws, id, false);
                        ws.execute(Command::InsertStorey {
                            reference: id,
                            above: false,
                            storey,
                        });
                        ui.close();
                    }
                    if ui.button(tr!("browser-storey-settings")).clicked() {
                        dialogs.storeys = true;
                        dialogs.selected_storey = Some(id);
                        ui.close();
                    }
                    if ui.button(tr!("browser-delete-storey")).clicked() {
                        dialogs.ask_delete_storey(id);
                        ui.close();
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
