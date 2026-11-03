//! Menu bar, toolbar, tool palette and status line (UI-001, UI-011, UI-017,
//! NFR-USE-003).

use std::cell::Cell;
use std::path::PathBuf;
use std::time::Duration;

use archicraft_model::units::format_length;
use egui::{Button, RichText, Ui};

use super::command_widgets::{bar_button, icon_and_label, menu_item, shortcut_text, tool_button};
use crate::commands::{
    CommandCtx, CommandId, CommandSpec, Enabled, MENUS, MenuEntry, Registry, edit, file, menu_title, tools, view,
    view3d,
};
use crate::input::InputMode;
use crate::tools::ToolKind;
use crate::tr;
use crate::views::camera::ViewPreset;
use crate::views::render3d::DisplayMode;
use crate::workspace::{Consumer, Pane, Workspace};

/// The registered command `id`. The ids this file uses are constants of
/// `crate::commands` and family members, whose tests check that they are
/// registered.
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

/// The toolbar below the menu bar (UI-011): Save at the far left (PRJ-003),
/// a separator, Undo and Redo, a separator, the Measure tool (MEA-001), a
/// separator, then Group and Ungroup (ELM-015), greyed out while they cannot
/// run, with their icons, labels and tooltips from the registry. A command
/// with a checked state, such as a tool, shows selected while it is on, as
/// in the menus. Returns the command of the clicked button.
pub fn toolbar(ui: &mut Ui, cx: &CommandCtx) -> Option<CommandId> {
    let mut chosen = None;
    ui.horizontal(|ui| {
        let ids = [
            file::SAVE,
            edit::UNDO,
            edit::REDO,
            tools::MEASURE,
            edit::GROUP,
            edit::UNGROUP,
        ];
        for (i, id) in ids.into_iter().enumerate() {
            // Save, the history, the measure tool and the groups stand apart.
            if i == 1 || i == 3 || i == 4 {
                ui.separator();
            }
            let spec = spec(id);
            let enabled = (spec.enabled)(cx);
            let atoms = icon_and_label(spec, TOOLBAR_ICON, spec.label());
            let button = match spec.checked_in(cx) {
                Some(on) => Button::selectable(on, atoms),
                None => Button::new(atoms),
            }
            .image_tint_follows_text_color(true);
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
    let mut button = |ui: &mut Ui, id: CommandId, label: String| {
        if tool_button(ui, cx, spec(id), &label) {
            chosen = Some(id);
        }
    };
    ui.label(RichText::new(tr!("toolbox-selection")).small().weak());
    button(ui, tools::command(ToolKind::Select), tr!("toolbox-select"));
    ui.add_space(6.0);
    ui.label(RichText::new(tr!("toolbox-modelling")).small().weak());
    button(ui, tools::command(ToolKind::Wall), tr!("toolbox-wall"));
    button(ui, tools::command(ToolKind::Slab), tr!("toolbox-slab"));
    ui.add_space(6.0);
    ui.label(RichText::new(tr!("toolbox-view-tools")).small().weak());
    button(ui, view::FIT_WINDOW, tr!("toolbox-fit"));
    button(ui, view::ORBIT, tr!("toolbox-orbit"));
    button(ui, view::SWITCH_PLAN_3D, tr!("toolbox-plan-3d"));
    button(ui, tools::command(ToolKind::Measure), tr!("toolbox-measure"));
    chosen
}

/// The label of the status line's problems button: "No problems", or the
/// count with a Fluent plural, such as "⚠ 1 problem" (NFR-L10N-001).
fn problems_label(ws: &Workspace, count: usize) -> String {
    tr!("status-problems", count = count, count_text = ws.count_text(count))
}

/// The status line with the input prompt, the last message, the indicator
/// of suspended groups (ELM-015) and the view-control bar of the active view
/// (UI-001).
pub fn status_bar(ui: &mut Ui, ws: &Workspace, cx: &CommandCtx, problem_count: usize) -> Option<CommandId> {
    // Shared by the buttons and the Views menu.
    let chosen = Cell::new(None);
    let button = |ui: &mut Ui, id: CommandId, label: &str| {
        if bar_button(ui, cx, spec(id), label) {
            chosen.set(Some(id));
        }
    };
    ui.horizontal(|ui| {
        // The prompt of the step in progress (INP-001, NFR-USE-003).
        let prompt = ws.consumer().step().map_or_else(
            || tr!("status-prompt-select"),
            |step| {
                let mut line = step.status_line();
                // INP-020: the prompts name clicks, which take points in both
                // modes; press-drag-release adds the drag.
                if ws.prefs.input_mode == InputMode::DragRelease {
                    line.push_str(" · ");
                    line.push_str(&tr!("status-prompt-press-drag-release"));
                }
                line
            },
        );
        ui.label(RichText::new(prompt).small());
        // MEA-001: the values of the measurement, in working units.
        if ws.tool.kind == ToolKind::Measure && ws.active_consumer == Consumer::Tool {
            let readout = ws
                .tool
                .measure
                .readout(&ws.doc.project.settings.units, &ws.prefs.number_format());
            if !readout.is_empty() {
                ui.separator();
                let text: Vec<String> = readout
                    .into_iter()
                    .map(|(label, value)| format!("{label} {value}"))
                    .collect();
                ui.label(RichText::new(text.join("  ")).small().monospace());
            }
        }
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
            button(ui, view::PROBLEMS, &problems_label(ws, problem_count));
            ui.separator();
            // ELM-015: while groups are suspended; a click resumes them.
            if cx.groups_suspended {
                button(ui, edit::SUSPEND_GROUPS, &tr!("status-groups-suspended"));
                ui.separator();
            }
            match ws.active_pane {
                Some(Pane::View3d(_)) => {
                    if let Some(view3d) = cx.view3d {
                        button(ui, view::FIT_WINDOW, &tr!("status-fit"));
                        button(ui, view::ORBIT, &tr!("status-orbit"));
                        egui::ComboBox::from_id_salt("display-mode")
                            .selected_text(tr!(view3d.mode.label_key()))
                            .show_ui(ui, |ui| {
                                for mode in DisplayMode::ALL {
                                    button(ui, view3d::display(mode), &tr!(mode.label_key()));
                                }
                            })
                            .response
                            .on_hover_text(tr!("status-display-mode-hint"));
                        ui.menu_button(tr!("status-views"), |ui| {
                            for preset in ViewPreset::ALL {
                                let id = view3d::preset(preset);
                                if menu_item(ui, cx, spec(id)) {
                                    chosen.set(Some(id));
                                }
                            }
                        })
                        .response
                        .on_hover_text(tr!("status-view-preset-hint"));
                        button(ui, view3d::TOGGLE_PROJECTION, &tr!("status-perspective"));
                    }
                }
                Some(Pane::Plan(storey)) => {
                    button(ui, view::FIT_WINDOW, &tr!("status-fit"));
                    button(ui, view::NEXT_ZOOM, &tr!("status-next-zoom"));
                    button(ui, view::PREVIOUS_ZOOM, &tr!("status-previous-zoom"));
                    if let Some(state) = ws.plan_views.get(&storey) {
                        // INP-014: the cursor's X and Y from the user origin,
                        // along its axes.
                        if let Some(cursor) = state.cursor {
                            let p = ws.frames.user_origin.to_local(cursor);
                            let units = &ws.doc.project.settings.units;
                            let nf = ws.prefs.number_format();
                            let x = format_length(p.x, units, &nf, true);
                            let y = format_length(p.y, units, &nf, true);
                            ui.label(RichText::new(tr!("status-cursor", x = x, y = y)).small().monospace());
                        }
                        // NFR-L10N-004: digit grouping applies to the zoom too.
                        let scale = ws.prefs.number_format().format_decimal(state.camera.scale, 0);
                        ui.label(RichText::new(tr!("status-zoom", scale = scale)).small().weak());
                    }
                    ui.label(RichText::new(tr!("status-scale")).small().weak());
                    let name = ws
                        .doc
                        .project
                        .storeys
                        .get(storey)
                        .map(|s| s.name.clone())
                        .unwrap_or_default();
                    ui.label(RichText::new(tr!("status-plan", storey = name)).small());
                }
                None => {}
            }
        });
    });
    chosen.into_inner()
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use archicraft_model::template::default_metric_project;

    use super::*;
    use crate::document::Document;
    use crate::i18n::{self, Localizer};
    use crate::persistence::prefs::Preferences;

    /// NFR-L10N-001: the problem count of the status line is a Fluent
    /// plural, so it reads "1 problem" and "3 problems"; none reads "No
    /// problems" as before.
    #[test]
    fn the_problem_count_is_a_plural() {
        let ws = Workspace::new(Document::new(default_metric_project(), 100), Preferences::default());
        let label = |count| i18n::with(Rc::new(Localizer::english()), || problems_label(&ws, count));
        assert_eq!(label(0), "No problems");
        assert_eq!(label(1), "⚠ 1 problem");
        assert_eq!(label(3), "⚠ 3 problems");
    }
}
