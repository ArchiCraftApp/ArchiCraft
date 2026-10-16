//! View cameras: plan (2D) and 3D (VIEW-014, VIEW-020, VIEW-021).
//!
//! Model coordinates are metres in `f64` with +Z up and +Y north. Screen
//! coordinates are egui points with +Y down.

use archicraft_model::glam::{self, DMat4, DVec2, DVec3, DVec4};
use archicraft_model::project::Projection;
use egui::{Pos2, Rect, Vec2};

/// The camera of a plan view: what model point is at the centre of the view
/// and how many screen points one metre takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera2d {
    pub center: DVec2,
    pub scale: f64,
}

impl Default for Camera2d {
    fn default() -> Self {
        Self {
            center: DVec2::new(5.0, 4.0),
            scale: 40.0,
        }
    }
}

pub const MIN_SCALE: f64 = 0.05;
pub const MAX_SCALE: f64 = 100_000.0;

impl Camera2d {
    pub fn to_screen(&self, rect: Rect, p: DVec2) -> Pos2 {
        let d = (p - self.center) * self.scale;
        Pos2::new(rect.center().x + d.x as f32, rect.center().y - d.y as f32)
    }

    pub fn to_world(&self, rect: Rect, p: Pos2) -> DVec2 {
        let dx = f64::from(p.x - rect.center().x) / self.scale;
        let dy = f64::from(rect.center().y - p.y) / self.scale;
        self.center + DVec2::new(dx, dy)
    }

    /// Moves the view by a screen-space drag.
    pub fn pan(&mut self, delta: Vec2) {
        self.center -= DVec2::new(f64::from(delta.x), -f64::from(delta.y)) / self.scale;
    }

    /// Zooms by `factor` keeping the model point under `anchor` in place.
    pub fn zoom_at(&mut self, rect: Rect, anchor: Pos2, factor: f64) {
        let before = self.to_world(rect, anchor);
        self.scale = (self.scale * factor).clamp(MIN_SCALE, MAX_SCALE);
        let after = self.to_world(rect, anchor);
        self.center += before - after;
    }

    /// Shows the box `min`–`max` with a margin (fit to window).
    pub fn fit(&mut self, rect: Rect, min: DVec2, max: DVec2) {
        let size = (max - min).max(DVec2::splat(1.0));
        let margin = 0.9;
        let sx = f64::from(rect.width()) / size.x;
        let sy = f64::from(rect.height()) / size.y;
        self.scale = (sx.min(sy) * margin).clamp(MIN_SCALE, MAX_SCALE);
        self.center = (min + max) / 2.0;
    }
}

/// Previous/next zoom (VIEW-014): at least 20 stored steps.
#[derive(Clone, Debug, Default)]
pub struct ZoomHistory {
    back: Vec<Camera2d>,
    forward: Vec<Camera2d>,
}

const ZOOM_STEPS: usize = 50;

impl ZoomHistory {
    /// Records the camera before a navigation step.
    pub fn record(&mut self, camera: Camera2d) {
        if self.back.last() != Some(&camera) {
            self.back.push(camera);
            if self.back.len() > ZOOM_STEPS {
                self.back.remove(0);
            }
        }
        self.forward.clear();
    }

    pub fn previous(&mut self, current: Camera2d) -> Option<Camera2d> {
        let camera = self.back.pop()?;
        self.forward.push(current);
        Some(camera)
    }

    pub fn next(&mut self, current: Camera2d) -> Option<Camera2d> {
        let camera = self.forward.pop()?;
        self.back.push(current);
        Some(camera)
    }
}

/// An orbiting 3D camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera3d {
    /// The point the camera looks at and orbits around.
    pub target: DVec3,
    pub distance: f64,
    /// Angle around +Z, measured from +X, in radians.
    pub yaw: f64,
    /// Angle above the horizon, in radians.
    pub pitch: f64,
    /// Vertical field of view of the perspective projection, in radians.
    pub fov_y: f64,
    pub projection: Projection,
}

/// Parallel-projection presets (VIEW-020).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewPreset {
    Top,
    Front,
    Side,
    Isometric,
    Dimetric,
}

impl ViewPreset {
    pub const ALL: [ViewPreset; 5] = [
        ViewPreset::Top,
        ViewPreset::Front,
        ViewPreset::Side,
        ViewPreset::Isometric,
        ViewPreset::Dimetric,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ViewPreset::Top => "Top",
            ViewPreset::Front => "Front",
            ViewPreset::Side => "Side",
            ViewPreset::Isometric => "Isometric",
            ViewPreset::Dimetric => "Dimetric",
        }
    }
}

const MAX_PITCH: f64 = 89.9_f64 * std::f64::consts::PI / 180.0;

impl Camera3d {
    pub fn new(projection: Projection) -> Self {
        Self {
            target: DVec3::new(5.0, 4.0, 1.5),
            distance: 30.0,
            yaw: (-60.0_f64).to_radians(),
            pitch: 30.0_f64.to_radians(),
            fov_y: 45.0_f64.to_radians(),
            projection,
        }
    }

    /// Unit vector from the target towards the eye.
    pub fn back(&self) -> DVec3 {
        DVec3::new(
            self.pitch.cos() * self.yaw.cos(),
            self.pitch.cos() * self.yaw.sin(),
            self.pitch.sin(),
        )
    }

    pub fn eye(&self) -> DVec3 {
        self.target + self.back() * self.distance
    }

    fn right(&self) -> DVec3 {
        DVec3::Z.cross(self.back()).normalize_or(DVec3::X)
    }

    fn up(&self) -> DVec3 {
        self.back().cross(self.right())
    }

    /// Model units per screen point at the target distance.
    fn units_per_point(&self, viewport_height: f32) -> f64 {
        2.0 * self.distance * (self.fov_y / 2.0).tan() / f64::from(viewport_height.max(1.0))
    }

    pub fn orbit(&mut self, delta: Vec2) {
        self.yaw -= f64::from(delta.x) * 0.008;
        self.pitch = (self.pitch + f64::from(delta.y) * 0.008).clamp(-MAX_PITCH, MAX_PITCH);
    }

    pub fn pan(&mut self, delta: Vec2, viewport_height: f32) {
        let k = self.units_per_point(viewport_height);
        self.target += (-self.right() * f64::from(delta.x) + self.up() * f64::from(delta.y)) * k;
    }

    /// Zooms by `factor` (< 1 moves closer) towards the model point under the
    /// cursor, at the depth of the target.
    pub fn zoom_at(&mut self, rect: Rect, cursor: Pos2, factor: f64) {
        let (origin, dir) = self.ray(rect, cursor);
        let normal = self.back();
        let denom = dir.dot(normal);
        let anchor = if denom.abs() > 1e-9 {
            origin + dir * ((self.target - origin).dot(normal) / denom)
        } else {
            self.target
        };
        let factor = factor.clamp(0.2, 5.0);
        self.distance = (self.distance * factor).clamp(0.05, 1e6);
        self.target = anchor + (self.target - anchor) * factor;
    }

    /// Shows a bounding box (fit to window).
    pub fn fit(&mut self, min: DVec3, max: DVec3) {
        self.target = (min + max) / 2.0;
        let radius = ((max - min).length() / 2.0).max(0.5);
        self.distance = radius / (self.fov_y / 2.0).sin() * 1.05;
    }

    pub fn apply_preset(&mut self, preset: ViewPreset) {
        let (yaw, pitch) = match preset {
            ViewPreset::Top => (-90.0, 89.9),
            ViewPreset::Front => (-90.0, 0.0),
            ViewPreset::Side => (0.0, 0.0),
            ViewPreset::Isometric => (-45.0, 35.264),
            ViewPreset::Dimetric => (-60.0, 20.705),
        };
        self.yaw = f64::to_radians(yaw);
        self.pitch = f64::to_radians(pitch);
        self.projection = Projection::Parallel;
    }

    /// View matrix with the model translated by `-origin`, so that the GPU's
    /// `f32` positions stay small (technology choices: local origin).
    pub fn view_matrix(&self, origin: DVec3) -> DMat4 {
        let up = if self.pitch.abs() > 89.0_f64.to_radians() {
            self.up()
        } else {
            DVec3::Z
        };
        glam::dcamera::rh::view::look_at_mat4(self.eye() - origin, self.target - origin, up)
    }

    /// Projection matrix for a viewport aspect ratio (width / height), with
    /// depth in 0…1 as wgpu expects.
    pub fn projection_matrix(&self, aspect: f64, scene_radius: f64) -> DMat4 {
        let aspect = aspect.max(1e-3);
        match self.projection {
            Projection::Perspective => {
                let near = (self.distance * 0.002).max(0.01);
                let far = (self.distance + scene_radius * 2.0).max(near * 10.0) * 2.0;
                glam::dcamera::rh::proj::directx::perspective(self.fov_y, aspect, near, far)
            }
            Projection::Parallel => {
                let half_h = self.distance * (self.fov_y / 2.0).tan();
                let half_w = half_h * aspect;
                let depth = self.distance + scene_radius * 2.0 + 1.0;
                glam::dcamera::rh::proj::directx::orthographic(-half_w, half_w, -half_h, half_h, -depth, depth)
            }
        }
    }

    /// The pick ray through a screen point: origin and unit direction.
    pub fn ray(&self, rect: Rect, p: Pos2) -> (DVec3, DVec3) {
        let ndc_x = f64::from((p.x - rect.left()) / rect.width().max(1.0)) * 2.0 - 1.0;
        let ndc_y = 1.0 - f64::from((p.y - rect.top()) / rect.height().max(1.0)) * 2.0;
        let aspect = f64::from(rect.width() / rect.height().max(1.0));
        let inverse = (self.projection_matrix(aspect, self.distance) * self.view_matrix(DVec3::ZERO)).inverse();
        let unproject = |z: f64| {
            let v = inverse * DVec4::new(ndc_x, ndc_y, z, 1.0);
            v.truncate() / v.w
        };
        let near = unproject(0.0);
        let far = unproject(1.0);
        (near, (far - near).normalize_or(-self.back()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))
    }

    #[test]
    fn plan_round_trip_and_zoom_anchor() {
        let mut c = Camera2d::default();
        let p = DVec2::new(3.0, -2.0);
        let s = c.to_screen(rect(), p);
        assert!(c.to_world(rect(), s).distance(p) < 1e-4);
        let anchor = Pos2::new(100.0, 200.0);
        let before = c.to_world(rect(), anchor);
        c.zoom_at(rect(), anchor, 2.5);
        assert!(c.to_world(rect(), anchor).distance(before) < 1e-6);
    }

    #[test]
    fn plan_fit_contains_box() {
        let mut c = Camera2d::default();
        c.fit(rect(), DVec2::new(-10.0, 0.0), DVec2::new(30.0, 5.0));
        let a = c.to_screen(rect(), DVec2::new(-10.0, 0.0));
        let b = c.to_screen(rect(), DVec2::new(30.0, 5.0));
        assert!(rect().contains(a) && rect().contains(b));
    }

    #[test]
    fn zoom_history() {
        let mut h = ZoomHistory::default();
        let a = Camera2d::default();
        let b = Camera2d { scale: 80.0, ..a };
        h.record(a);
        assert_eq!(h.previous(b), Some(a));
        assert_eq!(h.next(a), Some(b));
    }

    #[test]
    fn center_ray_hits_target() {
        for projection in [Projection::Perspective, Projection::Parallel] {
            let c = Camera3d::new(projection);
            let (origin, dir) = c.ray(rect(), rect().center());
            let to_target = c.target - origin;
            let distance_from_ray = (to_target - dir * to_target.dot(dir)).length();
            assert!(distance_from_ray < 1e-6, "{projection:?}: {distance_from_ray}");
        }
    }

    #[test]
    fn zoom_keeps_point_under_cursor() {
        let mut c = Camera3d::new(Projection::Perspective);
        let cursor = Pos2::new(600.0, 150.0);
        let (o1, d1) = c.ray(rect(), cursor);
        c.zoom_at(rect(), cursor, 0.5);
        let (o2, d2) = c.ray(rect(), cursor);
        // Both rays pass through the same anchor point on the target plane.
        let normal = c.back();
        let hit = |o: DVec3, d: DVec3, plane_point: DVec3| o + d * ((plane_point - o).dot(normal) / d.dot(normal));
        let p1 = hit(o1, d1, c.target);
        let p2 = hit(o2, d2, c.target);
        assert!(p1.distance(p2) < 1e-6);
    }
}
