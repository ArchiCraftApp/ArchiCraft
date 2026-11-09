//! The layout settings of annotation text (ANN-007).
//!
//! A text has a size in paper millimetres at the view's drawing scale
//! (VIEW-013, [`crate::DrawingScale`]), an alignment of its lines and an
//! anchor position: the point of the text block that stays on the text's
//! anchor point when the text grows. Size, alignment and line spacing come
//! from the text's [`TextStyle`], one of the project's
//! [`Project::text_styles`](crate::Project::text_styles); a text may
//! override the size ([`crate::Text`]). The types are plain data; the views
//! lay the text out.
//!
//! Like the other model enums they keep serde's default variant names. Files
//! are to store them through [`TextAlign::code`] and [`AnchorPos::code`]
//! (nothing writes them yet), so renaming a variant never changes a file.

use std::collections::HashSet;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

use serde::{Deserialize, Serialize};

use crate::ids::TextStyleId;
use crate::message::Message;
use crate::tolerance::ANGLE_TOLERANCE;

/// A named text style (ANN-007): the size, alignment and line spacing that
/// the texts using it share. Fonts, bold, italic, underline and pen colour
/// follow with the style editor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    pub id: TextStyleId,
    /// Unique within the project, for example "Standard 2.5 mm".
    pub name: String,
    /// The text size in paper millimetres at the view's drawing scale.
    pub size_mm: f64,
    pub align: TextAlign,
    /// The distance between consecutive lines as a multiple of the size.
    pub line_spacing: f64,
}

impl TextStyle {
    /// Checks the style on its own (ANN-007): the size and the line spacing
    /// must be positive finite numbers and the name must not be blank.
    pub fn check(&self) -> Result<(), Message> {
        if self.name.trim().is_empty() {
            return Err(Message::new("model-error-text-style-name"));
        }
        if !(self.size_mm.is_finite() && self.size_mm > 0.0) {
            return Err(Message::new("model-error-text-style-size").text("name", self.name.clone()));
        }
        if !(self.line_spacing.is_finite() && self.line_spacing > 0.0) {
            return Err(Message::new("model-error-text-style-line-spacing").text("name", self.name.clone()));
        }
        Ok(())
    }
}

/// Checks a project's text styles (ANN-007): each must pass
/// [`TextStyle::check`], and no two may share a name or an identity, so
/// that texts refer to exactly one style and users can tell them apart.
pub fn check_text_styles(styles: &[TextStyle]) -> Result<(), Message> {
    let mut names = HashSet::new();
    let mut ids = HashSet::new();
    for style in styles {
        style.check()?;
        if !names.insert(style.name.as_str()) {
            return Err(Message::new("model-error-text-style-duplicate-name").text("name", style.name.clone()));
        }
        if !ids.insert(style.id) {
            return Err(Message::new("model-error-duplicate-text-style-id").text("name", style.name.clone()));
        }
    }
    Ok(())
}

/// The reading direction of a text at `angle`, in radians counter-clockwise
/// from the plan's +X axis, turned by pi when it would point into the left
/// half-plane, so that text never reads upside down (ANN-007); vertical
/// text, and text within [`ANGLE_TOLERANCE`] of vertical, reads from bottom
/// to top, so two texts a hair either side of -pi/2 read the same way. The
/// result lies within that tolerance of (-pi/2, pi/2]. The views draw every
/// text at this angle, and mirroring a text keeps it there
/// ([`crate::Text`]).
pub fn upright(angle: f64) -> f64 {
    let a = (angle + FRAC_PI_2).rem_euclid(TAU) - FRAC_PI_2;
    if a > FRAC_PI_2 + ANGLE_TOLERANCE {
        a - PI
    } else if a <= -FRAC_PI_2 + ANGLE_TOLERANCE {
        a + PI
    } else {
        a
    }
}

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

    /// The name files are to store.
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

    /// The name files are to store.
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

    /// The position on the other side of the block's width: left and right
    /// swap, the middle column stays.
    pub fn flipped_across_width(self) -> Self {
        self.with_fractions(1.0 - self.x_fraction(), self.y_fraction())
    }

    /// The position on the other side of the block's height: top and bottom
    /// swap, the middle row stays.
    pub fn flipped_across_height(self) -> Self {
        self.with_fractions(self.x_fraction(), 1.0 - self.y_fraction())
    }

    /// The position at the fractions `x` and `y`, which are 0, 0.5 or 1.
    fn with_fractions(self, x: f64, y: f64) -> Self {
        Self::ALL
            .into_iter()
            .find(|a| a.x_fraction() == x && a.y_fraction() == y)
            .unwrap_or(self)
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

    /// ANN-007: text never reads upside down: 170 degrees reads at -10
    /// degrees, -170 at 10, pi at 0; 90 and -90 degrees both read bottom to
    /// top, and whole turns do not matter.
    #[test]
    fn upright_never_reads_upside_down() {
        let degrees = |d: f64| d.to_radians();
        assert!((upright(degrees(170.0)) - degrees(-10.0)).abs() < 1e-12);
        assert!((upright(degrees(-170.0)) - degrees(10.0)).abs() < 1e-12);
        assert!((upright(FRAC_PI_2) - FRAC_PI_2).abs() < 1e-12);
        assert!((upright(-FRAC_PI_2) - FRAC_PI_2).abs() < 1e-12);
        assert!((upright(PI) - 0.0).abs() < 1e-12);
        assert!((upright(degrees(30.0) + 4.0 * PI) - degrees(30.0)).abs() < 1e-12);
    }

    /// ANN-007: angles within ANGLE_TOLERANCE of -90 degrees, on either
    /// side, read bottom to top like -90 degrees itself, as do angles that
    /// close to 90 degrees.
    #[test]
    fn near_vertical_angles_read_bottom_to_top() {
        let near = 0.5 * ANGLE_TOLERANCE;
        for angle in [-FRAC_PI_2 + near, -FRAC_PI_2 - near, FRAC_PI_2 + near, FRAC_PI_2 - near] {
            assert!((upright(angle) - FRAC_PI_2).abs() <= ANGLE_TOLERANCE, "{angle}");
        }
    }

    /// ANN-007: flipping across the width swaps left and right, across the
    /// height top and bottom; the middle column and row stay, and flipping
    /// twice gives the position back.
    #[test]
    fn positions_flip_across_the_width_and_the_height() {
        assert_eq!(AnchorPos::BottomLeft.flipped_across_width(), AnchorPos::BottomRight);
        assert_eq!(AnchorPos::TopCenter.flipped_across_width(), AnchorPos::TopCenter);
        assert_eq!(AnchorPos::BottomLeft.flipped_across_height(), AnchorPos::TopLeft);
        assert_eq!(AnchorPos::MiddleRight.flipped_across_height(), AnchorPos::MiddleRight);
        for anchor in AnchorPos::ALL {
            assert_eq!(anchor.flipped_across_width().flipped_across_width(), anchor);
            assert_eq!(anchor.flipped_across_height().flipped_across_height(), anchor);
            let flipped = anchor.flipped_across_width();
            assert_eq!(flipped.x_fraction(), 1.0 - anchor.x_fraction());
            assert_eq!(flipped.y_fraction(), anchor.y_fraction());
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
