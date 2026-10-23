//! Plan views (VIEW-010, VIEW-014, DSP-004, DSP-014, SEL-001…SEL-004).
//!
//! The plan is generated from the 3D bodies by the geometry crate; this view
//! draws it with egui's anti-aliased painter: cut outlines heavier than
//! projected lines, overhead lines dashed, cut fills from the materials.
//! Points go through the point-input service (`crate::input`, INP-001): the
//! view offers its surface, translates egui events and draws the result.

use std::sync::Arc;

use archicraft_geometry::plan::{PlanDrawing, polygon_rings};
use archicraft_model::glam::DVec2;
use archicraft_model::units::{NumberFormat, UnitSettings};
use archicraft_model::{ElementId, ElementKind, KindTag, Project, StoreyId};
use egui::epaint::Vertex;
use egui::{
    Align2, Color32, CursorIcon, Event, FontId, Key, Mesh, PointerButton, Pos2, Rect, Sense, Shape, Stroke, StrokeKind,
    Ui,
};

use super::camera::{Camera2d, ZoomHistory};
use crate::input::{
    ConstraintReference, CursorState, Effect, Expect, Frame2d, Guide, GuideShape, Hovered, InputConsumer, InputContext,
    InputSettings, KeyInput, Modifiers, Outcome, PointInput, PointerEvent, Resolved, SnapIndex, SnapKind, SurfaceFrame,
    TrackerMode, ViewSurface,
};
use crate::tools::{ToolKind, ToolState};
use crate::tr;
use crate::workspace::{Consumer, InputFrames, Pane, Request, Workspace};

/// Per-view state of a plan view.
#[derive(Clone, Debug)]
pub struct PlanViewState {
    pub camera: Camera2d,
    pub zoom_history: ZoomHistory,
    fitted: bool,
    area_start: Option<Pos2>,
    /// Where the view was drawn last frame, for view commands.
    pub last_rect: Rect,
    /// Last pick position and the index into its candidates (Tab cycling).
    pick: Option<(DVec2, usize)>,
    last_record: f64,
    /// The tracker text drawn last frame (INP-002).
    pub tracker: Option<String>,
    /// The snap glyph and the command state drawn last frame (INP-008).
    pub glyph: Option<(SnapKind, CursorState)>,
    /// The label drawn beside the cursor last frame: the tracker and the
    /// snap kind (INP-002, INP-008).
    pub label: Option<String>,
    /// The division points of the hovered edge drawn as ticks last frame
    /// (INP-010).
    pub ticks: Vec<DVec2>,
    /// The preview outlines of the input in progress drawn last frame, each
    /// with whether it is closed (WAL-007).
    pub preview: Vec<(Vec<DVec2>, bool)>,
    /// The pending points of the input in progress marked last frame
    /// (UND-003).
    pub pending: Vec<DVec2>,
    /// The guides drawn dashed last frame (INP-006).
    pub guides: Vec<Guide>,
    /// The plane point under the pointer last frame, for the coordinate
    /// readout of the view-control bar (INP-014).
    pub cursor: Option<DVec2>,
}

impl Default for PlanViewState {
    fn default() -> Self {
        Self {
            camera: Camera2d::default(),
            zoom_history: ZoomHistory::default(),
            fitted: false,
            area_start: None,
            last_rect: Rect::NOTHING,
            pick: None,
            last_record: 0.0,
            tracker: None,
            glyph: None,
            label: None,
            ticks: Vec::new(),
            preview: Vec::new(),
            pending: Vec::new(),
            guides: Vec::new(),
            cursor: None,
        }
    }
}

impl PlanViewState {
    /// Records the camera before a navigation step, at most twice a second
    /// so that one wheel gesture is one step.
    fn record(&mut self, now: f64) {
        if now - self.last_record > 0.5 {
            self.zoom_history.record(self.camera);
        }
        self.last_record = now;
    }

    pub fn fit(&mut self, rect: Rect, plan: &PlanDrawing) {
        self.zoom_history.record(self.camera);
        match plan.bounds() {
            Some((min, max)) => self.camera.fit(rect, min, max),
            None => self.camera.fit(rect, DVec2::new(-2.0, -2.0), DVec2::new(12.0, 10.0)),
        }
    }

    pub fn zoom_to(&mut self, rect: Rect, plan: &PlanDrawing, ids: &[ElementId]) {
        let bounds = ids
            .iter()
            .filter_map(|id| plan.element(*id)?.bounds())
            .reduce(|(lo, hi), (l, h)| (lo.min(l), hi.max(h)));
        if let Some((min, max)) = bounds {
            self.zoom_history.record(self.camera);
            let pad = DVec2::splat(1.0);
            self.camera.fit(rect, min - pad, max + pad);
        }
    }

    pub fn previous_zoom(&mut self) {
        if let Some(c) = self.zoom_history.previous(self.camera) {
            self.camera = c;
        }
    }

    pub fn next_zoom(&mut self) {
        if let Some(c) = self.zoom_history.next(self.camera) {
            self.camera = c;
        }
    }
}

/// The surface of a plan view for point input (INP-001): the camera maps
/// screen pixels onto the storey's plane at its elevation.
#[derive(Clone, Copy, Debug)]
pub struct PlanSurface {
    pub camera: Camera2d,
    pub rect: Rect,
    pub elevation: f64,
}

impl ViewSurface for PlanSurface {
    fn to_plane(&self, screen: DVec2) -> Option<DVec2> {
        Some(
            self.camera
                .to_world(self.rect, Pos2::new(screen.x as f32, screen.y as f32)),
        )
    }

    fn to_screen(&self, plane: DVec2) -> DVec2 {
        let p = self.camera.to_screen(self.rect, plane);
        DVec2::new(f64::from(p.x), f64::from(p.y))
    }

    fn model_per_pixel(&self) -> f64 {
        1.0 / self.camera.scale
    }

    fn frame(&self) -> SurfaceFrame {
        SurfaceFrame::plan(self.elevation)
    }
}

/// What a plan view gives point input besides the consumer's step: the snap
/// targets of the storey, the tool's points entered so far and the settings,
/// from which each step's context is built (INP-001). The consumer is the
/// tool or a transient consumer such as placing the user origin.
struct PlanInput {
    storey: StoreyId,
    frame: SurfaceFrame,
    targets: Arc<SnapIndex>,
    pending: Vec<DVec2>,
    radius: f64,
    drag_threshold: f64,
    units: UnitSettings,
    number_format: NumberFormat,
    angle_increment_deg: f64,
    constraint_reference: ConstraintReference,
    snap_step: f64,
    tracker_xyz: bool,
    /// The division mode and scope (INP-010).
    input: InputSettings,
}

impl PlanInput {
    /// `click_distance` is how far in points the pointer moves before a
    /// press becomes a drag.
    fn new(ws: &mut Workspace, storey: StoreyId, surface: &PlanSurface, click_distance: f32) -> Self {
        let pending = if ws.active_consumer == Consumer::Tool && on_storey(ws.tool.storey, storey) {
            ws.tool.points.clone()
        } else {
            Vec::new()
        };
        Self {
            storey,
            frame: surface.frame(),
            targets: ws.snap_index(storey),
            pending,
            radius: f64::from(ws.prefs.snap_radius) * surface.model_per_pixel(),
            drag_threshold: f64::from(click_distance) * surface.model_per_pixel(),
            units: ws.doc.project.settings.units.clone(),
            number_format: ws.prefs.number_format(),
            angle_increment_deg: ws.prefs.angle_increment_deg,
            constraint_reference: ws.prefs.constraint_reference,
            snap_step: ws.prefs.snap_step,
            tracker_xyz: ws.prefs.tracker_xyz,
            input: ws.prefs.input,
        }
    }

    /// The context of the current step of the active consumer, the tool or
    /// a transient one, with the user origin of `frames` (INP-014). Points
    /// entered on another storey are no reference in this view.
    fn ctx<'a>(
        &'a self,
        project: &'a Project,
        frames: InputFrames,
        active: &Consumer,
        tool: &ToolState,
    ) -> InputContext<'a> {
        let here = on_storey(active.storey(tool), self.storey);
        let consumer = active.get(tool);
        let step = consumer.step().filter(|_| here);
        InputContext {
            project,
            storey: self.storey,
            targets: &self.targets,
            pending: &self.pending,
            previous: consumer.reference_point().filter(|_| here),
            guides: &[],
            input: self.input,
            expects: step.as_ref().map_or(Expect::Point, |s| s.expects),
            tracker: step.as_ref().map_or(TrackerMode::Relative, |s| s.tracker),
            tracker_xyz: self.tracker_xyz,
            radius: self.radius,
            drag_threshold: self.drag_threshold,
            units: &self.units,
            number_format: &self.number_format,
            angle_increment_deg: self.angle_increment_deg,
            constraint_reference: self.constraint_reference,
            // Construction grids (GRD-001) will orient the input frame.
            frame_rotation: 0.0,
            snap_step: self.snap_step,
            frame: self.frame,
            user_origin: frames.user_origin,
            marked: None,
        }
    }
}

/// Whether the input that started on `started` (none: not started) is on
/// `storey`.
fn on_storey(started: Option<StoreyId>, storey: StoreyId) -> bool {
    started.is_none_or(|s| s == storey)
}

fn screen(p: Pos2) -> DVec2 {
    DVec2::new(f64::from(p.x), f64::from(p.y))
}

fn modifiers(m: egui::Modifiers) -> Modifiers {
    Modifiers {
        shift: m.shift,
        command: m.command,
        alt: m.alt,
    }
}

/// Colours of the plan, for light and dark canvases.
struct Palette {
    paper: Color32,
    grid_minor: Color32,
    grid_major: Color32,
    cut: Color32,
    projected: Color32,
    overhead: Color32,
    reference: Color32,
    selection: Color32,
    hover: Color32,
    preview: Color32,
    snap: Color32,
}

impl Palette {
    fn new(dark: bool) -> Self {
        if dark {
            Self {
                paper: Color32::from_gray(30),
                grid_minor: Color32::from_gray(40),
                grid_major: Color32::from_gray(54),
                cut: Color32::from_gray(235),
                projected: Color32::from_gray(185),
                overhead: Color32::from_gray(120),
                reference: Color32::from_rgb(230, 90, 90),
                selection: Color32::from_rgb(70, 150, 255),
                hover: Color32::from_rgb(120, 170, 240),
                preview: Color32::from_rgb(255, 150, 50),
                snap: Color32::from_rgb(40, 200, 120),
            }
        } else {
            Self {
                paper: Color32::WHITE,
                grid_minor: Color32::from_gray(241),
                grid_major: Color32::from_gray(224),
                cut: Color32::BLACK,
                projected: Color32::from_gray(70),
                overhead: Color32::from_gray(150),
                reference: Color32::from_rgb(200, 40, 40),
                selection: Color32::from_rgb(38, 115, 242),
                hover: Color32::from_rgb(110, 160, 245),
                preview: Color32::from_rgb(230, 120, 30),
                snap: Color32::from_rgb(20, 160, 90),
            }
        }
    }
}

/// Line weights in screen points (DSP-004: cut heavier than projected).
const CUT_WIDTH: f32 = 1.8;
const PROJECTED_WIDTH: f32 = 0.9;
const HIGHLIGHT_WIDTH: f32 = 2.6;

fn rgb(c: archicraft_model::Rgb) -> Color32 {
    let [r, g, b] = c.to_u8();
    Color32::from_rgb(r, g, b)
}

fn draw_grid(painter: &egui::Painter, rect: Rect, camera: &Camera2d, palette: &Palette) {
    // The finest power-of-ten spacing that keeps lines at least 12 points apart.
    let mut step = 0.01;
    while step * camera.scale < 12.0 {
        step *= 10.0;
    }
    let min = camera.to_world(rect, rect.left_bottom());
    let max = camera.to_world(rect, rect.right_top());
    let lines = ((max.x - min.x) / step) as i64 + ((max.y - min.y) / step) as i64;
    if lines > 2000 {
        return;
    }
    let major = |v: f64| ((v / step).round() as i64).rem_euclid(10) == 0;
    let mut x = (min.x / step).floor() * step;
    while x <= max.x {
        let color = if major(x) {
            palette.grid_major
        } else {
            palette.grid_minor
        };
        let sx = camera.to_screen(rect, DVec2::new(x, 0.0)).x;
        painter.line_segment(
            [Pos2::new(sx, rect.top()), Pos2::new(sx, rect.bottom())],
            Stroke::new(1.0, color),
        );
        x += step;
    }
    let mut y = (min.y / step).floor() * step;
    while y <= max.y {
        let color = if major(y) {
            palette.grid_major
        } else {
            palette.grid_minor
        };
        let sy = camera.to_screen(rect, DVec2::new(0.0, y)).y;
        painter.line_segment(
            [Pos2::new(rect.left(), sy), Pos2::new(rect.right(), sy)],
            Stroke::new(1.0, color),
        );
        y += step;
    }
    // Project origin (INP-014).
    let o = camera.to_screen(rect, DVec2::ZERO);
    painter.line_segment(
        [o, o + egui::vec2(24.0, 0.0)],
        Stroke::new(1.5, Color32::from_rgb(210, 60, 60)),
    );
    painter.line_segment(
        [o, o - egui::vec2(0.0, 24.0)],
        Stroke::new(1.5, Color32::from_rgb(60, 160, 60)),
    );
}

/// The user origin (INP-014): its X and Y axes with its rotation, dashed
/// and with a ring at the origin to tell it from the project origin.
fn draw_user_origin(painter: &egui::Painter, rect: Rect, camera: &Camera2d, frame: Frame2d) {
    let o = camera.to_screen(rect, frame.origin);
    let (sin, cos) = frame.rotation.sin_cos();
    // Screen Y points down.
    let x = egui::vec2(cos as f32, -sin as f32) * 28.0;
    let y = egui::vec2(-sin as f32, -cos as f32) * 28.0;
    for (axis, color) in [(x, Color32::from_rgb(210, 60, 60)), (y, Color32::from_rgb(60, 160, 60))] {
        painter.extend(Shape::dashed_line(&[o, o + axis], Stroke::new(1.5, color), 4.0, 3.0));
    }
    painter.circle_stroke(o, 4.0, Stroke::new(1.2, Color32::from_rgb(210, 60, 60)));
}

fn element_outline(
    painter: &egui::Painter,
    rect: Rect,
    camera: &Camera2d,
    plan: &PlanDrawing,
    id: ElementId,
    stroke: Stroke,
) {
    let Some(element) = plan.element(id) else {
        return;
    };
    for polygon in &element.cut {
        for ring in polygon_rings(polygon) {
            let points: Vec<Pos2> = ring.iter().map(|p| camera.to_screen(rect, *p)).collect();
            painter.add(Shape::closed_line(points, stroke));
        }
    }
    for [a, b] in element.projected.iter().chain(&element.overhead) {
        painter.line_segment([camera.to_screen(rect, *a), camera.to_screen(rect, *b)], stroke);
    }
}

fn reference_line(painter: &egui::Painter, a: Pos2, b: Pos2, color: Color32) {
    let stroke = Stroke::new(1.2, color);
    painter.line_segment([a, b], stroke);
    let dir = (b - a).normalized();
    if dir.length() > 0.5 {
        let normal = egui::vec2(-dir.y, dir.x);
        let tip = a + (b - a) * 0.5;
        painter.line_segment([tip, tip - dir * 8.0 + normal * 4.0], stroke);
        painter.line_segment([tip, tip - dir * 8.0 - normal * 4.0], stroke);
    }
}

fn draw_plan(
    painter: &egui::Painter,
    rect: Rect,
    camera: &Camera2d,
    plan: &PlanDrawing,
    ws: &Workspace,
    palette: &Palette,
) {
    let project = &ws.doc.project;
    // Overhead (dashed) and projected lines first, cut on top.
    for element in &plan.elements {
        for [a, b] in &element.overhead {
            let points = [camera.to_screen(rect, *a), camera.to_screen(rect, *b)];
            painter.extend(Shape::dashed_line(
                &points,
                Stroke::new(PROJECTED_WIDTH, palette.overhead),
                6.0,
                4.0,
            ));
        }
        for [a, b] in &element.projected {
            painter.line_segment(
                [camera.to_screen(rect, *a), camera.to_screen(rect, *b)],
                Stroke::new(PROJECTED_WIDTH, palette.projected),
            );
        }
    }
    for region in &plan.cut_regions {
        let fill = project
            .material(region.material)
            .map_or(Color32::from_gray(200), |m| rgb(m.cut_fill.foreground));
        let mesh = Mesh {
            vertices: region
                .fill
                .points
                .iter()
                .map(|p| Vertex {
                    pos: camera.to_screen(rect, *p),
                    uv: egui::epaint::WHITE_UV,
                    color: fill,
                })
                .collect(),
            indices: region.fill.triangles.iter().flatten().copied().collect(),
            ..Mesh::default()
        };
        painter.add(Shape::mesh(mesh));
        for polygon in &region.polygons {
            for ring in polygon_rings(polygon) {
                let points: Vec<Pos2> = ring.iter().map(|p| camera.to_screen(rect, *p)).collect();
                painter.add(Shape::closed_line(points, Stroke::new(CUT_WIDTH, palette.cut)));
            }
        }
    }
    for element in &plan.elements {
        let selected = ws.selection.contains(&element.id);
        if let Some([a, b]) = element.reference_line
            && (selected || ws.prefs.show_reference_lines)
        {
            let color = if selected { palette.selection } else { palette.reference };
            reference_line(painter, camera.to_screen(rect, a), camera.to_screen(rect, b), color);
        }
    }
    if let Some(id) = ws.hover.filter(|id| !ws.selection.contains(id)) {
        element_outline(
            painter,
            rect,
            camera,
            plan,
            id,
            Stroke::new(HIGHLIGHT_WIDTH, palette.hover),
        );
    }
    for id in &ws.selection {
        element_outline(
            painter,
            rect,
            camera,
            plan,
            *id,
            Stroke::new(HIGHLIGHT_WIDTH, palette.selection),
        );
    }
    // The user origin next to the project origin (INP-014).
    if ws.frames.user_origin.is_placed() {
        draw_user_origin(painter, rect, camera, ws.frames.user_origin);
    }
}

/// The glyph of a snap of kind `snap` at its point `p` (INP-008, INP-024):
/// each kind has its own, and a free point shows the crosshair. Node and
/// intersection keep the square and the cross.
pub fn snap_glyph(p: Pos2, snap: SnapKind, color: Color32) -> Vec<Shape> {
    let stroke = Stroke::new(1.5, color);
    let seg = |a: (f32, f32), b: (f32, f32)| {
        Shape::line_segment([p + egui::vec2(a.0, a.1), p + egui::vec2(b.0, b.1)], stroke)
    };
    let ring = |r: f32| Shape::circle_stroke(p, r, stroke);
    let poly = |pts: &[(f32, f32)]| pts.iter().map(|&(x, y)| p + egui::vec2(x, y)).collect::<Vec<_>>();
    let cross = |h: f32| vec![seg((-h, -h), (h, h)), seg((-h, h), (h, -h))];
    let plus = |h: f32| vec![seg((-h, 0.0), (h, 0.0)), seg((0.0, -h), (0.0, h))];
    match snap {
        SnapKind::Node => vec![Shape::rect_stroke(
            Rect::from_center_size(p, egui::vec2(10.0, 10.0)),
            0.0,
            stroke,
            StrokeKind::Middle,
        )],
        SnapKind::Handle => vec![Shape::rect_filled(
            Rect::from_center_size(p, egui::vec2(8.0, 8.0)),
            0.0,
            color,
        )],
        SnapKind::Centre => [vec![ring(6.0)], plus(3.0)].concat(),
        SnapKind::AxisPoint => vec![Shape::closed_line(
            poly(&[(0.0, -7.0), (7.0, 0.0), (0.0, 7.0), (-7.0, 0.0)]),
            stroke,
        )],
        SnapKind::Centroid => vec![ring(6.0), Shape::circle_filled(p, 2.0, color)],
        SnapKind::Intersection => cross(6.0),
        SnapKind::Midpoint => vec![Shape::closed_line(
            poly(&[(0.0, -7.0), (6.0, 5.0), (-6.0, 5.0)]),
            stroke,
        )],
        SnapKind::Division => vec![Shape::closed_line(
            poly(&[(0.0, 7.0), (6.0, -5.0), (-6.0, -5.0)]),
            stroke,
        )],
        SnapKind::GuideIntersection => [cross(6.0), vec![ring(3.0)]].concat(),
        SnapKind::Perpendicular => vec![seg((-6.0, 6.0), (6.0, 6.0)), seg((0.0, 6.0), (0.0, -6.0))],
        SnapKind::Tangent => vec![ring(5.0), seg((-7.0, -5.0), (7.0, -5.0))],
        SnapKind::Edge => vec![ring(5.0)],
        SnapKind::Guide => vec![seg((-7.0, 0.0), (7.0, 0.0)), Shape::circle_filled(p, 2.5, color)],
        SnapKind::Grid => plus(5.0),
        SnapKind::Constrained => vec![Shape::convex_polygon(
            poly(&[(0.0, -4.0), (4.0, 0.0), (0.0, 4.0), (-4.0, 0.0)]),
            color,
            Stroke::NONE,
        )],
        SnapKind::Typed => vec![Shape::circle_filled(p, 2.5, color)],
        SnapKind::Free => crosshair(p, color),
    }
}

/// The ticks of the hovered edge's division points (INP-010): a short
/// stroke across the divided stretch at each point, 4 points to either
/// side, `to_screen` mapping model to screen positions.
pub fn division_ticks(hovered: &Hovered, to_screen: impl Fn(DVec2) -> Pos2, color: Color32) -> Vec<Shape> {
    let stroke = Stroke::new(1.0, color);
    (hovered.points.iter())
        .map(|&q| {
            let p = to_screen(q);
            let along = (to_screen(q + hovered.span.direction_at(q)) - p).normalized();
            let across = egui::vec2(-along.y, along.x) * 4.0;
            Shape::line_segment([p - across, p + across], stroke)
        })
        .collect()
}

/// A guide drawn dashed (INP-006, INP-007): its part inside the model
/// rectangle from `min` to `max`, `to_screen` mapping model to screen
/// positions; circles and arcs in pieces within half a pixel of the curve.
pub fn guide_dashes(
    shape: &GuideShape,
    min: DVec2,
    max: DVec2,
    to_screen: impl Fn(DVec2) -> Pos2,
    color: Color32,
) -> Vec<Shape> {
    let pixels_per_metre = f64::from((to_screen(min + DVec2::X) - to_screen(min)).length());
    (shape.clip(min, max, 0.5 / pixels_per_metre).into_iter())
        .flat_map(|polyline| {
            let points: Vec<Pos2> = polyline.into_iter().map(&to_screen).collect();
            Shape::dashed_line(&points, Stroke::new(1.0, color), 6.0, 4.0)
        })
        .collect()
}

/// The crosshair of a free point (INP-008): four arms around a gap.
pub fn crosshair(p: Pos2, color: Color32) -> Vec<Shape> {
    let stroke = Stroke::new(1.0, color);
    [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)]
        .into_iter()
        .map(|(x, y)| {
            let d = egui::vec2(x, y);
            Shape::line_segment([p + d * 3.0, p + d * 10.0], stroke)
        })
        .collect()
}

/// The overlay of a command state beside the snap glyph at `p`, above and
/// to the left of it (INP-008); none for [`CursorState::Normal`].
pub fn cursor_state_glyph(p: Pos2, state: CursorState, color: Color32) -> Vec<Shape> {
    let stroke = Stroke::new(1.5, color);
    let q = p + egui::vec2(-14.0, -14.0);
    let seg = |a: (f32, f32), b: (f32, f32)| {
        Shape::line_segment([q + egui::vec2(a.0, a.1), q + egui::vec2(b.0, b.1)], stroke)
    };
    let plus = |x: f32| vec![seg((x - 3.0, 0.0), (x + 3.0, 0.0)), seg((x, -3.0), (x, 3.0))];
    match state {
        CursorState::Normal => Vec::new(),
        CursorState::Copy => plus(0.0),
        CursorState::MultipleCopy => [plus(-5.0), plus(5.0)].concat(),
        CursorState::TrimTarget => vec![
            Shape::circle_stroke(q + egui::vec2(-3.0, 4.0), 2.0, stroke),
            Shape::circle_stroke(q + egui::vec2(3.0, 4.0), 2.0, stroke),
            seg((-2.0, 2.0), (4.0, -5.0)),
            seg((2.0, 2.0), (-4.0, -5.0)),
        ],
        CursorState::ChooseSide => vec![
            seg((-6.0, 0.0), (6.0, 0.0)),
            seg((-6.0, 0.0), (-3.0, -3.0)),
            seg((-6.0, 0.0), (-3.0, 3.0)),
            seg((6.0, 0.0), (3.0, -3.0)),
            seg((6.0, 0.0), (3.0, 3.0)),
        ],
    }
}

/// The label beside the cursor: the tracker, then the name of the snap kind
/// (INP-002, INP-008); empty for a free point without a tracker.
pub fn cursor_label(tracker: Option<String>, snap: SnapKind) -> String {
    let mut label = tracker.unwrap_or_default();
    if !snap.label().is_empty() {
        if !label.is_empty() {
            label.push('\n');
        }
        label.push_str(snap.label());
    }
    label
}

/// Shows the plan of `storey` and handles its input.
pub fn ui(ui: &mut Ui, ws: &mut Workspace, storey: StoreyId) {
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let mut state = ws.plan_views.remove(&storey).unwrap_or_default();
    state.last_rect = rect;
    let plan = ws.doc.plan(storey);
    if !state.fitted && rect.width() > 10.0 {
        state.fit(rect, &plan);
        state.zoom_history = ZoomHistory::default();
        state.fitted = true;
    }
    let pane = Pane::Plan(storey);
    if response.hovered() && ui.input(|i| i.pointer.any_pressed()) {
        ws.active_pane = Some(pane);
        ws.active_storey = storey;
        response.request_focus();
    }
    // Keep Tab, arrows and Esc in the view while it has focus: they belong to
    // point input (INP-003), not to focus navigation.
    if response.has_focus() {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                response.id,
                egui::EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                },
            );
        });
    }
    let active = ws.active_pane == Some(pane);
    let now = ui.input(|i| i.time);

    // Navigation (VIEW-014, HW-002).
    if response.hovered() {
        let events = ui.input(|i| i.raw.events.clone());
        let pointer = response.hover_pos().unwrap_or(rect.center());
        for event in events {
            match event {
                Event::MouseWheel {
                    unit, delta, modifiers, ..
                } => {
                    state.record(now);
                    if unit == egui::MouseWheelUnit::Point && !modifiers.command && !modifiers.ctrl {
                        state.camera.pan(delta);
                    } else {
                        let notches = if unit == egui::MouseWheelUnit::Point {
                            delta.y / 50.0
                        } else {
                            delta.y
                        };
                        state.camera.zoom_at(rect, pointer, 1.2f64.powf(f64::from(notches)));
                    }
                }
                Event::Zoom(factor) => {
                    state.record(now);
                    state.camera.zoom_at(rect, pointer, f64::from(factor));
                }
                _ => {}
            }
        }
    }
    if response.drag_started_by(PointerButton::Middle) {
        state.zoom_history.record(state.camera);
    }
    if response.dragged_by(PointerButton::Middle) {
        state.camera.pan(response.drag_delta());
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    }

    let camera = state.camera;
    let palette = Palette::new(ui.visuals().dark_mode);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, palette.paper);
    draw_grid(&painter, rect, &camera, &palette);

    let elevation = ws.doc.project.storeys.get(storey).map_or(0.0, |s| s.elevation);
    let surface = PlanSurface {
        camera,
        rect,
        elevation,
    };
    let pointer = response.hover_pos();
    let cursor_world = pointer.and_then(|p| surface.to_plane(screen(p)));
    let pick_tolerance = 4.0 / camera.scale;
    let drawing_tool = ws.takes_points();
    let selecting = !drawing_tool || ui.input(|i| i.modifiers.command);
    let mods = ui.input(|i| modifiers(i.modifiers));
    // Only the view under the cursor or taking the keys needs snap targets.
    state.cursor = cursor_world;
    let input = (drawing_tool && (cursor_world.is_some() || active)).then(|| {
        let click_distance = ui.ctx().options(|o| o.input_options.max_click_dist);
        PlanInput::new(ws, storey, &surface, click_distance)
    });

    // Hover: preselection for the selection tool (SEL-001).
    if let Some(world) = cursor_world {
        if selecting && !ws.tool.is_drawing() {
            let candidates = plan.pick(world, pick_tolerance);
            let index = match state.pick {
                Some((p, i)) if p.distance(world) <= pick_tolerance => i,
                _ => 0,
            };
            ws.hover = candidates.get(index % candidates.len().max(1)).copied();
        } else {
            ws.hover = None;
        }
    } else if response.hovered() || !ui.ui_contains_pointer() {
        ws.hover = None;
    }

    if active && !ui.ctx().text_edit_focused() {
        handle_keys(ui, ws, &mut state, input.as_ref(), cursor_world);
    }

    // Clicks of the selection tool.
    if response.clicked_by(PointerButton::Primary)
        && selecting
        && let Some(world) = cursor_world
    {
        let shift = ui.input(|i| i.modifiers.shift);
        let candidates = plan.pick(world, pick_tolerance);
        let index = match state.pick {
            Some((p, i)) if p.distance(world) <= pick_tolerance => i,
            _ => 0,
        };
        state.pick = Some((world, index));
        match candidates.get(index % candidates.len().max(1)) {
            Some(id) if shift => {
                if !ws.selection.remove(id) {
                    ws.selection.insert(*id);
                }
            }
            Some(id) => {
                ws.selection.clear();
                ws.selection.insert(*id);
            }
            None if !shift => ws.selection.clear(),
            None => {}
        }
    }
    // Points from the primary button in the chosen input mode (INP-020): a
    // press that becomes a drag takes its point where the button went down.
    if !selecting && let Some(input) = &input {
        let at = |p: Option<Pos2>| p.and_then(|p| surface.to_plane(screen(p)));
        let (press_origin, latest) = ui.input(|i| (i.pointer.press_origin(), i.pointer.latest_pos()));
        let events = [
            (
                response.drag_started_by(PointerButton::Primary),
                PointerEvent::Pressed,
                at(press_origin),
            ),
            (
                response.clicked_by(PointerButton::Primary),
                PointerEvent::Clicked,
                cursor_world,
            ),
            (
                response.drag_stopped_by(PointerButton::Primary),
                PointerEvent::Released,
                at(latest).or(cursor_world),
            ),
            // A double-click needs no position.
            (
                response.double_clicked_by(PointerButton::Primary),
                PointerEvent::DoubleClicked,
                Some(cursor_world.unwrap_or_default()),
            ),
        ];
        for (_, event, world) in events.into_iter().filter(|(happened, ..)| *happened) {
            let Some(world) = world else { continue };
            let ctx = input.ctx(&ws.doc.project, ws.frames, &ws.active_consumer, &ws.tool);
            let consumer = ws.active_consumer.get_mut(&mut ws.tool);
            let effect = ws
                .input
                .pointer(event, ws.prefs.input_mode, world, mods, consumer, &ctx);
            apply(ws, effect);
        }
    }

    // Point input: the resolved point and its tracker after this frame's
    // keys and clicks (INP-001, INP-002).
    let resolved: Option<(Resolved, Option<String>)> =
        cursor_world
            .filter(|_| !selecting)
            .zip(input.as_ref())
            .map(|(world, input)| {
                let ctx = input.ctx(&ws.doc.project, ws.frames, &ws.active_consumer, &ws.tool);
                let resolved = ws.input.resolve(world, mods, &ctx);
                let tracker = ws.input.tracker(resolved.point, &ctx);
                (resolved, tracker)
            });
    state.tracker = resolved.as_ref().and_then(|(_, tracker)| tracker.clone());
    state.glyph = None;
    state.label = None;
    state.ticks.clear();
    state.preview.clear();
    state.pending.clear();
    state.guides.clear();

    // Area selection (SEL-003): left to right = fully inside,
    // right to left = touching.
    if selecting && response.drag_started_by(PointerButton::Primary) {
        state.area_start = response.interact_pointer_pos();
    }
    let mut area: Option<(Rect, bool)> = None;
    if let (Some(start), Some(now_pos)) = (state.area_start, response.interact_pointer_pos().or(pointer)) {
        area = Some((Rect::from_two_pos(start, now_pos), now_pos.x >= start.x));
    }
    if response.drag_stopped_by(PointerButton::Primary)
        && let Some((r, fully)) = area.take()
    {
        state.area_start = None;
        let a = camera.to_world(rect, r.left_bottom());
        let b = camera.to_world(rect, r.right_top());
        let shift = ui.input(|i| i.modifiers.shift);
        if !shift {
            ws.selection.clear();
        }
        for element in &plan.elements {
            if element.in_rect(a.min(b), a.max(b), fully) {
                ws.selection.insert(element.id);
            }
        }
    }
    if !response.dragged_by(PointerButton::Primary) {
        state.area_start = None;
    }

    // Drawing.
    draw_plan(&painter, rect, &camera, &plan, ws, &palette);
    if let Some((r, fully)) = area.filter(|(r, _)| r.area() > 4.0) {
        let stroke = Stroke::new(1.0, palette.selection);
        painter.rect_filled(r, 0.0, palette.selection.gamma_multiply(0.08));
        if fully {
            painter.rect_stroke(r, 0.0, stroke, StrokeKind::Middle);
        } else {
            let corners = [
                r.left_top(),
                r.right_top(),
                r.right_bottom(),
                r.left_bottom(),
                r.left_top(),
            ];
            painter.extend(Shape::dashed_line(&corners, stroke, 5.0, 4.0));
        }
    }
    if let (Some((resolved, tracker)), Some(p)) = (resolved, pointer) {
        let target = camera.to_screen(rect, resolved.point);
        let mut preview = ws.consumer().preview(&ws.doc.project, &resolved);
        // Input that started on another storey is previewed only in that
        // storey's plan (UND-003).
        if !on_storey(ws.active_consumer.storey(&ws.tool), storey) {
            preview.outlines.clear();
        }
        for (ring, closed) in &preview.outlines {
            let points: Vec<Pos2> = ring.iter().map(|q| camera.to_screen(rect, *q)).collect();
            let stroke = Stroke::new(1.5, palette.preview);
            if *closed {
                painter.add(Shape::closed_line(points, stroke));
            } else {
                painter.add(Shape::line(points, stroke));
            }
        }
        state.preview = preview.outlines;
        let pending = if ws.active_consumer == Consumer::Tool && on_storey(ws.tool.storey, storey) {
            &ws.tool.points[..]
        } else {
            &[]
        };
        for q in pending {
            painter.circle_filled(camera.to_screen(rect, *q), 3.0, palette.preview);
        }
        state.pending = pending.to_vec();
        let (a, b) = (
            camera.to_world(rect, rect.left_bottom()),
            camera.to_world(rect, rect.right_top()),
        );
        for guide in &resolved.guides {
            let to_screen = |q| camera.to_screen(rect, q);
            painter.extend(guide_dashes(&guide.shape, a.min(b), a.max(b), to_screen, palette.snap));
        }
        state.guides.clone_from(&resolved.guides);
        if let Some(hovered) = &resolved.hovered {
            painter.extend(division_ticks(hovered, |q| camera.to_screen(rect, q), palette.snap));
            state.ticks.clone_from(&hovered.points);
        }
        painter.extend(snap_glyph(target, resolved.snap.kind, palette.snap));
        painter.extend(cursor_state_glyph(target, preview.cursor, palette.snap));
        state.glyph = Some((resolved.snap.kind, preview.cursor));
        let label = cursor_label(tracker, resolved.snap.kind);
        state.label = Some(label.clone());
        if !label.is_empty() {
            let galley_pos = p + egui::vec2(16.0, 14.0);
            let font = FontId::monospace(12.0);
            let rect_text = painter.text(galley_pos, Align2::LEFT_TOP, &label, font.clone(), Color32::TRANSPARENT);
            painter.rect_filled(
                rect_text.expand(3.0),
                3.0,
                ui.visuals().extreme_bg_color.gamma_multiply(0.92),
            );
            painter.text(galley_pos, Align2::LEFT_TOP, label, font, ui.visuals().text_color());
        }
        ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
        #[cfg(feature = "perf")]
        if ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
            ws.perf.pointer_feedback();
        }
    }
    if let (Some(id), Some(p)) = (ws.hover, pointer)
        && let Some(e) = ws.doc.project.element(id)
    {
        let text = format!("{} · {}", e.name, crate::tr!(e.tag().label_key()));
        painter.text(
            p + egui::vec2(14.0, -14.0),
            Align2::LEFT_BOTTOM,
            text,
            FontId::proportional(12.0),
            palette.hover,
        );
    }

    response.context_menu(|ui| context_menu(ui, ws));
    ws.plan_views.insert(storey, state);
}

/// Runs what a key, click or double-click asks for. Esc that found nothing
/// to cancel goes on down the ladder: a transient consumer, the selection,
/// then the drawing tool (INP-003). A transient consumer that is done hands
/// over its result (INP-014).
fn apply(ws: &mut Workspace, effect: Effect) {
    match effect {
        Effect::NothingToCancel if ws.active_consumer != Consumer::Tool => ws.end_consumer(),
        Effect::None => {}
        Effect::Execute(command) => {
            ws.execute(command);
        }
        Effect::Error(message) => ws.error(message),
        Effect::NothingToCancel if !ws.selection.is_empty() => ws.selection.clear(),
        Effect::NothingToCancel => ws.select_tool(ToolKind::Select),
    }
    ws.settle_consumer();
}

/// Translates the frame's key events into point input (INP-003) and handles
/// the plan's own keys: Tab cycling of the selection tool and R. Tab goes to
/// point input while a drawing tool takes typed values or has snap
/// candidates to cycle through (INP-009).
fn handle_keys(
    ui: &Ui,
    ws: &mut Workspace,
    state: &mut PlanViewState,
    input: Option<&PlanInput>,
    cursor_world: Option<DVec2>,
) {
    let events = ui.input(|i| i.events.clone());
    for event in events {
        let key = match event {
            Event::Text(text) if !ui.input(|i| i.modifiers.command) => KeyInput::Text(text),
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => match key {
                Key::Tab if ws.input.captures_keys(ws.consumer()) || (ws.takes_points() && ws.input.cycles()) => {
                    KeyInput::Tab
                }
                Key::Tab => {
                    // Cycle the highlight through overlapping candidates
                    // (SEL-001); the next click selects it.
                    if let Some(world) = cursor_world {
                        state.pick = Some(match state.pick {
                            Some((p, i)) => (p, i + 1),
                            None => (world, 1),
                        });
                    }
                    continue;
                }
                // INP-004: provisional keys; Problems is Ctrl+Shift+P.
                Key::P if !modifiers.command && ws.consumer().step().is_some() => {
                    if modifiers.shift {
                        KeyInput::SwitchReference
                    } else {
                        KeyInput::SwitchFields
                    }
                }
                Key::Enter => KeyInput::Enter,
                Key::Backspace => KeyInput::Backspace,
                Key::Escape => KeyInput::Escape,
                Key::R
                    if ws.active_consumer == Consumer::Tool
                        && ws.tool.kind == ToolKind::Wall
                        && ws.input.typed.is_empty()
                        && !modifiers.command =>
                {
                    // Cycle the reference-line location (WAL-007).
                    let mut defaults = ws.doc.project.tool_defaults.clone();
                    let Some(ElementKind::Wall(wall)) = defaults.get_mut(KindTag::Wall).map(|slot| &mut slot.kind)
                    else {
                        continue;
                    };
                    wall.reference_line = wall.reference_line.next();
                    let label = tr!(wall.reference_line.label_key());
                    if ws.execute(archicraft_model::Command::SetToolDefaults(defaults)) {
                        ws.info(format!("Wall reference line: {label}"));
                    }
                    continue;
                }
                _ => continue,
            },
            _ => continue,
        };
        let effect = match input {
            Some(input) => {
                let ctx = input.ctx(&ws.doc.project, ws.frames, &ws.active_consumer, &ws.tool);
                ws.input.key(key, ws.active_consumer.get_mut(&mut ws.tool), &ctx)
            }
            None if key == KeyInput::Escape => Effect::NothingToCancel,
            None => Effect::None,
        };
        apply(ws, effect);
    }
}

/// The plan's context menu. The user origin items come first, above the
/// input and selection items; while a transient consumer takes points, the
/// hovered element is not selected (INP-014).
fn context_menu(ui: &mut Ui, ws: &mut Workspace) {
    if ui.button("Place user origin").clicked() {
        ws.place_user_origin();
        ui.close();
    }
    if ui
        .add_enabled(
            ws.frames.user_origin.is_placed(),
            egui::Button::new("Reset user origin"),
        )
        .clicked()
    {
        ws.reset_user_origin();
        ui.close();
    }
    ui.separator();
    if ws.active_consumer != Consumer::Tool {
        if ui.button("Cancel input").clicked() {
            ws.end_consumer();
            ui.close();
        }
        return;
    }
    if ws.tool.is_drawing() {
        if ui.button("Finish").clicked() {
            ws.input = PointInput::default();
            let outcome = ws.tool.finish(&ws.doc.project);
            if let Outcome::Execute(command) = outcome {
                ws.execute(command);
            }
            ui.close();
        }
        if ui.button("Cancel input").clicked() {
            ws.reset_input();
            ui.close();
        }
        ui.separator();
    }
    if let Some(id) = ws.hover
        && !ws.selection.contains(&id)
    {
        ws.selection.clear();
        ws.selection.insert(id);
    }
    let has_selection = !ws.selection.is_empty();
    if ui.add_enabled(has_selection, egui::Button::new("Delete")).clicked() {
        ws.delete_selection();
        ui.close();
    }
    if ui.add_enabled(has_selection, egui::Button::new("Settings…")).clicked() {
        ws.requests.push(Request::Settings);
        ui.close();
    }
    if ui
        .add_enabled(has_selection, egui::Button::new("Zoom to selection"))
        .clicked()
    {
        ws.requests.push(Request::ZoomTo(ws.selected_ids()));
        ui.close();
    }
    ui.separator();
    if ui.button("Select all").clicked() {
        ws.select_all();
        ui.close();
    }
    if ui.button("Fit to window").clicked() {
        ws.requests.push(Request::ZoomTo(Vec::new()));
        ui.close();
    }
    if let Some(view) = ws.doc.project.views.first().map(|v| v.id)
        && ui.button("Show in 3D").clicked()
    {
        ws.requests.push(Request::OpenPane(Pane::View3d(view)));
        ui.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// INP-001: the plan view's surface maps screen and plane points as its
    /// camera does, on the plane at the storey elevation.
    #[test]
    fn plan_surface_agrees_with_the_camera() {
        let surface = PlanSurface {
            camera: Camera2d {
                center: DVec2::new(3.5, -2.0),
                scale: 64.0,
            },
            rect: Rect::from_min_max(Pos2::new(10.0, 20.0), Pos2::new(810.0, 620.0)),
            elevation: 2.8,
        };
        for p in [Pos2::new(10.0, 20.0), Pos2::new(123.5, 456.25), Pos2::new(810.0, 620.0)] {
            let plane = surface.to_plane(screen(p)).unwrap();
            assert_eq!(plane, surface.camera.to_world(surface.rect, p));
            assert_eq!(
                surface.to_screen(plane),
                screen(surface.camera.to_screen(surface.rect, plane))
            );
            assert!(surface.to_screen(plane).distance(screen(p)) < 1e-3);
        }
        assert_eq!(surface.model_per_pixel(), 1.0 / 64.0);
        assert_eq!(surface.frame(), SurfaceFrame::plan(2.8));
        assert_eq!(surface.frame().to_model(DVec2::new(1.0, 2.0)).z, 2.8);
    }

    /// INP-008: the snap radius is the preference in pixels times the model
    /// length per pixel; at 50 pixels per metre a node 10 pixels from the
    /// cursor is no candidate with the default 8 pixels and is one with 16.
    #[test]
    fn the_snap_radius_follows_the_preference_in_pixels() {
        use archicraft_model::Command;
        use archicraft_model::template::default_metric_project;

        use crate::document::Document;
        use crate::persistence::prefs::Preferences;

        let project = default_metric_project();
        let storey = project.storeys.ground();
        let end = DVec2::new(2.0, 0.0);
        let wall = project.new_wall(storey, DVec2::ZERO, end, 0);
        let mut ws = Workspace::new(Document::new(project, 10), Preferences::default());
        assert!(ws.execute(Command::AddElements(vec![wall])));
        ws.select_tool(ToolKind::Wall);
        let surface = PlanSurface {
            camera: Camera2d {
                center: DVec2::ZERO,
                scale: 50.0,
            },
            rect: Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0)),
            elevation: 0.0,
        };
        let cursor = end + DVec2::new(10.0 / 50.0, 0.0);
        assert_eq!(ws.prefs.snap_radius, 8.0);
        for (pixels, found) in [(8.0, false), (16.0, true)] {
            ws.prefs.snap_radius = pixels;
            let input = PlanInput::new(&mut ws, storey, &surface, 6.0);
            let ctx = input.ctx(&ws.doc.project, ws.frames, &ws.active_consumer, &ws.tool);
            assert_eq!(ctx.radius, f64::from(pixels) * surface.model_per_pixel());
            let resolved = ws.input.resolve(cursor, Modifiers::default(), &ctx);
            let node = resolved
                .candidates
                .iter()
                .any(|s| s.kind == SnapKind::Node && s.point == end);
            assert_eq!(node, found, "{pixels} px: {:?}", resolved.candidates);
        }
    }

    /// INP-006: a guide is drawn dashed, along its part inside the view:
    /// a constraint ray from its origin to the view's edge.
    #[test]
    fn guides_are_drawn_dashed_inside_the_view() {
        let ray = GuideShape::ray(DVec2::ZERO, DVec2::X);
        let to_screen = |q: DVec2| Pos2::new(100.0 + 10.0 * q.x as f32, 100.0 - 10.0 * q.y as f32);
        let (min, max) = (DVec2::new(-5.0, -2.0), DVec2::new(4.0, 2.0));
        let dashes = guide_dashes(&ray, min, max, to_screen, Color32::RED);
        assert!(dashes.len() > 1, "dashed, not one line: {dashes:?}");
        let mut xs = Vec::new();
        for dash in &dashes {
            let Shape::LineSegment { points, .. } = dash else {
                panic!("{dash:?} is no line");
            };
            for p in points {
                assert!((p.y - 100.0).abs() < 1e-4, "{p:?} is off the ray");
                xs.push(p.x);
            }
        }
        let (first, last) = xs.iter().fold((f32::MAX, f32::MIN), |(a, b), x| (a.min(*x), b.max(*x)));
        assert!((first - 100.0).abs() < 1e-3, "starts at the origin: {first}");
        assert!(last <= 140.0 + 1e-3 && last > 130.0, "ends at the view's edge: {last}");
        assert!(
            guide_dashes(
                &ray,
                DVec2::new(-5.0, 1.0),
                DVec2::new(4.0, 2.0),
                to_screen,
                Color32::RED
            )
            .is_empty()
        );
    }

    /// INP-024, INP-009: only area elements offer centroids, so on the
    /// 11.5 cm interior wall of two_storey_shell, at fit-to-window zoom in a
    /// 1000 by 700 pixel plan with the default 8 pixel radius, hovering the
    /// middle of its east face snaps to the face midpoint, although the
    /// wall's cut centroid would lie within the radius.
    #[test]
    fn a_thin_walls_face_midpoint_is_not_hidden_by_its_centroid() {
        use archicraft_geometry::geo::Centroid;
        use archicraft_geometry::plan::polygon_rings;
        use archicraft_model::samples;

        use crate::document::Document;
        use crate::persistence::prefs::Preferences;

        let project = samples::two_storey_shell();
        let storey = project.storeys.ground();
        let mut ws = Workspace::new(Document::new(project, 10), Preferences::default());
        ws.select_tool(ToolKind::Wall);
        let plan = ws.doc.plan(storey);
        let (min, max) = plan.bounds().unwrap();
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 700.0));
        let mut camera = Camera2d::default();
        camera.fit(rect, min, max);
        let surface = PlanSurface {
            camera,
            rect,
            elevation: 0.0,
        };

        let interior = (ws.doc.project.elements_on(storey))
            .find(|e| {
                e.as_wall()
                    .is_some_and(|w| w.start == DVec2::new(5.0, 0.0) && w.end == DVec2::new(5.0, 6.0))
            })
            .unwrap()
            .id;
        let cut = &plan.element(interior).unwrap().cut[0];
        let ring = polygon_rings(cut).remove(0);
        let east = |p: &DVec2| (p.x - 5.0575).abs() < 1e-9;
        let face = (0..ring.len())
            .map(|i| [ring[i], ring[(i + 1) % ring.len()]])
            .filter(|[a, b]| east(a) && east(b))
            .max_by(|[a, b], [c, d]| a.distance(*b).total_cmp(&c.distance(*d)))
            .unwrap();
        let midpoint = (face[0] + face[1]) / 2.0;
        let centroid = cut.centroid().unwrap();

        assert_eq!(ws.prefs.snap_radius, 8.0);
        let input = PlanInput::new(&mut ws, storey, &surface, 6.0);
        let ctx = input.ctx(&ws.doc.project, ws.frames, &ws.active_consumer, &ws.tool);
        let wall_centroid = DVec2::new(centroid.x(), centroid.y());
        assert!(midpoint.distance(wall_centroid) < ctx.radius, "{} m", ctx.radius);
        let resolved = ws.input.resolve(midpoint, Modifiers::default(), &ctx);
        let found = &resolved.candidates;
        assert!(found.iter().all(|s| s.kind != SnapKind::Centroid), "{found:?}");
        assert_eq!(resolved.snap.kind, SnapKind::Midpoint, "{found:?}");
        assert!(resolved.snap.point.distance(midpoint) < 1e-9, "{resolved:?}");
    }

    /// INP-010: the hovered edge's division points are drawn as ticks
    /// across it, one per point: three for four equal parts.
    #[test]
    fn division_points_are_drawn_as_ticks_across_the_edge() {
        let line = archicraft_geometry::plan::PlanElement {
            id: ElementId::new(),
            cut: Vec::new(),
            projected: vec![[DVec2::ZERO, DVec2::new(4.0, 0.0)]],
            overhead: Vec::new(),
            reference_line: None,
        };
        let mut index = SnapIndex::default();
        index.sync(0, &[&line]);
        let settings = InputSettings {
            division: crate::input::DivisionMode::Equal(4),
            ..InputSettings::default()
        };
        let hovered = index.hover(DVec2::new(1.3, 0.05), 0.2, None, settings).unwrap();
        let to_screen = |q: DVec2| Pos2::new(100.0 + 10.0 * q.x as f32, 100.0 - 10.0 * q.y as f32);
        let ticks = division_ticks(&hovered, to_screen, Color32::RED);
        assert_eq!(ticks.len(), 3);
        for (tick, x) in ticks.iter().zip([110.0, 120.0, 130.0]) {
            let Shape::LineSegment { points, .. } = tick else {
                panic!("{tick:?} is no line");
            };
            let expected = [Pos2::new(x, 96.0), Pos2::new(x, 104.0)];
            assert!(
                *points == expected || *points == [expected[1], expected[0]],
                "{points:?} is not across the edge at x = {x}"
            );
        }
    }
}
