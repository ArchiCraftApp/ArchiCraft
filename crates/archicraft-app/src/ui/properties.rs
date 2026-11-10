//! The properties panel and the settings dialog (UI-002, UI-003, ELM-001,
//! EDT-017).
//!
//! Both edit either the active tool's defaults (nothing selected) or the
//! selected elements. With several elements selected, fields whose values
//! differ show "Mixed", and a committed value goes to every editable selected
//! element. Locked elements are counted but not changed.

use archicraft_model::command::relink_to_storey;
use archicraft_model::project::DefaultsSlot;
use archicraft_model::units::{NumberFormat, UnitSettings, format_area, format_length, format_volume};
use archicraft_model::{
    AnchorPos, Command, Element, ElementKind, GroupId, IfcClass, KindTag, LayerId, MaterialId, Position,
    PredefinedType, Project, ReferenceLine, SlabReference, StoreyId, TextStyleId, TopLink,
};
use egui::{Id, Ui};

use super::command_widgets::command_button;
use super::widgets::{self, Edited, common};
use crate::commands::{self, CommandCtx, CommandId, Origin, Registry, Target};
use crate::input::{DivisionMode, DivisionScope, EQUAL_PARTS, PointInput};
use crate::tools::{MeasureMode, SlabMode, SpaceMode, ToolKind, WallMode};
use crate::tr;
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

fn gather(ws: &Workspace) -> Option<Editing> {
    let project = &ws.doc.project;
    if ws.selection.is_empty() {
        let kind = match ws.tool.kind {
            ToolKind::Select | ToolKind::Measure => return None,
            ToolKind::Wall => KindTag::Wall,
            ToolKind::Slab => KindTag::Slab,
            ToolKind::Space => KindTag::Space,
        };
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
            defaults.insert(DefaultsSlot {
                layer: e.layer,
                position: e.position,
                ifc_class: e.ifc_class,
                predefined_type: e.predefined_type,
                kind: e.kind,
            });
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
        self.row_of(ui, label, |ui, f| {
            add(ui, f);
            None
        });
    }

    /// A row whose label names the widget whose id `add` returns, for
    /// screen readers.
    fn row_of(&mut self, ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui, &mut Self) -> Option<Id>) {
        let label = ui.label(label);
        if let Some(field) = add(ui, self) {
            ui.ctx().accesskit_node_builder(field, |node| {
                node.push_labelled_by(label.id.accesskit_id());
            });
        }
        match self.layout {
            Layout::Strip => ui.add_space(8.0),
            Layout::Grid => ui.end_row(),
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
        self.row_of(ui, label, |ui, f| {
            let value = common(f.targets.iter().filter_map(&get));
            let width = f.width(110.0);
            let (response, picked) = widgets::choice_response(ui, f.id.with(key), value.as_ref(), options, width);
            if let Some(picked) = picked {
                for e in f.targets.iter_mut().filter(|e| get(e).is_some()) {
                    set(e, &picked);
                }
                f.changed = true;
            }
            Some(response.id)
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
    .map(|(i, r)| (i as u8, tr!(r.label_key())))
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

fn text_mut(e: &mut Element) -> Option<&mut archicraft_model::Text> {
    e.as_text_mut()
}

/// The name of an anchor position (ANN-007).
fn anchor_pos_label(pos: AnchorPos) -> String {
    match pos {
        AnchorPos::TopLeft => tr!("dlg-properties-anchor-top-left"),
        AnchorPos::TopCenter => tr!("dlg-properties-anchor-top-center"),
        AnchorPos::TopRight => tr!("dlg-properties-anchor-top-right"),
        AnchorPos::MiddleLeft => tr!("dlg-properties-anchor-middle-left"),
        AnchorPos::Center => tr!("dlg-properties-anchor-center"),
        AnchorPos::MiddleRight => tr!("dlg-properties-anchor-middle-right"),
        AnchorPos::BottomLeft => tr!("dlg-properties-anchor-bottom-left"),
        AnchorPos::BottomCenter => tr!("dlg-properties-anchor-bottom-center"),
        AnchorPos::BottomRight => tr!("dlg-properties-anchor-bottom-right"),
    }
}

/// The fields of texts (ANN-007): the content (several lines), the style,
/// the size in millimetres (empty for the style's size), the anchor
/// position and the angle. Each committed change is one step (ELM-001).
fn text_fields(ui: &mut Ui, f: &mut Fields<'_>) {
    if !f.targets.iter().any(|e| e.as_text().is_some()) {
        return;
    }
    let texts = |targets: &[Element]| -> Vec<archicraft_model::Text> {
        targets.iter().filter_map(Element::as_text).cloned().collect()
    };
    let field = f.id.with("text-content");
    f.row_of(ui, "Content", |ui, f| {
        let value = common(texts(f.targets).into_iter().map(|t| t.content));
        match widgets::multiline_text(ui, field, value.as_deref(), f.width(140.0)).1 {
            Edited::Changed(content) => {
                for t in f.targets.iter_mut().filter_map(text_mut) {
                    t.content.clone_from(&content);
                }
                f.changed = true;
            }
            Edited::Invalid(e) => f.error = Some(e),
            Edited::Unchanged => {}
        }
        Some(field)
    });
    let project = f.project;
    let styles: Vec<(TextStyleId, String)> = project.text_styles.iter().map(|s| (s.id, s.name.clone())).collect();
    f.choice(
        ui,
        "Text style",
        "text-style",
        &styles,
        |e| e.as_text().map(|t| t.style),
        |e, style| {
            if let Some(t) = text_mut(e) {
                t.style = *style;
            }
        },
    );
    let field = f.id.with("text-size");
    f.row_of(ui, "Size (mm)", |ui, f| {
        let targets = texts(f.targets);
        let value = common(targets.iter().map(|t| t.size_mm));
        let style_size = common(
            targets
                .iter()
                .filter_map(|t| project.text_style(t.style))
                .map(|s| s.size_mm.to_bits()),
        );
        let hint = style_size.map_or_else(String::new, |bits| {
            let size = widgets::format_trimmed(&f.nf, f64::from_bits(bits), 3);
            tr!("dlg-properties-text-style-size", size = size.as_str())
        });
        match widgets::optional_positive(ui, field, value, &hint, &f.nf, f.width(64.0)).1 {
            Edited::Changed(size) => {
                for t in f.targets.iter_mut().filter_map(text_mut) {
                    t.size_mm = size;
                }
                f.changed = true;
            }
            Edited::Invalid(e) => f.error = Some(e),
            Edited::Unchanged => {}
        }
        Some(field)
    });
    let positions: Vec<(AnchorPos, String)> = AnchorPos::ALL.iter().map(|p| (*p, anchor_pos_label(*p))).collect();
    f.choice(
        ui,
        "Anchor position",
        "text-anchor-pos",
        &positions,
        |e| e.as_text().map(|t| t.anchor_pos),
        |e, pos| {
            if let Some(t) = text_mut(e) {
                t.anchor_pos = *pos;
            }
        },
    );
    let field = f.id.with("text-angle");
    f.row_of(ui, "Angle", |ui, f| {
        let value = common(texts(f.targets).into_iter().map(|t| t.angle.to_bits())).map(f64::from_bits);
        match widgets::angle(ui, field, value, &f.units, &f.nf, f.width(64.0)) {
            Edited::Changed(angle) => {
                for t in f.targets.iter_mut().filter_map(text_mut) {
                    t.angle = angle;
                }
                f.changed = true;
            }
            Edited::Invalid(e) => f.error = Some(e),
            Edited::Unchanged => {}
        }
        Some(field)
    });
}

/// True if every target has a material: a kind without one, like a space
/// (SPC-001), has none to pick and no cut fill.
fn all_have_material(f: &Fields<'_>) -> bool {
    f.targets.iter().all(|e| e.kind.material().is_some())
}

/// The Material row, shown only when every selected element has a
/// material ([`all_have_material`]).
fn appearance_fields(ui: &mut Ui, f: &mut Fields<'_>) {
    if !all_have_material(f) {
        return;
    }
    let materials: Vec<(MaterialId, String)> = f.project.materials.iter().map(|m| (m.id, m.name.clone())).collect();
    f.choice(
        ui,
        "Material",
        "material",
        &materials,
        |e| e.kind.material(),
        |e, m| e.kind.set_material(*m),
    );
}

fn classification_fields(ui: &mut Ui, f: &mut Fields<'_>) {
    let layers: Vec<(LayerId, String)> = f.project.layers.iter().map(|l| (l.id, l.name.clone())).collect();
    f.choice(ui, "Layer", "layer", &layers, |e| Some(e.layer), |e, l| e.layer = *l);
    let kinds = common(f.targets.iter().map(Element::tag));
    if let Some(kind) = kinds {
        let classes: Vec<(IfcClass, String)> = kind.ifc_classes().iter().map(|c| (*c, tr!(c.label_key()))).collect();
        f.choice(
            ui,
            "IFC class",
            "class",
            &classes,
            |e| Some(e.ifc_class),
            |e, c| {
                e.ifc_class = *c;
                // IFX-024: a type the new class does not offer is dropped.
                if e.predefined_type
                    .is_some_and(|t| !e.tag().predefined_types(*c).contains(&t))
                {
                    e.predefined_type = None;
                }
            },
        );
        predefined_type_field(ui, f, kind);
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

/// The predefined type (IFX-024): none or one of the types the kind offers
/// with the class, shown when all targets have the same class and it has
/// types.
fn predefined_type_field(ui: &mut Ui, f: &mut Fields<'_>, kind: KindTag) {
    let Some(class) = common(f.targets.iter().map(|e| e.ifc_class)) else {
        return;
    };
    let offered = kind.predefined_types(class);
    if offered.is_empty() {
        return;
    }
    let mut types: Vec<(Option<PredefinedType>, String)> = vec![(None, "None".to_owned())];
    types.extend(offered.iter().map(|t| (Some(*t), tr!(t.label_key()))));
    f.choice(
        ui,
        "Predefined type",
        "predefined-type",
        &types,
        |e| Some(e.predefined_type),
        |e, t| e.predefined_type = *t,
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
            rows.push(("Area", format_area(s.outline.area(), units, &nf)));
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
        return;
    }
    let mut snaps = selected.iter().all(|e| !e.snap_excluded);
    if ui
        .checkbox(&mut snaps, "Snap targets")
        .on_hover_text("Cleared, the element stays visible but provides no snap targets (INP-017)")
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

/// The name and size of the selected group, such as "Group: Group 3 (5
/// elements)", and a field that renames it, one undo step (ELM-015).
fn group_row(ui: &mut Ui, ws: &mut Workspace) {
    let Some(group) = selected_group(ws) else {
        return;
    };
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
    ui.separator();
}

/// The properties strip below the menu bar. It keeps a height of two rows,
/// so that the views do not jump when the tool or selection changes.
pub fn strip(ui: &mut Ui, ws: &mut Workspace, cx: &CommandCtx) {
    ui.set_min_height(2.0 * ui.spacing().interact_size.y + ui.spacing().item_spacing.y);
    ui.horizontal_wrapped(|ui| {
        tool_modes(ui, ws, cx);
        let Some(mut editing) = gather(ws) else {
            ui.label(egui::RichText::new("Nothing selected. Choose a tool, or select elements to edit them.").weak());
            return;
        };
        ui.strong(summary(ws, &editing));
        ui.separator();
        group_row(ui, ws);
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
            text_fields(ui, &mut f);
            appearance_fields(ui, &mut f);
            classification_fields(ui, &mut f);
        }
        Layout::Grid => {
            section(ui, "Geometry and positioning", |ui| geometry_fields(ui, &mut f));
            if f.targets.iter().any(|e| e.as_text().is_some()) {
                section(ui, "Text", |ui| text_fields(ui, &mut f));
            }
            // Without a material both sections would be empty (SPC-001).
            if all_have_material(&f) {
                section(ui, "Model and appearance", |ui| appearance_fields(ui, &mut f));
                section(ui, "Plan display", |ui| {
                    ui.label("Cut fills come from the material.");
                    ui.end_row();
                });
            }
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

/// Geometry methods of the active tool, shown at the start of the strip:
/// the commands of the mode family, checked for the current mode (WAL-001,
/// SLB-001, SPC-001, UI-017).
fn tool_modes(ui: &mut Ui, ws: &mut Workspace, cx: &CommandCtx) {
    let modes: Vec<CommandId> = match ws.tool.kind {
        ToolKind::Wall => WallMode::ALL.map(commands::tools::wall_mode).to_vec(),
        ToolKind::Slab => SlabMode::ALL.map(commands::tools::slab_mode).to_vec(),
        ToolKind::Space => SpaceMode::ALL.map(commands::tools::space_mode).to_vec(),
        // MEA-001: a new mode starts a new measurement.
        ToolKind::Measure => {
            for mode in MeasureMode::ALL {
                if ui
                    .selectable_label(ws.tool.measure.mode == mode, mode.label())
                    .clicked()
                {
                    ws.tool.measure.set_mode(mode);
                    ws.input = PointInput::default();
                }
            }
            Vec::new()
        }
        ToolKind::Select => return,
    };
    for id in modes {
        if command_button(ui, cx, Registry::get().command(id)) {
            let target = Target::active(ws);
            ws.requests.push(Request::Invoke(id, target, Origin::Panel));
        }
    }
    ui.separator();
    division(ui, ws);
    ui.separator();
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
/// edge's ticks on the next frame.
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
    match &mut input.division {
        DivisionMode::Half => {}
        DivisionMode::Equal(n) => {
            widgets::integer(ui, n, EQUAL_PARTS, &nf);
        }
        DivisionMode::Percent(p) => {
            widgets::decimal(ui, p, 0.0..=100.0, 1, 0.5, &nf);
            ui.label(tr!("tool-division-percent-suffix"));
        }
        DivisionMode::Repeated(d) | DivisionMode::BestEqualNear(d) => {
            let units = &ws.doc.project.settings.units;
            match widgets::length(ui, Id::new("division-distance"), Some(*d), units, &nf, 70.0) {
                Edited::Changed(v) if v > 0.0 && v.is_finite() => *d = v,
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
    if input != before {
        ws.prefs.input = input;
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
