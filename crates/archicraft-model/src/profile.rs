//! Profiles: named cross-sections for columns and beams (SRS 3.4, PRF-004,
//! PRF-005).
//!
//! A profile is a project resource referenced by its stable [`ProfileId`].
//! Its shape is either a parametric standard shape stored by its dimensions
//! (PRF-005) or custom regions drawn in the profile editor, each a closed
//! outer ring with holes and a material (PRF-004). The model stores
//! parameters only: the section polygons are generated in archicraft-geometry.
//!
//! **Coordinates.** The profile plane is seen from the member's start
//! looking along its axis (for a column: looking down), x to the right and
//! y up, in metres. Parametric shapes are centred on their bounding box; the
//! insertion [`Profile::origin`] is the point of the plane that lies on the
//! member's axis, the centre ([`DVec2::ZERO`]) by default.

use glam::DVec2;
use serde::{Deserialize, Serialize};

use crate::ids::{MaterialId, ProfileId};

/// Which element kinds may use a profile (PRF-004). Walls and railings are
/// stored for PRF-010 but not enforced yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileUsage {
    pub columns: bool,
    pub beams: bool,
    pub walls: bool,
    pub railings: bool,
}

impl ProfileUsage {
    /// Usable by columns and beams, the members PRF-004 names.
    pub const MEMBERS: ProfileUsage = ProfileUsage {
        columns: true,
        beams: true,
        walls: false,
        railings: false,
    };
}

impl Default for ProfileUsage {
    fn default() -> Self {
        Self::MEMBERS
    }
}

/// One dimension of a [`ParametricShape`] (PRF-005), with the symbol of
/// EN 10365 in its doc comment. Radii may be zero (a sharp corner); every
/// other dimension is positive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProfileDimension {
    /// Overall width `b`.
    Width,
    /// Overall height `h`.
    Height,
    /// Outside diameter `D` (`d` for a solid circle).
    Diameter,
    /// Wall or leg thickness `t`.
    Thickness,
    /// Outer corner radius `ro` of a rectangular hollow section.
    OuterRadius,
    /// Web thickness `tw`.
    Web,
    /// Flange thickness `tf`.
    Flange,
    /// Root radius `r` (`r1` of an angle) between web and flange.
    RootRadius,
    /// Toe radius `r2` at the tips of an angle's legs.
    ToeRadius,
}

impl ProfileDimension {
    pub const ALL: [ProfileDimension; 9] = [
        ProfileDimension::Width,
        ProfileDimension::Height,
        ProfileDimension::Diameter,
        ProfileDimension::Thickness,
        ProfileDimension::OuterRadius,
        ProfileDimension::Web,
        ProfileDimension::Flange,
        ProfileDimension::RootRadius,
        ProfileDimension::ToeRadius,
    ];

    /// The stable code, for files and logs.
    pub fn code(self) -> &'static str {
        match self {
            ProfileDimension::Width => "width",
            ProfileDimension::Height => "height",
            ProfileDimension::Diameter => "diameter",
            ProfileDimension::Thickness => "thickness",
            ProfileDimension::OuterRadius => "outer_radius",
            ProfileDimension::Web => "web",
            ProfileDimension::Flange => "flange",
            ProfileDimension::RootRadius => "root_radius",
            ProfileDimension::ToeRadius => "toe_radius",
        }
    }

    /// The message key of the dimension's name (NFR-L10N-001).
    pub fn label_key(self) -> &'static str {
        match self {
            ProfileDimension::Width => "model-profile-dimension-width",
            ProfileDimension::Height => "model-profile-dimension-height",
            ProfileDimension::Diameter => "model-profile-dimension-diameter",
            ProfileDimension::Thickness => "model-profile-dimension-thickness",
            ProfileDimension::OuterRadius => "model-profile-dimension-outer-radius",
            ProfileDimension::Web => "model-profile-dimension-web",
            ProfileDimension::Flange => "model-profile-dimension-flange",
            ProfileDimension::RootRadius => "model-profile-dimension-root-radius",
            ProfileDimension::ToeRadius => "model-profile-dimension-toe-radius",
        }
    }

    /// Whether the dimension is a fillet radius, which may be zero.
    pub fn is_radius(self) -> bool {
        matches!(
            self,
            ProfileDimension::OuterRadius | ProfileDimension::RootRadius | ProfileDimension::ToeRadius
        )
    }
}

/// A parametric standard shape stored by its dimensions (PRF-005), in
/// metres. Every shape is centred on its bounding box, `width` along x and
/// `height` along y; [`ParametricShape::size`] gives the box.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ParametricShape {
    /// A solid rectangle `b` × `h`.
    Rectangle { width: f64, height: f64 },
    /// A solid circle of diameter `d`.
    Circle { diameter: f64 },
    /// A rectangular hollow section `b` × `h` × `t` with outer corner radius
    /// `ro` (zero: sharp corners); the inner radius follows from it.
    RectangularHollow {
        width: f64,
        height: f64,
        thickness: f64,
        outer_radius: f64,
    },
    /// A circular hollow section `D` × `t`.
    CircularHollow { diameter: f64, thickness: f64 },
    /// An I or H section: height `h`, flange width `b`, web `tw`, flanges
    /// `tf` and root radius `r`.
    IShape {
        height: f64,
        width: f64,
        web: f64,
        flange: f64,
        root_radius: f64,
    },
    /// A T section: overall height `h` with the flange at the top, flange
    /// width `b`, web `tw`, flange `tf` and root radius `r`.
    TShape {
        height: f64,
        width: f64,
        web: f64,
        flange: f64,
        root_radius: f64,
    },
    /// An angle: leg lengths `h` (vertical) and `b` (horizontal), meeting at
    /// the bottom left, thickness `t`, root radius `r1` and toe radius `r2`.
    LShape {
        height: f64,
        width: f64,
        thickness: f64,
        root_radius: f64,
        toe_radius: f64,
    },
    /// A channel: height `h`, flange width `b` including the web (on the
    /// left), web `tw`, flanges `tf` and root radius `r`.
    UShape {
        height: f64,
        width: f64,
        web: f64,
        flange: f64,
        root_radius: f64,
    },
    /// A hot-rolled Z section with sharp corners: height `h`, overall width
    /// `b` of both flanges and the web, web `tw` and flanges `tf`.
    /// Cold-formed lipped Z sections are not covered.
    ZShape {
        height: f64,
        width: f64,
        web: f64,
        flange: f64,
    },
}

impl ParametricShape {
    /// One shape of each variant, in the order of PRF-005, with typical
    /// dimensions (a 300 mm square, IPE 200, CHS 168.3 × 8, L 100 × 100 ×
    /// 10 and similar): the starting point of a new parametric profile.
    pub const ALL: [ParametricShape; 9] = [
        ParametricShape::Rectangle {
            width: 0.3,
            height: 0.3,
        },
        ParametricShape::Circle { diameter: 0.3 },
        ParametricShape::RectangularHollow {
            width: 0.2,
            height: 0.1,
            thickness: 0.008,
            outer_radius: 0.016,
        },
        ParametricShape::CircularHollow {
            diameter: 0.1683,
            thickness: 0.008,
        },
        ParametricShape::IShape {
            height: 0.2,
            width: 0.1,
            web: 0.0056,
            flange: 0.0085,
            root_radius: 0.012,
        },
        ParametricShape::TShape {
            height: 0.1,
            width: 0.1,
            web: 0.011,
            flange: 0.011,
            root_radius: 0.011,
        },
        ParametricShape::LShape {
            height: 0.1,
            width: 0.1,
            thickness: 0.01,
            root_radius: 0.012,
            toe_radius: 0.006,
        },
        ParametricShape::UShape {
            height: 0.2,
            width: 0.075,
            web: 0.0085,
            flange: 0.0115,
            root_radius: 0.0115,
        },
        ParametricShape::ZShape {
            height: 0.2,
            width: 0.15,
            web: 0.008,
            flange: 0.01,
        },
    ];

    /// The stable code of the variant, for files and logs.
    pub fn code(&self) -> &'static str {
        match self {
            ParametricShape::Rectangle { .. } => "rectangle",
            ParametricShape::Circle { .. } => "circle",
            ParametricShape::RectangularHollow { .. } => "rectangular_hollow",
            ParametricShape::CircularHollow { .. } => "circular_hollow",
            ParametricShape::IShape { .. } => "i",
            ParametricShape::TShape { .. } => "t",
            ParametricShape::LShape { .. } => "l",
            ParametricShape::UShape { .. } => "u",
            ParametricShape::ZShape { .. } => "z",
        }
    }

    /// The message key of the variant's name (NFR-L10N-001).
    pub fn label_key(&self) -> &'static str {
        match self {
            ParametricShape::Rectangle { .. } => "model-profile-shape-rectangle",
            ParametricShape::Circle { .. } => "model-profile-shape-circle",
            ParametricShape::RectangularHollow { .. } => "model-profile-shape-rectangular-hollow",
            ParametricShape::CircularHollow { .. } => "model-profile-shape-circular-hollow",
            ParametricShape::IShape { .. } => "model-profile-shape-i",
            ParametricShape::TShape { .. } => "model-profile-shape-t",
            ParametricShape::LShape { .. } => "model-profile-shape-l",
            ParametricShape::UShape { .. } => "model-profile-shape-u",
            ParametricShape::ZShape { .. } => "model-profile-shape-z",
        }
    }

    /// The shape's dimensions with their names, in the order of
    /// [`ProfileDimension::ALL`].
    pub fn dimensions(&self) -> Vec<(ProfileDimension, f64)> {
        let mut shape = *self;
        ProfileDimension::ALL
            .into_iter()
            .filter_map(|d| shape.dimension_mut(d).map(|v| (d, *v)))
            .collect()
    }

    /// The value of `dimension`, for editing it, or `None` if the shape
    /// has no such dimension.
    pub fn dimension_mut(&mut self, dimension: ProfileDimension) -> Option<&mut f64> {
        use ProfileDimension as D;
        match (self, dimension) {
            (
                ParametricShape::Rectangle { width, .. }
                | ParametricShape::RectangularHollow { width, .. }
                | ParametricShape::IShape { width, .. }
                | ParametricShape::TShape { width, .. }
                | ParametricShape::LShape { width, .. }
                | ParametricShape::UShape { width, .. }
                | ParametricShape::ZShape { width, .. },
                D::Width,
            ) => Some(width),
            (
                ParametricShape::Rectangle { height, .. }
                | ParametricShape::RectangularHollow { height, .. }
                | ParametricShape::IShape { height, .. }
                | ParametricShape::TShape { height, .. }
                | ParametricShape::LShape { height, .. }
                | ParametricShape::UShape { height, .. }
                | ParametricShape::ZShape { height, .. },
                D::Height,
            ) => Some(height),
            (ParametricShape::Circle { diameter } | ParametricShape::CircularHollow { diameter, .. }, D::Diameter) => {
                Some(diameter)
            }
            (
                ParametricShape::RectangularHollow { thickness, .. }
                | ParametricShape::CircularHollow { thickness, .. }
                | ParametricShape::LShape { thickness, .. },
                D::Thickness,
            ) => Some(thickness),
            (ParametricShape::RectangularHollow { outer_radius, .. }, D::OuterRadius) => Some(outer_radius),
            (
                ParametricShape::IShape { web, .. }
                | ParametricShape::TShape { web, .. }
                | ParametricShape::UShape { web, .. }
                | ParametricShape::ZShape { web, .. },
                D::Web,
            ) => Some(web),
            (
                ParametricShape::IShape { flange, .. }
                | ParametricShape::TShape { flange, .. }
                | ParametricShape::UShape { flange, .. }
                | ParametricShape::ZShape { flange, .. },
                D::Flange,
            ) => Some(flange),
            (
                ParametricShape::IShape { root_radius, .. }
                | ParametricShape::TShape { root_radius, .. }
                | ParametricShape::LShape { root_radius, .. }
                | ParametricShape::UShape { root_radius, .. },
                D::RootRadius,
            ) => Some(root_radius),
            (ParametricShape::LShape { toe_radius, .. }, D::ToeRadius) => Some(toe_radius),
            _ => None,
        }
    }

    /// The size of the bounding box: width along x, height along y (a
    /// circle's diameter in both).
    pub fn size(&self) -> DVec2 {
        match *self {
            ParametricShape::Circle { diameter } | ParametricShape::CircularHollow { diameter, .. } => {
                DVec2::splat(diameter)
            }
            ParametricShape::Rectangle { width, height }
            | ParametricShape::RectangularHollow { width, height, .. }
            | ParametricShape::IShape { width, height, .. }
            | ParametricShape::TShape { width, height, .. }
            | ParametricShape::LShape { width, height, .. }
            | ParametricShape::UShape { width, height, .. }
            | ParametricShape::ZShape { width, height, .. } => DVec2::new(width, height),
        }
    }
}

/// One closed region of a custom profile (PRF-004): an outer ring with
/// holes and the material it is made of. Valid regions have their outer
/// ring counter-clockwise and their holes clockwise
/// ([`crate::validate::validate_profile`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProfileRegion {
    pub outer: Vec<DVec2>,
    #[serde(default)]
    pub holes: Vec<Vec<DVec2>>,
    pub material: MaterialId,
}

/// The shape of a profile: parametric (PRF-005) or custom regions (PRF-004).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ProfileShape {
    /// A standard shape stored by its dimensions, made of one material.
    Parametric {
        shape: ParametricShape,
        material: MaterialId,
    },
    /// One or more regions that do not overlap; they may touch.
    Custom { regions: Vec<ProfileRegion> },
}

/// A named profile resource (SRS 3.4, PRF-004). See the module
/// documentation for the coordinate convention.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: ProfileId,
    pub name: String,
    pub shape: ProfileShape,
    /// The insertion origin in profile coordinates: the point on the
    /// member's axis.
    #[serde(default)]
    pub origin: DVec2,
    #[serde(default)]
    pub usage: ProfileUsage,
    /// The catalogue designation the profile was taken from, for example
    /// `IPE 200` (PRF-011), if any.
    #[serde(default)]
    pub catalogue: Option<String>,
}

impl Profile {
    /// A parametric profile with the default origin and usage and no
    /// catalogue designation.
    pub fn parametric(name: impl Into<String>, shape: ParametricShape, material: MaterialId) -> Self {
        Self {
            id: ProfileId::new(),
            name: name.into(),
            shape: ProfileShape::Parametric { shape, material },
            origin: DVec2::ZERO,
            usage: ProfileUsage::default(),
            catalogue: None,
        }
    }

    /// A custom profile with the default origin and usage.
    pub fn custom(name: impl Into<String>, regions: Vec<ProfileRegion>) -> Self {
        Self {
            id: ProfileId::new(),
            name: name.into(),
            shape: ProfileShape::Custom { regions },
            origin: DVec2::ZERO,
            usage: ProfileUsage::default(),
            catalogue: None,
        }
    }

    /// The bounding box in profile coordinates as (min, max): centred on
    /// zero for a parametric shape, spanning the outer rings for custom
    /// regions. `None` for a custom profile without points.
    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        match &self.shape {
            ProfileShape::Parametric { shape, .. } => {
                let half = shape.size() / 2.0;
                Some((-half, half))
            }
            ProfileShape::Custom { regions } => regions.iter().flat_map(|r| r.outer.iter()).fold(None, |b, &p| {
                Some(match b {
                    None => (p, p),
                    Some((min, max)) => (min.min(p), max.max(p)),
                })
            }),
        }
    }

    /// The materials the profile is made of, each once, in region order.
    pub fn materials(&self) -> Vec<MaterialId> {
        match &self.shape {
            ProfileShape::Parametric { material, .. } => vec![*material],
            ProfileShape::Custom { regions } => {
                let mut materials = Vec::new();
                for region in regions {
                    if !materials.contains(&region.material) {
                        materials.push(region.material);
                    }
                }
                materials
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(min: DVec2, size: f64) -> Vec<DVec2> {
        vec![
            min,
            min + DVec2::new(size, 0.0),
            min + DVec2::splat(size),
            min + DVec2::new(0.0, size),
        ]
    }

    /// PRF-005: a parametric shape is centred on its bounding box, so its
    /// bounds are (-width/2, -height/2)..(width/2, height/2), a circle's
    /// the diameter in both directions.
    #[test]
    fn parametric_bounds_are_centred_on_the_bounding_box() {
        let material = MaterialId::new();
        for shape in ParametricShape::ALL {
            let (width, height) = match shape {
                ParametricShape::Circle { diameter } | ParametricShape::CircularHollow { diameter, .. } => {
                    (diameter, diameter)
                }
                _ => {
                    let dims = shape.dimensions();
                    let get = |d| dims.iter().find(|(name, _)| *name == d).map(|(_, v)| *v).unwrap();
                    (get(ProfileDimension::Width), get(ProfileDimension::Height))
                }
            };
            let profile = Profile::parametric("P", shape, material);
            let half = DVec2::new(width / 2.0, height / 2.0);
            assert_eq!(profile.bounds(), Some((-half, half)), "{}", shape.code());
        }
    }

    /// PRF-004: a custom profile's bounds span the outer rings of all its
    /// regions; one without regions has none.
    #[test]
    fn custom_bounds_span_the_outer_rings() {
        let material = MaterialId::new();
        let profile = Profile::custom(
            "Two blocks",
            vec![
                ProfileRegion {
                    outer: square(DVec2::new(-0.25, -0.5), 0.25),
                    holes: Vec::new(),
                    material,
                },
                ProfileRegion {
                    outer: square(DVec2::new(0.0, -0.5), 0.75),
                    holes: vec![square(DVec2::new(0.25, -0.25), 0.25)],
                    material,
                },
            ],
        );
        assert_eq!(
            profile.bounds(),
            Some((DVec2::new(-0.25, -0.5), DVec2::new(0.75, 0.25)))
        );
        assert_eq!(Profile::custom("Empty", Vec::new()).bounds(), None);
    }

    /// PRF-004: the materials of a profile, each once in region order.
    #[test]
    fn materials_are_listed_once_in_region_order() {
        let (a, b) = (MaterialId::new(), MaterialId::new());
        let region = |material| ProfileRegion {
            outer: square(DVec2::ZERO, 0.1),
            holes: Vec::new(),
            material,
        };
        let custom = Profile::custom("Mixed", vec![region(b), region(a), region(b)]);
        assert_eq!(custom.materials(), [b, a]);
        let parametric = Profile::parametric("IPE 200", ParametricShape::ALL[4], a);
        assert_eq!(parametric.materials(), [a]);
    }

    /// PRF-005: `dimensions` lists every field of each shape once, with
    /// its value and the field's name as its code, so `dimension_mut`
    /// reaches all of them.
    #[test]
    fn dimensions_cover_every_field_of_each_shape() {
        for shape in ParametricShape::ALL {
            let json = serde_json::to_value(shape).unwrap();
            let (_, fields) = json.as_object().unwrap().iter().next().unwrap();
            let mut fields: Vec<(String, f64)> = fields
                .as_object()
                .unwrap()
                .iter()
                .map(|(name, value)| (name.clone(), value.as_f64().unwrap()))
                .collect();
            fields.sort_by(|a, b| a.0.cmp(&b.0));
            let mut listed: Vec<(String, f64)> = shape
                .dimensions()
                .into_iter()
                .map(|(d, v)| (d.code().to_owned(), v))
                .collect();
            listed.sort_by(|a, b| a.0.cmp(&b.0));
            assert_eq!(listed, fields, "{}", shape.code());
        }
    }

    /// The codes and label keys of the shapes and dimensions are distinct.
    #[test]
    fn codes_and_label_keys_are_distinct() {
        let mut codes: Vec<_> = ParametricShape::ALL.iter().map(ParametricShape::code).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), ParametricShape::ALL.len());
        let mut keys: Vec<_> = ProfileDimension::ALL.map(ProfileDimension::label_key).to_vec();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), ProfileDimension::ALL.len());
    }
}
