//! Menu bar, tool palette and status line (UI-001, UI-011, UI-017,
//! NFR-USE-003).

use std::path::PathBuf;
use std::time::Duration;

use archicraft_model::project::Projection;
use egui::{Button, RichText, Ui};

use super::actions::Action;
use crate::input::{InputConsumer, InputMode};
use crate::tools::ToolKind;
use crate::tr;
use crate::views::camera::ViewPreset;
use crate::views::render3d::DisplayMode;
use crate::workspace::{Pane, Workspace};

fn item(ui: &mut Ui, action: Action, enabled: bool, out: &mut Option<Action>) {
    item_labelled(ui, action, action.label(), enabled, out);
}

fn item_labelled(ui: &mut Ui, action: Action, label: &str, enabled: bool, out: &mut Option<Action>) {
    let mut button = Button::new(label);
    if let Some(s) = action.shortcut() {
        button = button.shortcut_text(ui.ctx().format_shortcut(&s));
    }
    if ui.add_enabled(enabled, button).clicked() {
        *out = Some(action);
        ui.close();
    }
}

/// The menu bar. Returns the chosen action, or a recent file to open.
pub fn menu_bar(ui: &mut Ui, ws: &Workspace, problems_open: bool) -> (Option<Action>, Option<PathBuf>) {
    let mut action = None;
    let mut recent = None;
    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button("File", |ui| {
            item(ui, Action::New, true, &mut action);
            item(ui, Action::Open, true, &mut action);
            ui.menu_button("Open recent", |ui| {
                if ws.prefs.recent.is_empty() {
                    ui.label(RichText::new("No recent projects").weak());
                }
                for path in &ws.prefs.recent {
                    if ui.button(path.display().to_string()).clicked() {
                        recent = Some(path.clone());
                        ui.close();
                    }
                }
            });
            ui.separator();
            item(ui, Action::Save, true, &mut action);
            item(ui, Action::SaveAs, true, &mut action);
            item(ui, Action::Close, true, &mut action);
            ui.separator();
            item(ui, Action::Preferences, true, &mut action);
            ui.separator();
            item(ui, Action::Quit, true, &mut action);
        });
        ui.menu_button("Edit", |ui| {
            let undo = ws
                .doc
                .history
                .undo_label()
                .map_or_else(|| "Undo".to_owned(), |l| format!("Undo {}", ws.render(&l)));
            item_labelled(ui, Action::Undo, &undo, ws.doc.history.can_undo(), &mut action);
            let redo = ws
                .doc
                .history
                .redo_label()
                .map_or_else(|| "Redo".to_owned(), |l| format!("Redo {}", ws.render(&l)));
            item_labelled(ui, Action::Redo, &redo, ws.doc.history.can_redo(), &mut action);
            ui.separator();
            item(ui, Action::Delete, !ws.selection.is_empty(), &mut action);
            item(ui, Action::SelectAll, true, &mut action);
            ui.separator();
            item(ui, Action::Settings, true, &mut action);
        });
        ui.menu_button("View", |ui| {
            item(ui, Action::FitWindow, true, &mut action);
            item(ui, Action::ZoomSelection, !ws.selection.is_empty(), &mut action);
            item(ui, Action::PreviousZoom, true, &mut action);
            item(ui, Action::NextZoom, true, &mut action);
            ui.separator();
            item(ui, Action::SwitchPlan3d, true, &mut action);
            item(ui, Action::NextTab, true, &mut action);
            item(ui, Action::PreviousTab, true, &mut action);
            item(ui, Action::OrbitMode, true, &mut action);
            ui.separator();
            let mut shown = ws.prefs.show_reference_lines;
            if ui.checkbox(&mut shown, Action::ToggleReferenceLines.label()).clicked() {
                action = Some(Action::ToggleReferenceLines);
                ui.close();
            }
            let mut problems = problems_open;
            if ui.checkbox(&mut problems, Action::Problems.label()).clicked() {
                action = Some(Action::Problems);
                ui.close();
            }
            #[cfg(feature = "perf")]
            crate::perf::menu_item(ui);
        });
        ui.menu_button("Design", |ui| {
            item(ui, Action::ToolSelect, true, &mut action);
            item(ui, Action::ToolWall, true, &mut action);
            item(ui, Action::ToolSlab, true, &mut action);
        });
        ui.menu_button("Project", |ui| {
            item(ui, Action::Storeys, true, &mut action);
            item(ui, Action::Layers, true, &mut action);
            item(ui, Action::Materials, true, &mut action);
            item(ui, Action::ProjectSettings, true, &mut action);
        });
        // The menu titles move to the command registry (archicraft-5eu);
        // Help is the pilot of the message catalogue (NFR-L10N-001).
        ui.menu_button(tr!("menu-help"), |ui| {
            item(ui, Action::Shortcuts, true, &mut action);
            item(ui, Action::About, true, &mut action);
        });
    });
    (action, recent)
}

/// The tool palette (UI-001): selection, modelling and view tools.
pub fn toolbox(ui: &mut Ui, ws: &mut Workspace) -> Option<Action> {
    let mut action = None;
    let tool_button = |ui: &mut Ui, ws: &mut Workspace, kind: ToolKind, action_for: Action| {
        let shortcut = action_for
            .shortcut()
            .map(|s| format!(" ({})", ui.ctx().format_shortcut(&s)))
            .unwrap_or_default();
        let response = ui
            .add_sized(
                [88.0, 26.0],
                egui::Button::selectable(ws.tool.kind == kind, kind.label()),
            )
            .on_hover_text(format!("{}{shortcut}", kind.description()));
        if response.clicked() {
            ws.select_tool(kind);
        }
    };
    ui.label(RichText::new("Selection").small().weak());
    tool_button(ui, ws, ToolKind::Select, Action::ToolSelect);
    ui.add_space(6.0);
    ui.label(RichText::new("Modelling").small().weak());
    tool_button(ui, ws, ToolKind::Wall, Action::ToolWall);
    tool_button(ui, ws, ToolKind::Slab, Action::ToolSlab);
    ui.add_space(6.0);
    ui.label(RichText::new("View tools").small().weak());
    for (label, a) in [
        ("Fit", Action::FitWindow),
        ("Orbit", Action::OrbitMode),
        ("Plan/3D", Action::SwitchPlan3d),
    ] {
        let tip = a
            .shortcut()
            .map(|s| format!("{} ({})", a.label(), ui.ctx().format_shortcut(&s)))
            .unwrap_or_else(|| a.label().to_owned());
        if ui
            .add_sized([88.0, 22.0], Button::new(label))
            .on_hover_text(tip)
            .clicked()
        {
            action = Some(a);
        }
    }
    action
}

/// The status line with the input prompt, the last message and the
/// view-control bar of the active view (UI-001).
pub fn status_bar(ui: &mut Ui, ws: &mut Workspace, problem_count: usize) -> Option<Action> {
    let mut action = None;
    ui.horizontal(|ui| {
        // The prompt of the step in progress (INP-001, NFR-USE-003).
        let prompt = ws.tool.step().map_or_else(
            || {
                "Click an element to select it, Shift-click to add or remove, drag a rectangle to select several \
                 · Tab cycles overlapping elements"
                    .to_owned()
            },
            |step| {
                let mut line = step.status_line();
                // INP-020: the prompts name clicks, which take points in both
                // modes; press-drag-release adds the drag.
                if ws.prefs.input_mode == InputMode::DragRelease {
                    line.push_str(" · Press-drag-release: press at a point, drag and release at the next");
                }
                line
            },
        );
        ui.label(RichText::new(prompt).small());
        if let Some(status) = &ws.status
            && status.since.elapsed() < Duration::from_secs(8)
        {
            ui.separator();
            let text = RichText::new(&status.text).small();
            if status.error {
                ui.colored_label(ui.visuals().error_fg_color, text);
            } else {
                ui.label(text);
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let label = if problem_count == 0 {
                "No problems".to_owned()
            } else {
                format!("⚠ {problem_count} problem(s)")
            };
            if ui.small_button(label).clicked() {
                action = Some(Action::Problems);
            }
            ui.separator();
            match ws.active_pane {
                Some(Pane::View3d(view)) => {
                    if let Some(state) = ws.views_3d.get_mut(&view) {
                        if ui.small_button("Fit").clicked() {
                            action = Some(Action::FitWindow);
                        }
                        ui.toggle_value(&mut state.orbit, "Orbit");
                        egui::ComboBox::from_id_salt("display-mode")
                            .selected_text(state.mode.label())
                            .show_ui(ui, |ui| {
                                for mode in DisplayMode::ALL {
                                    ui.selectable_value(&mut state.mode, mode, mode.label());
                                }
                            });
                        ui.menu_button("Views", |ui| {
                            for preset in ViewPreset::ALL {
                                if ui.button(preset.label()).clicked() {
                                    state.camera.apply_preset(preset);
                                    ui.close();
                                }
                            }
                        });
                        let perspective = state.camera.projection == Projection::Perspective;
                        if ui
                            .selectable_label(perspective, "Perspective")
                            .on_hover_text("Switch between perspective and parallel projection (VIEW-020)")
                            .clicked()
                        {
                            state.camera.projection = if perspective {
                                Projection::Parallel
                            } else {
                                Projection::Perspective
                            };
                        }
                    }
                }
                Some(Pane::Plan(storey)) => {
                    if ui.small_button("Fit").clicked() {
                        action = Some(Action::FitWindow);
                    }
                    if ui.small_button("Next zoom ▶").clicked() {
                        action = Some(Action::NextZoom);
                    }
                    if ui.small_button("◀ Previous zoom").clicked() {
                        action = Some(Action::PreviousZoom);
                    }
                    if let Some(state) = ws.plan_views.get(&storey) {
                        // NFR-L10N-004: digit grouping applies to the zoom too.
                        let scale = ws.prefs.number_format().format_decimal(state.camera.scale, 0);
                        ui.label(RichText::new(format!("{scale} pt/m")).small().weak());
                    }
                    ui.label(RichText::new("Scale 1:100").small().weak());
                    let name = ws
                        .doc
                        .project
                        .storeys
                        .get(storey)
                        .map(|s| s.name.clone())
                        .unwrap_or_default();
                    ui.label(RichText::new(format!("Plan: {name}")).small());
                }
                None => {}
            }
        });
    });
    action
}
