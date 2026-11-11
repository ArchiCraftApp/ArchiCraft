//! The cross-section and anchor of members: columns now, beams later
//! (COL-001, BEA-001).
//!
//! A member is swept from its cross-section along its axis. The section is
//! drawn in the profile plane (x to the right, y up, metres; see
//! [`crate::profile`]): a rectangle or circle centred on its bounding box, or
//! a profile of the project in its own profile coordinates. The member's
//! axis runs through one of nine points of the section's bounding box or,
//! for a profile, through the profile's insertion origin: its [`Anchor`].

use glam::DVec2;
use serde::{Deserialize, Serialize};

use crate::ids::ProfileId;

/// The cross-section of a member (COL-001): a rectangle `width` × `height`
/// or a circle of `diameter`, centred on its bounding box, or a profile of
/// the project (PRF-004), in its profile coordinates and made of its region
/// materials. `height` is the section's size along y, which is a column's
/// depth in plan.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum CrossSection {
    Rectangular {
        width: f64,
        height: f64,
    },
    Circular {
        diameter: f64,
    },
    /// The profile with this ID; its shape and bounds are the project's
    /// ([`crate::profile::Profile::bounds`]).
    Profile(ProfileId),
}

impl CrossSection {
    /// The size of the bounding box: width along x, height along y (a
    /// circle's diameter in both). `None` for a profile, whose size is the
    /// project's.
    pub fn size(&self) -> Option<DVec2> {
        match *self {
            CrossSection::Rectangular { width, height } => Some(DVec2::new(width, height)),
            CrossSection::Circular { diameter } => Some(DVec2::splat(diameter)),
            CrossSection::Profile(_) => None,
        }
    }

    /// The bounding box as (min, max), centred on zero. `None` for a
    /// profile, whose bounds are the project's.
    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        let half = self.size()? / 2.0;
        Some((-half, half))
    }

    /// The profile of a profile section.
    pub fn profile(&self) -> Option<ProfileId> {
        match *self {
            CrossSection::Profile(id) => Some(id),
            _ => None,
        }
    }

    /// The codes of the three kinds of section, in display order:
    /// rectangular, circular, profile ([`CrossSection::code`]).
    pub const CODES: [&'static str; 3] = ["rectangular", "circular", "profile"];

    /// The stable code of the section's kind, one of
    /// [`CrossSection::CODES`].
    pub fn code(&self) -> &'static str {
        match self {
            CrossSection::Rectangular { .. } => Self::CODES[0],
            CrossSection::Circular { .. } => Self::CODES[1],
            CrossSection::Profile(_) => Self::CODES[2],
        }
    }

    /// The message key of the label of the section kind `code`.
    pub fn label_key(code: &str) -> Option<&'static str> {
        Some(match code {
            "rectangular" => "model-section-rectangular",
            "circular" => "model-section-circular",
            "profile" => "model-section-profile",
            _ => return None,
        })
    }

    /// The section's dimensions with their values, for range checks; none
    /// for a profile, which its own validation checks.
    pub(crate) fn dimensions(&self) -> Vec<f64> {
        match *self {
            CrossSection::Rectangular { width, height } => vec![width, height],
            CrossSection::Circular { diameter } => vec![diameter],
            CrossSection::Profile(_) => Vec::new(),
        }
    }
}

/// The point of a member's cross-section its axis runs through (COL-001):
/// a corner, an edge midpoint or the centre of the section's bounding box,
/// or a profile's insertion origin. For a circle the nine are the points of
/// its bounding box, so [`Anchor::Centre`] is the circle's centre.
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
    /// The insertion origin of a profile section
    /// ([`crate::profile::Profile::origin`], PRF-004). Validation stores it
    /// as [`Anchor::Centre`] on a rectangular or circular section.
    ProfileOrigin,
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
    /// the bottom right; 4, the centre's, for [`Anchor::ProfileOrigin`].
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|a| *a == self).unwrap_or(4)
    }

    /// The stable code of the anchor, the value of its field: `top-left` to
    /// `bottom-right` and `profile-origin`.
    pub fn code(self) -> &'static str {
        match self {
            Anchor::TopLeft => "top-left",
            Anchor::TopCentre => "top-centre",
            Anchor::TopRight => "top-right",
            Anchor::Left => "left",
            Anchor::Centre => "centre",
            Anchor::Right => "right",
            Anchor::BottomLeft => "bottom-left",
            Anchor::BottomCentre => "bottom-centre",
            Anchor::BottomRight => "bottom-right",
            Anchor::ProfileOrigin => "profile-origin",
        }
    }

    /// The anchor of the code `code` ([`Anchor::code`]).
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .chain([Anchor::ProfileOrigin])
            .find(|a| a.code() == code)
    }

    /// The message key of the anchor's label (NFR-L10N-001); the nine
    /// points share the labels of the text anchor positions.
    pub fn label_key(self) -> &'static str {
        match self {
            Anchor::TopLeft => "model-anchor-top-left",
            Anchor::TopCentre => "model-anchor-top-center",
            Anchor::TopRight => "model-anchor-top-right",
            Anchor::Left => "model-anchor-middle-left",
            Anchor::Centre => "model-anchor-center",
            Anchor::Right => "model-anchor-middle-right",
            Anchor::BottomLeft => "model-anchor-bottom-left",
            Anchor::BottomCentre => "model-anchor-bottom-center",
            Anchor::BottomRight => "model-anchor-bottom-right",
            Anchor::ProfileOrigin => "model-anchor-profile-origin",
        }
    }

    /// The anchor's point of the box from `min` to `max`; the centre for
    /// [`Anchor::ProfileOrigin`], which the box does not know (see
    /// `Column::anchor_point`).
    pub fn point(self, (min, max): (DVec2, DVec2)) -> DVec2 {
        let centre = (min + max) / 2.0;
        let index = self.index();
        DVec2::new([min.x, centre.x, max.x][index % 3], [max.y, centre.y, min.y][index / 3])
    }

    /// The anchor on the other side of the section's x axis: top and bottom
    /// swap, the middle row stays. A mirror in plan maps a section that is
    /// symmetric about its x axis onto itself with its anchor flipped so.
    /// [`Anchor::ProfileOrigin`] stays.
    #[must_use]
    pub fn flipped_vertically(self) -> Self {
        if self == Anchor::ProfileOrigin {
            return self;
        }
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
        .bounds()
        .unwrap();
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
        assert_eq!(Anchor::ProfileOrigin.flipped_vertically(), Anchor::ProfileOrigin);
    }
}
