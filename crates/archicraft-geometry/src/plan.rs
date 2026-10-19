//! Plan drawings generated from the 3D bodies (VIEW-010, WAL-027, DSP-011,
//! DSP-012).
//!
//! A storey's plan is cut by a horizontal plane at the cut-plane height:
//! parts that the plane intersects are drawn **cut** (section outline and the
//! material's cut fill), parts below it down to the bottom of the view range
//! are drawn **projected** (their top outline), and parts above it up to the
//! top of the view range are drawn **overhead** (dashed). Cut regions of the
//! same material are merged, so joined walls show no seam. A part whose top
//! or bottom lies within the model tolerance of the cut plane is not cut
//! (NFR-ACC-003).

use std::collections::BTreeMap;

use archicraft_model::{Element, ElementId, MODEL_TOLERANCE, MaterialId, Project, StoreyId};
use geo::{LineString, MultiPolygon, Polygon};
use glam::DVec2;

use crate::evaluate::ModelGeometry;
use crate::slice::{loops_to_polygons, slice_loops};
use crate::triangulate::triangulate;

/// What one element contributes to a plan.
#[derive(Clone, Debug)]
pub struct PlanElement {
    pub id: ElementId,
    /// Section polygons of this element.
    pub cut: Vec<Polygon<f64>>,
    /// Uncut lines below the cut plane.
    pub projected: Vec<[DVec2; 2]>,
    /// Lines above the cut plane, drawn dashed.
    pub overhead: Vec<[DVec2; 2]>,
    /// Wall reference line (WAL-007).
    pub reference_line: Option<[DVec2; 2]>,
}

/// A filled triangle mesh in plan coordinates.
#[derive(Clone, Debug, Default)]
pub struct FillMesh {
    pub points: Vec<DVec2>,
    pub triangles: Vec<[u32; 3]>,
}

/// Merged cut regions of one material.
#[derive(Clone, Debug)]
pub struct CutRegion {
    pub material: MaterialId,
    pub polygons: MultiPolygon<f64>,
    pub fill: FillMesh,
}

/// The plan of one storey.
#[derive(Clone, Debug, Default)]
pub struct PlanDrawing {
    pub storey: Option<StoreyId>,
    /// Elements in model order (later ones are drawn on top).
    pub elements: Vec<PlanElement>,
    pub cut_regions: Vec<CutRegion>,
}

/// The vertical range of a plan view, relative to the storey elevation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewRange {
    pub cut: f64,
    pub bottom: f64,
    pub top: f64,
}

impl ViewRange {
    /// The default range of a storey (VIEW-010): from the storey elevation to
    /// the next storey, cut at the cut-plane height.
    pub fn for_storey(project: &Project, storey: StoreyId) -> Self {
        let height = project.storeys.get(storey).map_or(3.0, |s| s.height);
        Self {
            cut: project.cut_plane_height(storey),
            bottom: 0.0,
            top: height,
        }
    }
}

fn ring_coords(ring: &LineString<f64>) -> Vec<DVec2> {
    let mut points: Vec<DVec2> = ring.0.iter().map(|c| DVec2::new(c.x, c.y)).collect();
    if points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
    points
}

fn fill_mesh(polygons: &MultiPolygon<f64>) -> FillMesh {
    let mut mesh = FillMesh::default();
    for polygon in polygons {
        let outer = ring_coords(polygon.exterior());
        let holes: Vec<Vec<DVec2>> = polygon.interiors().iter().map(ring_coords).collect();
        match triangulate(&outer, &holes) {
            Ok(triangles) => {
                let base = mesh.points.len() as u32;
                mesh.points.extend(outer.iter().chain(holes.iter().flatten()));
                mesh.triangles
                    .extend(triangles.iter().map(|t| t.map(|i| base + i as u32)));
            }
            Err(e) => tracing::debug!("cut fill not triangulated: {e}"),
        }
    }
    mesh
}

/// Generates the plan of `storey`. `shown` decides which elements appear
/// (layer visibility and view filters); elements are drawn on their home
/// storey.
pub fn plan_drawing(
    project: &Project,
    geometry: &ModelGeometry,
    storey: StoreyId,
    range: ViewRange,
    shown: impl Fn(&Element) -> bool,
) -> PlanDrawing {
    let mut elements = Vec::new();
    let mut by_material: BTreeMap<MaterialId, Vec<Polygon<f64>>> = BTreeMap::new();
    for element in project.elements_on(storey).filter(|e| shown(e)) {
        let Some(body) = geometry.get(element.id) else {
            continue;
        };
        let mut plan = PlanElement {
            id: element.id,
            cut: Vec::new(),
            projected: Vec::new(),
            overhead: Vec::new(),
            reference_line: None,
        };
        for part in &body.parts {
            let Some((lo, hi)) = part.mesh.bounds() else {
                continue;
            };
            if lo.z < range.cut - MODEL_TOLERANCE && hi.z > range.cut + MODEL_TOLERANCE {
                let polygons = loops_to_polygons(slice_loops(&part.mesh, range.cut));
                by_material
                    .entry(part.material)
                    .or_default()
                    .extend(polygons.iter().cloned());
                plan.cut.extend(polygons);
            } else if hi.z <= range.cut + MODEL_TOLERANCE && hi.z >= range.bottom - MODEL_TOLERANCE {
                plan.projected.extend(part.mesh.up_facing_outline());
            } else if lo.z >= range.cut - MODEL_TOLERANCE && lo.z <= range.top + MODEL_TOLERANCE {
                plan.overhead.extend(part.mesh.down_facing_outline());
            }
        }
        if let Some(wall) = element.as_wall() {
            plan.reference_line = Some([wall.start, wall.end]);
        }
        elements.push(plan);
    }
    let cut_regions = by_material
        .into_iter()
        .map(|(material, polygons)| {
            let merged = geo::unary_union(&polygons);
            let fill = fill_mesh(&merged);
            CutRegion {
                material,
                polygons: merged,
                fill,
            }
        })
        .collect();
    PlanDrawing {
        storey: Some(storey),
        elements,
        cut_regions,
    }
}

fn point_in_polygon(p: DVec2, polygon: &Polygon<f64>) -> bool {
    let inside = |ring: &LineString<f64>| {
        let pts = ring_coords(ring);
        if pts.len() < 3 {
            return false;
        }
        let mut inside = false;
        let mut j = pts.len() - 1;
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[j]);
            if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
                inside = !inside;
            }
            j = i;
        }
        inside
    };
    inside(polygon.exterior()) && !polygon.interiors().iter().any(inside)
}

fn segment_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 > 0.0 {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(a + ab * t)
}

fn segments_intersect(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> bool {
    let o = |p: DVec2, q: DVec2, r: DVec2| (q - p).perp_dot(r - p);
    let (d1, d2, d3, d4) = (o(c, d, a), o(c, d, b), o(a, b, c), o(a, b, d));
    (d1 > 0.0) != (d2 > 0.0) && (d3 > 0.0) != (d4 > 0.0)
}

impl PlanElement {
    /// All outline segments of this element.
    pub fn segments(&self) -> Vec<[DVec2; 2]> {
        let mut out: Vec<[DVec2; 2]> = Vec::new();
        for polygon in &self.cut {
            for ring in std::iter::once(polygon.exterior()).chain(polygon.interiors()) {
                let pts = ring_coords(ring);
                for i in 0..pts.len() {
                    out.push([pts[i], pts[(i + 1) % pts.len()]]);
                }
            }
        }
        out.extend(self.projected.iter().chain(&self.overhead).copied());
        out
    }

    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        self.segments()
            .iter()
            .flatten()
            .fold(None, |acc: Option<(DVec2, DVec2)>, p| match acc {
                None => Some((*p, *p)),
                Some((lo, hi)) => Some((lo.min(*p), hi.max(*p))),
            })
    }

    /// True if `p` is on this element: inside a cut region or within
    /// `tolerance` of one of its lines.
    pub fn hit(&self, p: DVec2, tolerance: f64) -> bool {
        self.cut.iter().any(|poly| point_in_polygon(p, poly))
            || self
                .segments()
                .iter()
                .any(|[a, b]| segment_distance(p, *a, *b) <= tolerance)
    }

    /// Area selection (SEL-003): fully inside the rectangle, or touching it.
    pub fn in_rect(&self, min: DVec2, max: DVec2, fully_inside: bool) -> bool {
        let Some((lo, hi)) = self.bounds() else {
            return false;
        };
        if fully_inside {
            return lo.x >= min.x && lo.y >= min.y && hi.x <= max.x && hi.y <= max.y;
        }
        if hi.x < min.x || hi.y < min.y || lo.x > max.x || lo.y > max.y {
            return false;
        }
        let inside = |p: DVec2| p.x >= min.x && p.x <= max.x && p.y >= min.y && p.y <= max.y;
        let corners = [min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)];
        let segments = self.segments();
        segments.iter().flatten().any(|p| inside(*p))
            || segments
                .iter()
                .any(|[a, b]| (0..4).any(|i| segments_intersect(*a, *b, corners[i], corners[(i + 1) % 4])))
            || self.cut.iter().any(|poly| point_in_polygon((min + max) / 2.0, poly))
    }
}

impl PlanDrawing {
    /// Elements under `p`, topmost first: elements whose cut region contains
    /// the point come before elements that are only near a line (SEL-001).
    pub fn pick(&self, p: DVec2, tolerance: f64) -> Vec<ElementId> {
        let mut inside: Vec<ElementId> = Vec::new();
        let mut near: Vec<ElementId> = Vec::new();
        for element in self.elements.iter().rev() {
            if element.cut.iter().any(|poly| point_in_polygon(p, poly)) {
                inside.push(element.id);
            } else if element.hit(p, tolerance) {
                near.push(element.id);
            }
        }
        inside.extend(near);
        inside
    }

    pub fn element(&self, id: ElementId) -> Option<&PlanElement> {
        self.elements.iter().find(|e| e.id == id)
    }

    /// Bounds of everything drawn, for "fit to window".
    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        self.elements
            .iter()
            .filter_map(PlanElement::bounds)
            .reduce(|(lo, hi), (l, h)| (lo.min(l), hi.max(h)))
    }
}

/// Converts a plan polygon into rings without the repeated closing point.
pub fn polygon_rings(polygon: &Polygon<f64>) -> Vec<Vec<DVec2>> {
    std::iter::once(polygon.exterior())
        .chain(polygon.interiors())
        .map(ring_coords)
        .collect()
}
