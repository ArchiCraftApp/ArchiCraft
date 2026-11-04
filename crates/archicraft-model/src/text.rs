//! The layout settings of annotation text (ANN-007).
//!
//! A text has a size in paper millimetres at the view's drawing scale
//! (VIEW-013, [`crate::DrawingScale`]), an alignment of its lines and an
//! anchor position: the point of the text block that stays on the text's
//! anchor point when the text grows. Both types are plain data; the views
//! lay the text out.
//!
//! Like the other model enums they keep serde's default variant names, and
//! files store them through [`TextAlign::code`] and [`AnchorPos::code`], so
//! renaming a variant never changes a file.

use serde::{Deserialize, Serialize};

/// How the lines of a text block line up with each other (ANN-007).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl TextAlign {
    pub const ALL: [TextAlign; 3] = [TextAlign::Left, TextAlign::Center, TextAlign::Right];

    /// The name stored in files.
    pub fn code(self) -> &'static str {
        match self {
            TextAlign::Left => "left",
            TextAlign::Center => "center",
            TextAlign::Right => "right",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.code() == code)
    }
}

/// The point of a text block that sits on the text's anchor point (ANN-007):
/// a corner, the midpoint of an edge or the centre of the block. It stays in
/// place when the text grows, so a text anchored at a top position grows
/// downwards and one anchored at a bottom position grows upwards.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

    /// The name stored in files.
    pub fn code(self) -> &'static str {
        match self {
            AnchorPos::TopLeft => "top_left",
            AnchorPos::TopCenter => "top_center",
            AnchorPos::TopRight => "top_right",
            AnchorPos::MiddleLeft => "middle_left",
            AnchorPos::Center => "center",
            AnchorPos::MiddleRight => "middle_right",
            AnchorPos::BottomLeft => "bottom_left",
            AnchorPos::BottomCenter => "bottom_center",
            AnchorPos::BottomRight => "bottom_right",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.code() == code)
    }

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

    /// ANN-007: files store the settings under stable snake-case codes,
    /// which read back to the same value; unknown codes read as nothing.
    #[test]
    fn codes_are_snake_case_and_round_trip() {
        let codes: Vec<&str> = TextAlign::ALL.iter().map(|a| a.code()).collect();
        assert_eq!(codes, ["left", "center", "right"]);
        let codes: Vec<&str> = AnchorPos::ALL.iter().map(|a| a.code()).collect();
        assert_eq!(
            codes,
            [
                "top_left",
                "top_center",
                "top_right",
                "middle_left",
                "center",
                "middle_right",
                "bottom_left",
                "bottom_center",
                "bottom_right",
            ]
        );
        for align in TextAlign::ALL {
            assert_eq!(TextAlign::from_code(align.code()), Some(align));
        }
        for anchor in AnchorPos::ALL {
            assert_eq!(AnchorPos::from_code(anchor.code()), Some(anchor));
        }
        assert_eq!(TextAlign::from_code("Left"), None);
        assert_eq!(AnchorPos::from_code("middle"), None);
    }

    /// ANN-007: serde keeps the default variant names, as the other model
    /// enums do.
    #[test]
    fn serde_uses_the_default_variant_names() {
        assert_eq!(serde_json::to_string(&TextAlign::Center).unwrap(), "\"Center\"");
        assert_eq!(serde_json::to_string(&AnchorPos::BottomLeft).unwrap(), "\"BottomLeft\"");
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
