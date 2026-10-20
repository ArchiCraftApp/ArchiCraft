//! Plan transformations of elements (EDT-001…EDT-003).
//!
//! A [`PlanTransform`] is a rigid motion in plan, optionally with a mirror:
//! `p' = R·p + t` with an orthonormal `R`. Every element kind applies it to
//! its own parameters through [`crate::kinds::Kind::transform`], and refuses
//! with [`TransformError`] when its parameters cannot represent the result.

use glam::{DMat2, DVec2};

/// A kind cannot represent a transformed element. Commands turn it into a
/// message naming the element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransformError {
    /// The element's kind does not support the transformation, for example
    /// a mirror.
    Unsupported,
}

/// A translation, rotation or mirror in plan (EDT-001…EDT-003).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlanTransform {
    /// Orthonormal linear part: the identity, a rotation, or a rotation and
    /// a mirror.
    linear: DMat2,
    translation: DVec2,
}

impl PlanTransform {
    /// A translation by `delta` (EDT-001).
    pub fn translation(delta: DVec2) -> Self {
        Self {
            linear: DMat2::IDENTITY,
            translation: delta,
        }
    }

    /// A counter-clockwise rotation by `angle` radians about `center`
    /// (EDT-002).
    pub fn rotation(center: DVec2, angle: f64) -> Self {
        let (sin, cos) = angle.sin_cos();
        let linear = DMat2::from_cols(DVec2::new(cos, sin), DVec2::new(-sin, cos));
        Self {
            linear,
            translation: center - linear * center,
        }
    }

    /// A mirror about the line through `a` and `b` (EDT-003). `None` if the
    /// two points coincide.
    pub fn mirror(a: DVec2, b: DVec2) -> Option<Self> {
        let d = (b - a).try_normalize()?;
        // Reflection matrix 2·d·dᵀ − I.
        let linear = DMat2::from_cols(
            DVec2::new(2.0 * d.x * d.x - 1.0, 2.0 * d.x * d.y),
            DVec2::new(2.0 * d.x * d.y, 2.0 * d.y * d.y - 1.0),
        );
        Some(Self {
            linear,
            translation: a - linear * a,
        })
    }

    /// The transformed point.
    pub fn apply(&self, p: DVec2) -> DVec2 {
        if self.is_translation() {
            // As exact as a plain addition, so moves do not drift (EDT-001).
            return p + self.translation;
        }
        self.linear * p + self.translation
    }

    /// True if the transformation only translates.
    pub fn is_translation(&self) -> bool {
        self.linear == DMat2::IDENTITY
    }

    /// True if the transformation mirrors, which reverses the orientation of
    /// polygons and swaps the sides of lines.
    pub fn is_mirror(&self) -> bool {
        self.linear.determinant() < 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_reflect_about_the_line() {
        let t = PlanTransform::mirror(DVec2::new(1.0, 0.0), DVec2::new(1.0, 5.0)).unwrap();
        assert_eq!(t.apply(DVec2::new(3.0, 2.0)), DVec2::new(-1.0, 2.0));
        assert!(t.is_mirror() && !t.is_translation());
    }

    #[test]
    fn translations_add_exactly() {
        let t = PlanTransform::translation(DVec2::new(0.1, 0.2));
        assert!(t.is_translation() && !t.is_mirror());
        assert_eq!(t.apply(DVec2::new(0.7, 0.0)), DVec2::new(0.7 + 0.1, 0.2));
    }
}
