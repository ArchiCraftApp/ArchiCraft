//! Menu bar, toolbar, tool palette and status line (UI-001, UI-011, UI-017,
//! NFR-USE-003). The view-control bar is `ui/view_bar.rs`.

use std::path::PathBuf;
use std::time::Duration;

use archicraft_model::units::format_length;
use egui::{Button, RichText, Ui};

use super::command_widgets::{TOOLBOX_BUTTON, bar_button, icon_and_label, menu_item, titled_tooltip, tool_button};
use crate::commands::{
    CommandCtx, CommandId, CommandSpec, MENUS, MenuEntry, Registry, edit, file, menu_title, toolbox_groups, tools, view,
};
use crate::input::InputMode;
use crate::persistence::prefs::MAX_TOOLBOX_COLUMNS;
use crate::tools::ToolKind;
use crate::tr;
use crate::views::transfer::TransferMode;
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

/// The side of the toolbar's icons, in points.
const TOOLBAR_ICON: f32 = 16.0;

/// The toolbar below the menu bar (UI-011): Save at the far left (PRJ-003),
/// a separator, Undo and Redo, a separator, Pick up and Inject parameters
/// (ELM-003), a separator, the Measure tool (MEA-001), a separator, then
/// Group and Ungroup (ELM-015), greyed out while they cannot
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
            edit::PICK_UP_PARAMETERS,
            edit::INJECT_PARAMETERS,
            tools::MEASURE,
            edit::GROUP,
            edit::UNGROUP,
        ];
        for (i, id) in ids.into_iter().enumerate() {
            // Save, the history, the parameter transfer, the measure tool
            // and the groups stand apart.
            if [1, 3, 5, 6].contains(&i) {
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
                .on_hover_ui(|ui| titled_tooltip(ui, cx, spec, &spec.label_in(cx)))
                .on_disabled_hover_ui(|ui| titled_tooltip(ui, cx, spec, &spec.label_in(cx)));
            if response.clicked() {
                chosen = Some(spec.id);
            }
        }
    });
    chosen
}

/// The id of the toolbox panel, under which egui keeps its rectangle.
pub const TOOLBOX_PANEL: &str = "toolbox";

/// The width of the toolbox's contents with `columns` columns of tool
/// buttons `spacing` apart.
pub fn toolbox_content_width(columns: u8, spacing: f32) -> f32 {
    let columns = f32::from(columns.clamp(1, MAX_TOOLBOX_COLUMNS));
    columns * TOOLBOX_BUTTON + (columns - 1.0) * spacing
}

/// The columns of tool buttons that fit into a toolbox `width` wide inside:
/// one up to the width of two buttons and the `spacing` between them, two
/// from there (UI-001).
pub fn toolbox_columns(width: f32, spacing: f32) -> u8 {
    (1..=MAX_TOOLBOX_COLUMNS)
        .rev()
        .find(|&columns| width + 0.5 >= toolbox_content_width(columns, spacing))
        .unwrap_or(1)
}

/// The toolbox, the tool palette (see [`tool_palette`]) in a left panel
/// whose edge the user drags to switch between one and two columns of
/// tools (UI-001, UI-015). While a drag is in progress the panel follows
/// the pointer between the widths of one and two columns, and `columns`
/// follows the width; otherwise the panel is exactly as wide as `columns`
/// columns, so on release it snaps to the columns shown. Returns the
/// command of the clicked button.
pub fn toolbox(ui: &mut Ui, cx: &CommandCtx, columns: &mut u8) -> Option<CommandId> {
    let spacing = ui.spacing().item_spacing.x;
    let frame = egui::Frame::side_top_panel(ui.style());
    // The panel keeps room for its separator line in its outer margin.
    let line = ui.visuals().widgets.noninteractive.bg_stroke.width.round();
    let margin = frame.total_margin().sum().x + line;
    let outer = |columns| toolbox_content_width(columns, spacing) + margin;
    let panel = egui::Panel::left(TOOLBOX_PANEL).frame(frame).resizable(true);
    // A drag of the edge cannot start or end while the panel has one exact
    // size, so any drag in progress opens the range.
    let panel = if ui.ctx().dragged_id().is_some() {
        panel.size_range(outer(1)..=outer(MAX_TOOLBOX_COLUMNS))
    } else {
        panel.exact_size(outer(*columns))
    };
    panel
        .show(ui, |ui| {
            *columns = toolbox_columns(ui.available_width(), spacing);
            tool_palette(ui, cx, Registry::get().specs(), *columns)
        })
        .inner
}

/// The tool palette of `specs` (UI-001) in `columns` columns: per
/// [`ToolGroup`](crate::commands::ToolGroup) that has a tool, in group
/// order (selection, modelling, 2D and annotation, view), a thin separator
/// that names the group on hover, then its tools in rows, each an icon
/// button with the tool's name, shortcut and description as tooltip
/// (UI-011). Returns the command of the clicked button.
pub fn tool_palette(ui: &mut Ui, cx: &CommandCtx, specs: &[CommandSpec], columns: u8) -> Option<CommandId> {
    let mut chosen = None;
    let width = toolbox_content_width(columns, ui.spacing().item_spacing.x);
    for (group, tools) in toolbox_groups(specs) {
        group_separator(ui, &tr!(group.title_key()), width);
        for row in tools.chunks(usize::from(columns.max(1))) {
            ui.horizontal(|ui| {
                for spec in row {
                    let label = spec.toolbox.map(|place| tr!(place.label)).unwrap_or_default();
                    if tool_button(ui, cx, spec, &label) {
                        chosen = Some(spec.id);
                    }
                }
            });
        }
    }
    chosen
}

/// The separator above a group of the tool palette, `width` wide: a thin
/// line that shows the group's `title` on hover and has it as accessible
/// name (UI-001).
fn group_separator(ui: &mut Ui, title: &str, width: f32) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 9.0), egui::Sense::hover());
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    ui.painter().hline(rect.x_range(), rect.center().y, stroke);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, title));
    response.on_hover_text(title);
}

/// The label of the status line's problems button: "No problems", or the
/// count with a Fluent plural, such as "⚠ 1 problem" (NFR-L10N-001).
fn problems_label(ws: &Workspace, count: usize) -> String {
    tr!("status-problems", count = count, count_text = ws.count_text(count))
}

/// The status line (UI-001): the prompt for the next input step, the
/// measurement of the measure tool, the last message, the problem count,
/// the indicators of suspended groups (ELM-015) and of contour detection
/// while it takes the clicks (INP-018) and, in a plan, the cursor's
/// coordinates and the storey.
pub fn status_bar(ui: &mut Ui, ws: &Workspace, cx: &CommandCtx, problem_count: usize) -> Option<CommandId> {
    let mut chosen = None;
    ui.horizontal(|ui| {
        // The prompt of the step in progress (INP-001, NFR-USE-003); a
        // latched parameter transfer takes the clicks first (ELM-003).
        let prompt = match cx.transfer {
            Some(TransferMode::PickUp) => Some(tr!("status-prompt-pick-up")),
            Some(TransferMode::Inject) => Some(tr!("status-prompt-inject")),
            None => None,
        };
        let prompt = prompt.unwrap_or_else(|| {
            ws.consumer().step().map_or_else(
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
            )
        });
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
            if bar_button(ui, cx, spec(view::PROBLEMS), &problems_label(ws, problem_count)) {
                chosen = Some(view::PROBLEMS);
            }
            ui.separator();
            // ELM-015: while groups are suspended; a click resumes them.
            if cx.groups_suspended {
                if bar_button(ui, cx, spec(edit::SUSPEND_GROUPS), &tr!("status-groups-suspended")) {
                    chosen = Some(edit::SUSPEND_GROUPS);
                }
                ui.separator();
            }
            // INP-018: while detection is on and takes the tool's clicks; a
            // click switches it off.
            if ws.active_consumer == Consumer::Tool && ws.tool.shows_detection() {
                if bar_button(ui, cx, spec(tools::CONTOUR_DETECTION), &tr!("status-contour-detection")) {
                    chosen = Some(tools::CONTOUR_DETECTION);
                }
                ui.separator();
            }
            if let Some(Pane::Plan(storey)) = ws.active_pane {
                let name = ws
                    .doc
                    .project
                    .storeys
                    .get(storey)
                    .map(|s| s.name.clone())
                    .unwrap_or_default();
                ui.label(RichText::new(tr!("status-plan", storey = name)).small());
                // INP-014: the cursor's X and Y from the user origin, along
                // its axes.
                if let Some(cursor) = ws.plan_views.get(&storey).and_then(|state| state.cursor) {
                    let p = ws.frames.user_origin.to_local(cursor);
                    let units = &ws.doc.project.settings.units;
                    let nf = ws.prefs.number_format();
                    let x = format_length(p.x, units, &nf, true);
                    let y = format_length(p.y, units, &nf, true);
                    ui.label(RichText::new(tr!("status-cursor", x = x, y = y)).small().monospace());
                }
            }
        });
    });
    chosen
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use archicraft_model::template::default_metric_project;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use super::*;
    use crate::commands::{Category, ToolGroup};
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

    /// The vertical positions of the labels `labels` in the tool palette of
    /// `specs`.
    fn palette_rows(specs: Vec<CommandSpec>, labels: &[&str]) -> Vec<f32> {
        let cx = CommandCtx::default();
        let harness = Harness::new_ui(move |ui| {
            tool_palette(ui, &cx, &specs, 1);
        });
        labels
            .iter()
            .map(|label| harness.get_by_label(label).rect().top())
            .collect()
    }

    /// UI-001: one column of tool buttons fits up to the width of two
    /// buttons and the spacing between them, two from there; each count's
    /// width gives that count back.
    #[test]
    fn the_toolbox_width_gives_its_columns() {
        let spacing = 8.0;
        assert_eq!(toolbox_content_width(1, spacing), TOOLBOX_BUTTON);
        assert_eq!(toolbox_content_width(2, spacing), 2.0 * TOOLBOX_BUTTON + spacing);
        assert_eq!(toolbox_columns(0.0, spacing), 1);
        assert_eq!(toolbox_columns(2.0 * TOOLBOX_BUTTON + spacing - 1.0, spacing), 1);
        assert_eq!(toolbox_columns(2.0 * TOOLBOX_BUTTON + spacing, spacing), 2);
        assert_eq!(toolbox_columns(500.0, spacing), 2);
        for columns in 1..=MAX_TOOLBOX_COLUMNS {
            assert_eq!(
                toolbox_columns(toolbox_content_width(columns, spacing), spacing),
                columns
            );
        }
    }

    /// UI-001: the toolbox shows the Selection, Modelling, 2D and annotation
    /// and View tools separators (named by their groups) with their tools in
    /// that order; a group without tools has no separator, and a stub
    /// Annotation2d tool after the Text tool (ANN-007) follows it under the
    /// 2D and annotation heading, between Modelling and View tools.
    #[test]
    fn the_tool_palette_groups_the_registrys_tools() {
        let specs = Registry::get().specs().to_vec();
        let rows = palette_rows(
            specs.clone(),
            &[
                "Selection",
                "Select",
                "Modelling",
                "Wall",
                "Slab",
                "2D and annotation",
                "Text",
                "View tools",
                "Fit",
                "Orbit",
                "Plan/3D",
                "Measure",
            ],
        );
        assert!(rows.is_sorted(), "{rows:?}");
        let harness = {
            let cx = CommandCtx::default();
            let specs: Vec<CommandSpec> = specs
                .iter()
                .filter(|s| s.toolbox.is_none_or(|t| t.group != ToolGroup::Annotation2d))
                .cloned()
                .collect();
            Harness::new_ui(move |ui| {
                tool_palette(ui, &cx, &specs, 1);
            })
        };
        assert!(
            harness.query_by_label("2D and annotation").is_none(),
            "a group without tools has no separator"
        );

        let mut stubbed = specs;
        // The stub's label key is not in the catalogue, so it shows as the key.
        stubbed.push(
            CommandSpec::new("tool.stub-line", Category::Design, |_, _, _| {}).toolbox(
                ToolGroup::Annotation2d,
                1,
                "toolbox-stub-line",
            ),
        );
        let rows = palette_rows(
            stubbed,
            &[
                "Selection",
                "Modelling",
                "Slab",
                "2D and annotation",
                "Text",
                "toolbox-stub-line",
                "View tools",
                "Fit",
            ],
        );
        assert!(rows.is_sorted(), "{rows:?}");
    }
}
