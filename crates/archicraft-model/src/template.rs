//! The default metric template (PRJ-001, DSP-002, MAT-003, LAY-004,
//! Appendix D.1, D.2 and D.4).
//!
//! Creating a project from the template gives every object a new identity
//! (IFX-011).

use indexmap::IndexMap;

use crate::element::{IfcClass, Position, TopLink};
use crate::ids::{BuildingId, MaterialId, ProjectId, SiteId, ViewId};
use crate::kinds::{ReferenceLine, SlabReference};
use crate::layer::Layer;
use crate::material::{CutFill, FillPattern, Material, MaterialCategory, Rgb};
use crate::project::{
    Building, Location, NameCounters, Project, ProjectInfo, ProjectSettings, Projection, Site, SlabDefaults,
    ToolDefaults, View3d, WallDefaults,
};
use crate::storey::{Storey, StoreyList};

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

fn material(entry: &MaterialEntry) -> Material {
    let (name, code, category, [r, g, b], opacity, priority, pattern) = *entry;
    let color = Rgb::from_u8(r, g, b);
    Material {
        id: MaterialId::new(),
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

/// A new project from the default metric template.
pub fn default_metric_project() -> Project {
    let materials: Vec<Material> = MATERIALS.iter().map(material).collect();
    let layers: Vec<Layer> = LAYERS.iter().map(|name| Layer::new(*name)).collect();
    let find_material = |name: &str| {
        materials
            .iter()
            .find(|m| m.name == name)
            .map(|m| m.id)
            .unwrap_or(materials[0].id)
    };
    let find_layer = |name: &str| {
        layers
            .iter()
            .find(|l| l.name == name)
            .map(|l| l.id)
            .unwrap_or(layers[0].id)
    };

    let ground = Storey::new("00 Ground floor", 0.0, 3.0);
    let first = Storey::new("01 First floor", 3.0, 3.0);
    let ground_id = ground.id;
    let storeys = match StoreyList::new(vec![ground, first], ground_id) {
        Ok(list) => list,
        Err(e) => unreachable!("default storeys are valid: {e}"),
    };

    let tool_defaults = ToolDefaults {
        wall: WallDefaults {
            thickness: 0.25,
            reference_line: ReferenceLine::Outside,
            base_offset: 0.0,
            top: TopLink::Relative {
                storeys: 1,
                offset: 0.0,
            },
            material: find_material("Brick"),
            layer: find_layer("Exterior walls"),
            position: Position::Exterior,
            ifc_class: IfcClass::Wall,
        },
        slab: SlabDefaults {
            thickness: 0.20,
            reference: SlabReference::Top,
            offset: 0.0,
            material: find_material("Reinforced concrete"),
            layer: find_layer("Slabs"),
            position: Position::Undefined,
            ifc_class: IfcClass::Slab,
        },
    };

    Project {
        id: ProjectId::new(),
        info: ProjectInfo {
            name: "Untitled project".to_owned(),
            ..ProjectInfo::default()
        },
        settings: ProjectSettings::default(),
        site: Site {
            id: SiteId::new(),
            name: "Site".to_owned(),
            location: Location::default(),
        },
        building: Building {
            id: BuildingId::new(),
            name: "Building".to_owned(),
        },
        storeys,
        layers,
        materials,
        elements: IndexMap::new(),
        tool_defaults,
        views: vec![
            View3d {
                id: ViewId::new(),
                name: "3D perspective".to_owned(),
                projection: Projection::Perspective,
            },
            View3d {
                id: ViewId::new(),
                name: "3D parallel".to_owned(),
                projection: Projection::Parallel,
            },
        ],
        counters: NameCounters::default(),
        data_version: 0,
        revision: 0,
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
        assert!(p.material(p.tool_defaults.wall.material).is_some());
        assert!(p.layer(p.tool_defaults.slab.layer).is_some());
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
