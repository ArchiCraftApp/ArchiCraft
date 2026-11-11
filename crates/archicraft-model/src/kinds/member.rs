//! The cross-section and anchor of members: columns now, beams later
//! (COL-001, BEA-001).
//!
//! A member is swept from its cross-section along its axis. The section is
//! drawn in the profile plane (x to the right, y up, metres; see
//! [`crate::profile`]), centred on its bounding box; the member's axis runs
//! through one of nine points of that box, its [`Anchor`].

use glam::DVec2;
use serde::{Deserialize, Serialize};

/// The cross-section of a member (COL-001): a rectangle `width` × `height`
/// or a circle of `diameter`, centred on its bounding box. `height` is the
/// section's size along y, which is a column's depth in plan.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum CrossSection {
    Rectangular { width: f64, height: f64 },
    Circular { diameter: f64 },
}

impl CrossSection {
    /// The size of the bounding box: width along x, height along y (a
    /// circle's diameter in both).
    pub fn size(&self) -> DVec2 {
        match *self {
            CrossSection::Rectangular { width, height } => DVec2::new(width, height),
            CrossSection::Circular { diameter } => DVec2::splat(diameter),
        }
    }

    /// The bounding box as (min, max), centred on zero.
    pub fn bounds(&self) -> (DVec2, DVec2) {
        let half = self.size() / 2.0;
        (-half, half)
    }

    /// The section's dimensions with their values, for range checks.
    pub(crate) fn dimensions(&self) -> Vec<f64> {
        match *self {
            CrossSection::Rectangular { width, height } => vec![width, height],
            CrossSection::Circular { diameter } => vec![diameter],
        }
    }
}

/// The point of a member's cross-section its axis runs through (COL-001):
/// a corner, an edge midpoint or the centre of the section's bounding box.
/// For a circle these are the points of its bounding box, so
/// [`Anchor::Centre`] is the circle's centre.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Anchor {
    TopLeft,
    TopCentre,
    TopRight,
    Left,
    #[default]
    Centre,
    Right,
    BottomLeft,
    BottomCentre,
    BottomRight,
}

impl Anchor {
    /// The nine anchors row by row from the top left, the order of
    /// `Section::nine_point` in archicraft-geometry.
    pub const ALL: [Anchor; 9] = [
        Anchor::TopLeft,
        Anchor::TopCentre,
        Anchor::TopRight,
        Anchor::Left,
        Anchor::Centre,
        Anchor::Right,
        Anchor::BottomLeft,
        Anchor::BottomCentre,
        Anchor::BottomRight,
    ];

    /// The anchor's place in [`Anchor::ALL`]: 0 for the top left to 8 for
    /// the bottom right.
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|a| *a == self).unwrap_or(4)
    }

    /// The anchor's point of the box from `min` to `max`.
    pub fn point(self, (min, max): (DVec2, DVec2)) -> DVec2 {
        let centre = (min + max) / 2.0;
        let index = self.index();
        DVec2::new([min.x, centre.x, max.x][index % 3], [max.y, centre.y, min.y][index / 3])
    }

    /// The anchor on the other side of the section's x axis: top and bottom
    /// swap, the middle row stays. A mirror in plan maps a section that is
    /// symmetric about its x axis onto itself with its anchor flipped so.
    #[must_use]
    pub fn flipped_vertically(self) -> Self {
        let index = self.index();
        Self::ALL[(2 - index / 3) * 3 + index % 3]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// COL-001: the nine anchors are the corners, edge midpoints and centre
    /// of the bounding box, row by row from the top left.
    #[test]
    fn anchors_are_the_nine_points_of_the_box() {
        let bounds = CrossSection::Rectangular {
            width: 0.4,
            height: 0.2,
        }
        .bounds();
        let points: Vec<(f64, f64)> = Anchor::ALL.iter().map(|a| a.point(bounds).into()).collect();
        assert_eq!(
            points,
            [
                (-0.2, 0.1),
                (0.0, 0.1),
                (0.2, 0.1),
                (-0.2, 0.0),
                (0.0, 0.0),
                (0.2, 0.0),
                (-0.2, -0.1),
                (0.0, -0.1),
                (0.2, -0.1),
            ]
        );
        assert_eq!(Anchor::default(), Anchor::Centre);
    }

    /// Flipping swaps the top and bottom rows and keeps the middle row; it
    /// is its own inverse.
    #[test]
    fn flipping_swaps_top_and_bottom() {
        assert_eq!(Anchor::TopLeft.flipped_vertically(), Anchor::BottomLeft);
        assert_eq!(Anchor::BottomCentre.flipped_vertically(), Anchor::TopCentre);
        assert_eq!(Anchor::Right.flipped_vertically(), Anchor::Right);
        for anchor in Anchor::ALL {
            assert_eq!(anchor.flipped_vertically().flipped_vertically(), anchor);
        }
    }
}
