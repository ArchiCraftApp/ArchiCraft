//! Snapping of point input (INP-008, INP-009).
//!
//! Points closer than the model tolerance are one point (NFR-ACC-003):
//! coincident nodes are one target, and coincident candidates are one
//! candidate before precedence decides.

use archicraft_model::glam::DVec2;
use archicraft_model::tolerance::{coincident, is_parallel};

use super::targets::SnapIndex;

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

pub(crate) fn closest_on_segment(p: DVec2, [a, b]: [DVec2; 2]) -> DVec2 {
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 > 0.0 {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    a + ab * t
}

pub(super) fn segment_intersection([a, b]: [DVec2; 2], [c, d]: [DVec2; 2]) -> Option<DVec2> {
    let r = b - a;
    let s = d - c;
    let denom = r.perp_dot(s);
    if is_parallel(r, s) {
        return None;
    }
    let t = (c - a).perp_dot(s) / denom;
    let u = (c - a).perp_dot(r) / denom;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then(|| a + r * t)
}

/// Finds the snap point for `cursor` within `radius` (model units), with the
/// fixed precedence of INP-009: nodes, intersections, midpoints,
/// perpendicular feet from the previous point, edges, then the snap grid.
/// The targets are those of `index` near the cursor and the consumer's
/// `pending` points, which are not in the index (NFR-PERF-005).
///
/// Candidates within the model tolerance are one point (NFR-ACC-003): a
/// candidate coincident with the best so far only raises its precedence and
/// keeps its point, so the target found first wins, however near the cursor
/// the other one lies.
pub fn snap(
    index: &SnapIndex,
    pending: &[DVec2],
    cursor: DVec2,
    radius: f64,
    grid_step: f64,
    previous: Option<DVec2>,
) -> Snap {
    let mut nodes = index.nodes_near(cursor, radius);
    nodes.extend_from_slice(pending);
    let near = index.edges_near(cursor, radius);
    choose(&nodes, &near, cursor, radius, grid_step, previous)
}

/// The snap among `nodes` and the `near` edges, which lie within `radius`
/// of the cursor, in the order of precedence of [`snap`].
pub(super) fn choose(
    nodes: &[DVec2],
    near: &[[DVec2; 2]],
    cursor: DVec2,
    radius: f64,
    grid_step: f64,
    previous: Option<DVec2>,
) -> Snap {
    let mut best: Option<(SnapKind, f64, DVec2)> = None;
    let mut consider = |kind: SnapKind, p: DVec2| {
        let d = p.distance(cursor);
        if d > radius {
            return;
        }
        best = match best {
            Some((k, bd, bp)) if coincident(p, bp) => Some((kind.min(k), bd, bp)),
            Some((k, bd, _)) if (k, bd) <= (kind, d) => best,
            _ => Some((kind, d, p)),
        };
    };
    for &n in nodes {
        consider(SnapKind::Node, n);
    }
    for (i, a) in near.iter().enumerate() {
        for b in &near[i + 1..] {
            if let Some(p) = segment_intersection(*a, *b) {
                consider(SnapKind::Intersection, p);
            }
        }
    }
    for e in near {
        consider(SnapKind::Midpoint, (e[0] + e[1]) / 2.0);
        if let Some(prev) = previous {
            let dir = (e[1] - e[0]).normalize_or_zero();
            let foot = e[0] + dir * (prev - e[0]).dot(dir);
            if coincident(closest_on_segment(foot, *e), foot) {
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
    use archicraft_geometry::plan::PlanDrawing;
    use archicraft_model::template::default_metric_project;
    use archicraft_model::{Command, History};

    use super::*;
    use crate::input::targets::tests::index_of;

    #[test]
    fn snapping_precedence() {
        let targets = index_of(
            &[DVec2::new(1.0, 1.0)],
            &[
                [DVec2::new(0.0, 0.0), DVec2::new(4.0, 0.0)],
                [DVec2::new(2.0, -2.0), DVec2::new(2.0, 2.0)],
            ],
        );
        let s = snap(&targets, &[], DVec2::new(1.05, 0.98), 0.2, 0.05, None);
        assert_eq!(s.kind, SnapKind::Node);
        let s = snap(&targets, &[], DVec2::new(2.1, 0.1), 0.2, 0.05, None);
        assert_eq!((s.kind, s.point), (SnapKind::Intersection, DVec2::new(2.0, 0.0)));
        let s = snap(&targets, &[], DVec2::new(3.0, 0.05), 0.2, 0.0, None);
        assert_eq!(s.kind, SnapKind::Edge);
        let s = snap(&targets, &[], DVec2::new(10.02, 10.01), 0.2, 0.05, None);
        assert_eq!(s.kind, SnapKind::Grid);
        assert!(s.point.distance(DVec2::new(10.0, 10.0)) < 1e-12);
    }

    const MICROMETRE: f64 = 1e-6;

    /// NFR-ACC-003: plan nodes closer than the model tolerance are one snap
    /// target, at the point stored first.
    #[test]
    fn nodes_within_the_tolerance_are_one_target() {
        let mut project = default_metric_project();
        let storey = project.storeys.ground();
        let corner = DVec2::new(4.0, 0.0);
        let near = corner + DVec2::new(3.0, 4.0) * MICROMETRE;
        let walls = vec![
            project.new_wall(storey, DVec2::ZERO, corner, 0),
            project.new_wall(storey, near, DVec2::new(4.0, 3.0), 1),
        ];
        History::default()
            .execute(&mut project, Command::AddElements(walls))
            .unwrap();
        let mut targets = SnapIndex::default();
        targets.update(&project, storey, &PlanDrawing::default());
        assert_eq!(targets.nodes_near(corner, 1e-3), [corner], "5 µm apart: one target");
        assert_eq!(targets.nodes_near(corner, 100.0).len(), 3);
        for cursor in [corner + DVec2::new(0.05, 0.0), near + DVec2::new(0.0, 0.05)] {
            let s = snap(&targets, &[], cursor, 0.2, 0.0, None);
            assert_eq!((s.kind, s.point), (SnapKind::Node, corner));
        }
    }

    /// NFR-ACC-003: candidates closer than the model tolerance are one point:
    /// the first one stored wins, whichever is nearer the cursor, and a
    /// coincident candidate of lower precedence does not move it.
    #[test]
    fn coincident_candidates_are_one_point() {
        let first = DVec2::new(1.0, 1.0);
        let second = first + DVec2::new(5.0 * MICROMETRE, 0.0);
        let targets = index_of(&[first, second], &[[second, second + DVec2::new(2.0, 0.0)]]);
        let s = snap(&targets, &[], second + DVec2::new(0.01, 0.0), 0.2, 0.0, None);
        assert_eq!((s.kind, s.point), (SnapKind::Node, first));
        // 20 µm apart: two points, the nearer node wins.
        let apart = first + DVec2::new(20.0 * MICROMETRE, 0.0);
        let targets = index_of(&[first, apart], &[]);
        let s = snap(&targets, &[], apart + DVec2::new(0.01, 0.0), 0.2, 0.0, None);
        assert_eq!((s.kind, s.point), (SnapKind::Node, apart));
    }

    /// NFR-ACC-003: edges that are not parallel within the angle tolerance
    /// intersect, however short their overlap; parallel ones do not.
    #[test]
    fn intersections_use_the_angle_tolerance() {
        let a = [DVec2::new(-1.0, 0.0), DVec2::new(1.0, 0.0)];
        assert_eq!(
            segment_intersection(a, [DVec2::new(0.0, -1.0), DVec2::new(0.0, 1.0)]),
            Some(DVec2::ZERO)
        );
        let tilt = DVec2::from_angle(0.0001_f64.to_radians());
        assert_eq!(
            segment_intersection(a, [-tilt, tilt]),
            None,
            "parallel within the tolerance"
        );
        let tilt = DVec2::from_angle(0.01_f64.to_radians());
        assert!(segment_intersection(a, [-tilt, tilt]).is_some_and(|p| p.length() < MICROMETRE));
        assert_eq!(
            segment_intersection(a, [DVec2::ONE, DVec2::ONE]),
            None,
            "a point is no edge"
        );
    }

    /// NFR-ACC-003: the perpendicular foot from the previous point snaps when
    /// it lies on the edge within the tolerance.
    #[test]
    fn perpendicular_foot_on_the_edge() {
        let targets = index_of(&[], &[[DVec2::ZERO, DVec2::new(4.0, 0.0)]]);
        let previous = Some(DVec2::new(4.0 + 5.0 * MICROMETRE, 3.0));
        let s = snap(&targets, &[], DVec2::new(3.95, 0.1), 0.2, 0.0, previous);
        assert_eq!(s.kind, SnapKind::Perpendicular);
        let previous = Some(DVec2::new(4.0 + 20.0 * MICROMETRE, 3.0));
        let s = snap(&targets, &[], DVec2::new(3.95, 0.1), 0.2, 0.0, previous);
        assert_eq!(s.kind, SnapKind::Edge, "the foot lies beyond the edge");
    }
}
