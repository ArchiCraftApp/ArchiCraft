//! The properties panel and the settings dialog (UI-002, UI-003, ELM-001,
//! EDT-017).
//!
//! Both edit either the active tool's defaults (nothing selected) or the
//! selected elements. With several elements selected, fields whose values
//! differ show "Mixed", and a committed value goes to every editable selected
//! element. Locked elements are counted but not changed.

use archicraft_geometry::kinds::column::section_area;
use archicraft_model::command::relink_to_storey;
use archicraft_model::fields::{FieldSpec, Section, fields_of};
use archicraft_model::project::DefaultsSlot;
use archicraft_model::units::{format_area, format_length, format_volume};
use archicraft_model::{Command, Element, GroupId, KindTag, MODEL_TOLERANCE, PlanMeasure, StoreyId};
use egui::{Id, Ui};

use super::command_widgets::command_button;
use super::fields::Fields;
use super::strip::{Place, UnitKey};
use super::widgets::{self, Edited, common};
use crate::commands::{self, CommandCtx, CommandId, Origin, Registry, Target};
use crate::input::{DivisionMode, DivisionScope, EQUAL_PARTS};
use crate::tools::{ColumnMode, MeasureMode, SlabMode, SpaceMode, ToolKind, WallMode};
use crate::tr;
use crate::workspace::{Request, Workspace};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// One row, as in the properties strip: no widget is higher than a
    /// line.
    Strip,
    /// One unit per line, as in the strip's overflow popup.
    Overflow,
    /// Label and value per row, as in the settings dialog.
    Grid,
}

/// What is being edited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Subject {
    Defaults(KindTag),
    Selection,
}

#[derive(Clone)]
struct Editing {
    subject: Subject,
    /// Working copies: the editable selected elements, or a prototype built
    /// from the tool defaults.
    targets: Vec<Element>,
    selected: usize,
}

fn gather(ws: &Workspace) -> Option<Editing> {
    let project = &ws.doc.project;
    if ws.selection.is_empty() {
        let kind = ws.tool.kind.element_kind()?;
        return Some(Editing {
            subject: Subject::Defaults(kind),
            targets: vec![project.new_element(kind, project.storeys.ground(), 0)?],
            selected: 0,
        });
    }
    let ids = ws.selected_ids();
    let targets: Vec<Element> = ids
        .iter()
        .filter_map(|id| project.element(*id))
        .filter(|e| project.is_editable(e))
        .cloned()
        .collect();
    Some(Editing {
        subject: Subject::Selection,
        targets,
        selected: ids.len(),
    })
}

fn apply(ws: &mut Workspace, editing: Editing) {
    match editing.subject {
        Subject::Selection => {
            ws.execute(Command::UpdateElements(editing.targets));
        }
        Subject::Defaults(kind) => {
            let Some(e) = editing.targets.into_iter().next() else {
                return;
            };
            if e.tag() != kind {
                return;
            }
            // The edited prototype becomes the slot (ELM-001).
            let mut defaults = ws.doc.project.tool_defaults.clone();
            defaults.insert(DefaultsSlot::from_prototype(e));
            ws.execute(Command::SetToolDefaults(defaults));
        }
    }
}

/// The UI-only hints of empty text fields, by field key: the default-name
/// pattern of the room name for the tool defaults, the suggested `number`
/// ([`number_hint`]) and the style's size of a text without its own `size`
/// (SPC-001, ANN-007).
fn hints(ws: &Workspace, editing: &Editing) -> Vec<(&'static str, String)> {
    let has = |key: &str| editing.targets.iter().any(|e| fields_of(e).any(|s| s.key == key));
    let mut hints = Vec::new();
    if editing.subject != Subject::Selection && has("long_name") {
        hints.push(("long_name", tr!("tool-space-default-name", n = "{n}")));
    }
    if has("number")
        && let Some(hint) = number_hint(ws, editing)
    {
        hints.push(("number", hint));
    }
    if has("size") {
        let project = &ws.doc.project;
        let style_size = common(
            editing
                .targets
                .iter()
                .filter_map(|e| project.text_style(e.kind.text_style()?))
                .map(|s| s.size_mm.to_bits()),
        );
        if let Some(bits) = style_size {
            let size = widgets::format_trimmed(&ws.prefs.number_format(), f64::from_bits(bits), 3);
            hints.push(("size", tr!("dlg-properties-text-style-size", size = size.as_str())));
        }
    }
    hints
}

/// The hint of an empty number (SPC-001): for the tool defaults
/// [`Project::suggested_space_number`](archicraft_model::Project::suggested_space_number)
/// of the active storey, the number the next element gets suggested; for
/// one selected element the same pattern with its place among the elements
/// of its kind on its storey, so that the only space of the ground storey
/// shows `0.01`, not the `0.02` suggested for a further one. `None` for
/// several targets.
fn number_hint(ws: &Workspace, editing: &Editing) -> Option<String> {
    let project = &ws.doc.project;
    let [element] = editing.targets.as_slice() else {
        return None;
    };
    match editing.subject {
        Subject::Defaults(_) => project.suggested_space_number(ws.active_storey),
        Subject::Selection => {
            let number = project.storeys.number(element.storey)?;
            let place = project
                .elements_on(element.storey)
                .filter(|e| e.tag() == element.tag())
                .position(|e| e.id == element.id)?;
            Some(format!("{number}.{:02}", place + 1))
        }
    }
}

fn summary(ws: &Workspace, editing: &Editing) -> String {
    match editing.subject {
        Subject::Defaults(kind) => {
            let kind = tr!(kind.label_key());
            tr!("dlg-properties-tool-defaults", kind = kind.as_str())
        }
        Subject::Selection => {
            let project = &ws.doc.project;
            let kinds = common(
                ws.selection
                    .iter()
                    .filter_map(|id| project.element(*id))
                    .map(Element::tag),
            );
            let what = kinds.map_or_else(
                || tr!("dlg-properties-elements", count = editing.selected),
                |k| {
                    if editing.selected == 1 {
                        tr!(k.label_key())
                    } else {
                        tr!(k.noun_key(), count = editing.selected)
                    }
                },
            );
            let count_text = ws.count_text(editing.selected);
            let editable_text = ws.count_text(editing.targets.len());
            if editing.selected == 1 {
                what
            } else if editing.targets.len() == editing.selected {
                tr!(
                    "dlg-properties-selected",
                    count_text = count_text.as_str(),
                    what = what.as_str()
                )
            } else {
                tr!(
                    "dlg-properties-selected-editable",
                    count_text = count_text.as_str(),
                    what = what.as_str(),
                    editable_text = editable_text.as_str()
                )
            }
        }
    }
}

/// Read-only values of one selected element: its plan measure, such as a
/// wall's length or a space's net area, a column's cross-section area and
/// height (COL-001; the height only while no height field shows it, as for
/// a top linked to a storey), and its volume (MEA-003, SPC-001).
fn derived(ws: &mut Workspace) -> Vec<(String, String)> {
    let ids = ws.selected_ids();
    let [id] = ids.as_slice() else {
        return Vec::new();
    };
    let geometry = ws.doc.geometry();
    let project = &ws.doc.project;
    let Some(element) = project.element(*id) else {
        return Vec::new();
    };
    let units = &project.settings.units;
    let nf = ws.prefs.number_format();
    let mut rows = Vec::new();
    match element.kind.plan_measure() {
        Some(PlanMeasure::Length(length)) => {
            rows.push((tr!("dlg-properties-length"), format_length(length, units, &nf, true)))
        }
        Some(PlanMeasure::Area(area)) => rows.push((tr!("dlg-properties-area"), format_area(area, units, &nf))),
        None => {}
    }
    if let Some(area) = section_area(project, element) {
        rows.push((tr!("dlg-properties-section-area"), format_area(area, units, &nf)));
        let height_shown = fields_of(element).any(|s| s.key == "height" && (s.get)(element, project).is_some());
        if let (false, Some((bottom, top))) = (height_shown, project.vertical_extent(element)) {
            rows.push((
                tr!("dlg-properties-height"),
                format_length(top - bottom, units, &nf, true),
            ));
        }
    }
    if let Some(v) = geometry.get(*id).map(|g| g.volume()) {
        rows.push((tr!("dlg-properties-volume"), format_volume(v, units, &nf)));
    }
    rows
}

/// The rows of [`derived`] values in the settings dialog.
fn derived_rows(ui: &mut Ui, rows: &[(String, String)]) {
    for (label, value) in rows {
        ui.label(label);
        ui.label(egui::RichText::new(value).weak());
        ui.end_row();
    }
}

fn selection_only_fields(ui: &mut Ui, ws: &mut Workspace, editing: &mut Editing, layout: Layout) -> Option<String> {
    if editing.subject != Subject::Selection || editing.targets.is_empty() {
        return None;
    }
    let id = Id::new("properties").with(layout as u8);
    let mut error = None;
    let label_row = |ui: &mut Ui, label: &str| {
        ui.label(label);
    };
    if editing.targets.len() == 1 {
        label_row(ui, &tr!("dlg-properties-name"));
        let width = if layout == Layout::Grid { 180.0 } else { 70.0 };
        match widgets::text(ui, id.with("name"), Some(&editing.targets[0].name), width) {
            Edited::Changed(name) if !name.is_empty() => {
                editing.targets[0].name = name;
                let targets = editing.targets.clone();
                ws.execute(Command::UpdateElements(targets));
            }
            Edited::Invalid(e) => error = Some(e),
            _ => {}
        }
        if layout == Layout::Grid {
            ui.end_row();
        }
    }
    let storeys: Vec<(StoreyId, String)> = ws
        .doc
        .project
        .storeys
        .highest_first()
        .map(|s| (s.id, s.name.clone()))
        .collect();
    label_row(ui, &tr!("dlg-properties-home-storey"));
    let current = common(editing.targets.iter().map(|e| e.storey));
    let width = if layout == Layout::Grid { 180.0 } else { 110.0 };
    if let Some(target) = widgets::choice(ui, id.with("storey"), current.as_ref(), &storeys, width) {
        let project = &ws.doc.project;
        let moved: Vec<Element> = editing
            .targets
            .iter()
            .filter_map(|e| relink_to_storey(project, e, target))
            .collect();
        ws.execute(Command::UpdateElements(moved));
    }
    if layout == Layout::Grid {
        ui.end_row();
    }
    error
}

fn lock_toggle(ui: &mut Ui, ws: &mut Workspace) {
    let project = &ws.doc.project;
    let selected: Vec<&Element> = ws.selection.iter().filter_map(|id| project.element(*id)).collect();
    if selected.is_empty() {
        return;
    }
    let all_locked = selected.iter().all(|e| e.locked);
    let mut locked = all_locked;
    if ui
        .checkbox(&mut locked, tr!("dlg-properties-locked-toggle"))
        .on_hover_text(tr!("dlg-properties-locked-toggle-hint"))
        .changed()
    {
        let changed: Vec<Element> = selected
            .iter()
            .filter(|e| e.locked != locked)
            .map(|e| Element { locked, ..(*e).clone() })
            .collect();
        ws.execute(Command::UpdateElements(changed));
        return;
    }
    let mut snaps = selected.iter().all(|e| !e.snap_excluded);
    if ui
        .checkbox(&mut snaps, tr!("dlg-properties-snap-targets"))
        .on_hover_text(tr!("dlg-properties-snap-targets-hint"))
        .changed()
    {
        let changed: Vec<Element> = selected
            .iter()
            .filter(|e| e.snap_excluded == snaps)
            .map(|e| Element {
                snap_excluded: !snaps,
                ..(*e).clone()
            })
            .collect();
        ws.execute(Command::UpdateElements(changed));
    }
}

/// The group the selection is (ELM-015): one outermost group whose visible
/// members are all selected, and nothing else.
fn selected_group(ws: &Workspace) -> Option<GroupId> {
    let (groups, loose) = ws.selection_items();
    let ([group], []) = (&groups[..], &loose[..]) else {
        return None;
    };
    let project = &ws.doc.project;
    let visible = project
        .group_members(*group)
        .into_iter()
        .filter(|id| project.element(*id).is_some_and(|e| project.is_visible(e)))
        .count();
    (visible == ws.selection.len()).then_some(*group)
}

/// The name and size of the selected group `group`, such as "Group: Group
/// 3 (5 elements)", and a field that renames it, one undo step (ELM-015).
fn group_row(ui: &mut Ui, ws: &mut Workspace, group: GroupId) {
    let project = &ws.doc.project;
    let Some(name) = project.group(group).map(|g| g.name.clone()) else {
        return;
    };
    let count = project.group_members(group).len();
    ui.label(tr!(
        "dlg-properties-group",
        name = name.as_str(),
        count = count,
        count_text = ws.count_text(count)
    ));
    if let Edited::Changed(name) = widgets::text(ui, Id::new("properties-group-name"), Some(&name), 110.0) {
        ws.execute(Command::RenameGroup { group, name });
    }
}

/// The id of the properties strip's row, under which
/// [`RowLayout::load`](super::strip::RowLayout::load) finds its last layout.
pub fn strip_id() -> Id {
    Id::new("properties-strip")
}

/// One unit of the properties strip (UI-002): a label and its widget, or a
/// button, shown in the row or moved into the overflow popup as a whole.
/// `NameAndStorey` holds the Name and Home storey fields, `Lock` the Locked
/// and Snap targets checkboxes, `Derived` the derived value of its index
/// (MEA-003).
#[derive(Clone, Copy, Debug)]
enum Unit {
    Mode(CommandId),
    Measure(MeasureMode),
    Division,
    Empty,
    Summary,
    Group(GroupId),
    AllLocked,
    NameAndStorey,
    Field(&'static FieldSpec),
    Lock,
    Derived(usize),
    Settings,
}

impl Unit {
    /// The unit's group, in the strip's order: tool modes | Division |
    /// summary | group | Name, Home storey | geometry fields |
    /// classification fields | the other fields | Locked, Snap targets |
    /// derived values | Settings….
    fn group(self) -> u8 {
        match self {
            Unit::Mode(_) | Unit::Measure(_) => 0,
            Unit::Division => 1,
            Unit::Empty | Unit::Summary => 2,
            Unit::Group(_) => 3,
            Unit::AllLocked | Unit::NameAndStorey => 4,
            Unit::Field(spec) => 5 + field_group(spec.section),
            Unit::Lock => 8,
            Unit::Derived(_) => 9,
            Unit::Settings => 10,
        }
    }
}

/// The group of a field's section in the strip: geometry, then
/// classification, then the rest.
fn field_group(section: Section) -> u8 {
    match section {
        Section::Geometry => 0,
        Section::Classification => 1,
        Section::Kind | Section::ModelAppearance | Section::PlanDisplay => 2,
    }
}

/// What the units of one frame share.
struct StripState<'a> {
    cx: &'a CommandCtx,
    editing: Option<Editing>,
    hints: Vec<(&'static str, String)>,
    derived: Vec<(String, String)>,
    summary: String,
    error: Option<String>,
}

/// The geometry methods of the tool `kind` (WAL-001, SLB-001, SPC-001,
/// COL-002, UI-017).
fn modes(kind: ToolKind) -> Vec<CommandId> {
    match kind {
        ToolKind::Wall => WallMode::ALL.map(commands::tools::wall_mode).to_vec(),
        ToolKind::Slab => SlabMode::ALL.map(commands::tools::slab_mode).to_vec(),
        ToolKind::Space => SpaceMode::ALL.map(commands::tools::space_mode).to_vec(),
        ToolKind::Column => ColumnMode::ALL.map(commands::tools::column_mode).to_vec(),
        // ANN-007: the anchor snaps like any point, with the division.
        ToolKind::Text | ToolKind::Measure | ToolKind::Select => Vec::new(),
    }
}

/// The strip's units in their order (UI-002): the active tool's methods and
/// its division, then what is edited, its group, name and home storey, its
/// fields by group, its lock and snap toggles, its derived values and the
/// Settings button.
fn units(ws: &Workspace, editing: Option<&Editing>, visible: &[&'static FieldSpec], derived: usize) -> Vec<Unit> {
    let mut units: Vec<Unit> = modes(ws.tool.kind).into_iter().map(Unit::Mode).collect();
    if ws.tool.kind == ToolKind::Measure {
        // MEA-001: a new mode starts a new measurement.
        units.extend(MeasureMode::ALL.map(Unit::Measure));
    }
    if ws.tool.kind != ToolKind::Select {
        units.push(Unit::Division);
    }
    let Some(editing) = editing else {
        units.push(Unit::Empty);
        return units;
    };
    units.push(Unit::Summary);
    units.extend(selected_group(ws).map(Unit::Group));
    let selection = editing.subject == Subject::Selection;
    if editing.targets.is_empty() {
        units.extend([Unit::AllLocked, Unit::Lock]);
        return units;
    }
    if selection {
        units.push(Unit::NameAndStorey);
    }
    let mut fields: Vec<&'static FieldSpec> = visible.to_vec();
    fields.sort_by_key(|spec| field_group(spec.section));
    units.extend(fields.into_iter().map(Unit::Field));
    if selection {
        units.push(Unit::Lock);
        units.extend((0..derived).map(Unit::Derived));
    }
    units.push(Unit::Settings);
    units
}

/// The key of `unit`: its name names what its width depends on, other than
/// values that change while editing, such as a derived length.
fn key(unit: Unit, ws: &Workspace, st: &StripState<'_>) -> UnitKey {
    let name = match unit {
        Unit::Division => format!("division:{}", division_label(ws.prefs.input.division)),
        Unit::Summary => format!("summary:{}", st.summary),
        Unit::Group(group) => format!("group:{}", ws.doc.project.group(group).map_or("", |g| g.name.as_str())),
        Unit::Field(spec) => format!("field:{}", spec.key),
        Unit::Derived(i) => format!("derived:{}", st.derived[i].0),
        // Mode and Measure name their command and mode.
        _ => format!("{unit:?}"),
    };
    UnitKey {
        name,
        group: unit.group(),
    }
}

/// The properties strip below the toolbar (UI-002): one row of fixed
/// height that never wraps, its units in a fixed order with a separator
/// between groups; the units that do not fit are in the popup of a chevron
/// at the right edge ([`super::strip`]).
pub fn strip(ui: &mut Ui, ws: &mut Workspace, cx: &CommandCtx) {
    let editing = gather(ws);
    let (hints, visible, summary) = match &editing {
        Some(editing) => {
            let mut targets = editing.targets.clone();
            let visible = fields(ws, &mut targets, editing.subject, Layout::Strip, Vec::new()).visible();
            (hints(ws, editing), visible, summary(ws, editing))
        }
        None => (Vec::new(), Vec::new(), String::new()),
    };
    let derived = match &editing {
        Some(editing) if editing.subject == Subject::Selection && !editing.targets.is_empty() => derived(ws),
        _ => Vec::new(),
    };
    let units = units(ws, editing.as_ref(), &visible, derived.len());
    let mut st = StripState {
        cx,
        editing,
        hints,
        derived,
        summary,
        error: None,
    };
    let keys: Vec<UnitKey> = units.iter().map(|&u| key(u, ws, &st)).collect();
    super::strip::show(
        ui,
        strip_id(),
        &keys,
        &tr!("dlg-properties-more"),
        &mut |ui, i, place| {
            let layout = match place {
                Place::Row => Layout::Strip,
                Place::Overflow => Layout::Overflow,
            };
            draw_unit(ui, ws, &mut st, units[i], layout);
        },
    );
    if let Some(e) = st.error {
        ws.error(e);
    }
}

/// The field renderer over `targets`, the working copies of `subject`.
fn fields<'a>(
    ws: &'a Workspace,
    targets: &'a mut Vec<Element>,
    subject: Subject,
    layout: Layout,
    hints: Vec<(&'static str, String)>,
) -> Fields<'a> {
    Fields {
        layout,
        id: Id::new("properties").with(layout as u8),
        units: ws.doc.project.settings.units.clone(),
        nf: ws.prefs.number_format(),
        project: &ws.doc.project,
        targets,
        selection: subject == Subject::Selection,
        hints,
        changed: false,
        error: None,
    }
}

/// Draws `unit` in `layout`, the row's or the overflow popup's.
fn draw_unit(ui: &mut Ui, ws: &mut Workspace, st: &mut StripState<'_>, unit: Unit, layout: Layout) {
    match unit {
        Unit::Mode(id) => {
            if command_button(ui, st.cx, Registry::get().command(id)) {
                let target = Target::active(ws);
                ws.requests.push(Request::Invoke(id, target, Origin::Panel));
            }
        }
        Unit::Measure(mode) => {
            if ui
                .selectable_label(ws.tool.measure.mode == mode, mode.label())
                .clicked()
            {
                ws.tool.measure.set_mode(mode);
                ws.reset_input();
            }
        }
        Unit::Division => division(ui, ws),
        Unit::Empty => {
            ui.label(egui::RichText::new(tr!("dlg-properties-nothing-selected")).weak());
        }
        Unit::Summary => {
            ui.strong(st.summary.as_str());
        }
        Unit::Group(group) => group_row(ui, ws, group),
        Unit::AllLocked => {
            ui.label(tr!("dlg-properties-all-locked"));
        }
        Unit::NameAndStorey => {
            if let Some(editing) = st.editing.as_mut() {
                st.error = st.error.take().or(selection_only_fields(ui, ws, editing, layout));
            }
        }
        Unit::Field(spec) => {
            let Some(editing) = st.editing.as_mut() else {
                return;
            };
            let mut targets = std::mem::take(&mut editing.targets);
            let mut f = fields(ws, &mut targets, editing.subject, layout, st.hints.clone());
            f.show(ui, &[spec]);
            let (changed, error) = (f.changed, f.error.take());
            editing.targets = targets;
            st.error = st.error.take().or(error);
            if changed {
                apply(ws, editing.clone());
            }
        }
        Unit::Lock => lock_toggle(ui, ws),
        Unit::Derived(i) => {
            let (label, value) = &st.derived[i];
            ui.label(label);
            ui.label(egui::RichText::new(value).weak());
        }
        Unit::Settings => {
            if ui
                .button(tr!("dlg-properties-settings"))
                .on_hover_text(tr!("dlg-properties-settings-hint"))
                .clicked()
            {
                ws.requests.push(Request::Settings);
            }
        }
    }
}

/// The fields of the settings dialog (UI-003).
fn edit_fields(ui: &mut Ui, ws: &mut Workspace, editing: &mut Editing) -> Option<String> {
    let mut error = egui::Grid::new("settings-selection")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| selection_only_fields(ui, ws, editing, Layout::Grid))
        .inner;
    let hints = hints(ws, editing);
    // The dialog shows the derived values with the geometry.
    let derived = match editing.subject {
        Subject::Selection => derived(ws),
        Subject::Defaults(_) => Vec::new(),
    };
    let mut targets = std::mem::take(&mut editing.targets);
    let mut f = fields(ws, &mut targets, editing.subject, Layout::Grid, hints);
    let visible = f.visible();
    dialog_sections(ui, &mut f, &visible, &derived);
    let changed = f.changed;
    error = error.or(f.error.take());
    editing.targets = targets;
    if changed {
        apply(ws, editing.clone());
    }
    error
}

/// The settings dialog's sections (UI-003), each with the visible fields of
/// its [`Section`] in display order. The fields of a kind's own section are
/// titled with the kind's label; when every target is of one kind that has
/// such fields, its geometry joins them, as all of a space's fields are in
/// 'Space'. The derived values follow the geometry.
fn dialog_sections(ui: &mut Ui, f: &mut Fields<'_>, visible: &[&'static FieldSpec], derived: &[(String, String)]) {
    let of = |section: Section| -> Vec<&'static FieldSpec> {
        visible.iter().copied().filter(|s| s.section == section).collect()
    };
    let mut kinds: Vec<KindTag> = Vec::new();
    for tag in f.targets.iter().map(Element::tag) {
        if !kinds.contains(&tag) {
            kinds.push(tag);
        }
    }
    let kind_fields = of(Section::Kind);
    if let ([kind], false) = (kinds.as_slice(), kind_fields.is_empty()) {
        let fields: Vec<&'static FieldSpec> = visible
            .iter()
            .copied()
            .filter(|s| matches!(s.section, Section::Kind | Section::Geometry))
            .collect();
        section(ui, &tr!(kind.label_key()), |ui| {
            f.show(ui, &fields);
            derived_rows(ui, derived);
        });
    } else {
        let geometry = of(Section::Geometry);
        if !geometry.is_empty() || !derived.is_empty() {
            section(ui, "Geometry and positioning", |ui| {
                f.show(ui, &geometry);
                derived_rows(ui, derived);
            });
        }
        let mut shown: Vec<&str> = Vec::new();
        for kind in kinds {
            let fields: Vec<&'static FieldSpec> = kind_fields
                .iter()
                .copied()
                .filter(|s| !shown.contains(&s.key) && kind.descriptor().fields.iter().any(|k| k.key == s.key))
                .collect();
            if fields.is_empty() {
                continue;
            }
            shown.extend(fields.iter().map(|s| s.key));
            section(ui, &tr!(kind.label_key()), |ui| f.show(ui, &fields));
        }
    }
    let appearance = of(Section::ModelAppearance);
    if !appearance.is_empty() {
        section(ui, "Model and appearance", |ui| f.show(ui, &appearance));
    }
    // Without a material there is no cut fill to explain (SPC-001).
    let plan_display = of(Section::PlanDisplay);
    if !plan_display.is_empty() || visible.iter().any(|s| s.key == "material") {
        section(ui, "Plan display", |ui| {
            f.show(ui, &plan_display);
            ui.label("Cut fills come from the material.");
            ui.end_row();
        });
    }
    let classification = of(Section::Classification);
    if !classification.is_empty() {
        section(ui, "Classification", |ui| f.show(ui, &classification));
    }
}

fn section(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    egui::CollapsingHeader::new(title).default_open(true).show(ui, |ui| {
        egui::Grid::new(title).num_columns(2).spacing([12.0, 6.0]).show(ui, add);
    });
}

/// The division modes in the order of the control, each with its value
/// when chosen anew (INP-010).
const DIVISION_MODES: [DivisionMode; 5] = [
    DivisionMode::Half,
    DivisionMode::Equal(4),
    DivisionMode::Percent(50.0),
    DivisionMode::Repeated(1.0),
    DivisionMode::BestEqualNear(1.0),
];

/// The name of a division mode's kind in the control.
fn division_label(mode: DivisionMode) -> String {
    match mode {
        DivisionMode::Half => tr!("tool-division-half"),
        DivisionMode::Equal(_) => tr!("tool-division-equal"),
        DivisionMode::Percent(_) => tr!("tool-division-percent"),
        DivisionMode::Repeated(_) => tr!("tool-division-repeated"),
        DivisionMode::BestEqualNear(_) => tr!("tool-division-best-equal"),
    }
}

/// The 'Division' control of the drawing tools (INP-010): the one active
/// division mode, its value and the scope, kept in the preferences. The
/// plan views read them each frame, so a change replaces the hovered
/// edge's ticks on the next frame; the preferences file is written once a
/// drag or an edit of the value ends, not on every frame of it.
fn division(ui: &mut Ui, ws: &mut Workspace) {
    let before = ws.prefs.input;
    let mut input = before;
    ui.label(tr!("tool-division")).on_hover_text(tr!("tool-division-hint"));
    // The current mode stands for its kind; the two distance modes share
    // their distance.
    let distance = match input.division {
        DivisionMode::Repeated(d) | DivisionMode::BestEqualNear(d) => Some(d),
        _ => None,
    };
    let modes: Vec<(DivisionMode, String)> = DIVISION_MODES
        .iter()
        .map(|&mode| {
            let mode = match (mode, distance) {
                _ if std::mem::discriminant(&mode) == std::mem::discriminant(&input.division) => input.division,
                (DivisionMode::Repeated(_), Some(d)) => DivisionMode::Repeated(d),
                (DivisionMode::BestEqualNear(_), Some(d)) => DivisionMode::BestEqualNear(d),
                _ => mode,
            };
            (mode, division_label(mode))
        })
        .collect();
    if let Some(picked) = widgets::choice(ui, Id::new("division-mode"), Some(&input.division), &modes, 150.0) {
        input.division = picked;
    }
    let nf = ws.prefs.number_format();
    // The number field being dragged or typed into, and whether that ended
    // this frame.
    let mut value = None;
    match &mut input.division {
        DivisionMode::Half => {}
        DivisionMode::Equal(n) => {
            value = Some(widgets::integer(ui, n, EQUAL_PARTS, &nf));
        }
        DivisionMode::Percent(p) => {
            value = Some(widgets::decimal(ui, p, 0.0..=100.0, 1, 0.5, &nf));
            ui.label(tr!("tool-division-percent-suffix"));
        }
        DivisionMode::Repeated(d) | DivisionMode::BestEqualNear(d) => {
            let units = &ws.doc.project.settings.units;
            // As DivisionMode::sanitized, a distance within the model
            // tolerance is no distance.
            match widgets::length(ui, Id::new("division-distance"), Some(*d), units, &nf, 70.0) {
                Edited::Changed(v) if v > MODEL_TOLERANCE && v.is_finite() => *d = v,
                Edited::Changed(_) => ws.error(tr!("tool-division-distance-positive")),
                Edited::Invalid(e) => ws.error(e),
                Edited::Unchanged => {}
            }
        }
    }
    let scopes = [
        (DivisionScope::BetweenNodes, tr!("tool-division-scope-nodes")),
        (
            DivisionScope::BetweenIntersections,
            tr!("tool-division-scope-intersections"),
        ),
        (DivisionScope::WholeElement, tr!("tool-division-scope-element")),
    ];
    if let Some(scope) = widgets::choice(ui, Id::new("division-scope"), Some(&input.scope), &scopes, 150.0) {
        input.scope = scope;
    }
    let input = input.sanitized();
    let editing = value.as_ref().is_some_and(|r| r.dragged() || r.has_focus());
    let ended = value.as_ref().is_some_and(|r| r.drag_stopped() || r.lost_focus());
    if input != before {
        ws.prefs.input = input;
    }
    if (input != before && !editing) || ended {
        ws.requests.push(Request::SavePreferences);
    }
}

/// The full settings dialog of the tool or selection (UI-003).
pub fn settings_dialog(ui: &mut Ui, ws: &mut Workspace) {
    let Some(mut editing) = gather(ws) else {
        ui.label("Choose a tool or select elements to see their settings.");
        return;
    };
    ui.heading(summary(ws, &editing));
    if editing.targets.is_empty() {
        ui.label("The selected elements are locked.");
        return;
    }
    if let Some(e) = edit_fields(ui, ws, &mut editing) {
        ui.colored_label(ui.visuals().error_fg_color, e);
    }
}
