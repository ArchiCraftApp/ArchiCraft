//! Writes a project as an `ifcx_alpha` document (SRS Section 5.2).
//!
//! Rounding (IFX-007, NFR-ACC-002): "coordinates rounded to 10⁻⁶ m" applies
//! to the coordinates in the standard namespaces, which viewers read: mesh
//! points, curve points (the wall `Axis`) and the translation of every
//! `usd::xformop` placement. Parameters in the `archicraft::` namespace,
//! including plan coordinates such as wall start and end or slab outlines,
//! are written at shortest round-trip precision, so that loading gives back
//! the model bit for bit. Both are deterministic and exponent-free through
//! [`crate::json`].

use std::collections::HashSet;

use archicraft_geometry::{ModelGeometry, TriMesh};
use archicraft_model::glam::DVec3;
use archicraft_model::material::Material;
use archicraft_model::project::{NameCounters, SlabDefaults, WallDefaults};
use archicraft_model::{Element, ElementKind, KindTag, MaterialId, Project, ReferenceLine, TopLink};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::schema::{self, FORMAT_VERSION};
use crate::container::{Header, IfcxFile, Import, Node};
use crate::json::round_coordinate;
use crate::registry;
use crate::validate::Problem;

pub const IFCX_VERSION: &str = "ifcx_alpha";
const CLASS_URI: &str = "https://identifier.buildingsmart.org/uri/buildingsmart/ifc/4.3/class/";

/// Values for the header that do not come from the project.
#[derive(Clone, Debug)]
pub struct WriteOptions {
    /// Author from the user preferences (IFX-002, SET-001).
    pub author: String,
    /// ISO 8601 UTC time of the save.
    pub timestamp: String,
    /// `header.dataVersion`: the save counter (O-16).
    pub data_version: u64,
}

/// The document and anything worth telling the user about it.
#[derive(Clone, Debug)]
pub struct WriteOutcome {
    pub file: IfcxFile,
    pub warnings: Vec<Problem>,
}

/// `header.id`, derived from the project UUID (IFX-013).
pub fn dataset_id(project: &Project) -> String {
    format!("archicraft:project/{}", project.id)
}

/// Identity of a sub-node regenerated with geometry: a name-based UUID from
/// the owner's UUID and the role (IFX-010).
pub fn sub_node_id(owner: Uuid, role: &str) -> Uuid {
    Uuid::new_v5(&owner, role.as_bytes())
}

fn class(code: &str) -> Value {
    json!({"code": code, "uri": format!("{CLASS_URI}{code}")})
}

/// Child keys are `<Kind>_<first 8 hex digits of the UUID>` (IFX-014); on
/// the rare collision within one parent, the full UUID is used.
fn child_key(kind: &str, id: Uuid, used: &mut HashSet<String>) -> String {
    let simple = id.simple().to_string();
    let short = format!("{kind}_{}", &simple[..8]);
    let key = if used.contains(&short) {
        format!("{kind}_{simple}")
    } else {
        short
    };
    used.insert(key.clone());
    key
}

fn point2(p: archicraft_model::glam::DVec2) -> Value {
    json!([p.x, p.y])
}

fn rgb(c: archicraft_model::Rgb) -> Value {
    json!(c.0)
}

fn top_link(top: TopLink) -> Value {
    match top {
        TopLink::Fixed { height } => json!({"link": "fixed", "value": height}),
        TopLink::Storey { storey, offset } => json!({"link": "storey", "value": offset, "storey": storey.to_string()}),
        TopLink::Relative { storeys, offset } => json!({"link": "relative", "value": offset, "relative": storeys}),
    }
}

fn reference_line(map: &mut Map<String, Value>, line: ReferenceLine) {
    let code = match line {
        ReferenceLine::Outside => "outside",
        ReferenceLine::Center => "center",
        ReferenceLine::Inside => "inside",
        ReferenceLine::Custom(_) => "custom",
    };
    map.insert("referenceLine".into(), json!(code));
    if let ReferenceLine::Custom(offset) = line {
        map.insert("referenceOffset".into(), json!(offset));
    }
}

fn wall_defaults(d: &WallDefaults) -> Value {
    let mut map = Map::new();
    map.insert("thickness".into(), json!(d.thickness));
    reference_line(&mut map, d.reference_line);
    map.insert("baseOffset".into(), json!(d.base_offset));
    map.insert("top".into(), top_link(d.top));
    map.insert("material".into(), json!(d.material.to_string()));
    map.insert("layer".into(), json!(d.layer.to_string()));
    map.insert("position".into(), json!(d.position.code()));
    map.insert("ifcClass".into(), json!(d.ifc_class.code()));
    Value::Object(map)
}

fn slab_defaults(d: &SlabDefaults) -> Value {
    json!({
        "thickness": d.thickness,
        "reference": d.reference.code(),
        "offset": d.offset,
        "material": d.material.to_string(),
        "layer": d.layer.to_string(),
        "position": d.position.code(),
        "ifcClass": d.ifc_class.code(),
    })
}

fn material_record(m: &Material) -> Value {
    let mut fill = Map::new();
    fill.insert("pattern".into(), json!(m.cut_fill.pattern.code()));
    fill.insert("foreground".into(), rgb(m.cut_fill.foreground));
    if let Some(background) = m.cut_fill.background {
        fill.insert("background".into(), rgb(background));
    }
    json!({
        "id": m.id.to_string(),
        "name": m.name,
        "code": m.code,
        "description": m.description,
        "category": m.category.code(),
        "color": rgb(m.color),
        "opacity": m.opacity,
        "priority": m.priority,
        "cutFill": fill,
    })
}

/// `archicraft::project`: settings and every resource, used or not (IFX-060,
/// IFX-067).
fn project_attribute(p: &Project) -> Value {
    let units = &p.settings.units;
    let grid = &p.settings.plan_grid;
    json!({
        "formatVersion": FORMAT_VERSION.to_string(),
        "info": {
            "number": p.info.number,
            "client": p.info.client,
            "address": p.info.address,
            "author": p.info.author,
        },
        "settings": {
            "lengthUnit": units.length.code(),
            "lengthDecimals": units.length_decimals,
            "angleUnit": units.angle.code(),
            "angleDecimals": units.angle_decimals,
            "slopeFormat": units.slope.code(),
            "areaUnit": units.area.code(),
            "volumeUnit": units.volume.code(),
            "cutPlaneHeight": p.settings.cut_plane_height,
            "chordDeviation": p.settings.tessellation.chord_deviation,
            "maxSegmentAngle": p.settings.tessellation.max_angle_deg,
            "planGrid": {
                "snapStep": [grid.snap_step.x, grid.snap_step.y],
                "snapEnabled": grid.snap_enabled,
                "mainSpacing": [grid.main_spacing.x, grid.main_spacing.y],
                "auxiliarySpacing": [grid.auxiliary_spacing.x, grid.auxiliary_spacing.y],
                "rotation": grid.rotation,
                "origin": [grid.origin.x, grid.origin.y],
                "visible": grid.visible,
            },
        },
        "location": {
            "altitude": p.site.location.altitude,
            "surveyPoint": p.site.location.survey_point,
            "trueNorth": p.site.location.true_north,
        },
        "groundStorey": p.storeys.ground().to_string(),
        "layers": p.layers.iter().map(|l| json!({
            "id": l.id.to_string(),
            "name": l.name,
            "visible": l.visible,
            "locked": l.locked,
        })).collect::<Vec<_>>(),
        "materials": p.materials.iter().map(material_record).collect::<Vec<_>>(),
        "toolDefaults": {
            "wall": wall_defaults(&p.tool_defaults.wall),
            "slab": slab_defaults(&p.tool_defaults.slab),
        },
        "views": p.views.iter().map(|v| json!({
            "id": v.id.to_string(),
            "name": v.name,
            "projection": v.projection.code(),
        })).collect::<Vec<_>>(),
        "counters": counters(&p.counters),
    })
}

/// Name counters (ELM-019) by kind code: every kind that has a tool, so that
/// files keep the same keys, plus any other kind with a non-zero counter.
fn counters(counters: &NameCounters) -> Value {
    KindTag::ALL
        .iter()
        .filter(|kind| kind.descriptor().has_tool || counters.get(**kind) != 0)
        .map(|&kind| (kind.code().to_owned(), json!(counters.get(kind))))
        .collect::<Map<_, _>>()
        .into()
}

fn element_attributes(e: &Element) -> (&'static str, Value) {
    match &e.kind {
        ElementKind::Wall(w) => {
            let mut map = Map::new();
            map.insert("start".into(), point2(w.start));
            map.insert("end".into(), point2(w.end));
            map.insert("thickness".into(), json!(w.thickness));
            reference_line(&mut map, w.reference_line);
            map.insert("baseOffset".into(), json!(w.base_offset));
            map.insert("top".into(), top_link(w.top));
            map.insert("material".into(), json!(w.material.to_string()));
            (schema::WALL, Value::Object(map))
        }
        ElementKind::Slab(s) => (
            schema::SLAB,
            json!({
                "outline": s.outline.iter().map(|p| point2(*p)).collect::<Vec<_>>(),
                "holes": s.holes.iter().map(|h| h.iter().map(|p| point2(*p)).collect::<Vec<_>>()).collect::<Vec<_>>(),
                "thickness": s.thickness,
                "reference": s.reference.code(),
                "offset": s.offset,
                "material": s.material.to_string(),
            }),
        ),
    }
}

/// Mesh points rounded to 10⁻⁶ m (IFX-007) and a triangle list (IFX-032).
fn mesh_attribute(mesh: &TriMesh) -> Value {
    let points: Vec<Value> = mesh
        .points
        .iter()
        .map(|p| json!([round_coordinate(p.x), round_coordinate(p.y), round_coordinate(p.z)]))
        .collect();
    json!({"points": points, "faceVertexIndices": mesh.indices})
}

fn curve_attribute(points: &[DVec3]) -> Value {
    let points: Vec<Value> = points
        .iter()
        .map(|p| json!([round_coordinate(p.x), round_coordinate(p.y), round_coordinate(p.z)]))
        .collect();
    json!({"points": points})
}

/// `usd::xformop` translating by `t`: a row-major 4×4 matrix with the
/// translation in the fourth row (IFX-031). Every placement goes through
/// here, so the translation is a coordinate rounded to 10⁻⁶ m like mesh and
/// curve points (IFX-007); the exact value stays in the `archicraft::`
/// parameter it comes from (NFR-ACC-002).
fn transform(t: DVec3) -> Value {
    let row = [round_coordinate(t.x), round_coordinate(t.y), round_coordinate(t.z)];
    json!({"transform": [[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0], [row[0], row[1], row[2], 1.0]]})
}

/// Builds the IFCX document for a project and its evaluated geometry. The
/// geometry must have been evaluated from the same project revision.
pub fn write(project: &Project, geometry: &ModelGeometry, options: &WriteOptions) -> WriteOutcome {
    let mut warnings = Vec::new();
    let mut data: Vec<Node> = Vec::new();
    let mut used_materials: HashSet<MaterialId> = HashSet::new();

    let project_uuid = project.id.uuid();
    let site_uuid = project.site.id.uuid();
    let building_uuid = project.building.id.uuid();

    let mut root = Node::new(project_uuid.to_string());
    root.children.insert(
        child_key("Site", site_uuid, &mut HashSet::new()),
        Some(site_uuid.to_string()),
    );
    root.attributes.insert("bsi::ifc::class".into(), class("IfcProject"));
    root.attributes
        .insert("bsi::ifc::prop::Name".into(), json!(project.info.name));
    if !project.info.description.is_empty() {
        root.attributes
            .insert("bsi::ifc::prop::Description".into(), json!(project.info.description));
    }
    root.attributes
        .insert(schema::PROJECT.into(), project_attribute(project));
    data.push(root);

    let mut site = Node::new(site_uuid.to_string());
    site.children.insert(
        child_key("Building", building_uuid, &mut HashSet::new()),
        Some(building_uuid.to_string()),
    );
    site.attributes.insert("bsi::ifc::class".into(), class("IfcSite"));
    site.attributes
        .insert("bsi::ifc::prop::Name".into(), json!(project.site.name));
    site.attributes.insert(
        "bsi::ifc::prop::RefElevation".into(),
        json!(project.site.location.altitude),
    );
    data.push(site);

    let mut building = Node::new(building_uuid.to_string());
    let mut storey_keys = HashSet::new();
    for storey in project.storeys.iter() {
        building.children.insert(
            child_key("Storey", storey.id.uuid(), &mut storey_keys),
            Some(storey.id.to_string()),
        );
    }
    building
        .attributes
        .insert("bsi::ifc::class".into(), class("IfcBuilding"));
    building
        .attributes
        .insert("bsi::ifc::prop::Name".into(), json!(project.building.name));
    data.push(building);

    for storey in project.storeys.iter() {
        let mut node = Node::new(storey.id.to_string());
        let mut element_keys = HashSet::new();
        let elements: Vec<&Element> = project.elements_on(storey.id).collect();
        for e in &elements {
            node.children.insert(
                child_key(e.kind.descriptor().node_prefix, e.id.uuid(), &mut element_keys),
                Some(e.id.to_string()),
            );
        }
        node.attributes
            .insert("bsi::ifc::class".into(), class("IfcBuildingStorey"));
        node.attributes
            .insert("bsi::ifc::prop::Name".into(), json!(storey.name));
        node.attributes
            .insert("usd::xformop".into(), transform(DVec3::new(0.0, 0.0, storey.elevation)));
        let mut storey_attr = Map::new();
        storey_attr.insert("elevation".into(), json!(storey.elevation));
        storey_attr.insert("height".into(), json!(storey.height));
        if let Some(cut) = storey.cut_plane_height {
            storey_attr.insert("cutPlaneHeight".into(), json!(cut));
        }
        node.attributes
            .insert(schema::STOREY.into(), Value::Object(storey_attr));
        data.push(node);

        for e in elements {
            write_element(e, geometry, &mut data, &mut used_materials, &mut warnings);
        }
    }

    for material in project.materials.iter().filter(|m| used_materials.contains(&m.id)) {
        let mut node = Node::new(material.id.to_string());
        node.attributes
            .insert("bsi::ifc::prop::Name".into(), json!(material.name));
        node.attributes
            .insert("bsi::ifc::presentation::diffuseColor".into(), rgb(material.color));
        node.attributes
            .insert("bsi::ifc::presentation::opacity".into(), json!(material.opacity));
        data.push(node);
    }

    let file = IfcxFile {
        header: Header {
            id: dataset_id(project),
            ifcx_version: IFCX_VERSION.to_owned(),
            data_version: options.data_version.to_string(),
            author: options.author.clone(),
            timestamp: options.timestamp.clone(),
        },
        imports: [registry::IFC_CORE, registry::IFC_PROP, registry::USD]
            .into_iter()
            .map(|uri| Import {
                uri: uri.to_owned(),
                integrity: None,
            })
            .collect(),
        schemas: schema::declarations(),
        data,
    };
    WriteOutcome { file, warnings }
}

fn write_element(
    e: &Element,
    geometry: &ModelGeometry,
    data: &mut Vec<Node>,
    used_materials: &mut HashSet<MaterialId>,
    warnings: &mut Vec<Problem>,
) {
    let mut node = Node::new(e.id.to_string());
    let mut parts: Vec<Node> = Vec::new();
    match geometry.get(e.id) {
        Some(g) => {
            for part in &g.parts {
                let id = sub_node_id(e.id.uuid(), &part.name);
                node.children.insert(part.name.clone(), Some(id.to_string()));
                let mut body = Node::new(id.to_string());
                body.inherits.insert("material".into(), Some(part.material.to_string()));
                body.attributes
                    .insert("usd::usdgeom::mesh".into(), mesh_attribute(&part.mesh));
                used_materials.insert(part.material);
                parts.push(body);
            }
            if !g.axis.is_empty() {
                let id = sub_node_id(e.id.uuid(), "Axis");
                node.children.insert("Axis".into(), Some(id.to_string()));
                let mut axis = Node::new(id.to_string());
                axis.attributes
                    .insert("usd::usdgeom::basiscurves".into(), curve_attribute(&g.axis));
                parts.push(axis);
            }
        }
        None => warnings.push(Problem::warning(
            Some(&e.id.to_string()),
            format!("{} has no valid geometry and was saved without a body.", e.name),
        )),
    }
    node.attributes
        .insert("bsi::ifc::class".into(), class(e.ifc_class.code()));
    node.attributes.insert("bsi::ifc::prop::Name".into(), json!(e.name));
    if let Some(external) = e.position.is_external() {
        node.attributes
            .insert("bsi::ifc::prop::IsExternal".into(), json!(external));
    }
    node.attributes.insert(
        schema::ELEMENT.into(),
        json!({
            "kind": e.tag().code(),
            "layer": e.layer.to_string(),
            "locked": e.locked,
            "position": e.position.code(),
        }),
    );
    let (key, value) = element_attributes(e);
    node.attributes.insert(key.into(), value);
    data.push(node);
    data.extend(parts);
}

/// Serialises a document deterministically (IFX-007), compact or indented
/// (IFX-008).
pub fn to_bytes(file: &IfcxFile, pretty: bool) -> Vec<u8> {
    // The container types always serialise.
    let value = serde_json::to_value(file).unwrap_or(Value::Null);
    crate::json::to_string(&value, pretty).into_bytes()
}
