//! Menu bar, toolbar, tool palette and status line (UI-001, UI-011, UI-017,
//! NFR-USE-003).

use std::path::PathBuf;
use std::time::Duration;

use archicraft_model::project::Projection;
use archicraft_model::units::format_length;
use egui::{Atom, Atoms, Button, RichText, Ui};

use crate::commands::{
    CommandCtx, CommandId, CommandSpec, Enabled, MENUS, MenuEntry, Registry, edit, file, menu_title, tools, view,
};
use crate::input::InputMode;
use crate::tools::ToolKind;
use crate::tr;
use crate::views::camera::ViewPreset;
use crate::views::render3d::DisplayMode;
use crate::workspace::{Pane, Workspace};

/// The text of the first default binding of `spec`, if it has one.
fn shortcut_text(ui: &Ui, spec: &CommandSpec) -> Option<String> {
    spec.default_bindings
        .first()
        .map(|b| ui.ctx().format_shortcut(&b.shortcut()))
}

/// The registered command `id`. The ids this file uses are constants of
/// `crate::commands`, whose tests check that they are registered.
fn spec(id: CommandId) -> &'static CommandSpec {
    Registry::get()
        .spec(id.as_str())
        .unwrap_or_else(|| panic!("no command {id}"))
}

/// The menu item of `spec`: its label, shortcut, enabled state and, for an
/// on/off command, a check box, all from the registry.
fn item(ui: &mut Ui, cx: &CommandCtx, spec: &CommandSpec, out: &mut Option<CommandId>) {
    let label = spec.label_in(cx);
    let enabled = (spec.enabled)(cx).is_yes();
    let clicked = if let Some(checked) = spec.checked {
        let mut on = checked(cx);
        let mut atoms = Atoms::new(label);
        if let Some(text) = shortcut_text(ui, spec) {
            atoms.push_right(Atom::grow());
            atoms.push_right(RichText::new(text).weak());
        }
        ui.add_enabled(enabled, egui::Checkbox::new(&mut on, atoms)).clicked()
    } else {
        let mut button = Button::new(label);
        if let Some(text) = shortcut_text(ui, spec) {
            button = button.shortcut_text(text);
        }
        ui.add_enabled(enabled, button).clicked()
    };
    if clicked {
        *out = Some(spec.id);
        ui.close();
    }
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
                item(ui, cx, spec, &mut chosen.command);
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
        if let Some(keys) = shortcut_text(ui, spec) {
            ui.weak(keys);
        }
    });
    ui.label(spec.description());
    if let Enabled::No(reason) = enabled {
        ui.colored_label(ui.visuals().warn_fg_color, tr!(reason));
    }
}

/// The toolbar below the menu bar (UI-011): Undo and Redo, greyed out while
/// they cannot run, with their labels and tooltips from the registry.
/// Returns the command of the clicked button.
pub fn toolbar(ui: &mut Ui, cx: &CommandCtx) -> Option<CommandId> {
    let mut chosen = None;
    ui.horizontal(|ui| {
        // egui's default proportional font draws ⟲ and ⟳ from its emoji
        // icon font; ↶ and ↷ are only in the monospace font.
        for (id, arrow) in [(edit::UNDO, "⟲"), (edit::REDO, "⟳")] {
            let spec = spec(id);
            let enabled = (spec.enabled)(cx);
            let response = ui
                .add_enabled(enabled.is_yes(), Button::new((arrow, spec.label())))
                .on_hover_ui(|ui| tooltip(ui, cx, spec, enabled))
                .on_disabled_hover_ui(|ui| tooltip(ui, cx, spec, enabled));
            if response.clicked() {
                chosen = Some(spec.id);
            }
        }
    });
    chosen
}

/// The tool palette (UI-001): selection, modelling and view tools. Returns
/// the command of the clicked button.
pub fn toolbox(ui: &mut Ui, ws: &Workspace) -> Option<CommandId> {
    let mut chosen = None;
    let tool_button = |ui: &mut Ui, kind: ToolKind, chosen: &mut Option<CommandId>| {
        let spec = spec(tools::command(kind));
        let shortcut = shortcut_text(ui, spec).map(|s| format!(" ({s})")).unwrap_or_default();
        let response = ui
            .add_sized(
                [88.0, 26.0],
                egui::Button::selectable(ws.tool.kind == kind, kind.label()),
            )
            .on_hover_text(format!("{}{shortcut}", kind.description()));
        if response.clicked() {
            *chosen = Some(spec.id);
        }
    };
    ui.label(RichText::new("Selection").small().weak());
    tool_button(ui, ToolKind::Select, &mut chosen);
    ui.add_space(6.0);
    ui.label(RichText::new("Modelling").small().weak());
    tool_button(ui, ToolKind::Wall, &mut chosen);
    tool_button(ui, ToolKind::Slab, &mut chosen);
    ui.add_space(6.0);
    ui.label(RichText::new("View tools").small().weak());
    for (label, id) in [
        ("Fit", view::FIT_WINDOW),
        ("Orbit", view::ORBIT),
        ("Plan/3D", view::SWITCH_PLAN_3D),
    ] {
        let spec = spec(id);
        let tip = shortcut_text(ui, spec)
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
                chosen = Some(view::PROBLEMS);
            }
            ui.separator();
            match ws.active_pane {
                Some(Pane::View3d(view_id)) => {
                    if let Some(state) = ws.views_3d.get_mut(&view_id) {
                        if ui.small_button("Fit").clicked() {
                            chosen = Some(view::FIT_WINDOW);
                        }
                        if ui.selectable_label(state.orbit, "Orbit").clicked() {
                            chosen = Some(view::ORBIT);
                        }
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
                        chosen = Some(view::FIT_WINDOW);
                    }
                    if ui.small_button("Next zoom ▶").clicked() {
                        chosen = Some(view::NEXT_ZOOM);
                    }
                    if ui.small_button("◀ Previous zoom").clicked() {
                        chosen = Some(view::PREVIOUS_ZOOM);
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
