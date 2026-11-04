//! The drawing scale of 2D views (VIEW-013, Appendix D.1).
//!
//! A drawing scale 1:n says that one paper millimetre stands for n model
//! millimetres. Line weights, hatch spacing and annotation sizes are given in
//! paper millimetres (DSP-019) and become model lengths through
//! [`DrawingScale::paper_mm_to_model`]: that one conversion serves the line
//! weights and hatch spacing of the plan renderer and the text heights and
//! symbol sizes of the annotations (ANN-004 to ANN-008). Zooming a view
//! changes how large the drawing is on screen, never its scale.

use std::fmt;
use std::num::NonZeroU32;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::message::Message;

/// Millimetres per metre.
const MM_PER_M: f64 = 1000.0;

/// A drawing scale 1:`denominator` (VIEW-013). Serialised as the integer
/// denominator.
///
/// The order compares denominators, so 1:50 < 1:100: the lesser value is
/// the larger scale, the one that draws the model larger on paper.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct DrawingScale(NonZeroU32);

impl DrawingScale {
    /// The largest denominator accepted.
    pub const MAX: u32 = 10_000;

    /// The scales offered for choice in ascending order of denominator,
    /// from 1:1 to 1:5000.
    pub const PRESETS: [DrawingScale; 14] = [
        Self::of(1),
        Self::of(2),
        Self::of(5),
        Self::of(10),
        Self::of(20),
        Self::of(25),
        Self::of(50),
        Self::of(100),
        Self::of(200),
        Self::of(250),
        Self::of(500),
        Self::of(1000),
        Self::of(2000),
        Self::of(5000),
    ];

    /// 1:`denominator` for a constant of at least 1.
    const fn of(denominator: u32) -> Self {
        Self(NonZeroU32::MIN.saturating_add(denominator - 1))
    }

    /// The scale 1:`denominator`, if the denominator is 1 to [`Self::MAX`].
    pub fn new(denominator: u32) -> Result<Self, ScaleError> {
        match NonZeroU32::new(denominator) {
            Some(n) if denominator <= Self::MAX => Ok(Self(n)),
            _ => Err(ScaleError::OutOfRange),
        }
    }

    /// The n of 1:n.
    pub fn denominator(self) -> u32 {
        self.0.get()
    }

    /// Paper length over model length, 1/n.
    pub fn ratio(self) -> f64 {
        1.0 / f64::from(self.denominator())
    }

    /// The model length in metres that `mm` paper millimetres stand for.
    pub fn paper_mm_to_model(self, mm: f64) -> f64 {
        mm / MM_PER_M * f64::from(self.denominator())
    }

    /// The paper millimetres that `meters` of model length take; the inverse
    /// of [`Self::paper_mm_to_model`].
    pub fn model_to_paper_mm(self, meters: f64) -> f64 {
        meters * MM_PER_M / f64::from(self.denominator())
    }
}

/// 1:100 (Appendix D.1).
impl Default for DrawingScale {
    fn default() -> Self {
        Self::of(100)
    }
}

/// "1:100".
impl fmt::Display for DrawingScale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "1:{}", self.denominator())
    }
}

/// Reads "1:50", "1 : 50" or the bare denominator "50".
impl FromStr for DrawingScale {
    type Err = ScaleError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let trimmed = text.trim();
        let invalid = || ScaleError::Invalid(trimmed.to_owned());
        let denominator = match trimmed.split_once(':') {
            Some((paper, model)) => {
                if paper.trim() != "1" {
                    return Err(invalid());
                }
                model.trim()
            }
            None => trimmed,
        };
        if denominator.is_empty() || !denominator.bytes().all(|b| b.is_ascii_digit()) {
            return Err(invalid());
        }
        // All digits, so a parse error is an overflow.
        let n: u32 = denominator.parse().map_err(|_| ScaleError::OutOfRange)?;
        Self::new(n)
    }
}

impl From<DrawingScale> for u32 {
    fn from(scale: DrawingScale) -> Self {
        scale.denominator()
    }
}

impl TryFrom<u32> for DrawingScale {
    type Error = ScaleError;

    fn try_from(denominator: u32) -> Result<Self, Self::Error> {
        Self::new(denominator)
    }
}

/// Why a drawing scale was rejected. `Display` is the key form of
/// [`ScaleError::message`], for logs only (NFR-L10N-001).
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ScaleError {
    /// The text is not of the form 1:n or n.
    #[error("{}", self.message())]
    Invalid(String),
    /// The denominator is 0 or above [`DrawingScale::MAX`].
    #[error("{}", self.message())]
    OutOfRange,
}

impl ScaleError {
    /// The message to show the user.
    pub fn message(&self) -> Message {
        match self {
            ScaleError::Invalid(text) => Message::new("model-error-parse-scale").text("text", text.as_str()),
            ScaleError::OutOfRange => Message::new("model-error-scale-range")
                .count("max", usize::try_from(DrawingScale::MAX).unwrap_or(usize::MAX)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scale(n: u32) -> DrawingScale {
        DrawingScale::new(n).unwrap()
    }

    /// VIEW-013: typed scales read as 1:n, with or without spaces, and a
    /// bare number is the denominator.
    #[test]
    fn typed_scales_parse() {
        assert_eq!("1:50".parse(), Ok(scale(50)));
        assert_eq!("1 : 200".parse(), Ok(scale(200)));
        assert_eq!("50".parse(), Ok(scale(50)));
        assert_eq!(" 1:10000 ".parse(), Ok(scale(DrawingScale::MAX)));
    }

    /// VIEW-013: a zero, negative, non-numeric or too large scale is rejected
    /// with a message.
    #[test]
    fn invalid_scales_are_rejected_with_a_message() {
        for (text, error) in [
            ("1:0", ScaleError::OutOfRange),
            ("0:1", ScaleError::Invalid("0:1".into())),
            ("1:-5", ScaleError::Invalid("1:-5".into())),
            ("abc", ScaleError::Invalid("abc".into())),
            ("", ScaleError::Invalid(String::new())),
            ("1:100000", ScaleError::OutOfRange),
            ("1:99999999999", ScaleError::OutOfRange),
        ] {
            assert_eq!(text.parse::<DrawingScale>(), Err(error.clone()), "{text:?}");
        }
        assert_eq!(
            ScaleError::Invalid("abc".into()).message().key(),
            "model-error-parse-scale"
        );
        assert_eq!(ScaleError::OutOfRange.message().key(), "model-error-scale-range");
        assert_eq!(
            ScaleError::Invalid("abc".into()).message().arg("text"),
            Some(&crate::message::Arg::Text("abc".into()))
        );
        assert_eq!(
            ScaleError::OutOfRange.message().arg("max"),
            Some(&crate::message::Arg::Count(10_000))
        );
    }

    /// VIEW-013: a scale shows as 1:n and every preset reads back; scales
    /// order by denominator, and the presets run from 1:1 to 1:5000.
    #[test]
    fn scales_display_as_one_to_n() {
        assert_eq!(DrawingScale::default().to_string(), "1:100");
        for preset in DrawingScale::PRESETS {
            assert_eq!(preset.to_string().parse(), Ok(preset));
        }
        assert!(scale(50) < scale(100));
        assert!(DrawingScale::PRESETS.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(DrawingScale::PRESETS.first(), Some(&scale(1)));
        assert_eq!(DrawingScale::PRESETS.last(), Some(&scale(5000)));
    }

    /// VIEW-013: a scale round-trips through serde as its denominator, and
    /// an out-of-range denominator is refused.
    #[test]
    fn serde_round_trips_the_denominator() {
        let json = serde_json::to_string(&scale(50)).unwrap();
        assert_eq!(json, "50");
        assert_eq!(serde_json::from_str::<DrawingScale>(&json).unwrap(), scale(50));
        assert!(serde_json::from_str::<DrawingScale>("0").is_err());
        assert!(serde_json::from_str::<DrawingScale>("10001").is_err());
    }

    /// Appendix D.1, VIEW-013: the default is 1:100; half a paper millimetre
    /// is 5 cm in the model at 1:100 and 2.5 cm at 1:50, and model to paper
    /// is the inverse.
    #[test]
    fn paper_millimetres_convert_at_the_scale() {
        assert_eq!(DrawingScale::default(), scale(100));
        assert!((DrawingScale::default().ratio() - 0.01).abs() < 1e-15);
        assert!((DrawingScale::default().paper_mm_to_model(0.5) - 0.05).abs() < 1e-12);
        assert!((scale(50).paper_mm_to_model(0.5) - 0.025).abs() < 1e-12);
        for s in [scale(1), scale(50), scale(100), scale(DrawingScale::MAX)] {
            for mm in [0.0, 0.13, 0.5, 2.5, 100.0] {
                assert!((s.model_to_paper_mm(s.paper_mm_to_model(mm)) - mm).abs() < 1e-9);
            }
        }
    }

    /// VIEW-013, ANN-004: an annotation's 2.5 mm paper text height is 25 cm
    /// in the model at 1:100 and 12.5 cm at 1:50, through the same
    /// conversion as line weights and hatch spacing.
    #[test]
    fn annotation_sizes_are_paper_millimetres_at_the_scale() {
        let text_height_mm = 2.5;
        assert!((scale(100).paper_mm_to_model(text_height_mm) - 0.25).abs() < 1e-12);
        assert!((scale(50).paper_mm_to_model(text_height_mm) - 0.125).abs() < 1e-12);
    }
}
