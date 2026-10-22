//! Snapping of point input (INP-008, INP-009).
//!
//! Every snap target near the cursor is a candidate; the candidates are
//! ordered by a fixed precedence (INP-009), and the first one wins unless
//! the user cycles through them with Tab. The precedence has the seven
//! levels of the SRS, and inside a level a fixed rank among the kinds of
//! that level, before the distance from the cursor decides:
//!
//! | Level              | Kinds, by rank                                   |
//! |--------------------|--------------------------------------------------|
//! | 1 node             | node, handle, centre, axis point, centroid       |
//! | 2 intersection     | intersection                                     |
//! | 3 division point   | midpoint                                         |
//! | 4 guide intersection | guide intersection                             |
//! | 5 edge             | perpendicular foot, tangent point, point on edge |
//! | 6 guide            | point on a guide                                 |
//! | 7 grid             | grid point                                       |
//!
//! Derived points sit in the level whose nature they share. Perpendicular
//! and tangent points outrank the plain point on the same edge, which is
//! always nearer the cursor.
//!
//! Points closer than the model tolerance are one point (NFR-ACC-003):
//! coincident nodes are one target, and coincident candidates are one
//! candidate, at the point found first and with the best kind of them.

use std::f64::consts::TAU;

use archicraft_model::MODEL_TOLERANCE;
use archicraft_model::glam::DVec2;
use archicraft_model::tolerance::{coincident, is_parallel};

use super::guide::{Guide, GuideShape};
use super::targets::SnapIndex;

/// The seven levels of snap precedence (INP-009), best first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SnapLevel {
    Node,
    Intersection,
    Division,
    GuideIntersection,
    Edge,
    Guide,
    Grid,
}

/// Kinds of snap: the targets of each precedence level in their rank within
/// the level (INP-008, INP-009), then the results of point input that are
/// no snap target. The order of the variants is the order of precedence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SnapKind {
    /// An element node: a corner or an end.
    Node,
    /// A handle of a selected element.
    Handle,
    /// The centre of an arc or a circle.
    Centre,
    /// An axis point of a column or a beam.
    AxisPoint,
    /// The centroid of a polygon (INP-024).
    Centroid,
    Intersection,
    /// The midpoint of an edge, the division point of mode half (INP-010).
    Midpoint,
    GuideIntersection,
    /// The foot of the perpendicular from the previous point.
    Perpendicular,
    /// A tangent point from the previous point.
    Tangent,
    /// The point of an edge nearest the cursor.
    Edge,
    /// The point of a guide nearest the cursor.
    Guide,
    Grid,
    /// A point on the angle constraint (INP-006).
    Constrained,
    /// A point from typed values (INP-003).
    Typed,
    /// The cursor itself: nothing to snap to.
    Free,
}

impl SnapKind {
    /// The kinds of snap targets, in order of precedence.
    pub const TARGETS: [SnapKind; 13] = [
        SnapKind::Node,
        SnapKind::Handle,
        SnapKind::Centre,
        SnapKind::AxisPoint,
        SnapKind::Centroid,
        SnapKind::Intersection,
        SnapKind::Midpoint,
        SnapKind::GuideIntersection,
        SnapKind::Perpendicular,
        SnapKind::Tangent,
        SnapKind::Edge,
        SnapKind::Guide,
        SnapKind::Grid,
    ];

    /// Every kind: the snap targets in order of precedence, then the points
    /// of point input that are no snap target.
    pub const ALL: [SnapKind; 16] = [
        SnapKind::Node,
        SnapKind::Handle,
        SnapKind::Centre,
        SnapKind::AxisPoint,
        SnapKind::Centroid,
        SnapKind::Intersection,
        SnapKind::Midpoint,
        SnapKind::GuideIntersection,
        SnapKind::Perpendicular,
        SnapKind::Tangent,
        SnapKind::Edge,
        SnapKind::Guide,
        SnapKind::Grid,
        SnapKind::Constrained,
        SnapKind::Typed,
        SnapKind::Free,
    ];

    /// The precedence level of a snap target, or `None` for a typed,
    /// constrained or free point.
    pub fn level(self) -> Option<SnapLevel> {
        Some(match self {
            SnapKind::Node | SnapKind::Handle | SnapKind::Centre | SnapKind::AxisPoint | SnapKind::Centroid => {
                SnapLevel::Node
            }
            SnapKind::Intersection => SnapLevel::Intersection,
            SnapKind::Midpoint => SnapLevel::Division,
            SnapKind::GuideIntersection => SnapLevel::GuideIntersection,
            SnapKind::Perpendicular | SnapKind::Tangent | SnapKind::Edge => SnapLevel::Edge,
            SnapKind::Guide => SnapLevel::Guide,
            SnapKind::Grid => SnapLevel::Grid,
            SnapKind::Constrained | SnapKind::Typed | SnapKind::Free => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            SnapKind::Node => "node",
            SnapKind::Handle => "handle",
            SnapKind::Centre => "centre",
            SnapKind::AxisPoint => "axis point",
            SnapKind::Centroid => "centroid",
            SnapKind::Intersection => "intersection",
            SnapKind::Midpoint => "midpoint",
            SnapKind::GuideIntersection => "guide intersection",
            SnapKind::Perpendicular => "perpendicular",
            SnapKind::Tangent => "tangent",
            SnapKind::Edge => "on edge",
            SnapKind::Guide => "guide",
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

/// A circle or an arc among the snap targets (INP-008): its centre and
/// radius, and the angles it spans, from `start` by `sweep` radians,
/// counter-clockwise when the sweep is positive. A circle spans a full turn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArcTarget {
    pub centre: DVec2,
    pub radius: f64,
    pub start: f64,
    pub sweep: f64,
}

impl ArcTarget {
    pub fn circle(centre: DVec2, radius: f64) -> Self {
        Self {
            centre,
            radius,
            start: 0.0,
            sweep: TAU,
        }
    }

    /// True if the point at `angle` lies on the arc, its ends within the
    /// model tolerance along the curve (NFR-ACC-003).
    pub fn spans(&self, angle: f64) -> bool {
        let sweep = self.sweep.abs();
        if sweep >= TAU {
            return true;
        }
        let from = if self.sweep < 0.0 {
            self.start - sweep
        } else {
            self.start
        };
        let past_start = (angle - from).rem_euclid(TAU);
        let slack = MODEL_TOLERANCE / self.radius.max(MODEL_TOLERANCE);
        past_start <= sweep + slack || past_start >= TAU - slack
    }

    /// The points where lines from `from` touch the arc (INP-008):
    /// `c + r·(cos(θ ± φ), sin(θ ± φ))` with θ the direction from the centre
    /// `c` to `from` and φ = acos(r / |from − c|). None from inside the
    /// circle, one from a point on it, and of an arc only those it spans.
    pub fn tangent_points(&self, from: DVec2) -> Vec<DVec2> {
        let offset = from - self.centre;
        let distance = offset.length();
        if distance < self.radius - MODEL_TOLERANCE || distance == 0.0 {
            return Vec::new();
        }
        let theta = offset.y.atan2(offset.x);
        let phi = (self.radius / distance).min(1.0).acos();
        let mut points: Vec<DVec2> = Vec::new();
        for angle in [theta + phi, theta - phi] {
            let point = self.centre + DVec2::from_angle(angle) * self.radius;
            if self.spans(angle) && !points.iter().any(|p| coincident(*p, point)) {
                points.push(point);
            }
        }
        points
    }

    /// The distance from `p` to the nearer of the centre and the circle: no
    /// more than the distance to any target of the arc.
    pub(crate) fn reach(&self, p: DVec2) -> f64 {
        let distance = p.distance(self.centre);
        distance.min((distance - self.radius).abs())
    }
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

/// The snap point for `cursor`: the first of its [`candidates`] without
/// guides, or the cursor itself.
pub fn snap(
    index: &SnapIndex,
    pending: &[DVec2],
    cursor: DVec2,
    radius: f64,
    grid_step: f64,
    previous: Option<DVec2>,
) -> Snap {
    let found = candidates(index, pending, cursor, radius, grid_step, previous, &[]);
    found.first().copied().unwrap_or(Snap {
        point: cursor,
        kind: SnapKind::Free,
    })
}

/// The snap candidates within `radius` (model units) of `cursor`, in the
/// order of precedence of INP-009 (see the module documentation), ties
/// broken by the distance from the cursor. The targets are those of `index`
/// near the cursor, the consumer's `pending` points, which are not in the
/// index (NFR-PERF-005), and the `guides` shown; perpendicular feet and
/// tangent points start at `previous`, and the snap grid has the spacing
/// `grid_step` (none when zero).
///
/// Candidates within the model tolerance are one point (NFR-ACC-003): a
/// candidate coincident with one found before only raises its precedence and
/// keeps its point, so the target found first stays, however near the
/// cursor the other one lies.
pub fn candidates(
    index: &SnapIndex,
    pending: &[DVec2],
    cursor: DVec2,
    radius: f64,
    grid_step: f64,
    previous: Option<DVec2>,
    guides: &[Guide],
) -> Vec<Snap> {
    let mut nodes = index.targets_near(cursor, radius);
    nodes.extend(pending.iter().map(|&point| Snap {
        point,
        kind: SnapKind::Node,
    }));
    let near = index.edges_near(cursor, radius);
    let arcs = index.arcs_near(cursor, radius);
    let guides: Vec<GuideShape> = guides
        .iter()
        .map(|g| g.shape)
        .filter(|shape| shape.distance(cursor) <= radius)
        .collect();
    let targets = Targets {
        nodes: &nodes,
        edges: &near,
        arcs: &arcs,
        guides: &guides,
    };
    collect(&targets, cursor, radius, grid_step, previous)
}

/// The targets near the cursor, from the index and beyond it.
pub(super) struct Targets<'a> {
    /// Nodes with their node-level kind.
    pub nodes: &'a [Snap],
    pub edges: &'a [[DVec2; 2]],
    pub arcs: &'a [ArcTarget],
    pub guides: &'a [GuideShape],
}

/// The candidates among the `targets`, which lie within `radius` of the
/// cursor, in the order of [`candidates`].
pub(super) fn collect(
    &Targets {
        nodes,
        edges: near,
        arcs,
        guides,
    }: &Targets,
    cursor: DVec2,
    radius: f64,
    grid_step: f64,
    previous: Option<DVec2>,
) -> Vec<Snap> {
    let mut found: Vec<Snap> = Vec::new();
    let mut add = |kind: SnapKind, point: DVec2| {
        if point.distance(cursor) > radius {
            return;
        }
        match found.iter_mut().find(|s| coincident(s.point, point)) {
            Some(s) => s.kind = s.kind.min(kind),
            None => found.push(Snap { point, kind }),
        }
    };
    for n in nodes {
        add(n.kind, n.point);
    }
    for (i, a) in near.iter().enumerate() {
        for b in &near[i + 1..] {
            if let Some(p) = segment_intersection(*a, *b) {
                add(SnapKind::Intersection, p);
            }
        }
    }
    for e in near {
        add(SnapKind::Midpoint, (e[0] + e[1]) / 2.0);
        if let Some(prev) = previous {
            let dir = (e[1] - e[0]).normalize_or_zero();
            let foot = e[0] + dir * (prev - e[0]).dot(dir);
            if coincident(closest_on_segment(foot, *e), foot) {
                add(SnapKind::Perpendicular, foot);
            }
        }
        add(SnapKind::Edge, closest_on_segment(cursor, *e));
    }
    for arc in arcs {
        add(SnapKind::Centre, arc.centre);
        for t in previous.map(|prev| arc.tangent_points(prev)).unwrap_or_default() {
            add(SnapKind::Tangent, t);
        }
    }
    let others = (near.iter().map(|e| GuideShape::Segment(*e)))
        .chain(arcs.iter().map(|a| GuideShape::Arc(*a)))
        .collect::<Vec<_>>();
    for (i, g) in guides.iter().enumerate() {
        for other in others.iter().chain(&guides[i + 1..]) {
            for p in g.intersect(other) {
                add(SnapKind::GuideIntersection, p);
            }
        }
        add(SnapKind::Guide, g.project(cursor));
    }
    if grid_step > 0.0 {
        add(SnapKind::Grid, (cursor / grid_step).round() * grid_step);
    }
    found.sort_by(|a, b| (a.kind.cmp(&b.kind)).then(a.point.distance(cursor).total_cmp(&b.point.distance(cursor))));
    found
}

#[cfg(test)]
mod tests {
    use archicraft_geometry::plan::PlanDrawing;
    use archicraft_model::template::default_metric_project;
    use archicraft_model::{Command, History};

    use super::*;
    use crate::input::GuideOrigin;
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

    fn v(x: f64, y: f64) -> DVec2 {
        DVec2::new(x, y)
    }

    /// INP-009: seven candidates within the radius, one of each level and
    /// three edge points, come out in the SRS order: node, intersection,
    /// division point, guide intersection, edge, guide, grid; points of one
    /// level by their distance from the cursor.
    #[test]
    fn candidates_follow_the_seven_levels() {
        let targets = index_of(
            &[v(0.5, 0.3)],
            &[
                [v(-0.3, -4.0), v(-0.3, 6.0)],
                [v(-5.0, 0.4), v(7.0, 0.4)],
                [v(0.2, -0.6), v(0.6, -0.6)],
            ],
        );
        let guides = [Guide::new(
            GuideShape::Segment([v(0.7, -5.0), v(0.7, 5.0)]),
            GuideOrigin::Persistent,
        )];
        let cursor = v(0.02, -0.03);
        let found = candidates(&targets, &[], cursor, 1.0, 0.25, None, &guides);
        let expected = [
            (SnapKind::Node, v(0.5, 0.3)),
            (SnapKind::Intersection, v(-0.3, 0.4)),
            (SnapKind::Midpoint, v(0.4, -0.6)),
            (SnapKind::GuideIntersection, v(0.7, 0.4)),
            (SnapKind::Edge, v(-0.3, -0.03)),
            (SnapKind::Edge, v(0.02, 0.4)),
            (SnapKind::Edge, v(0.2, -0.6)),
            (SnapKind::Guide, v(0.7, -0.03)),
            (SnapKind::Grid, v(0.0, 0.0)),
        ];
        assert_eq!(found.len(), expected.len(), "{found:?}");
        for (snap, (kind, point)) in found.iter().zip(expected) {
            assert_eq!(snap.kind, kind, "{found:?}");
            assert!(snap.point.distance(point) < 1e-12, "{snap:?} is not at {point}");
        }
        let levels: Vec<SnapLevel> = found.iter().filter_map(|s| s.kind.level()).collect();
        let mut seven = levels.clone();
        seven.dedup();
        assert_eq!(
            seven,
            [
                SnapLevel::Node,
                SnapLevel::Intersection,
                SnapLevel::Division,
                SnapLevel::GuideIntersection,
                SnapLevel::Edge,
                SnapLevel::Guide,
                SnapLevel::Grid,
            ]
        );
        assert_eq!(snap(&targets, &[], cursor, 1.0, 0.25, None).kind, SnapKind::Node);
    }

    /// INP-009: every guide shape meets the edges and arcs near the cursor
    /// and the other guides at guide-intersection level, and the cursor
    /// projected onto it is a point on a guide.
    #[test]
    fn every_guide_shape_snaps() {
        let line = GuideShape::line(v(0.0, 0.0), v(1.0, 0.0));
        let ray = GuideShape::ray(v(0.6, 0.0), v(0.0, 1.0));
        let circle = GuideShape::Circle {
            centre: v(0.0, 0.0),
            radius: 0.5,
        };
        let edge = [v(-0.4, -1.0), v(-0.4, 0.6)];
        let arc = ArcTarget::circle(v(0.3, -0.8), 1.0);
        let guides = [line, ray, circle];
        let targets = Targets {
            nodes: &[],
            edges: &[edge],
            arcs: &[arc],
            guides: &guides,
        };
        let cursor = v(0.05, 0.1);
        let found = collect(&targets, cursor, 1.0, 0.0, None);
        let has = |kind: SnapKind, point: DVec2| found.iter().any(|s| s.kind == kind && s.point.distance(point) < 1e-9);
        for point in [
            v(-0.4, 0.0), // line × edge
            v(0.6, 0.0),  // line × ray
            v(0.5, 0.0),  // line × circle guide
            v(-0.4, 0.3), // circle guide × edge
            v(-0.3, 0.0), // line × arc
        ] {
            assert!(has(SnapKind::GuideIntersection, point), "{point} missing in {found:?}");
        }
        assert!(has(SnapKind::Guide, v(0.05, 0.0)), "on the line: {found:?}");
        let on_circle = cursor.normalize() * 0.5;
        assert!(has(SnapKind::Guide, on_circle), "on the circle guide: {found:?}");
    }

    /// INP-009: the kinds are ordered by level, then by their rank within
    /// the level, as the module documentation's table lists them.
    #[test]
    fn kinds_rank_within_their_level() {
        use SnapKind::*;
        let by_level = |level| SnapKind::TARGETS.into_iter().filter(move |k| k.level() == Some(level));
        assert_eq!(
            by_level(SnapLevel::Node).collect::<Vec<_>>(),
            [Node, Handle, Centre, AxisPoint, Centroid]
        );
        assert_eq!(
            by_level(SnapLevel::Edge).collect::<Vec<_>>(),
            [Perpendicular, Tangent, Edge]
        );
        assert!(
            SnapKind::TARGETS
                .windows(2)
                .all(|w| w[0] < w[1] && w[0].level() <= w[1].level())
        );
        for kind in [Constrained, Typed, Free] {
            assert_eq!(kind.level(), None);
        }
    }

    /// INP-009: the perpendicular foot from the previous point comes before
    /// the plain point on the same edge, which lies nearer the cursor.
    #[test]
    fn the_perpendicular_foot_outranks_the_nearer_edge_point() {
        let targets = index_of(&[], &[[DVec2::ZERO, v(6.0, 0.0)]]);
        let cursor = v(2.3, 0.05);
        let found = candidates(&targets, &[], cursor, 0.5, 0.0, Some(v(2.0, 3.0)), &[]);
        assert_eq!(
            found,
            [
                Snap {
                    point: v(2.0, 0.0),
                    kind: SnapKind::Perpendicular
                },
                Snap {
                    point: v(2.3, 0.0),
                    kind: SnapKind::Edge
                },
            ]
        );
        assert!(found[1].point.distance(cursor) < found[0].point.distance(cursor));
    }

    /// INP-009, NFR-ACC-003: a point that is both a node and the
    /// intersection of two edges is one candidate, at node level.
    #[test]
    fn a_node_on_an_intersection_is_one_candidate() {
        let targets = index_of(
            &[v(1.0, 1.0)],
            &[[v(0.0, 0.0), v(2.0, 2.0)], [v(0.0, 2.0), v(2.0, 0.0)]],
        );
        let found = candidates(&targets, &[], v(1.05, 1.02), 0.2, 0.0, None, &[]);
        let at_crossing: Vec<&Snap> = found.iter().filter(|s| coincident(s.point, v(1.0, 1.0))).collect();
        assert_eq!(
            at_crossing,
            [&Snap {
                point: v(1.0, 1.0),
                kind: SnapKind::Node
            }]
        );
        assert_eq!(found[0].kind, SnapKind::Node);
        assert!(found[1..].iter().all(|s| s.kind == SnapKind::Edge), "{found:?}");
    }
}
