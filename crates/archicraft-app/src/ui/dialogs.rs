//! Dialogs: storeys, layers, materials, project settings, preferences, the
//! problems list, keyboard shortcuts and about.

use archicraft_model::command::StoreyElements;
use archicraft_model::material::{CutFill, FillPattern, Material, MaterialCategory, Rgb};
use archicraft_model::project::Location;
use archicraft_model::units::{NumberFormat, format_length};
use archicraft_model::{Command, Layer, LayerId, MaterialId, ProjectInfo, ProjectSettings, StoreyId};
use egui::{Context, Id, RichText, Ui};

use super::widgets::{self, Edited};
use crate::commands::Registry;
use crate::input::{ConstraintReference, InputMode};
use crate::persistence::prefs::{Preferences, SeparatorChoice, ThemeChoice};
use crate::tr;
use crate::workspace::{Request, Workspace};

/// Asking how to handle the elements of a storey being deleted (STR-004).
#[derive(Clone, Debug)]
pub struct DeleteStorey {
    pub storey: StoreyId,
    pub move_elements: bool,
    pub target: Option<StoreyId>,
}

/// Draft of the project settings, applied with OK.
#[derive(Clone, Debug)]
pub struct ProjectDraft {
    pub info: ProjectInfo,
    pub settings: ProjectSettings,
    pub site: String,
    pub building: String,
    pub location: Location,
}

#[derive(Debug, Default)]
pub struct Dialogs {
    pub storeys: bool,
    pub storey_shift_above: bool,
    pub selected_storey: Option<StoreyId>,
    pub delete_storey: Option<DeleteStorey>,
    pub layers: bool,
    pub layer_filter: String,
    pub delete_layer: Option<(LayerId, Option<LayerId>)>,
    pub materials: bool,
    pub material_filter: String,
    pub selected_material: Option<MaterialId>,
    pub delete_material: Option<(MaterialId, Option<MaterialId>)>,
    pub project: Option<ProjectDraft>,
    pub preferences: Option<Preferences>,
    pub settings: bool,
    pub shortcuts: bool,
    pub about: bool,
    pub problems: bool,
}

impl Dialogs {
    pub fn ask_delete_storey(&mut self, storey: StoreyId) {
        self.delete_storey = Some(DeleteStorey {
            storey,
            move_elements: true,
            target: None,
        });
    }

    pub fn open_project_settings(&mut self, ws: &Workspace) {
        let p = &ws.doc.project;
        self.project = Some(ProjectDraft {
            info: p.info.clone(),
            settings: p.settings.clone(),
            site: p.site.name.clone(),
            building: p.building.name.clone(),
            location: p.site.location,
        });
    }

    /// Shows the open dialogs. Returns preferences the user confirmed.
    pub fn show(&mut self, ctx: &Context, ws: &mut Workspace) -> Option<Preferences> {
        self.storeys_window(ctx, ws);
        self.delete_storey_window(ctx, ws);
        self.layers_window(ctx, ws);
        self.materials_window(ctx, ws);
        self.project_window(ctx, ws);
        self.settings_window(ctx, ws);
        self.shortcuts_window(ctx);
        self.about_window(ctx);
        self.preferences_window(ctx, &ws.prefs.number_format())
    }

    fn storeys_window(&mut self, ctx: &Context, ws: &mut Workspace) {
        let mut open = self.storeys;
        egui::Window::new("Storeys")
            .open(&mut open)
            .resizable(true)
            .default_width(620.0)
            .show(ctx, |ui| {
                let units = ws.doc.project.settings.units.clone();
                let nf = ws.prefs.number_format();
                ui.checkbox(&mut self.storey_shift_above, "Changes move the storeys above")
                    .on_hover_text(
                        "STR-002: changing a height or elevation moves all storeys above by the same amount",
                    );
                let rows: Vec<_> = ws
                    .doc
                    .project
                    .storeys
                    .highest_first()
                    .map(|s| (s.id, ws.doc.project.storeys.number(s.id).unwrap_or(0), s.clone()))
                    .collect();
                egui::Grid::new("storeys")
                    .striped(true)
                    .num_columns(6)
                    .spacing([10.0, 4.0])
                    .show(ui, |ui| {
                        for header in ["No.", "Name", "Elevation", "Height", "Cut plane", "Elements"] {
                            ui.strong(header);
                        }
                        ui.end_row();
                        for (id, number, storey) in rows {
                            let row_id = Id::new("storey-row").with(id);
                            if ui
                                .selectable_label(self.selected_storey == Some(id), format!("{number}"))
                                .clicked()
                            {
                                self.selected_storey = Some(id);
                            }
                            if let Edited::Changed(name) =
                                widgets::text(ui, row_id.with("name"), Some(&storey.name), 160.0)
                            {
                                ws.execute(Command::RenameStorey { storey: id, name });
                            }
                            match widgets::length(ui, row_id.with("elev"), Some(storey.elevation), &units, &nf, 80.0) {
                                Edited::Changed(elevation) => {
                                    ws.execute(Command::SetStoreyElevation {
                                        storey: id,
                                        elevation,
                                        shift_above: self.storey_shift_above,
                                    });
                                }
                                Edited::Invalid(e) => ws.error(e),
                                Edited::Unchanged => {}
                            }
                            match widgets::length(ui, row_id.with("height"), Some(storey.height), &units, &nf, 80.0) {
                                Edited::Changed(height) => {
                                    ws.execute(Command::SetStoreyHeight {
                                        storey: id,
                                        height,
                                        shift_above: self.storey_shift_above,
                                    });
                                }
                                Edited::Invalid(e) => ws.error(e),
                                Edited::Unchanged => {}
                            }
                            let cut = storey
                                .cut_plane_height
                                .unwrap_or(ws.doc.project.settings.cut_plane_height);
                            match widgets::length(ui, row_id.with("cut"), Some(cut), &units, &nf, 70.0) {
                                Edited::Changed(height) => {
                                    ws.execute(Command::SetStoreyCutPlane {
                                        storey: id,
                                        height: Some(height),
                                    });
                                }
                                Edited::Invalid(e) => ws.error(e),
                                Edited::Unchanged => {}
                            }
                            ui.label(ws.doc.project.elements_on(id).count().to_string());
                            ui.end_row();
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    let selected = self
                        .selected_storey
                        .filter(|s| ws.doc.project.storeys.get(*s).is_some());
                    for (label, above) in [("Insert above", true), ("Insert below", false)] {
                        if ui.add_enabled(selected.is_some(), egui::Button::new(label)).clicked()
                            && let Some(id) = selected
                        {
                            let storey = super::browser::new_storey(ws, id, above);
                            ws.execute(Command::InsertStorey {
                                reference: id,
                                above,
                                storey,
                            });
                        }
                    }
                    if ui
                        .add_enabled(selected.is_some(), egui::Button::new("Delete…"))
                        .clicked()
                        && let Some(id) = selected
                    {
                        self.ask_delete_storey(id);
                    }
                });
                ui.label(
                    RichText::new(format!(
                        "Elevations are relative to project zero. Default cut plane: {} above the storey.",
                        format_length(ws.doc.project.settings.cut_plane_height, &units, &nf, true)
                    ))
                    .weak(),
                );
            });
        self.storeys = open;
    }

    fn delete_storey_window(&mut self, ctx: &Context, ws: &mut Workspace) {
        let Some(mut prompt) = self.delete_storey.take() else {
            return;
        };
        let project = &ws.doc.project;
        let Some(storey) = project.storeys.get(prompt.storey).cloned() else {
            return;
        };
        let count = project.elements_on(prompt.storey).count();
        let others: Vec<(StoreyId, String)> = project
            .storeys
            .highest_first()
            .filter(|s| s.id != prompt.storey)
            .map(|s| (s.id, s.name.clone()))
            .collect();
        if prompt.target.is_none() {
            prompt.target = project
                .storeys
                .relative(prompt.storey, -1)
                .or_else(|| project.storeys.relative(prompt.storey, 1))
                .map(|s| s.id);
        }
        let mut keep_open = true;
        let modal = egui::Modal::new(Id::new("delete-storey")).show(ctx, |ui| {
            ui.heading(format!("Delete storey “{}”?", storey.name));
            if count == 0 {
                ui.label("The storey has no elements.");
            } else {
                ui.label(format!("The storey contains {count} element(s)."));
                ui.radio_value(
                    &mut prompt.move_elements,
                    true,
                    "Move them to another storey, keeping their position:",
                );
                ui.add_enabled_ui(prompt.move_elements, |ui| {
                    if let Some(t) = widgets::choice(
                        ui,
                        Id::new("delete-storey-target"),
                        prompt.target.as_ref(),
                        &others,
                        200.0,
                    ) {
                        prompt.target = Some(t);
                    }
                });
                ui.radio_value(&mut prompt.move_elements, false, "Delete them");
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Delete storey").clicked() {
                    let elements = match (prompt.move_elements && count > 0, prompt.target) {
                        (true, Some(target)) => StoreyElements::MoveTo(target),
                        _ => StoreyElements::Delete,
                    };
                    ws.execute(Command::DeleteStorey {
                        storey: prompt.storey,
                        elements,
                    });
                    ws.prune_selection();
                    keep_open = false;
                }
                if ui.button("Cancel").clicked() {
                    keep_open = false;
                }
            });
        });
        if modal.should_close() {
            keep_open = false;
        }
        if keep_open {
            self.delete_storey = Some(prompt);
        }
    }

    fn layers_window(&mut self, ctx: &Context, ws: &mut Workspace) {
        let mut open = self.layers;
        egui::Window::new("Layers")
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Search");
                    ui.text_edit_singleline(&mut self.layer_filter);
                    if ui.button("New layer").clicked() {
                        ws.execute(Command::AddLayer(Layer::new("New layer")));
                    }
                });
                let filter = self.layer_filter.to_lowercase();
                let layers: Vec<Layer> = ws
                    .doc
                    .project
                    .layers
                    .iter()
                    .filter(|l| l.name.to_lowercase().contains(&filter))
                    .cloned()
                    .collect();
                egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                    egui::Grid::new("layers").striped(true).num_columns(6).show(ui, |ui| {
                        for header in ["Visible", "Locked", "Snaps", "Name", "Elements", ""] {
                            ui.strong(header);
                        }
                        ui.end_row();
                        for layer in layers {
                            let mut visible = layer.visible;
                            if ui.checkbox(&mut visible, "").changed() {
                                ws.execute(Command::UpdateLayer(Layer {
                                    visible,
                                    ..layer.clone()
                                }));
                                ws.prune_selection();
                            }
                            let mut locked = layer.locked;
                            if ui.checkbox(&mut locked, "").changed() {
                                ws.execute(Command::UpdateLayer(Layer {
                                    locked,
                                    ..layer.clone()
                                }));
                            }
                            // Cleared, the layer's elements stay visible but
                            // provide no snap targets (INP-017).
                            let mut snaps = !layer.snap_excluded;
                            if ui.checkbox(&mut snaps, "").changed() {
                                ws.execute(Command::UpdateLayer(Layer {
                                    snap_excluded: !snaps,
                                    ..layer.clone()
                                }));
                            }
                            if let Edited::Changed(name) =
                                widgets::text(ui, Id::new("layer").with(layer.id), Some(&layer.name), 180.0)
                            {
                                ws.execute(Command::UpdateLayer(Layer { name, ..layer.clone() }));
                            }
                            ui.label(ws.doc.project.element_count_on_layer(layer.id).to_string());
                            if ui.small_button("Delete…").clicked() {
                                self.delete_layer = Some((layer.id, None));
                            }
                            ui.end_row();
                        }
                    });
                });
                if let Some((id, target)) = self.delete_layer.as_mut() {
                    ui.separator();
                    let others: Vec<(LayerId, String)> = ws
                        .doc
                        .project
                        .layers
                        .iter()
                        .filter(|l| l.id != *id)
                        .map(|l| (l.id, l.name.clone()))
                        .collect();
                    if target.is_none() {
                        *target = others.first().map(|(l, _)| *l);
                    }
                    let name = ws.doc.project.layer(*id).map(|l| l.name.clone()).unwrap_or_default();
                    let mut done = false;
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!("Delete “{name}” and move its elements to"));
                        if let Some(t) = widgets::choice(ui, Id::new("layer-target"), target.as_ref(), &others, 160.0) {
                            *target = Some(t);
                        }
                        if ui.button("Delete").clicked() {
                            if let Some(t) = *target {
                                ws.execute(Command::DeleteLayer {
                                    layer: *id,
                                    reassign_to: t,
                                });
                            }
                            done = true;
                        }
                        if ui.button("Cancel").clicked() {
                            done = true;
                        }
                    });
                    if done {
                        self.delete_layer = None;
                    }
                }
            });
        self.layers = open;
    }

    fn materials_window(&mut self, ctx: &Context, ws: &mut Workspace) {
        let mut open = self.materials;
        egui::Window::new("Materials")
            .open(&mut open)
            .resizable(true)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Search");
                    ui.text_edit_singleline(&mut self.material_filter);
                });
                ui.columns(2, |cols| {
                    let filter = self.material_filter.to_lowercase();
                    egui::ScrollArea::vertical()
                        .id_salt("material-list")
                        .max_height(380.0)
                        .show(&mut cols[0], |ui| {
                            for m in ws.doc.project.materials.iter().filter(|m| {
                                m.name.to_lowercase().contains(&filter) || m.code.to_lowercase().contains(&filter)
                            }) {
                                ui.horizontal(|ui| {
                                    let [r, g, b] = m.color.to_u8();
                                    let (rect, _) =
                                        ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                                    ui.painter().rect_filled(rect, 2.0, egui::Color32::from_rgb(r, g, b));
                                    if ui
                                        .selectable_label(self.selected_material == Some(m.id), &m.name)
                                        .clicked()
                                    {
                                        self.selected_material = Some(m.id);
                                        self.delete_material = None;
                                    }
                                });
                            }
                        });
                    let ui = &mut cols[1];
                    let Some(material) = self
                        .selected_material
                        .and_then(|id| ws.doc.project.material(id))
                        .cloned()
                    else {
                        ui.label("Select a material to edit it.");
                        return;
                    };
                    material_editor(ui, ws, &material);
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("Duplicate").clicked() {
                            let copy = Material {
                                id: MaterialId::new(),
                                name: format!("{} copy", material.name),
                                ..material.clone()
                            };
                            self.selected_material = Some(copy.id);
                            ws.execute(Command::AddMaterial(copy));
                        }
                        if ui.button("Delete…").clicked() {
                            self.delete_material = Some((material.id, None));
                        }
                    });
                    if let Some((id, target)) = self.delete_material.as_mut() {
                        let others: Vec<(MaterialId, String)> = ws
                            .doc
                            .project
                            .materials
                            .iter()
                            .filter(|m| m.id != *id)
                            .map(|m| (m.id, m.name.clone()))
                            .collect();
                        if target.is_none() {
                            *target = others.first().map(|(m, _)| *m);
                        }
                        ui.label("Replace it in every element and tool with:");
                        if let Some(t) =
                            widgets::choice(ui, Id::new("material-target"), target.as_ref(), &others, 180.0)
                        {
                            *target = Some(t);
                        }
                        let mut done = false;
                        ui.horizontal(|ui| {
                            if ui.button("Delete").clicked() {
                                if let Some(t) = *target {
                                    ws.execute(Command::DeleteMaterial {
                                        material: *id,
                                        replacement: t,
                                    });
                                }
                                done = true;
                            }
                            if ui.button("Cancel").clicked() {
                                done = true;
                            }
                        });
                        if done {
                            self.delete_material = None;
                            self.selected_material = None;
                        }
                    }
                });
            });
        self.materials = open;
    }

    fn project_window(&mut self, ctx: &Context, ws: &mut Workspace) {
        let Some(mut draft) = self.project.take() else {
            return;
        };
        let mut open = true;
        let mut close = false;
        egui::Window::new("Project settings")
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                let units = draft.settings.units.clone();
                let nf = ws.prefs.number_format();
                egui::Grid::new("project-settings")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        let text_row = |ui: &mut Ui, label: &str, value: &mut String| {
                            ui.label(label);
                            ui.text_edit_singleline(value);
                            ui.end_row();
                        };
                        text_row(ui, "Project name", &mut draft.info.name);
                        text_row(ui, "Number", &mut draft.info.number);
                        text_row(ui, "Description", &mut draft.info.description);
                        text_row(ui, "Client", &mut draft.info.client);
                        text_row(ui, "Address", &mut draft.info.address);
                        text_row(ui, "Author", &mut draft.info.author);
                        text_row(ui, "Site", &mut draft.site);
                        text_row(ui, "Building", &mut draft.building);

                        widgets::unit_settings_editor(ui, &mut draft.settings.units, &nf);
                        ui.label("Plan cut plane");
                        if let Edited::Changed(v) = widgets::length(
                            ui,
                            Id::new("cut-plane"),
                            Some(draft.settings.cut_plane_height),
                            &units,
                            &nf,
                            100.0,
                        ) {
                            draft.settings.cut_plane_height = v;
                        }
                        ui.end_row();
                        ui.label("Altitude of project zero");
                        if let Edited::Changed(v) = widgets::length(
                            ui,
                            Id::new("altitude"),
                            Some(draft.location.altitude),
                            &units,
                            &nf,
                            100.0,
                        ) {
                            draft.location.altitude = v;
                        }
                        ui.end_row();
                        ui.label("Survey point (E, N)");
                        ui.horizontal(|ui| {
                            for (i, key) in ["east", "north"].iter().enumerate() {
                                if let Edited::Changed(v) = widgets::length(
                                    ui,
                                    Id::new("survey").with(key),
                                    Some(draft.location.survey_point[i]),
                                    &units,
                                    &nf,
                                    100.0,
                                ) {
                                    draft.location.survey_point[i] = v;
                                }
                            }
                        });
                        ui.end_row();
                        ui.label("Project north to true north");
                        if let Edited::Changed(v) = widgets::angle(
                            ui,
                            Id::new("north"),
                            Some(draft.location.true_north),
                            &units,
                            &nf,
                            100.0,
                        ) {
                            draft.location.true_north = v;
                        }
                        ui.end_row();
                        ui.label("Curve chord deviation");
                        if let Edited::Changed(v) = widgets::length(
                            ui,
                            Id::new("chord"),
                            Some(draft.settings.tessellation.chord_deviation),
                            &units,
                            &nf,
                            100.0,
                        ) {
                            draft.settings.tessellation.chord_deviation = v;
                        }
                        ui.end_row();
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() {
                        ws.execute(Command::SetInfo(draft.info.clone()));
                        ws.execute(Command::SetSettings(draft.settings.clone()));
                        ws.execute(Command::SetSpatialNames {
                            site: draft.site.clone(),
                            building: draft.building.clone(),
                        });
                        ws.execute(Command::SetLocation(draft.location));
                        close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        if open && !close {
            self.project = Some(draft);
        }
    }

    fn settings_window(&mut self, ctx: &Context, ws: &mut Workspace) {
        let mut open = self.settings;
        egui::Window::new("Settings")
            .open(&mut open)
            .resizable(true)
            .default_width(380.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| super::properties::settings_dialog(ui, ws));
            });
        self.settings = open;
    }

    fn shortcuts_window(&mut self, ctx: &Context) {
        egui::Window::new("Keyboard shortcuts")
            .open(&mut self.shortcuts)
            .resizable(true)
            .show(ctx, |ui| {
                egui::Grid::new("shortcuts").striped(true).show(ui, |ui| {
                    for spec in Registry::get().specs() {
                        if spec.default_bindings.is_empty() {
                            continue;
                        }
                        let keys: Vec<String> = spec
                            .default_bindings
                            .iter()
                            .map(|b| ctx.format_shortcut(&b.shortcut()))
                            .collect();
                        ui.label(spec.label());
                        ui.monospace(keys.join(", "));
                        ui.end_row();
                    }
                    for (label, keys) in [
                        ("Pan", "Middle mouse drag, or two-finger drag"),
                        ("Zoom at cursor", "Mouse wheel, or pinch"),
                        ("Orbit (3D)", "Shift + middle drag, Alt + drag, or orbit mode"),
                        ("Cycle reference line (wall tool)", "R"),
                        ("Constrain angle", "Hold Shift"),
                        ("Next typed field", "Tab"),
                        ("Remove last point", "Backspace"),
                    ] {
                        ui.label(label);
                        ui.monospace(keys);
                        ui.end_row();
                    }
                });
            });
    }

    /// The About window (NFR-SEC-001, NFR-SEC-002), with its text from the
    /// message catalogue (NFR-L10N-001). The fixed id keeps the window's
    /// place when the language changes its title.
    fn about_window(&mut self, ctx: &Context) {
        egui::Window::new(tr!("dlg-about-title"))
            .id(Id::new("about-window"))
            .open(&mut self.about)
            .resizable(false)
            .show(ctx, |ui| {
                ui.heading(tr!("dlg-about-heading", version = env!("CARGO_PKG_VERSION")));
                ui.label(tr!("dlg-about-summary"));
                ui.label(tr!("dlg-about-format", version = archicraft_ifcx::WRITTEN_VERSION));
                ui.label(tr!("dlg-about-privacy"));
            });
    }

    /// The Preferences window. Its numbers use `nf`, the number format of
    /// the applied preferences, until OK applies the draft (NFR-L10N-004).
    fn preferences_window(&mut self, ctx: &Context, nf: &NumberFormat) -> Option<Preferences> {
        let mut draft = self.preferences.take()?;
        let mut open = true;
        let mut result = None;
        let mut close = false;
        egui::Window::new("Preferences").open(&mut open).show(ctx, |ui| {
            egui::Grid::new("preferences")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Author (written to saved files)");
                    ui.text_edit_singleline(&mut draft.author);
                    ui.end_row();
                    ui.label("Autosave every (minutes)");
                    widgets::integer(ui, &mut draft.autosave_minutes, 1..=120, nf);
                    ui.end_row();
                    ui.label("Backup versions kept");
                    widgets::integer(ui, &mut draft.backups, 0..=20, nf);
                    ui.end_row();
                    ui.label("Undo steps (new projects)");
                    widgets::integer(ui, &mut draft.undo_steps, 100..=10_000, nf);
                    ui.end_row();
                    ui.label("Theme");
                    let themes = vec![
                        (ThemeChoice::System, "Follow system".to_owned()),
                        (ThemeChoice::Light, "Light".to_owned()),
                        (ThemeChoice::Dark, "Dark".to_owned()),
                    ];
                    if let Some(t) = widgets::choice(ui, Id::new("theme"), Some(&draft.theme), &themes, 140.0) {
                        draft.theme = t;
                    }
                    ui.end_row();
                    ui.label("Decimal separator");
                    let separators = vec![
                        (SeparatorChoice::Automatic, "From system".to_owned()),
                        (SeparatorChoice::Point, "Point (1.5)".to_owned()),
                        (SeparatorChoice::Comma, "Comma (1,5)".to_owned()),
                    ];
                    if let Some(s) = widgets::choice(
                        ui,
                        Id::new("separator"),
                        Some(&draft.decimal_separator),
                        &separators,
                        140.0,
                    ) {
                        draft.decimal_separator = s;
                    }
                    ui.end_row();
                    ui.label("Digit grouping");
                    ui.checkbox(&mut draft.digit_grouping, "");
                    ui.end_row();
                    ui.label("Show wall reference lines");
                    ui.checkbox(&mut draft.show_reference_lines, "");
                    ui.end_row();
                    // IFX-008: compact by default; autosaves stay compact.
                    ui.label("Saved files");
                    ui.checkbox(&mut draft.indented_ifcx, "Indented IFCX (human-readable)")
                        .on_hover_text(
                            "Easier to read and compare, but the files become larger. Autosaves stay compact.",
                        );
                    ui.end_row();
                });
            // How points are entered (SET-001, Appendix D.1).
            ui.add_space(6.0);
            ui.strong("Input");
            ui.group(|ui| {
                egui::Grid::new("preferences-input")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Snap radius (points)");
                        widgets::decimal(ui, &mut draft.snap_radius, 2.0..=40.0, 1, 1.0, nf);
                        ui.end_row();
                        ui.label("Snap grid step (m)");
                        widgets::decimal(ui, &mut draft.snap_step, 0.001..=10.0, 3, 0.01, nf);
                        ui.end_row();
                        // INP-006, Appendix D.1: 45° by default, 15° selectable.
                        ui.label("Angle increment (°)");
                        ui.horizontal(|ui| {
                            widgets::decimal(ui, &mut draft.angle_increment_deg, 1.0..=90.0, 1, 1.0, nf);
                            for (deg, label) in [(15.0, "15°"), (45.0, "45°")] {
                                ui.selectable_value(&mut draft.angle_increment_deg, deg, label);
                            }
                        });
                        ui.end_row();
                        ui.label("Constrain directions from");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut draft.constraint_reference, ConstraintReference::XAxis, "X axis");
                            ui.radio_value(
                                &mut draft.constraint_reference,
                                ConstraintReference::Frame,
                                "Grid orientation",
                            )
                            .on_hover_text("The orientation of the construction grid, once one is placed.");
                        });
                        ui.end_row();
                        // INP-002: distance and angle, X/Y/Z on demand.
                        ui.label("Tracker");
                        ui.checkbox(&mut draft.tracker_xyz, "Show X, Y and Z");
                        ui.end_row();
                        // INP-020: click-click by default.
                        ui.label("Input mode");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut draft.input_mode, InputMode::ClickClick, "Click-click");
                            ui.radio_value(&mut draft.input_mode, InputMode::DragRelease, "Press-drag-release")
                                .on_hover_text(
                                    "Press at a point, drag and release at the next. A click still takes a point.",
                                );
                        });
                        ui.end_row();
                    });
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    result = Some(draft.clone());
                    close = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
        if open && !close {
            self.preferences = Some(draft);
        }
        result
    }
}

fn material_editor(ui: &mut Ui, ws: &mut Workspace, material: &Material) {
    let mut m = material.clone();
    let id = Id::new("material-editor").with(material.id);
    let mut commit = false;
    let nf = ws.prefs.number_format();
    egui::Grid::new(id).num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        ui.label("Name");
        if let Edited::Changed(name) = widgets::text(ui, id.with("name"), Some(&m.name), 160.0) {
            m.name = name;
            commit = true;
        }
        ui.end_row();
        ui.label("Short code");
        if let Edited::Changed(code) = widgets::text(ui, id.with("code"), Some(&m.code), 80.0) {
            m.code = code;
            commit = true;
        }
        ui.end_row();
        ui.label("Category");
        let categories: Vec<(MaterialCategory, String)> = MaterialCategory::ALL
            .iter()
            .map(|c| (*c, c.code().to_lowercase()))
            .collect();
        if let Some(c) = widgets::choice(ui, id.with("category"), Some(&m.category), &categories, 140.0) {
            m.category = c;
            commit = true;
        }
        ui.end_row();
        ui.label("Colour");
        let mut rgb = m.color.0.map(|c| c as f32);
        let response = ui.color_edit_button_rgb(&mut rgb);
        if response.changed() {
            m.color = Rgb(rgb.map(f64::from));
            commit = true;
        }
        ui.end_row();
        ui.label("Opacity");
        let mut opacity = m.opacity;
        let response = widgets::slider(ui, &mut opacity, 0.0..=1.0, 2, &nf);
        if response.changed() && !response.dragged() || response.drag_stopped() {
            m.opacity = opacity;
            commit |= opacity != material.opacity;
        }
        ui.end_row();
        ui.label("Intersection priority")
            .on_hover_text("1–999; higher cuts lower at junctions (WAL-023)");
        let mut priority = m.priority;
        let response = widgets::integer(ui, &mut priority, 1..=999, &nf);
        if response.changed() && !response.dragged() || response.drag_stopped() {
            m.priority = priority;
            commit |= priority != material.priority;
        }
        ui.end_row();
        ui.label("Cut fill");
        ui.horizontal(|ui| {
            let patterns: Vec<(FillPattern, String)> =
                FillPattern::ALL.iter().map(|p| (*p, p.code().to_owned())).collect();
            if let Some(p) = widgets::choice(ui, id.with("pattern"), Some(&m.cut_fill.pattern), &patterns, 100.0) {
                m.cut_fill = CutFill {
                    pattern: p,
                    ..m.cut_fill
                };
                commit = true;
            }
            let mut fg = m.cut_fill.foreground.0.map(|c| c as f32);
            if ui.color_edit_button_rgb(&mut fg).changed() {
                m.cut_fill.foreground = Rgb(fg.map(f64::from));
                commit = true;
            }
        });
        ui.end_row();
        ui.label("Description");
        if let Edited::Changed(d) = widgets::text(ui, id.with("description"), Some(&m.description), 160.0) {
            m.description = d;
            commit = true;
        }
        ui.end_row();
    });
    if commit {
        ws.execute(Command::UpdateMaterial(m));
    }
}

/// The problems panel (UI-012): failed geometry and file-reading reports.
pub fn problems_panel(ui: &mut Ui, ws: &mut Workspace, load_problems: &[archicraft_ifcx::Problem]) {
    let geometry = ws.doc.geometry();
    ui.horizontal(|ui| {
        ui.strong("Problems");
        ui.label(format!(
            "{} geometry, {} from opening the file",
            geometry.problems.len(),
            load_problems.len()
        ));
    });
    egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
        if geometry.problems.is_empty() && load_problems.is_empty() {
            ui.label(RichText::new("No problems.").weak());
        }
        for (id, message) in &geometry.problems {
            let name = ws
                .doc
                .project
                .element(*id)
                .map_or_else(|| id.to_string(), |e| e.name.clone());
            if ui.link(format!("{name}: {message}")).clicked() {
                ws.selection.clear();
                ws.selection.insert(*id);
                ws.requests.push(Request::ZoomTo(vec![*id]));
            }
        }
        for problem in load_problems {
            let color = match problem.severity {
                archicraft_ifcx::Severity::Error => ui.visuals().error_fg_color,
                archicraft_ifcx::Severity::Warning => ui.visuals().warn_fg_color,
                archicraft_ifcx::Severity::Info => ui.visuals().text_color(),
            };
            ui.colored_label(color, ws.render(&problem.located()));
        }
    });
}
