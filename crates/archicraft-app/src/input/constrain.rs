//! The angle constraint of point input (INP-006).

use archicraft_model::glam::DVec2;
use archicraft_model::tolerance::coincident;
use serde::{Deserialize, Serialize};

/// What the directions of the angle constraint are measured from (INP-006):
/// the model X axis or the orientation of the input frame, the construction
/// grid (GRD-001).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintReference {
    #[default]
    XAxis,
    Frame,
}

impl ConstraintReference {
    /// The angle in radians the allowed directions start from, given the
    /// rotation of the input frame.
    pub fn rotation(self, frame_rotation: f64) -> f64 {
        match self {
            ConstraintReference::XAxis => 0.0,
            ConstraintReference::Frame => frame_rotation,
        }
    }
}

/// Restricts a point to the nearest allowed direction around `from`
/// (INP-006): `rotation` plus a multiple of `increment_deg`, keeping the
/// point's distance along that direction. Returns the point and the chosen
/// unit direction, which point input turns into the constraint guide. A
/// point that coincides with `from` (NFR-ACC-003) has no direction and
/// stays.
pub fn constrain(from: DVec2, to: DVec2, increment_deg: f64, rotation: f64) -> (DVec2, Option<DVec2>) {
    if coincident(from, to) {
        return (to, None);
    }
    let d = to - from;
    let step = increment_deg.to_radians();
    let angle = rotation + ((d.y.atan2(d.x) - rotation) / step).round() * step;
    let dir = DVec2::from_angle(angle);
    (from + dir * d.dot(dir).max(0.0), Some(dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn direction_deg(to: DVec2, rotation_deg: f64) -> f64 {
        let (_, dir) = constrain(DVec2::ZERO, to, 45.0, rotation_deg.to_radians());
        dir.unwrap().to_angle().to_degrees().rem_euclid(360.0)
    }

    /// INP-006: from the X axis the allowed directions are the multiples of
    /// the increment; the point keeps its distance along the direction.
    #[test]
    fn directions_are_multiples_of_the_increment_from_the_x_axis() {
        for (deg, expected) in [(10.0, 0.0), (30.0, 45.0), (80.0, 90.0), (200.0, 180.0), (-50.0, 315.0)] {
            let to = DVec2::from_angle(f64::to_radians(deg)) * 2.0;
            assert!((direction_deg(to, 0.0) - expected).abs() < 1e-9, "{deg}° → {expected}°");
        }
        let (point, _) = constrain(DVec2::new(1.0, 1.0), DVec2::new(4.0, 1.4), 45.0, 0.0);
        assert!(point.distance(DVec2::new(4.0, 1.0)) < 1e-12);
    }

    /// INP-006: with the frame rotated by 30° the allowed directions are
    /// 30°, 75°, 120° and so on.
    #[test]
    fn directions_start_at_the_frame_rotation() {
        for (deg, expected) in [(28.0, 30.0), (70.0, 75.0), (125.0, 120.0), (5.0, 345.0), (215.0, 210.0)] {
            let to = DVec2::from_angle(f64::to_radians(deg)) * 3.0;
            assert!(
                (direction_deg(to, 30.0) - expected).abs() < 1e-9,
                "{deg}° → {expected}°"
            );
        }
        assert_eq!(ConstraintReference::XAxis.rotation(0.5), 0.0);
        assert_eq!(ConstraintReference::Frame.rotation(0.5), 0.5);
    }

    /// NFR-ACC-003: a point on the start point has no direction.
    #[test]
    fn a_point_on_the_start_has_no_direction() {
        let from = DVec2::new(1.0, 2.0);
        assert_eq!(constrain(from, from, 45.0, 0.0), (from, None));
    }
}
