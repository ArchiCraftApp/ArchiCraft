//! The default metric template (PRJ-001, DSP-002, MAT-003, LAY-004,
//! Appendix D.1, D.2 and D.4).
//!
//! Creating a project from the template gives every object a new identity
//! (IFX-011). Seeded fixtures pass their own identity source instead
//! ([`default_metric_project_with`]).

use indexmap::IndexMap;
use uuid::Uuid;

use crate::changes::ChangeLog;
use crate::element::KindTag;
use crate::ids::{BuildingId, LayerId, MaterialId, ProjectId, SiteId, StoreyId, TextStyleId, ViewId};
use crate::layer::Layer;
use crate::material::{CutFill, FillPattern, Material, MaterialCategory, Rgb};
use crate::project::{
    Building, Location, NameCounters, Project, ProjectInfo, ProjectSettings, Projection, Site, ToolDefaults, View3d,
};
use crate::relations::RelationStore;
use crate::storey::{Storey, StoreyList};
use crate::text::{TextAlign, TextStyle};

/// Name, short code, category, colour (sRGB bytes), opacity, intersection
/// priority (Appendix D.4) and cut-fill pattern of the default materials.
type MaterialEntry = (
    &'static str,
    &'static str,
    MaterialCategory,
    [u8; 3],
    f64,
    u16,
    FillPattern,
);

const MATERIALS: [MaterialEntry; 25] = [
    (
        "Reinforced concrete",
        "RC",
        MaterialCategory::Concrete,
        [158, 158, 153],
        1.0,
        900,
        FillPattern::Concrete,
    ),
    (
        "Concrete",
        "C",
        MaterialCategory::Concrete,
        [179, 179, 173],
        1.0,
        880,
        FillPattern::Concrete,
    ),
    (
        "Brick",
        "BRK",
        MaterialCategory::Masonry,
        [184, 107, 82],
        1.0,
        800,
        FillPattern::Masonry,
    ),
    (
        "Sand-lime brick",
        "SLB",
        MaterialCategory::Masonry,
        [224, 222, 209],
        1.0,
        800,
        FillPattern::Masonry,
    ),
    (
        "Aerated concrete",
        "AAC",
        MaterialCategory::Masonry,
        [230, 230, 224],
        1.0,
        800,
        FillPattern::Masonry,
    ),
    (
        "Clay block",
        "CB",
        MaterialCategory::Masonry,
        [204, 133, 97],
        1.0,
        800,
        FillPattern::Masonry,
    ),
    (
        "Softwood",
        "SW",
        MaterialCategory::Wood,
        [217, 179, 122],
        1.0,
        740,
        FillPattern::Timber,
    ),
    (
        "Glulam",
        "GL",
        MaterialCategory::Wood,
        [219, 184, 128],
        1.0,
        760,
        FillPattern::Timber,
    ),
    (
        "Cross-laminated timber",
        "CLT",
        MaterialCategory::Wood,
        [224, 194, 143],
        1.0,
        760,
        FillPattern::Timber,
    ),
    (
        "Steel",
        "S",
        MaterialCategory::Steel,
        [140, 148, 158],
        1.0,
        860,
        FillPattern::Solid,
    ),
    (
        "Aluminium",
        "AL",
        MaterialCategory::Aluminium,
        [199, 204, 209],
        1.0,
        500,
        FillPattern::Solid,
    ),
    (
        "Glass",
        "GLS",
        MaterialCategory::Glass,
        [153, 204, 230],
        0.3,
        500,
        FillPattern::Solid,
    ),
    (
        "Mineral wool",
        "MW",
        MaterialCategory::Insulation,
        [242, 217, 115],
        1.0,
        400,
        FillPattern::Insulation,
    ),
    (
        "EPS/XPS",
        "EPS",
        MaterialCategory::Insulation,
        [235, 237, 242],
        1.0,
        400,
        FillPattern::Insulation,
    ),
    (
        "Wood fibre insulation",
        "WF",
        MaterialCategory::Insulation,
        [191, 158, 115],
        1.0,
        400,
        FillPattern::Insulation,
    ),
    (
        "Plaster",
        "PL",
        MaterialCategory::Gypsum,
        [242, 240, 230],
        1.0,
        300,
        FillPattern::Solid,
    ),
    (
        "Render",
        "RE",
        MaterialCategory::Other,
        [237, 232, 219],
        1.0,
        300,
        FillPattern::Solid,
    ),
    (
        "Gypsum board",
        "GB",
        MaterialCategory::Gypsum,
        [245, 245, 245],
        1.0,
        350,
        FillPattern::Solid,
    ),
    (
        "Screed",
        "SCR",
        MaterialCategory::Concrete,
        [191, 189, 179],
        1.0,
        600,
        FillPattern::Concrete,
    ),
    (
        "Natural stone",
        "NS",
        MaterialCategory::Stone,
        [179, 173, 158],
        1.0,
        560,
        FillPattern::Diagonal,
    ),
    (
        "Tiles",
        "TI",
        MaterialCategory::Other,
        [204, 199, 189],
        1.0,
        280,
        FillPattern::Solid,
    ),
    (
        "Soil",
        "SO",
        MaterialCategory::Soil,
        [128, 102, 71],
        1.0,
        50,
        FillPattern::Soil,
    ),
    (
        "Gravel",
        "GR",
        MaterialCategory::Soil,
        [158, 153, 143],
        1.0,
        50,
        FillPattern::Soil,
    ),
    (
        "Membrane",
        "MB",
        MaterialCategory::Plastic,
        [64, 64, 71],
        1.0,
        200,
        FillPattern::Solid,
    ),
    (
        "Air gap",
        "AIR",
        MaterialCategory::Other,
        [230, 242, 255],
        1.0,
        100,
        FillPattern::Solid,
    ),
];

/// Layer names of the default template (LAY-004).
pub const LAYERS: [&str; 11] = [
    "Structure",
    "Exterior walls",
    "Interior walls",
    "Slabs",
    "Roof",
    "Openings",
    "Stairs and railings",
    "Terrain",
    "Furniture",
    "2D construction",
    "References",
];

/// Name and size in paper millimetres of the template's text styles
/// (ANN-007), each left-aligned with [`TEXT_LINE_SPACING`]. The SRS gives no
/// values; these are proposals (archicraft-h4d7). The first is the text
/// tool's default and, as the first of a project's styles, the style that
/// the reader gives a text or the tool's defaults of a file whose style is
/// missing (PRJ-008).
pub const TEXT_STYLES: [(&str, f64); 4] = [
    ("Standard 2.5 mm", 2.5),
    ("Dimension 2.5 mm", 2.5),
    ("Small 1.8 mm", 1.8),
    ("Heading 5.0 mm", 5.0),
];

/// The line spacing of the template's text styles, a multiple of the size.
pub const TEXT_LINE_SPACING: f64 = 1.2;

/// The namespace of the name-based (version 5) identities of the
/// template's text styles.
const TEXT_STYLE_NAMESPACE: Uuid = Uuid::from_u128(0x3c0b_8e51_7d2a_4f96_b1e4_5a9c_06d3_72f8);

/// The identity of the template's text style `name`, the same in every
/// project: text styles are no IFCX nodes, so IFX-011's new identities for
/// a project from the template do not apply to them, and fixed identities
/// let texts, files and migrations of older files refer to a preset
/// without a lookup.
pub fn template_text_style_id(name: &str) -> TextStyleId {
    TextStyleId::from_uuid(Uuid::new_v5(&TEXT_STYLE_NAMESPACE, name.as_bytes()))
}

/// The template's text styles (ANN-007), in the order of [`TEXT_STYLES`].
pub fn template_text_styles() -> Vec<TextStyle> {
    TEXT_STYLES
        .iter()
        .map(|&(name, size_mm)| TextStyle {
            id: template_text_style_id(name),
            name: name.to_owned(),
            size_mm,
            align: TextAlign::Left,
            line_spacing: TEXT_LINE_SPACING,
        })
        .collect()
}

fn material(entry: &MaterialEntry, id: MaterialId) -> Material {
    let (name, code, category, [r, g, b], opacity, priority, pattern) = *entry;
    let color = Rgb::from_u8(r, g, b);
    Material {
        id,
        name: name.to_owned(),
        code: code.to_owned(),
        description: String::new(),
        category,
        color,
        opacity,
        priority,
        cut_fill: CutFill {
            pattern,
            foreground: color.scaled(0.85),
            background: None,
        },
    }
}

/// The materials and layers that the template's tool defaults are resolved
/// against (Appendix D.2): each is found by name, else the first one is
/// used.
#[derive(Clone, Copy, Debug)]
pub struct TemplateContext<'a> {
    materials: &'a [Material],
    layers: &'a [Layer],
}

impl<'a> TemplateContext<'a> {
    pub fn new(materials: &'a [Material], layers: &'a [Layer]) -> Self {
        Self { materials, layers }
    }

    /// The material named `name`, else the first (else a new identity).
    pub fn material(&self, name: &str) -> MaterialId {
        self.materials
            .iter()
            .find(|m| m.name == name)
            .or_else(|| self.materials.first())
            .map_or_else(MaterialId::new, |m| m.id)
    }

    /// The layer named `name`, else the first (else a new identity).
    pub fn layer(&self, name: &str) -> LayerId {
        self.layers
            .iter()
            .find(|l| l.name == name)
            .or_else(|| self.layers.first())
            .map_or_else(LayerId::new, |l| l.id)
    }

    /// The template's text style named `name` (ANN-007). The presets have
    /// fixed identities ([`template_text_style_id`]), so this needs no list.
    pub fn text_style(&self, name: &str) -> TextStyleId {
        template_text_style_id(name)
    }
}

/// The template's tool defaults: one slot per kind with template defaults,
/// in registry order (ELM-001, ELM-011, Appendix D.2).
pub fn template_tool_defaults(context: &TemplateContext<'_>) -> ToolDefaults {
    KindTag::ALL
        .iter()
        .filter_map(|kind| kind.descriptor().template_defaults)
        .map(|template| template(context))
        .collect()
}

/// A new project from the default metric template, with random (version 4)
/// identities (IFX-011).
pub fn default_metric_project() -> Project {
    default_metric_project_with(&mut |_| Uuid::new_v4())
}

/// A new project from the default metric template whose identities come
/// from `ids`, called once per object with its role: `project`, `site`,
/// `building`, `storey`, `layer`, `material` and `view`, in a fixed order.
/// The tool defaults refer to the materials and layers by these identities.
/// [`crate::samples::FixtureBuilder`] passes name-based (version 5) UUIDs, so
/// that its fixtures are reproducible. The text styles keep their fixed
/// identities ([`template_text_style_id`]).
pub fn default_metric_project_with(ids: &mut dyn FnMut(&str) -> Uuid) -> Project {
    let id = ids("project");
    let site = ids("site");
    let building = ids("building");
    let materials: Vec<Material> = MATERIALS
        .iter()
        .map(|entry| material(entry, MaterialId::from_uuid(ids("material"))))
        .collect();
    let layers: Vec<Layer> = LAYERS
        .iter()
        .map(|name| Layer {
            id: LayerId::from_uuid(ids("layer")),
            ..Layer::new(*name)
        })
        .collect();
    let tool_defaults = template_tool_defaults(&TemplateContext::new(&materials, &layers));

    let ground = Storey {
        id: StoreyId::from_uuid(ids("storey")),
        ..Storey::new("00 Ground floor", 0.0, 3.0)
    };
    let first = Storey {
        id: StoreyId::from_uuid(ids("storey")),
        ..Storey::new("01 First floor", 3.0, 3.0)
    };
    let ground_id = ground.id;
    let storeys = match StoreyList::new(vec![ground, first], ground_id) {
        Ok(list) => list,
        Err(e) => unreachable!("default storeys are valid: {e}"),
    };

    Project {
        id: ProjectId::from_uuid(id),
        info: ProjectInfo {
            name: "Untitled project".to_owned(),
            ..ProjectInfo::default()
        },
        settings: ProjectSettings::default(),
        site: Site {
            id: SiteId::from_uuid(site),
            name: "Site".to_owned(),
            location: Location::default(),
        },
        building: Building {
            id: BuildingId::from_uuid(building),
            name: "Building".to_owned(),
        },
        storeys,
        layers,
        materials,
        profiles: Vec::new(),
        text_styles: template_text_styles(),
        elements: IndexMap::new(),
        groups: IndexMap::new(),
        relations: RelationStore::default(),
        tool_defaults,
        views: vec![
            View3d {
                id: ViewId::from_uuid(ids("view")),
                name: "3D perspective".to_owned(),
                projection: Projection::Perspective,
            },
            View3d {
                id: ViewId::from_uuid(ids("view")),
                name: "3D parallel".to_owned(),
                projection: Projection::Parallel,
            },
        ],
        counters: NameCounters::default(),
        group_counter: 0,
        data_version: 0,
        revision: 0,
        changes: ChangeLog::default(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn template_contents() {
        let p = default_metric_project();
        assert_eq!(p.materials.len(), 25);
        assert!(p.materials.iter().all(|m| m.validate().is_ok()));
        assert_eq!(p.layers.len(), LAYERS.len());
        assert_eq!(p.storeys.len(), 2);
        assert_eq!(p.storeys.as_slice()[1].elevation, 3.0);
        assert_eq!(p.views.len(), 2);
        let wall = p.tool_defaults.get(KindTag::Wall).unwrap();
        assert!(p.material(wall.kind.material().unwrap()).is_some());
        assert!(p.layer(p.tool_defaults.get(KindTag::Slab).unwrap().layer).is_some());
    }

    #[test]
    fn every_project_gets_new_identities() {
        let a = default_metric_project();
        let b = default_metric_project();
        assert_ne!(a.id, b.id);
        let ids_a: HashSet<_> = a.materials.iter().map(|m| m.id).collect();
        assert!(b.materials.iter().all(|m| !ids_a.contains(&m.id)));
    }
}
