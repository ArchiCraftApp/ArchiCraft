//! Menu bar, tool palette and status line (UI-001, UI-011, UI-017,
//! NFR-USE-003).

use std::path::PathBuf;
use std::time::Duration;

use archicraft_model::project::Projection;
use archicraft_model::units::format_length;
use egui::{Button, RichText, Ui};

use crate::commands::{CommandCtx, CommandId, Registry};
use crate::input::InputMode;
use crate::tools::ToolKind;
use crate::tr;
use crate::views::camera::ViewPreset;
use crate::views::render3d::DisplayMode;
use crate::workspace::{Pane, Workspace};

/// The registered command `id`. Every command id this file names is
/// checked against the registry by
/// `commands::tests::shell_names_registered_commands`.
fn spec(id: &str) -> &'static crate::commands::CommandSpec {
    Registry::get().spec(id).unwrap_or_else(|| panic!("no command {id}"))
}

/// The text of the first default binding of `id`, if it has one.
fn shortcut_text(ui: &Ui, id: &str) -> Option<String> {
    spec(id)
        .default_bindings
        .first()
        .map(|b| ui.ctx().format_shortcut(&b.shortcut()))
}

/// The menu item of the command `id`: its label, shortcut, enabled state
/// and, for an on/off command, a check box, all from the registry.
fn item(ui: &mut Ui, cx: &CommandCtx, id: &str, out: &mut Option<CommandId>) {
    let spec = spec(id);
    let label = spec.label_in(cx);
    let clicked = if let Some(checked) = spec.checked {
        let mut on = checked(cx);
        ui.checkbox(&mut on, label).clicked()
    } else {
        let mut button = Button::new(label);
        if let Some(text) = shortcut_text(ui, id) {
            button = button.shortcut_text(text);
        }
        ui.add_enabled((spec.enabled)(cx).is_yes(), button).clicked()
    };
    if clicked {
        *out = Some(spec.id);
        ui.close();
    }
}

/// The menu bar. Returns the chosen command, or a recent file to open.
pub fn menu_bar(ui: &mut Ui, ws: &Workspace, cx: &CommandCtx) -> (Option<CommandId>, Option<PathBuf>) {
    let mut chosen = None;
    let mut recent = None;
    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button("File", |ui| {
            item(ui, cx, "file.new", &mut chosen);
            item(ui, cx, "file.open", &mut chosen);
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
            item(ui, cx, "file.save", &mut chosen);
            item(ui, cx, "file.save-as", &mut chosen);
            item(ui, cx, "file.close", &mut chosen);
            ui.separator();
            item(ui, cx, "file.preferences", &mut chosen);
            ui.separator();
            item(ui, cx, "file.quit", &mut chosen);
        });
        ui.menu_button("Edit", |ui| {
            item(ui, cx, "edit.undo", &mut chosen);
            item(ui, cx, "edit.redo", &mut chosen);
            ui.separator();
            item(ui, cx, "edit.delete", &mut chosen);
            item(ui, cx, "edit.select-all", &mut chosen);
            ui.separator();
            item(ui, cx, "edit.settings", &mut chosen);
        });
        ui.menu_button("View", |ui| {
            item(ui, cx, "view.fit-window", &mut chosen);
            item(ui, cx, "view.zoom-selection", &mut chosen);
            item(ui, cx, "view.previous-zoom", &mut chosen);
            item(ui, cx, "view.next-zoom", &mut chosen);
            ui.separator();
            item(ui, cx, "view.switch-plan-3d", &mut chosen);
            item(ui, cx, "view.next-tab", &mut chosen);
            item(ui, cx, "view.previous-tab", &mut chosen);
            item(ui, cx, "view.orbit", &mut chosen);
            ui.separator();
            item(ui, cx, "view.reference-lines", &mut chosen);
            item(ui, cx, "view.problems", &mut chosen);
            ui.separator();
            item(ui, cx, "view.place-user-origin", &mut chosen);
            item(ui, cx, "view.reset-user-origin", &mut chosen);
            #[cfg(feature = "perf")]
            crate::perf::menu_item(ui);
        });
        ui.menu_button("Design", |ui| {
            item(ui, cx, "tool.select", &mut chosen);
            item(ui, cx, "tool.wall", &mut chosen);
            item(ui, cx, "tool.slab", &mut chosen);
        });
        ui.menu_button("Project", |ui| {
            item(ui, cx, "project.storeys", &mut chosen);
            item(ui, cx, "project.layers", &mut chosen);
            item(ui, cx, "project.materials", &mut chosen);
            item(ui, cx, "project.settings", &mut chosen);
        });
        // The menu titles move to the command registry (archicraft-5eu);
        // Help is the pilot of the message catalogue (NFR-L10N-001).
        ui.menu_button(tr!("menu-help"), |ui| {
            item(ui, cx, "help.shortcuts", &mut chosen);
            item(ui, cx, "help.about", &mut chosen);
        });
    });
    (chosen, recent)
}

/// The tool palette (UI-001): selection, modelling and view tools. Returns
/// the command of a clicked view tool.
pub fn toolbox(ui: &mut Ui, ws: &mut Workspace) -> Option<CommandId> {
    let mut chosen = None;
    let tool_button = |ui: &mut Ui, ws: &mut Workspace, kind: ToolKind, command: &str| {
        let shortcut = shortcut_text(ui, command)
            .map(|s| format!(" ({s})"))
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
    tool_button(ui, ws, ToolKind::Select, "tool.select");
    ui.add_space(6.0);
    ui.label(RichText::new("Modelling").small().weak());
    tool_button(ui, ws, ToolKind::Wall, "tool.wall");
    tool_button(ui, ws, ToolKind::Slab, "tool.slab");
    ui.add_space(6.0);
    ui.label(RichText::new("View tools").small().weak());
    for (label, id) in [
        ("Fit", "view.fit-window"),
        ("Orbit", "view.orbit"),
        ("Plan/3D", "view.switch-plan-3d"),
    ] {
        let spec = spec(id);
        let tip = shortcut_text(ui, id)
            .map(|s| format!("{} ({s})", spec.label()))
            .unwrap_or_else(|| spec.label());
        if ui
            .add_sized([88.0, 22.0], Button::new(label))
            .on_hover_text(tip)
            .clicked()
        {
            chosen = Some(spec.id);
        }
    }
    chosen
}

/// The status line with the input prompt, the last message and the
/// view-control bar of the active view (UI-001).
pub fn status_bar(ui: &mut Ui, ws: &mut Workspace, problem_count: usize) -> Option<CommandId> {
    let mut chosen = None;
    ui.horizontal(|ui| {
        // The prompt of the step in progress (INP-001, NFR-USE-003).
        let prompt = ws.consumer().step().map_or_else(
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
                chosen = Some(spec("view.problems").id);
            }
            ui.separator();
            match ws.active_pane {
                Some(Pane::View3d(view)) => {
                    if let Some(state) = ws.views_3d.get_mut(&view) {
                        if ui.small_button("Fit").clicked() {
                            chosen = Some(spec("view.fit-window").id);
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
                        chosen = Some(spec("view.fit-window").id);
                    }
                    if ui.small_button("Next zoom ▶").clicked() {
                        chosen = Some(spec("view.next-zoom").id);
                    }
                    if ui.small_button("◀ Previous zoom").clicked() {
                        chosen = Some(spec("view.previous-zoom").id);
                    }
                    if let Some(state) = ws.plan_views.get(&storey) {
                        // INP-014: the cursor's X and Y from the user origin,
                        // along its axes.
                        if let Some(cursor) = state.cursor {
                            let p = ws.frames.user_origin.to_local(cursor);
                            let units = &ws.doc.project.settings.units;
                            let nf = ws.prefs.number_format();
                            let x = format_length(p.x, units, &nf, true);
                            let y = format_length(p.y, units, &nf, true);
                            ui.label(RichText::new(format!("X {x}  Y {y}")).small().monospace());
                        }
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
    chosen
}
