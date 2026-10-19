//! Snapping of point input (INP-008, INP-009).

use archicraft_geometry::plan::{PlanDrawing, polygon_rings};
use archicraft_model::glam::DVec2;
use archicraft_model::{Project, StoreyId};

/// Kinds of snap, in order of precedence (INP-009).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SnapKind {
    Node,
    Intersection,
    Midpoint,
    Perpendicular,
    Edge,
    Grid,
    Constrained,
    Typed,
    Free,
}

impl SnapKind {
    pub fn label(self) -> &'static str {
        match self {
            SnapKind::Node => "node",
            SnapKind::Intersection => "intersection",
            SnapKind::Midpoint => "midpoint",
            SnapKind::Perpendicular => "perpendicular",
            SnapKind::Edge => "on edge",
            SnapKind::Grid => "grid",
            SnapKind::Constrained => "constrained",
            SnapKind::Typed => "typed",
            SnapKind::Free => "",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snap {
    pub point: DVec2,
    pub kind: SnapKind,
}

/// Snap targets of one storey's plan (INP-008).
#[derive(Clone, Debug, Default)]
pub struct SnapTargets {
    pub nodes: Vec<DVec2>,
    pub edges: Vec<[DVec2; 2]>,
}

impl SnapTargets {
    pub fn from_plan(plan: &PlanDrawing, project: &Project, storey: StoreyId) -> Self {
        let mut targets = SnapTargets::default();
        for element in &plan.elements {
            for polygon in &element.cut {
                for ring in polygon_rings(polygon) {
                    for i in 0..ring.len() {
                        targets.nodes.push(ring[i]);
                        targets.edges.push([ring[i], ring[(i + 1) % ring.len()]]);
                    }
                }
            }
            for segment in element.projected.iter().chain(&element.overhead) {
                targets.nodes.extend(segment);
                targets.edges.push(*segment);
            }
        }
        for element in project.elements_on(storey).filter(|e| project.is_visible(e)) {
            if let Some(wall) = element.as_wall() {
                targets.nodes.extend([wall.start, wall.end]);
                targets.edges.push([wall.start, wall.end]);
            }
        }
        targets
    }
}

fn closest_on_segment(p: DVec2, [a, b]: [DVec2; 2]) -> DVec2 {
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 > 0.0 {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    a + ab * t
}

fn segment_intersection([a, b]: [DVec2; 2], [c, d]: [DVec2; 2]) -> Option<DVec2> {
    let r = b - a;
    let s = d - c;
    let denom = r.perp_dot(s);
    if denom.abs() < 1e-12 {
        return None;
    }
    let t = (c - a).perp_dot(s) / denom;
    let u = (c - a).perp_dot(r) / denom;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then(|| a + r * t)
}

/// Finds the snap point for `cursor` within `radius` (model units), with the
/// fixed precedence of INP-009: nodes, intersections, midpoints,
/// perpendicular feet from the previous point, edges, then the snap grid.
pub fn snap(targets: &SnapTargets, cursor: DVec2, radius: f64, grid_step: f64, previous: Option<DVec2>) -> Snap {
    let mut best: Option<(SnapKind, f64, DVec2)> = None;
    let mut consider = |kind: SnapKind, p: DVec2| {
        let d = p.distance(cursor);
        if d <= radius && best.is_none_or(|(k, bd, _)| (kind, d) < (k, bd)) {
            best = Some((kind, d, p));
        }
    };
    for &n in &targets.nodes {
        consider(SnapKind::Node, n);
    }
    let near: Vec<[DVec2; 2]> = targets
        .edges
        .iter()
        .copied()
        .filter(|e| closest_on_segment(cursor, *e).distance(cursor) <= radius)
        .collect();
    for (i, a) in near.iter().enumerate() {
        for b in &near[i + 1..] {
            if let Some(p) = segment_intersection(*a, *b) {
                consider(SnapKind::Intersection, p);
            }
        }
    }
    for e in &near {
        consider(SnapKind::Midpoint, (e[0] + e[1]) / 2.0);
        if let Some(prev) = previous {
            let dir = (e[1] - e[0]).normalize_or_zero();
            let foot = e[0] + dir * (prev - e[0]).dot(dir);
            if closest_on_segment(foot, *e).distance(foot) < 1e-9 {
                consider(SnapKind::Perpendicular, foot);
            }
        }
        consider(SnapKind::Edge, closest_on_segment(cursor, *e));
    }
    if let Some((kind, _, point)) = best {
        return Snap { point, kind };
    }
    if grid_step > 0.0 {
        let point = (cursor / grid_step).round() * grid_step;
        if point.distance(cursor) <= radius {
            return Snap {
                point,
                kind: SnapKind::Grid,
            };
        }
    }
    Snap {
        point: cursor,
        kind: SnapKind::Free,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapping_precedence() {
        let targets = SnapTargets {
            nodes: vec![DVec2::new(1.0, 1.0)],
            edges: vec![
                [DVec2::new(0.0, 0.0), DVec2::new(4.0, 0.0)],
                [DVec2::new(2.0, -2.0), DVec2::new(2.0, 2.0)],
            ],
        };
        let s = snap(&targets, DVec2::new(1.05, 0.98), 0.2, 0.05, None);
        assert_eq!(s.kind, SnapKind::Node);
        let s = snap(&targets, DVec2::new(2.1, 0.1), 0.2, 0.05, None);
        assert_eq!((s.kind, s.point), (SnapKind::Intersection, DVec2::new(2.0, 0.0)));
        let s = snap(&targets, DVec2::new(3.0, 0.05), 0.2, 0.0, None);
        assert_eq!(s.kind, SnapKind::Edge);
        let s = snap(&targets, DVec2::new(10.02, 10.01), 0.2, 0.05, None);
        assert_eq!(s.kind, SnapKind::Grid);
        assert!(s.point.distance(DVec2::new(10.0, 10.0)) < 1e-12);
    }
}
