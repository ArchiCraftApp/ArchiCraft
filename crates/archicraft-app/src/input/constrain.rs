//! The angle constraint of point input (INP-006).

use archicraft_model::glam::DVec2;

/// Restricts a point to the nearest multiple of `increment_deg` around
/// `from` (INP-006), keeping its distance along that direction.
pub fn constrain(from: DVec2, to: DVec2, increment_deg: f64) -> DVec2 {
    let d = to - from;
    if d.length() < 1e-12 {
        return to;
    }
    let step = increment_deg.to_radians();
    let angle = (d.y.atan2(d.x) / step).round() * step;
    let dir = DVec2::new(angle.cos(), angle.sin());
    from + dir * d.dot(dir).max(0.0)
}
