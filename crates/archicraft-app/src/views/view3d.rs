//! 3D views (VIEW-020, VIEW-021, VIEW-050).

use archicraft_geometry::ModelGeometry;
use archicraft_model::glam::DVec3;
use archicraft_model::project::Projection;
use archicraft_model::{ElementId, Project, ViewId};
use eframe::egui_wgpu;
use egui::{Align2, Color32, CursorIcon, Event, FontId, PointerButton, Pos2, Rect, Sense, Ui};

use super::camera::Camera3d;
use super::render3d::{DisplayMode, Renderer3d, Viewport, build_scene};
use crate::workspace::{Pane, Request, SceneKey, Workspace};

#[derive(Debug)]
pub struct View3dState {
    pub camera: Camera3d,
    pub mode: DisplayMode,
    /// Persistent orbit mode: the primary button orbits (VIEW-021).
    pub orbit: bool,
    fitted: bool,
    viewport: Option<Viewport>,
    last_rect: Rect,
}

impl View3dState {
    pub fn new(projection: Projection) -> Self {
        Self {
            camera: Camera3d::new(projection),
            mode: DisplayMode::ShadedWithEdges,
            orbit: false,
            fitted: false,
            viewport: None,
            last_rect: Rect::NOTHING,
        }
    }

    pub fn fit(&mut self, bounds: Option<(DVec3, DVec3)>) {
        match bounds {
            Some((lo, hi)) => self.camera.fit(lo, hi),
            None => self
                .camera
                .fit(DVec3::new(-2.0, -2.0, 0.0), DVec3::new(12.0, 10.0, 3.0)),
        }
    }

    pub fn last_rect(&self) -> Rect {
        self.last_rect
    }

    /// Releases the GPU texture of a closed view.
    pub fn release(self, render_state: Option<&egui_wgpu::RenderState>) {
        if let (Some(rs), Some(viewport)) = (render_state, self.viewport) {
            Renderer3d::free_viewport(rs, viewport);
        }
    }
}

/// Bounds of the visible elements, in model coordinates.
pub fn model_bounds(project: &Project, geometry: &ModelGeometry, ids: Option<&[ElementId]>) -> Option<(DVec3, DVec3)> {
    project
        .elements
        .values()
        .filter(|e| project.is_visible(e))
        .filter(|e| ids.is_none_or(|ids| ids.contains(&e.id)))
        .filter_map(|e| {
            let z = project.storeys.get(e.storey)?.elevation;
            let (lo, hi) = geometry.get(e.id)?.bounds()?;
            let offset = DVec3::new(0.0, 0.0, z);
            Some((lo + offset, hi + offset))
        })
        .reduce(|(a, b), (c, d)| (a.min(c), b.max(d)))
}

/// The nearest visible element under a pick ray and the hit point.
pub fn pick(project: &Project, geometry: &ModelGeometry, origin: DVec3, dir: DVec3) -> Option<(ElementId, DVec3)> {
    let mut best: Option<(f64, ElementId)> = None;
    for element in project.elements.values().filter(|e| project.is_visible(e)) {
        let (Some(storey), Some(g)) = (project.storeys.get(element.storey), geometry.get(element.id)) else {
            continue;
        };
        let local_origin = origin - DVec3::new(0.0, 0.0, storey.elevation);
        for part in &g.parts {
            if let Some(t) = part.mesh.ray_hit(local_origin, dir)
                && best.is_none_or(|(b, _)| t < b)
            {
                best = Some((t, element.id));
            }
        }
    }
    best.map(|(t, id)| (id, origin + dir * t))
}

fn background(dark: bool) -> [f32; 4] {
    if dark {
        [0.15, 0.16, 0.18, 1.0]
    } else {
        [0.92, 0.93, 0.95, 1.0]
    }
}

/// Shows a 3D view and handles its navigation and picking.
pub fn ui(ui: &mut Ui, ws: &mut Workspace, view: ViewId, render_state: Option<&egui_wgpu::RenderState>) {
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let projection = ws
        .doc
        .project
        .view(view)
        .map_or(Projection::Perspective, |v| v.projection);
    let mut state = ws
        .views_3d
        .remove(&view)
        .unwrap_or_else(|| View3dState::new(projection));
    state.last_rect = rect;
    let pane = Pane::View3d(view);
    if response.hovered() && ui.input(|i| i.pointer.any_pressed()) {
        ws.active_pane = Some(pane);
    }
    let geometry = ws.doc.geometry();
    if !state.fitted {
        state.fit(model_bounds(&ws.doc.project, &geometry, None));
        state.fitted = true;
    }

    // Navigation (VIEW-021, HW-002, HW-007).
    if response.hovered() {
        let pointer = response.hover_pos().unwrap_or(rect.center());
        for event in ui.input(|i| i.raw.events.clone()) {
            match event {
                Event::MouseWheel {
                    unit, delta, modifiers, ..
                } => {
                    if unit == egui::MouseWheelUnit::Point && !modifiers.command && !modifiers.ctrl {
                        if modifiers.shift {
                            state.camera.orbit(delta);
                        } else {
                            state.camera.pan(delta, rect.height());
                        }
                    } else {
                        let notches = if unit == egui::MouseWheelUnit::Point {
                            delta.y / 50.0
                        } else {
                            delta.y
                        };
                        state.camera.zoom_at(rect, pointer, 1.2f64.powf(-f64::from(notches)));
                    }
                }
                Event::Zoom(factor) => state.camera.zoom_at(rect, pointer, 1.0 / f64::from(factor.max(0.01))),
                _ => {}
            }
        }
    }
    let shift = ui.input(|i| i.modifiers.shift);
    if response.dragged_by(PointerButton::Middle) {
        if shift {
            state.camera.orbit(response.drag_delta());
        } else {
            state.camera.pan(response.drag_delta(), rect.height());
        }
    }
    if response.dragged_by(PointerButton::Primary) && (state.orbit || ui.input(|i| i.modifiers.alt)) {
        state.camera.orbit(response.drag_delta());
        ui.ctx().set_cursor_icon(CursorIcon::AllScroll);
    }

    // Picking: click selects, double-click sets the orbit pivot (VIEW-021).
    let hit_at = |state: &View3dState, ws: &Workspace, p: Pos2| {
        let (origin, dir) = state.camera.ray(rect, p);
        pick(&ws.doc.project, &geometry, origin, dir)
    };
    if response.clicked_by(PointerButton::Primary)
        && let Some(p) = response.interact_pointer_pos()
    {
        let hit = hit_at(&state, ws, p).map(|(id, _)| id);
        let shift = ui.input(|i| i.modifiers.shift);
        match hit {
            Some(id) if shift => {
                if !ws.selection.remove(&id) {
                    ws.selection.insert(id);
                }
            }
            Some(id) => {
                ws.selection.clear();
                ws.selection.insert(id);
            }
            None if !shift => ws.selection.clear(),
            None => {}
        }
    }
    if response.double_clicked_by(PointerButton::Primary)
        && let Some(p) = response.interact_pointer_pos()
        && let Some((_, point)) = hit_at(&state, ws, p)
    {
        state.camera.target = point;
        state.camera.distance = (state.camera.eye() - point).length().max(0.1);
        ws.info("Orbit pivot set");
    }

    let painter = ui.painter_at(rect);
    let dark = ui.visuals().dark_mode;
    match render_state {
        Some(rs) => {
            let key = SceneKey::of(ws);
            let renderer = ws.renderer.get_or_insert_with(|| Renderer3d::new(rs));
            if ws.scene.as_ref().is_none_or(|(k, _)| *k != key) {
                let selection = ws.selection.clone();
                let data = build_scene(&ws.doc.project, &geometry, &selection, None);
                ws.scene = Some((key, renderer.upload_scene(&data)));
            }
            if let Some((_, scene)) = &ws.scene {
                let ppp = ui.ctx().pixels_per_point();
                let size = [
                    (rect.width() * ppp).round() as u32,
                    (rect.height() * ppp).round() as u32,
                ];
                renderer.ensure_viewport(rs, &mut state.viewport, size);
                if let Some(viewport) = state.viewport.as_mut() {
                    let aspect = f64::from(rect.width() / rect.height().max(1.0));
                    let view_proj =
                        state.camera.projection_matrix(aspect, scene.radius) * state.camera.view_matrix(scene.origin);
                    renderer.render(viewport, scene, view_proj, state.mode, background(dark));
                    painter.image(
                        viewport.texture_id,
                        rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
            }
        }
        None => {
            painter.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "3D views need a GPU (wgpu), which is not available.",
                FontId::proportional(14.0),
                ui.visuals().weak_text_color(),
            );
        }
    }
    let label = format!(
        "{} · {}{}",
        match state.camera.projection {
            Projection::Perspective => "Perspective",
            Projection::Parallel => "Parallel",
        },
        state.mode.label(),
        if state.orbit { " · Orbit" } else { "" }
    );
    painter.text(
        rect.left_top() + egui::vec2(8.0, 6.0),
        Align2::LEFT_TOP,
        label,
        FontId::proportional(12.0),
        ui.visuals().weak_text_color(),
    );

    response.context_menu(|ui| {
        if ui
            .button(if state.orbit { "Leave orbit mode" } else { "Orbit mode" })
            .clicked()
        {
            state.orbit = !state.orbit;
            ui.close();
        }
        if ui.button("Fit to window").clicked() {
            state.fit(model_bounds(&ws.doc.project, &geometry, None));
            ui.close();
        }
        if ui
            .add_enabled(!ws.selection.is_empty(), egui::Button::new("Zoom to selection"))
            .clicked()
        {
            ws.requests.push(Request::ZoomTo(ws.selected_ids()));
            ui.close();
        }
        if ui
            .add_enabled(!ws.selection.is_empty(), egui::Button::new("Delete"))
            .clicked()
        {
            ws.delete_selection();
            ui.close();
        }
    });
    ws.views_3d.insert(view, state);
}
