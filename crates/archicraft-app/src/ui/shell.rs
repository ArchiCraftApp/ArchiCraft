//! Menu bar, toolbar, tool palette and status line (UI-001, UI-011, UI-017,
//! NFR-USE-003).

use std::path::PathBuf;
use std::time::Duration;

use archicraft_model::project::Projection;
use archicraft_model::units::format_length;
use egui::{Button, RichText, Ui};

use super::command_widgets::{bar_button, icon_and_label, menu_item, shortcut_text, tool_button};
use crate::commands::{
    CommandCtx, CommandId, CommandSpec, Enabled, MENUS, MenuEntry, Registry, edit, file, menu_title, tools, view,
};
use crate::input::InputMode;
use crate::tools::ToolKind;
use crate::tr;
use crate::views::camera::ViewPreset;
use crate::views::render3d::DisplayMode;
use crate::workspace::{Pane, Workspace};

/// The registered command `id`. The ids this file uses are constants of
/// `crate::commands`, whose tests check that they are registered.
fn spec(id: CommandId) -> &'static CommandSpec {
    Registry::get().command(id)
}

/// What the user chose in the menu bar.
#[derive(Default)]
struct Chosen {
    command: Option<CommandId>,
    recent: Option<PathBuf>,
}

/// The entries of the menu `path`, built from the menu places of the
/// registry (UI-017).
fn menu(ui: &mut Ui, ws: &Workspace, cx: &CommandCtx, path: &str, chosen: &mut Chosen) {
    for entry in Registry::get().menu(path) {
        match entry {
            MenuEntry::Command(spec) => {
                if menu_item(ui, cx, spec) {
                    chosen.command = Some(spec.id);
                }
                if spec.id == file::OPEN {
                    open_recent(ui, ws, &mut chosen.recent);
                }
            }
            MenuEntry::Submenu(sub) => {
                let title = menu_title(sub).unwrap_or_else(|| sub.to_owned());
                ui.menu_button(title, |ui| menu(ui, ws, cx, sub, chosen));
            }
            MenuEntry::Separator => {
                ui.separator();
            }
        }
    }
    #[cfg(feature = "perf")]
    if path == "view" {
        crate::perf::menu_item(ui);
    }
}

/// The Open recent submenu, a hook after File > Open…, with the recently
/// opened project files.
fn open_recent(ui: &mut Ui, ws: &Workspace, recent: &mut Option<PathBuf>) {
    ui.menu_button(tr!("menu-file-recent"), |ui| {
        if ws.prefs.recent.is_empty() {
            ui.label(RichText::new(tr!("menu-file-recent-empty")).weak());
        }
        for path in &ws.prefs.recent {
            if ui.button(path.display().to_string()).clicked() {
                *recent = Some(path.clone());
                ui.close();
            }
        }
    });
}

/// The menu bar (UI-011, UI-017): the menus of [`MENUS`] with the commands
/// placed in them. Returns the chosen command, or a recent file to open.
pub fn menu_bar(ui: &mut Ui, ws: &Workspace, cx: &CommandCtx) -> (Option<CommandId>, Option<PathBuf>) {
    let mut chosen = Chosen::default();
    egui::MenuBar::new().ui(ui, |ui| {
        for path in MENUS {
            let title = menu_title(path).unwrap_or_else(|| path.to_owned());
            ui.menu_button(title, |ui| menu(ui, ws, cx, path, &mut chosen));
        }
    });
    (chosen.command, chosen.recent)
}

/// The tooltip of a toolbar button (UI-011): the command's label in the
/// state `cx`, such as "Undo Create wall", with its shortcut, then its
/// description and, while it cannot run, the reason from the catalogue.
fn tooltip(ui: &mut Ui, cx: &CommandCtx, spec: &CommandSpec, enabled: Enabled) {
    ui.set_max_width(ui.spacing().tooltip_width);
    ui.horizontal(|ui| {
        ui.strong(spec.label_in(cx));
        if let Some(keys) = shortcut_text(spec, &cx.keymap, |b| ui.ctx().format_shortcut(&b.shortcut())) {
            ui.weak(keys);
        }
    });
    ui.label(spec.description());
    if let Enabled::No(reason) = enabled {
        ui.colored_label(ui.visuals().warn_fg_color, tr!(reason));
    }
}

/// The side of the toolbar's icons, in points.
const TOOLBAR_ICON: f32 = 16.0;

/// The toolbar below the menu bar (UI-011): Undo and Redo, greyed out while
/// they cannot run, with their icons, labels and tooltips from the registry.
/// Returns the command of the clicked button.
pub fn toolbar(ui: &mut Ui, cx: &CommandCtx) -> Option<CommandId> {
    let mut chosen = None;
    ui.horizontal(|ui| {
        for id in [edit::UNDO, edit::REDO] {
            let spec = spec(id);
            let enabled = (spec.enabled)(cx);
            let button =
                Button::new(icon_and_label(spec, TOOLBAR_ICON, spec.label())).image_tint_follows_text_color(true);
            let response = ui
                .add_enabled(enabled.is_yes(), button)
                .on_hover_ui(|ui| tooltip(ui, cx, spec, enabled))
                .on_disabled_hover_ui(|ui| tooltip(ui, cx, spec, enabled));
            if response.clicked() {
                chosen = Some(spec.id);
            }
        }
    });
    chosen
}

/// The tool palette (UI-001): selection, modelling and view tools, each an
/// icon and a label, with the command's description and shortcut as tooltip
/// (UI-011). Returns the command of the clicked button.
pub fn toolbox(ui: &mut Ui, cx: &CommandCtx) -> Option<CommandId> {
    let mut chosen = None;
    let mut button = |ui: &mut Ui, id: CommandId, label: &str| {
        if tool_button(ui, cx, spec(id), label) {
            chosen = Some(id);
        }
    };
    ui.label(RichText::new("Selection").small().weak());
    button(ui, tools::command(ToolKind::Select), ToolKind::Select.label());
    ui.add_space(6.0);
    ui.label(RichText::new("Modelling").small().weak());
    for kind in [ToolKind::Wall, ToolKind::Slab] {
        button(ui, tools::command(kind), kind.label());
    }
    ui.add_space(6.0);
    ui.label(RichText::new("View tools").small().weak());
    for (label, id) in [
        ("Fit", view::FIT_WINDOW),
        ("Orbit", view::ORBIT),
        ("Plan/3D", view::SWITCH_PLAN_3D),
    ] {
        button(ui, id, label);
    }
    chosen
}

/// The status line with the input prompt, the last message and the
/// view-control bar of the active view (UI-001).
pub fn status_bar(ui: &mut Ui, ws: &mut Workspace, cx: &CommandCtx, problem_count: usize) -> Option<CommandId> {
    let mut chosen = None;
    let mut button = |ui: &mut Ui, id: CommandId, label: &str| {
        if bar_button(ui, cx, spec(id), label) {
            chosen = Some(id);
        }
    };
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
            button(ui, view::PROBLEMS, &label);
            ui.separator();
            match ws.active_pane {
                Some(Pane::View3d(view_id)) => {
                    if let Some(state) = ws.views_3d.get_mut(&view_id) {
                        button(ui, view::FIT_WINDOW, "Fit");
                        button(ui, view::ORBIT, "Orbit");
                        egui::ComboBox::from_id_salt("display-mode")
                            .selected_text(tr!(state.mode.label_key()))
                            .show_ui(ui, |ui| {
                                for mode in DisplayMode::ALL {
                                    ui.selectable_value(&mut state.mode, mode, tr!(mode.label_key()));
                                }
                            });
                        ui.menu_button("Views", |ui| {
                            for preset in ViewPreset::ALL {
                                if ui.button(tr!(preset.label_key())).clicked() {
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
                    button(ui, view::FIT_WINDOW, "Fit");
                    button(ui, view::NEXT_ZOOM, "Next zoom ▶");
                    button(ui, view::PREVIOUS_ZOOM, "◀ Previous zoom");
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
