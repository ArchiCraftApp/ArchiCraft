//! The layout settings of annotation text (ANN-007).
//!
//! A text has a size in paper millimetres at the view's drawing scale
//! (VIEW-013, [`crate::DrawingScale`]), an alignment of its lines and an
//! anchor position: the point of the text block that stays on the text's
//! anchor point when the text grows. Both types are plain data; the views
//! lay the text out.

use serde::{Deserialize, Serialize};

/// How the lines of a text block line up with each other (ANN-007).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// The point of a text block that sits on the text's anchor point (ANN-007):
/// a corner, the midpoint of an edge or the centre of the block. It stays in
/// place when the text grows, so a text anchored at a top position grows
/// downwards and one anchored at a bottom position grows upwards.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorPos {
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    Center,
    MiddleRight,
    #[default]
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl AnchorPos {
    /// All nine positions, row by row from the top left.
    pub const ALL: [AnchorPos; 9] = [
        AnchorPos::TopLeft,
        AnchorPos::TopCenter,
        AnchorPos::TopRight,
        AnchorPos::MiddleLeft,
        AnchorPos::Center,
        AnchorPos::MiddleRight,
        AnchorPos::BottomLeft,
        AnchorPos::BottomCenter,
        AnchorPos::BottomRight,
    ];

    /// Where the position lies across the block's width: 0 at the left edge,
    /// 0.5 in the middle, 1 at the right edge.
    pub fn x_fraction(self) -> f64 {
        match self {
            AnchorPos::TopLeft | AnchorPos::MiddleLeft | AnchorPos::BottomLeft => 0.0,
            AnchorPos::TopCenter | AnchorPos::Center | AnchorPos::BottomCenter => 0.5,
            AnchorPos::TopRight | AnchorPos::MiddleRight | AnchorPos::BottomRight => 1.0,
        }
    }

    /// Where the position lies down the block's height, in reading
    /// orientation: 0 at the top edge, 0.5 in the middle, 1 at the bottom edge.
    pub fn y_fraction(self) -> f64 {
        match self {
            AnchorPos::TopLeft | AnchorPos::TopCenter | AnchorPos::TopRight => 0.0,
            AnchorPos::MiddleLeft | AnchorPos::Center | AnchorPos::MiddleRight => 0.5,
            AnchorPos::BottomLeft | AnchorPos::BottomCenter | AnchorPos::BottomRight => 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ANN-007: a new text is left-aligned and anchored at its bottom left.
    #[test]
    fn defaults_are_left_and_bottom_left() {
        assert_eq!(TextAlign::default(), TextAlign::Left);
        assert_eq!(AnchorPos::default(), AnchorPos::BottomLeft);
    }

    /// ANN-007: the settings serialise under stable snake-case names.
    #[test]
    fn serde_names_are_snake_case() {
        let align = [TextAlign::Left, TextAlign::Center, TextAlign::Right];
        let names: Vec<String> = align.iter().map(|a| serde_json::to_string(a).unwrap()).collect();
        assert_eq!(names, ["\"left\"", "\"center\"", "\"right\""]);
        let names: Vec<String> = AnchorPos::ALL
            .iter()
            .map(|a| serde_json::to_string(a).unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "\"top_left\"",
                "\"top_center\"",
                "\"top_right\"",
                "\"middle_left\"",
                "\"center\"",
                "\"middle_right\"",
                "\"bottom_left\"",
                "\"bottom_center\"",
                "\"bottom_right\"",
            ]
        );
        for anchor in AnchorPos::ALL {
            let json = serde_json::to_string(&anchor).unwrap();
            assert_eq!(serde_json::from_str::<AnchorPos>(&json).unwrap(), anchor);
        }
    }

    /// ANN-007: each position names a corner, an edge midpoint or the centre.
    #[test]
    fn positions_name_their_point_of_the_block() {
        let points: Vec<(f64, f64)> = AnchorPos::ALL
            .iter()
            .map(|a| (a.x_fraction(), a.y_fraction()))
            .collect();
        assert_eq!(
            points,
            [
                (0.0, 0.0),
                (0.5, 0.0),
                (1.0, 0.0),
                (0.0, 0.5),
                (0.5, 0.5),
                (1.0, 0.5),
                (0.0, 1.0),
                (0.5, 1.0),
                (1.0, 1.0),
            ]
        );
    }
}
