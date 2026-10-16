//! Materials (SRS 3.4, MAT-001, DSP-011, D-5).
//!
//! One material concept carries 3D appearance, the 2D cut representation, the
//! IFC material category and the junction priority.

use serde::{Deserialize, Serialize};

use crate::ids::MaterialId;

/// An RGB colour with components in 0…1 (linear in the sense of IFCX
/// `diffuseColor`, which takes plain 0…1 components).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rgb(pub [f64; 3]);

impl Rgb {
    pub const BLACK: Rgb = Rgb([0.0, 0.0, 0.0]);
    pub const WHITE: Rgb = Rgb([1.0, 1.0, 1.0]);

    pub fn from_u8(r: u8, g: u8, b: u8) -> Self {
        Rgb([f64::from(r) / 255.0, f64::from(g) / 255.0, f64::from(b) / 255.0])
    }

    pub fn to_u8(self) -> [u8; 3] {
        self.0.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)
    }

    /// Mixes towards black (`factor` < 1) for outlines and cut fills.
    pub fn scaled(self, factor: f64) -> Self {
        Rgb(self.0.map(|c| (c * factor).clamp(0.0, 1.0)))
    }
}

/// Material category (IFX-041). Written as `bsi::ifc::material` once a bSDD
/// dictionary has been chosen (SRS O-7); until then it is kept in the
/// ArchiCraft namespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MaterialCategory {
    Concrete,
    Masonry,
    Wood,
    Steel,
    Aluminium,
    Glass,
    Insulation,
    Gypsum,
    Stone,
    Soil,
    Plastic,
    Other,
}

impl MaterialCategory {
    pub const ALL: [MaterialCategory; 12] = [
        MaterialCategory::Concrete,
        MaterialCategory::Masonry,
        MaterialCategory::Wood,
        MaterialCategory::Steel,
        MaterialCategory::Aluminium,
        MaterialCategory::Glass,
        MaterialCategory::Insulation,
        MaterialCategory::Gypsum,
        MaterialCategory::Stone,
        MaterialCategory::Soil,
        MaterialCategory::Plastic,
        MaterialCategory::Other,
    ];

    /// Category code as in IFX-041.
    pub fn code(self) -> &'static str {
        match self {
            MaterialCategory::Concrete => "CONCRETE",
            MaterialCategory::Masonry => "MASONRY",
            MaterialCategory::Wood => "WOOD",
            MaterialCategory::Steel => "STEEL",
            MaterialCategory::Aluminium => "ALUMINIUM",
            MaterialCategory::Glass => "GLASS",
            MaterialCategory::Insulation => "INSULATION",
            MaterialCategory::Gypsum => "GYPSUM",
            MaterialCategory::Stone => "STONE",
            MaterialCategory::Soil => "SOIL",
            MaterialCategory::Plastic => "PLASTIC",
            MaterialCategory::Other => "OTHER",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.code() == code)
    }
}

/// Pattern of a 2D fill (DSP-009, DSP-010). Increment 0 draws every pattern
/// as a solid tone; hatches arrive with the plan attributes of increment 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FillPattern {
    Solid,
    Concrete,
    Masonry,
    Timber,
    Insulation,
    Soil,
    Diagonal,
}

impl FillPattern {
    pub const ALL: [FillPattern; 7] = [
        FillPattern::Solid,
        FillPattern::Concrete,
        FillPattern::Masonry,
        FillPattern::Timber,
        FillPattern::Insulation,
        FillPattern::Soil,
        FillPattern::Diagonal,
    ];

    pub fn code(self) -> &'static str {
        match self {
            FillPattern::Solid => "solid",
            FillPattern::Concrete => "concrete",
            FillPattern::Masonry => "masonry",
            FillPattern::Timber => "timber",
            FillPattern::Insulation => "insulation",
            FillPattern::Soil => "soil",
            FillPattern::Diagonal => "diagonal",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.code() == code)
    }
}

/// The cut fill of a material: pattern, foreground and optional background.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CutFill {
    pub pattern: FillPattern,
    pub foreground: Rgb,
    pub background: Option<Rgb>,
}

/// A material resource (MAT-001).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub id: MaterialId,
    pub name: String,
    /// Optional short code, for example `RC` (DSP-003).
    pub code: String,
    pub description: String,
    pub category: MaterialCategory,
    /// Base colour of the 3D appearance.
    pub color: Rgb,
    /// 0 (invisible) … 1 (opaque).
    pub opacity: f64,
    /// Intersection priority 1…999 used for junctions (WAL-023, Appendix D.4).
    pub priority: u16,
    pub cut_fill: CutFill,
}

impl Material {
    pub const MIN_PRIORITY: u16 = 1;
    pub const MAX_PRIORITY: u16 = 999;

    /// Checks the value ranges of MAT-001 (ELM-017).
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("A material needs a name.".into());
        }
        if !(0.0..=1.0).contains(&self.opacity) || !self.opacity.is_finite() {
            return Err("Opacity must be between 0 and 1.".into());
        }
        if !(Self::MIN_PRIORITY..=Self::MAX_PRIORITY).contains(&self.priority) {
            return Err("Intersection priority must be between 1 and 999.".into());
        }
        let in_range = |c: Rgb| c.0.iter().all(|v| (0.0..=1.0).contains(v));
        if !in_range(self.color) || !in_range(self.cut_fill.foreground) {
            return Err("Colour components must be between 0 and 1.".into());
        }
        Ok(())
    }
}
