//! 3D views (VIEW-020, VIEW-021, VIEW-050).

use archicraft_geometry::ModelGeometry;
use archicraft_model::glam::DVec3;
use archicraft_model::project::Projection;
use archicraft_model::{ElementId, Project, ViewId};
use eframe::egui_wgpu;
use egui::{Align2, Color32, CursorIcon, Event, FontId, PointerButton, Pos2, Rect, Sense, Ui};

use super::camera::Camera3d;
use super::plan::{modifiers, transfer_cursor, transfer_glyph, transfer_tooltip};
use super::render3d::{DisplayMode, Renderer3d, Viewport, build_scene};
use super::transfer;
use crate::commands::view_keys;
use crate::workspace::{Consumer, Pane, Request, SceneKey, Workspace};

#[derive(Debug)]
pub struct View3dState {
    pub camera: Camera3d,
    pub mode: DisplayMode,
    /// Persistent orbit mode: the primary button orbits (VIEW-021).
    pub orbit: bool,
    fitted: bool,
    /// The camera distance of the last fit to window, against which the
    /// view-control bar reads the zoom (UI-001).
    fit_distance: f64,
    viewport: Option<Viewport>,
    last_rect: Rect,
}

impl View3dState {
    pub fn new(projection: Projection) -> Self {
        let camera = Camera3d::new(projection);
        Self {
            camera,
            fit_distance: camera.distance,
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
        self.fit_distance = self.camera.distance;
    }

    /// The zoom in percent: the camera distance of the last fit to window
    /// against the current one, so 100 % is the fitted view and 200 % half
    /// its distance (UI-001).
    pub fn zoom_percent(&self) -> f64 {
        self.fit_distance / self.camera.distance * 100.0
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

/// The nearest visible element under a pick ray and the hit point. Elements
/// of kinds without a body are never picked in 3D (ANN-002).
pub fn pick(project: &Project, geometry: &ModelGeometry, origin: DVec3, dir: DVec3) -> Option<(ElementId, DVec3)> {
    let mut best: Option<(f64, ElementId)> = None;
    for element in project
        .elements
        .values()
        .filter(|e| e.kind.descriptor().has_body && project.is_visible(e))
    {
        let (Some(storey), Some(g)) = (project.storeys.get(element.storey), geometry.get(element.id)) else {
            continue;
        };
        let local_origin = origin - DVec3::new(0.0, 0.0, storey.elevation);
        for part in g.bodies() {
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
    // While a parameter transfer gesture is held or its mode latched, a
    // click picks up or injects through the handler the plan uses too
    // (ELM-003); an Alt-drag still orbits.
    let hit_at = |state: &View3dState, ws: &Workspace, p: Pos2| {
        let (origin, dir) = state.camera.ray(rect, p);
        pick(&ws.doc.project, &geometry, origin, dir)
    };
    let held = ui.input(|i| modifiers(i.modifiers));
    // Contour detection's edge-only Alt-click exists only in the plan, so
    // the gestures are always free here; a transient consumer and an open
    // text editor keep their clicks, as in the plan.
    let transfer_mode = ws
        .transfer
        .mode(held, &ws.keymap, true)
        .filter(|_| ws.tool.editor.is_none() && ws.active_consumer == Consumer::Tool);
    // The candidate of a transfer under the pointer, tinted and previewed
    // before the click (ELM-003, EDT-020).
    let candidate = transfer_mode
        .zip(response.hover_pos().filter(|_| !response.dragged()))
        .and_then(|(_, p)| hit_at(&state, ws, p).map(|(id, _)| id));
    match candidate {
        Some(id) => ws.hover_3d = Some((view, id)),
        None if ws.hover_3d.is_some_and(|(v, _)| v == view) => ws.hover_3d = None,
        None => {}
    }
    // Esc ends a latched transfer mode here too, as in the plan (ELM-003);
    // invoked another way, its command waits for the active view.
    if ws.active_pane == Some(pane) && !ui.ctx().text_edit_focused() {
        let pressed = ui.input(|i| {
            i.events.iter().any(|event| match event {
                Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => ws
                    .keymap
                    .matching(&[view_keys::CANCEL_STEP], *key, *modifiers)
                    .is_some(),
                _ => false,
            })
        });
        let invoked = ws.view_commands.contains(&view_keys::CANCEL_STEP);
        if pressed || invoked {
            ws.transfer.cancel();
        }
    }
    if response.clicked_by(PointerButton::Primary)
        && let Some(p) = response.interact_pointer_pos()
    {
        let hit = hit_at(&state, ws, p).map(|(id, _)| id);
        match transfer_mode {
            // The second click of a double-click transfers nothing, so the
            // first one's status line stays (ELM-003).
            Some(_) if response.double_clicked_by(PointerButton::Primary) => {}
            Some(mode) => transfer::click(ws, mode, hit),
            // A grouped element selects its group; Shift toggles it whole
            // (ELM-015).
            None => ws.click_select(hit, held.shift),
        }
    }
    if response.double_clicked_by(PointerButton::Primary)
        && transfer_mode.is_none()
        && let Some(p) = response.interact_pointer_pos()
        && let Some((_, point)) = hit_at(&state, ws, p)
    {
        state.camera.target = point;
        state.camera.distance = (state.camera.eye() - point).length().max(0.1);
        ws.info(crate::tr!("status-orbit-pivot-set"));
    }

    let painter = ui.painter_at(rect);
    let dark = ui.visuals().dark_mode;
    match render_state {
        Some(rs) => {
            let key = SceneKey::of(ws);
            let renderer = ws.renderer.get_or_insert_with(|| Renderer3d::new(rs));
            if ws.scene.as_ref().is_none_or(|(k, _)| *k != key) {
                let selection = ws.selection.clone();
                let data = build_scene(&ws.doc.project, &geometry, &selection, key.hover);
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
                crate::tr!("view-3d-no-gpu"),
                FontId::proportional(14.0),
                ui.visuals().weak_text_color(),
            );
        }
    }
    if let Some(mode) = transfer_mode
        && let Some(p) = response.hover_pos()
        && !response.dragged()
    {
        ui.ctx().set_cursor_icon(transfer_cursor(mode));
        painter.extend(transfer_glyph(p, mode, ui.visuals().selection.stroke.color));
        if let Some(preview) = candidate.and_then(|id| transfer::preview(ws, mode, id)) {
            transfer_tooltip(&response, &preview);
        }
    }
    let projection = match state.camera.projection {
        Projection::Perspective => crate::tr!("view-projection-perspective"),
        Projection::Parallel => crate::tr!("view-projection-parallel"),
    };
    let mode = crate::tr!(state.mode.label_key());
    let label = if state.orbit {
        crate::tr!("view-3d-overlay-orbit", projection = projection, mode = mode)
    } else {
        crate::tr!("view-3d-overlay", projection = projection, mode = mode)
    };
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
        super::group_menu_items(ui, ws);
    });
    ws.views_3d.insert(view, state);
}

#[cfg(test)]
mod tests {
    use archicraft_model::glam::DVec2;
    use archicraft_model::kinds::Dummy;
    use archicraft_model::template::default_metric_project;
    use archicraft_model::{Command, History};

    use super::*;

    /// IFX-016, VIEW-001: a ray through a part of a compound element (the
    /// test kind with parts, which stacks two half-high parts on top of its
    /// box) picks the element at the part's face, and the bounds of the
    /// element include its parts.
    #[test]
    fn a_ray_through_a_part_picks_the_element() {
        let mut project = default_metric_project();
        let storey = project.storeys.ground();
        let material = project.materials[0].id;
        // A 2 × 1 × 1 m box from (3, 1) with two 1 × 1 × 0.5 m parts on top.
        let mut compound = Dummy::new(DVec2::new(3.0, 1.0), DVec2::new(2.0, 1.0), material);
        compound.parts = true;
        let compound = compound.into_element(&project, storey, 0);
        let id = compound.id;
        History::default()
            .execute(&mut project, Command::AddElements(vec![compound]))
            .unwrap();
        let geometry = ModelGeometry::evaluate(&project);
        assert!(geometry.problems.is_empty(), "{:?}", geometry.problems);
        let elevation = project.storeys.get(storey).unwrap().elevation;
        let along_x = |height: f64| {
            pick(
                &project,
                &geometry,
                DVec3::new(-10.0, 1.5, elevation + height),
                DVec3::X,
            )
        };

        // Above the box, through the parts: the first part's face at x = 3.
        let (hit, point) = along_x(1.25).expect("a part is hit");
        assert_eq!(hit, id);
        assert!((point.x - 3.0).abs() < 1e-9, "{point}");
        // Through the box itself, and above everything.
        assert_eq!(along_x(0.5).map(|(hit, _)| hit), Some(id));
        assert!(along_x(1.75).is_none());

        let (_, hi) = model_bounds(&project, &geometry, Some(&[id])).unwrap();
        assert!((hi.z - (elevation + 1.5)).abs() < 1e-9, "{hi}");
    }

    /// ANN-002: a ray never picks an element whose kind has no body (the
    /// body-less test kind), by the descriptor's flag: even where a geometry
    /// wrongly carries a box for it, the ray passes on to the box behind.
    #[test]
    fn a_ray_never_picks_a_kind_without_a_body() {
        use archicraft_model::kinds::Note;

        let mut project = default_metric_project();
        let storey = project.storeys.ground();
        let material = project.materials[0].id;
        let near = Dummy::new(DVec2::new(1.0, 1.0), DVec2::ONE, material).into_element(&project, storey, 0);
        let far = Dummy::new(DVec2::new(5.0, 1.0), DVec2::ONE, material).into_element(&project, storey, 1);
        let note = Note::new(storey, DVec2::new(1.5, 1.5), "Shaft").into_element(&project, storey, 0);
        let (near_id, far_id, note_id) = (near.id, far.id, note.id);
        History::default()
            .execute(&mut project, Command::AddElements(vec![near, far, note]))
            .unwrap();
        let mut geometry = ModelGeometry::evaluate(&project);
        assert!(geometry.problems.is_empty(), "{:?}", geometry.problems);
        let ray = |project: &Project, geometry: &ModelGeometry| {
            pick(project, geometry, DVec3::new(-10.0, 1.5, 0.5), DVec3::X).map(|(hit, _)| hit)
        };
        assert_eq!(ray(&project, &geometry), Some(near_id));

        // The note now has the near box's body, and the near box none.
        let a_box = geometry.elements.remove(&near_id).unwrap();
        geometry.elements.insert(note_id, a_box);
        assert_eq!(ray(&project, &geometry), Some(far_id));
    }
}
