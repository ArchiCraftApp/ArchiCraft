//! The project browser (VIEW-003, STR-003, UI-005).

use archicraft_model::units::format_length;
use archicraft_model::{Command, Storey, StoreyId};
use egui::{RichText, Ui};

use super::dialogs::Dialogs;
use crate::workspace::{Pane, Request, Workspace};

/// A new storey for inserting next to `reference`, named by its future
/// number.
pub fn new_storey(ws: &Workspace, reference: StoreyId, above: bool) -> Storey {
    let project = &ws.doc.project;
    let number = project.storeys.number(reference).unwrap_or(0) + if above { 1 } else { 0 };
    let height = project.storeys.get(reference).map_or(3.0, |s| s.height);
    let name = if number >= 0 {
        format!("{number:02} Storey")
    } else {
        format!("-{:02} Basement", -number)
    };
    Storey::new(name, 0.0, height)
}

pub fn ui(ui: &mut Ui, ws: &mut Workspace, dialogs: &mut Dialogs) {
    egui::CollapsingHeader::new(RichText::new("Storeys").strong())
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
                let text = format!("{number:>3}  {name}  ({})", format_length(elevation, &units, &nf, true));
                let response = ui.selectable_label(active, text);
                if response.double_clicked() || response.clicked() {
                    ws.requests.push(Request::OpenPane(Pane::Plan(id)));
                }
                response.context_menu(|ui| {
                    if ui.button("Open plan").clicked() {
                        ws.requests.push(Request::OpenPane(Pane::Plan(id)));
                        ui.close();
                    }
                    if ui.button("Insert storey above").clicked() {
                        let storey = new_storey(ws, id, true);
                        ws.execute(Command::InsertStorey {
                            reference: id,
                            above: true,
                            storey,
                        });
                        ui.close();
                    }
                    if ui.button("Insert storey below").clicked() {
                        let storey = new_storey(ws, id, false);
                        ws.execute(Command::InsertStorey {
                            reference: id,
                            above: false,
                            storey,
                        });
                        ui.close();
                    }
                    if ui.button("Rename, elevation, height…").clicked() {
                        dialogs.storeys = true;
                        dialogs.selected_storey = Some(id);
                        ui.close();
                    }
                    if ui.button("Delete storey…").clicked() {
                        dialogs.ask_delete_storey(id);
                        ui.close();
                    }
                });
            }
        });
    egui::CollapsingHeader::new(RichText::new("3D views").strong())
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
    egui::CollapsingHeader::new(RichText::new("Project").strong())
        .default_open(true)
        .show(ui, |ui| {
            let project = &ws.doc.project;
            ui.label(format!("Site: {}", project.site.name));
            ui.label(format!("Building: {}", project.building.name));
            ui.label(format!("{} elements", project.elements.len()));
            ui.horizontal_wrapped(|ui| {
                if ui.small_button("Layers…").clicked() {
                    dialogs.layers = true;
                }
                if ui.small_button("Materials…").clicked() {
                    dialogs.materials = true;
                }
                if ui.small_button("Settings…").clicked() {
                    dialogs.open_project_settings(ws);
                }
            });
        });
}
