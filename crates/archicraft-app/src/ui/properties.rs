//! The properties panel and the settings dialog (UI-002, UI-003, ELM-001,
//! EDT-017).
//!
//! Both edit either the active tool's defaults (nothing selected) or the
//! selected elements. With several elements selected, fields whose values
//! differ show "Mixed", and a committed value goes to every editable selected
//! element. Locked elements are counted but not changed.

use archicraft_model::command::relink_to_storey;
use archicraft_model::glam::DVec2;
use archicraft_model::project::{SlabDefaults, WallDefaults};
use archicraft_model::units::{NumberFormat, UnitSettings, format_area, format_length, format_volume};
use archicraft_model::validate::signed_area;
use archicraft_model::{
    Command, Element, ElementKind, IfcClass, KindTag, LayerId, MaterialId, Position, Project, ReferenceLine,
    SlabReference, StoreyId, TopLink,
};
use egui::{Id, Ui};

use super::widgets::{self, Edited, common};
use crate::tools::{SlabMode, ToolKind, WallMode};
use crate::workspace::{Request, Workspace};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// One wrapped row, as in the properties strip.
    Strip,
    /// Label and value per row, as in the settings dialog.
    Grid,
}

/// What is being edited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Subject {
    Defaults(KindTag),
    Selection,
}

struct Editing {
    subject: Subject,
    /// Working copies: the editable selected elements, or a prototype built
    /// from the tool defaults.
    targets: Vec<Element>,
    selected: usize,
}

/// An element built from a tool's defaults, for editing them; `None` for a
/// kind without tool defaults.
fn prototype(project: &Project, kind: KindTag) -> Option<Element> {
    let storey = project.storeys.ground();
    match kind {
        KindTag::Wall => Some(project.new_wall(storey, DVec2::ZERO, DVec2::X, 0)),
        KindTag::Slab => {
            Some(project.new_slab(storey, vec![DVec2::ZERO, DVec2::X, DVec2::ONE, DVec2::Y], Vec::new(), 0))
        }
        _ => None,
    }
}

fn gather(ws: &Workspace) -> Option<Editing> {
    let project = &ws.doc.project;
    if ws.selection.is_empty() {
        let kind = match ws.tool.kind {
            ToolKind::Select => return None,
            ToolKind::Wall => KindTag::Wall,
            ToolKind::Slab => KindTag::Slab,
        };
        return Some(Editing {
            subject: Subject::Defaults(kind),
            targets: vec![prototype(project, kind)?],
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

fn wall_defaults(e: &Element, old: &WallDefaults) -> WallDefaults {
    match e.as_wall() {
        Some(w) => WallDefaults {
            thickness: w.thickness,
            reference_line: w.reference_line,
            base_offset: w.base_offset,
            top: w.top,
            material: w.material,
            layer: e.layer,
            position: e.position,
            ifc_class: e.ifc_class,
        },
        None => old.clone(),
    }
}

fn slab_defaults(e: &Element, old: &SlabDefaults) -> SlabDefaults {
    match e.as_slab() {
        Some(s) => SlabDefaults {
            thickness: s.thickness,
            reference: s.reference,
            offset: s.offset,
            material: s.material,
            layer: e.layer,
            position: e.position,
            ifc_class: e.ifc_class,
        },
        None => old.clone(),
    }
}

fn apply(ws: &mut Workspace, editing: Editing) {
    match editing.subject {
        Subject::Selection => {
            ws.execute(Command::UpdateElements(editing.targets));
        }
        Subject::Defaults(kind) => {
            let Some(e) = editing.targets.first() else {
                return;
            };
            let mut defaults = ws.doc.project.tool_defaults.clone();
            match kind {
                KindTag::Wall => defaults.wall = wall_defaults(e, &defaults.wall),
                KindTag::Slab => defaults.slab = slab_defaults(e, &defaults.slab),
                // A kind without a tool has no defaults to edit.
                _ => return,
            }
            ws.execute(Command::SetToolDefaults(defaults));
        }
    }
}

/// Field rendering context.
struct Fields<'a> {
    layout: Layout,
    id: Id,
    units: UnitSettings,
    nf: NumberFormat,
    project: &'a Project,
    targets: &'a mut Vec<Element>,
    changed: bool,
    error: Option<String>,
}

impl Fields<'_> {
    fn width(&self, strip: f32) -> f32 {
        match self.layout {
            Layout::Strip => strip,
            Layout::Grid => 180.0,
        }
    }

    fn row(&mut self, ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui, &mut Self)) {
        match self.layout {
            Layout::Strip => {
                ui.label(label);
                add(ui, self);
                ui.add_space(8.0);
            }
            Layout::Grid => {
                ui.label(label);
                add(ui, self);
                ui.end_row();
            }
        }
    }

    fn length(
        &mut self,
        ui: &mut Ui,
        label: &str,
        key: &str,
        get: impl Fn(&Element) -> Option<f64>,
        set: impl Fn(&mut Element, f64),
    ) {
        if !self.targets.iter().any(|e| get(e).is_some()) {
            return;
        }
        self.row(ui, label, |ui, f| {
            let value = common(f.targets.iter().filter_map(&get));
            let width = f.width(64.0);
            match widgets::length(ui, f.id.with(key), value, &f.units, &f.nf, width) {
                Edited::Changed(v) => {
                    for e in f.targets.iter_mut().filter(|e| get(e).is_some()) {
                        set(e, v);
                    }
                    f.changed = true;
                }
                Edited::Invalid(e) => f.error = Some(e),
                Edited::Unchanged => {}
            }
        });
    }

    fn choice<T: Clone + PartialEq>(
        &mut self,
        ui: &mut Ui,
        label: &str,
        key: &str,
        options: &[(T, String)],
        get: impl Fn(&Element) -> Option<T>,
        set: impl Fn(&mut Element, &T),
    ) {
        if !self.targets.iter().any(|e| get(e).is_some()) {
            return;
        }
        self.row(ui, label, |ui, f| {
            let value = common(f.targets.iter().filter_map(&get));
            let width = f.width(110.0);
            if let Some(picked) = widgets::choice(ui, f.id.with(key), value.as_ref(), options, width) {
                for e in f.targets.iter_mut().filter(|e| get(e).is_some()) {
                    set(e, &picked);
                }
                f.changed = true;
            }
        });
    }
}

fn wall(e: &Element) -> Option<&archicraft_model::Wall> {
    e.as_wall()
}

fn wall_mut(e: &mut Element) -> Option<&mut archicraft_model::Wall> {
    e.as_wall_mut()
}

fn slab_mut(e: &mut Element) -> Option<&mut archicraft_model::Slab> {
    e.as_slab_mut()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TopMode {
    Fixed,
    Storey,
    Relative,
}

fn top_mode(top: TopLink) -> TopMode {
    match top {
        TopLink::Fixed { .. } => TopMode::Fixed,
        TopLink::Storey { .. } => TopMode::Storey,
        TopLink::Relative { .. } => TopMode::Relative,
    }
}

fn geometry_fields(ui: &mut Ui, f: &mut Fields<'_>) {
    f.length(
        ui,
        "Thickness",
        "thickness",
        |e| match &e.kind {
            ElementKind::Wall(w) => Some(w.thickness),
            ElementKind::Slab(s) => Some(s.thickness),
            // Other kinds show the common fields only.
            _ => None,
        },
        |e, v| match &mut e.kind {
            ElementKind::Wall(w) => w.thickness = v,
            ElementKind::Slab(s) => s.thickness = v,
            _ => {}
        },
    );
    let reference_options: Vec<(u8, String)> = [
        ReferenceLine::Outside,
        ReferenceLine::Center,
        ReferenceLine::Inside,
        ReferenceLine::Custom(0.0),
    ]
    .iter()
    .enumerate()
    .map(|(i, r)| (i as u8, r.label().to_owned()))
    .collect();
    let reference_index = |r: ReferenceLine| match r {
        ReferenceLine::Outside => 0u8,
        ReferenceLine::Center => 1,
        ReferenceLine::Inside => 2,
        ReferenceLine::Custom(_) => 3,
    };
    f.choice(
        ui,
        "Reference line",
        "refline",
        &reference_options,
        |e| wall(e).map(|w| reference_index(w.reference_line)),
        |e, i| {
            if let Some(w) = wall_mut(e) {
                w.reference_line = match i {
                    0 => ReferenceLine::Outside,
                    1 => ReferenceLine::Center,
                    2 => ReferenceLine::Inside,
                    _ => ReferenceLine::Custom(w.thickness / 3.0),
                };
            }
        },
    );
    f.length(
        ui,
        "Offset from exterior face",
        "refoffset",
        |e| match wall(e)?.reference_line {
            ReferenceLine::Custom(o) => Some(o),
            _ => None,
        },
        |e, v| {
            if let Some(w) = wall_mut(e) {
                w.reference_line = ReferenceLine::Custom(v);
            }
        },
    );
    f.length(
        ui,
        "Base offset",
        "base",
        |e| wall(e).map(|w| w.base_offset),
        |e, v| {
            if let Some(w) = wall_mut(e) {
                w.base_offset = v;
            }
        },
    );
    let modes = vec![
        (TopMode::Fixed, "Fixed height".to_owned()),
        (TopMode::Storey, "Linked to storey".to_owned()),
        (TopMode::Relative, "Home storey + n".to_owned()),
    ];
    let project = f.project;
    f.choice(
        ui,
        "Top",
        "topmode",
        &modes,
        |e| wall(e).map(|w| top_mode(w.top)),
        |e, mode| {
            let extent = project.vertical_extent(e);
            let home = e.storey;
            if let Some(w) = wall_mut(e) {
                let (bottom, top) = extent.unwrap_or((0.0, 3.0));
                w.top = match mode {
                    TopMode::Fixed => TopLink::Fixed { height: top - bottom },
                    TopMode::Storey => {
                        let s = project
                            .storeys
                            .relative(home, 1)
                            .unwrap_or_else(|| project.storeys.storey_at_elevation(top));
                        TopLink::Storey {
                            storey: s.id,
                            offset: top - s.elevation,
                        }
                    }
                    TopMode::Relative => TopLink::Relative {
                        storeys: 1,
                        offset: 0.0,
                    },
                };
            }
        },
    );
    f.length(
        ui,
        "Height",
        "height",
        |e| match wall(e)?.top {
            TopLink::Fixed { height } => Some(height),
            _ => None,
        },
        |e, v| {
            if let Some(w) = wall_mut(e) {
                w.top = TopLink::Fixed { height: v };
            }
        },
    );
    let storeys: Vec<(StoreyId, String)> = project
        .storeys
        .highest_first()
        .map(|s| (s.id, s.name.clone()))
        .collect();
    f.choice(
        ui,
        "Top storey",
        "topstorey",
        &storeys,
        |e| match wall(e)?.top {
            TopLink::Storey { storey, .. } => Some(storey),
            _ => None,
        },
        |e, s| {
            if let Some(w) = wall_mut(e)
                && let TopLink::Storey { offset, .. } = w.top
            {
                w.top = TopLink::Storey { storey: *s, offset };
            }
        },
    );
    let relative: Vec<(i32, String)> = [-2, -1, 0, 1, 2, 3]
        .iter()
        .map(|n| (*n, format!("Home {n:+}")))
        .collect();
    f.choice(
        ui,
        "Linked storey",
        "toprel",
        &relative,
        |e| match wall(e)?.top {
            TopLink::Relative { storeys, .. } => Some(storeys),
            _ => None,
        },
        |e, n| {
            if let Some(w) = wall_mut(e)
                && let TopLink::Relative { offset, .. } = w.top
            {
                w.top = TopLink::Relative { storeys: *n, offset };
            }
        },
    );
    f.length(
        ui,
        "Top offset",
        "topoffset",
        |e| match wall(e)?.top {
            TopLink::Storey { offset, .. } | TopLink::Relative { offset, .. } => Some(offset),
            TopLink::Fixed { .. } => None,
        },
        |e, v| {
            if let Some(w) = wall_mut(e) {
                w.top = match w.top {
                    TopLink::Storey { storey, .. } => TopLink::Storey { storey, offset: v },
                    TopLink::Relative { storeys, .. } => TopLink::Relative { storeys, offset: v },
                    fixed => fixed,
                };
            }
        },
    );
    let references = vec![
        (SlabReference::Top, "Top face".to_owned()),
        (SlabReference::Bottom, "Bottom face".to_owned()),
    ];
    f.choice(
        ui,
        "Reference plane",
        "slabref",
        &references,
        |e| e.as_slab().map(|s| s.reference),
        |e, r| {
            if let Some(s) = slab_mut(e) {
                s.reference = *r;
            }
        },
    );
    f.length(
        ui,
        "Elevation offset",
        "slaboffset",
        |e| e.as_slab().map(|s| s.offset),
        |e, v| {
            if let Some(s) = slab_mut(e) {
                s.offset = v;
            }
        },
    );
}

fn appearance_fields(ui: &mut Ui, f: &mut Fields<'_>) {
    let materials: Vec<(MaterialId, String)> = f.project.materials.iter().map(|m| (m.id, m.name.clone())).collect();
    f.choice(
        ui,
        "Material",
        "material",
        &materials,
        |e| Some(e.kind.material()),
        |e, m| e.kind.set_material(*m),
    );
}

fn classification_fields(ui: &mut Ui, f: &mut Fields<'_>) {
    let layers: Vec<(LayerId, String)> = f.project.layers.iter().map(|l| (l.id, l.name.clone())).collect();
    f.choice(ui, "Layer", "layer", &layers, |e| Some(e.layer), |e, l| e.layer = *l);
    let kinds = common(f.targets.iter().map(Element::tag));
    if let Some(kind) = kinds {
        let classes: Vec<(IfcClass, String)> = kind.ifc_classes().iter().map(|c| (*c, c.code().to_owned())).collect();
        f.choice(
            ui,
            "IFC class",
            "class",
            &classes,
            |e| Some(e.ifc_class),
            |e, c| e.ifc_class = *c,
        );
    }
    let positions = vec![
        (Position::Undefined, "Undefined".to_owned()),
        (Position::Exterior, "Exterior".to_owned()),
        (Position::Interior, "Interior".to_owned()),
    ];
    f.choice(
        ui,
        "Position",
        "position",
        &positions,
        |e| Some(e.position),
        |e, p| e.position = *p,
    );
}

fn summary(ws: &Workspace, editing: &Editing) -> String {
    match editing.subject {
        Subject::Defaults(kind) => format!("{} tool defaults", crate::tr!(kind.label_key())),
        Subject::Selection => {
            let project = &ws.doc.project;
            let kinds = common(
                ws.selection
                    .iter()
                    .filter_map(|id| project.element(*id))
                    .map(Element::tag),
            );
            let what = kinds.map_or("elements".to_owned(), |k| {
                if editing.selected == 1 {
                    crate::tr!(k.label_key())
                } else {
                    crate::tr!(k.noun_key(), count = editing.selected)
                }
            });
            if editing.selected == 1 {
                what
            } else if editing.targets.len() == editing.selected {
                format!("{} {what} selected", editing.selected)
            } else {
                format!(
                    "{} {what} selected, {} editable",
                    editing.selected,
                    editing.targets.len()
                )
            }
        }
    }
}

/// Read-only values derived from the geometry (MEA-003).
fn derived(ui: &mut Ui, ws: &mut Workspace, layout: Layout) {
    let ids = ws.selected_ids();
    let [id] = ids.as_slice() else {
        return;
    };
    let geometry = ws.doc.geometry();
    let project = &ws.doc.project;
    let Some(element) = project.element(*id) else {
        return;
    };
    let units = &project.settings.units;
    let nf = ws.prefs.number_format();
    let volume = geometry.get(*id).map(|g| g.volume());
    let mut rows: Vec<(&str, String)> = Vec::new();
    match &element.kind {
        ElementKind::Wall(w) => rows.push(("Length", format_length(w.length(), units, &nf, true))),
        ElementKind::Slab(s) => {
            let area = signed_area(&s.outline).abs() - s.holes.iter().map(|h| signed_area(h).abs()).sum::<f64>();
            rows.push(("Area", format_area(area, units, &nf)));
        }
        _ => {}
    }
    if let Some(v) = volume {
        rows.push(("Volume", format_volume(v, units, &nf)));
    }
    for (label, value) in rows {
        ui.label(label);
        ui.label(egui::RichText::new(value).weak());
        if layout == Layout::Grid {
            ui.end_row();
        } else {
            ui.add_space(8.0);
        }
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
        label_row(ui, "Name");
        let width = if layout == Layout::Strip { 70.0 } else { 180.0 };
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
    label_row(ui, "Home storey");
    let current = common(editing.targets.iter().map(|e| e.storey));
    let width = if layout == Layout::Strip { 110.0 } else { 180.0 };
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
        .checkbox(&mut locked, "Locked")
        .on_hover_text("Locked elements reject moves, deletion and parameter changes (ELM-013)")
        .changed()
    {
        let changed: Vec<Element> = selected
            .iter()
            .filter(|e| e.locked != locked)
            .map(|e| Element { locked, ..(*e).clone() })
            .collect();
        ws.execute(Command::UpdateElements(changed));
    }
}

/// The properties strip below the menu bar. It keeps a height of two rows,
/// so that the views do not jump when the tool or selection changes.
pub fn strip(ui: &mut Ui, ws: &mut Workspace) {
    ui.set_min_height(2.0 * ui.spacing().interact_size.y + ui.spacing().item_spacing.y);
    ui.horizontal_wrapped(|ui| {
        tool_modes(ui, ws);
        let Some(mut editing) = gather(ws) else {
            ui.label(egui::RichText::new("Nothing selected. Choose a tool, or select elements to edit them.").weak());
            return;
        };
        ui.strong(summary(ws, &editing));
        ui.separator();
        if editing.targets.is_empty() {
            ui.label("The selected elements are locked.");
            lock_toggle(ui, ws);
            return;
        }
        let error = edit_fields(ui, ws, &mut editing, Layout::Strip);
        if let Some(e) = error {
            ws.error(e);
        }
        if editing.subject == Subject::Selection {
            lock_toggle(ui, ws);
            derived(ui, ws, Layout::Strip);
        }
        if ui
            .button("Settings…")
            .on_hover_text("All settings (Ctrl/Cmd+T)")
            .clicked()
        {
            ws.requests.push(Request::Settings);
        }
    });
}

fn edit_fields(ui: &mut Ui, ws: &mut Workspace, editing: &mut Editing, layout: Layout) -> Option<String> {
    let mut error = match layout {
        Layout::Strip => selection_only_fields(ui, ws, editing, layout),
        Layout::Grid => {
            egui::Grid::new("settings-selection")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| selection_only_fields(ui, ws, editing, layout))
                .inner
        }
    };
    let mut targets = std::mem::take(&mut editing.targets);
    let mut f = Fields {
        layout,
        id: Id::new("properties").with(layout as u8),
        units: ws.doc.project.settings.units.clone(),
        nf: ws.prefs.number_format(),
        project: &ws.doc.project,
        targets: &mut targets,
        changed: false,
        error: None,
    };
    match layout {
        Layout::Strip => {
            geometry_fields(ui, &mut f);
            appearance_fields(ui, &mut f);
            classification_fields(ui, &mut f);
        }
        Layout::Grid => {
            section(ui, "Geometry and positioning", |ui| geometry_fields(ui, &mut f));
            section(ui, "Model and appearance", |ui| appearance_fields(ui, &mut f));
            section(ui, "Plan display", |ui| {
                ui.label("Cut fills come from the material.");
                ui.end_row();
            });
            section(ui, "Classification", |ui| classification_fields(ui, &mut f));
        }
    }
    let changed = f.changed;
    error = error.or(f.error.take());
    editing.targets = targets;
    if changed {
        apply(
            ws,
            Editing {
                subject: editing.subject,
                targets: editing.targets.clone(),
                selected: editing.selected,
            },
        );
    }
    error
}

fn section(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    egui::CollapsingHeader::new(title).default_open(true).show(ui, |ui| {
        egui::Grid::new(title).num_columns(2).spacing([12.0, 6.0]).show(ui, add);
    });
}

/// Geometry methods of the active tool, shown at the start of the strip.
fn tool_modes(ui: &mut Ui, ws: &mut Workspace) {
    match ws.tool.kind {
        ToolKind::Wall => {
            for mode in WallMode::ALL {
                if ui.selectable_label(ws.tool.wall_mode == mode, mode.label()).clicked() {
                    ws.tool.wall_mode = mode;
                    ws.reset_input();
                }
            }
            ui.separator();
        }
        ToolKind::Slab => {
            for mode in SlabMode::ALL {
                if ui.selectable_label(ws.tool.slab_mode == mode, mode.label()).clicked() {
                    ws.tool.slab_mode = mode;
                    ws.reset_input();
                }
            }
            ui.separator();
        }
        ToolKind::Select => {}
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
    egui::Grid::new("settings-common")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            if editing.subject == Subject::Selection {
                derived(ui, ws, Layout::Grid);
            }
        });
    if let Some(e) = edit_fields(ui, ws, &mut editing, Layout::Grid) {
        ui.colored_label(ui.visuals().error_fg_color, e);
    }
}
