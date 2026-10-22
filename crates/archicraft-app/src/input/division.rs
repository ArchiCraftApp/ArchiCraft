//! Division points of a curve (INP-010).
//!
//! Hovering an edge shows the division points of exactly one active mode,
//! which [`DivisionMode`] holds by construction. The points are measured
//! along the curve, by arc length on circles and arcs; the "nearer end" of
//! the percentage and the repeated distance is the end nearer the cursor's
//! projection onto the curve. The ends themselves are no division points:
//! they are nodes.

use std::f64::consts::TAU;

use archicraft_model::MODEL_TOLERANCE;
use archicraft_model::glam::DVec2;
use serde::{Deserialize, Serialize};

use super::snap::ArcTarget;

/// The most division points of one curve, which bounds the cost per frame.
pub const MAX_DIVISION_POINTS: usize = 100;

/// The range of the number of equal parts (INP-010).
pub const EQUAL_PARTS: std::ops::RangeInclusive<u8> = 2..=100;

/// A curve that can be divided (INP-010): an edge, a circle or an arc.
/// Guide lines share it (INP-012).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Curve {
    Segment([DVec2; 2]),
    /// A full circle, which starts and ends at angle 0 and runs
    /// counter-clockwise. Its nearer end is the start for a cursor in the
    /// upper half (angles below π), so a percentage runs counter-clockwise
    /// from angle 0 there and clockwise in the lower half.
    Circle {
        centre: DVec2,
        radius: f64,
    },
    Arc(ArcTarget),
}

impl Curve {
    /// The length along the curve.
    pub fn length(&self) -> f64 {
        match *self {
            Curve::Segment([a, b]) => a.distance(b),
            Curve::Circle { radius, .. } => TAU * radius,
            Curve::Arc(arc) => arc.sweep.abs().min(TAU) * arc.radius,
        }
    }

    /// The point `s` along the curve from its start.
    pub fn point_at(&self, s: f64) -> DVec2 {
        match *self {
            Curve::Segment([a, b]) => a + (b - a).normalize_or_zero() * s,
            Curve::Circle { centre, radius } => centre + DVec2::from_angle(s / radius.max(MODEL_TOLERANCE)) * radius,
            Curve::Arc(arc) => {
                let angle = arc.start + arc.sweep.signum() * s / arc.radius.max(MODEL_TOLERANCE);
                arc.centre + DVec2::from_angle(angle) * arc.radius
            }
        }
    }

    /// Where the projection of `p` onto the curve lies, as a share of the
    /// length from the start, in [0, 1].
    pub fn parameter(&self, p: DVec2) -> f64 {
        match *self {
            Curve::Segment([a, b]) => {
                let ab = b - a;
                let len2 = ab.length_squared();
                if len2 > 0.0 {
                    ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            }
            Curve::Circle { centre, .. } => (p - centre).to_angle().rem_euclid(TAU) / TAU,
            Curve::Arc(arc) => {
                let sweep = arc.sweep.abs().min(TAU);
                if sweep == 0.0 {
                    return 0.0;
                }
                let along = (arc.sweep.signum() * ((p - arc.centre).to_angle() - arc.start)).rem_euclid(TAU);
                // Beyond the arc the projection is the nearer end.
                if along <= sweep {
                    along / sweep
                } else if along - sweep < TAU - along {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

/// The one active division mode (INP-010).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum DivisionMode {
    /// The midpoint.
    #[default]
    Half,
    /// N equal parts, N in 2…100.
    Equal(u8),
    /// A percentage of the length from the nearer end.
    Percent(f64),
    /// A fixed distance repeated from the nearer end, in metres.
    Repeated(f64),
    /// The equal division whose parts come nearest a target distance, in
    /// metres.
    BestEqualNear(f64),
}

impl DivisionMode {
    /// The mode with its value in the allowed range: 2…100 equal parts, a
    /// percentage in 0…100 and a positive, finite distance; a value that is
    /// no number takes the mode's default.
    pub fn sanitized(self) -> Self {
        let distance = |d: f64| if d.is_finite() && d > MODEL_TOLERANCE { d } else { 1.0 };
        match self {
            DivisionMode::Half => DivisionMode::Half,
            DivisionMode::Equal(n) => DivisionMode::Equal(n.clamp(*EQUAL_PARTS.start(), *EQUAL_PARTS.end())),
            DivisionMode::Percent(p) => DivisionMode::Percent(if p.is_nan() { 50.0 } else { p.clamp(0.0, 100.0) }),
            DivisionMode::Repeated(d) => DivisionMode::Repeated(distance(d)),
            DivisionMode::BestEqualNear(d) => DivisionMode::BestEqualNear(distance(d)),
        }
    }

    /// The division points of a curve `length` long, as distances from its
    /// start, in the order they are measured: from the start, or for a
    /// percentage and a repeated distance from the nearer end, which is the
    /// start while the cursor's `parameter` is below one half. Points at
    /// the ends are left out; there are at most [`MAX_DIVISION_POINTS`].
    pub fn stations(self, length: f64, parameter: f64) -> Vec<f64> {
        let from_start = parameter < 0.5;
        let from_nearer = |s: f64| if from_start { s } else { length - s };
        let equal = |n: usize| (1..n).map(|i| length * i as f64 / n as f64).collect::<Vec<_>>();
        let stations = match self.sanitized() {
            DivisionMode::Half => equal(2),
            DivisionMode::Equal(n) => equal(n as usize),
            DivisionMode::Percent(p) => vec![from_nearer(length * p / 100.0)],
            DivisionMode::Repeated(d) => (1..=MAX_DIVISION_POINTS)
                .map(|i| d * i as f64)
                .take_while(|s| *s < length - MODEL_TOLERANCE)
                .map(from_nearer)
                .collect(),
            DivisionMode::BestEqualNear(d) => equal(((length / d).round() as usize).clamp(1, MAX_DIVISION_POINTS + 1)),
        };
        stations
            .into_iter()
            .filter(|s| *s > MODEL_TOLERANCE && *s < length - MODEL_TOLERANCE)
            .take(MAX_DIVISION_POINTS)
            .collect()
    }
}

/// Which stretch of an element a hovered edge's division points span
/// (INP-010).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DivisionScope {
    /// The hovered edge, from node to node.
    #[default]
    BetweenNodes,
    /// The stretch of the hovered edge between its intersections with other
    /// elements.
    BetweenIntersections,
    /// The whole element: a wall's reference line, a slab's outline, a
    /// cut ring.
    WholeElement,
}

/// The division points of `curve` in `mode`, the nearer end taken from the
/// cursor's projection onto it (INP-010).
pub fn divide(curve: &Curve, mode: DivisionMode, cursor: DVec2) -> Vec<DVec2> {
    let stations = mode.stations(curve.length(), curve.parameter(cursor));
    stations.into_iter().map(|s| curve.point_at(s)).collect()
}

/// The point-input settings kept in the preferences: the division mode and
/// its scope (INP-010). No Appendix D default exists: half, between nodes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InputSettings {
    pub division: DivisionMode,
    pub scope: DivisionScope,
}

impl InputSettings {
    /// The settings with the division mode's value in its allowed range
    /// ([`DivisionMode::sanitized`]).
    pub fn sanitized(self) -> Self {
        Self {
            division: self.division.sanitized(),
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use super::*;

    fn v(x: f64, y: f64) -> DVec2 {
        DVec2::new(x, y)
    }

    /// The 3.5 m edge of the acceptance tests, along X.
    const EDGE: Curve = Curve::Segment([DVec2::ZERO, DVec2::new(3.5, 0.0)]);

    fn xs(points: &[DVec2]) -> Vec<f64> {
        assert!(points.iter().all(|p| p.y.abs() < 1e-12), "{points:?}");
        points.iter().map(|p| p.x).collect()
    }

    fn assert_near(found: &[f64], expected: &[f64]) {
        assert_eq!(found.len(), expected.len(), "{found:?} is not {expected:?}");
        for (f, e) in found.iter().zip(expected) {
            assert!((f - e).abs() < 1e-9, "{found:?} is not {expected:?}");
        }
    }

    /// INP-010: half gives the midpoint, wherever the cursor is.
    #[test]
    fn half_gives_one_point() {
        for cursor in [v(0.3, 0.1), v(3.2, -0.1)] {
            assert_near(&xs(&divide(&EDGE, DivisionMode::Half, cursor)), &[1.75]);
        }
    }

    /// INP-010: N equal parts give N − 1 points between the ends.
    #[test]
    fn equal_parts_give_n_minus_one_points() {
        let points = divide(&EDGE, DivisionMode::Equal(4), v(1.0, 0.1));
        assert_near(&xs(&points), &[0.875, 1.75, 2.625]);
    }

    /// INP-010: N is clamped to 2…100.
    #[test]
    fn equal_parts_outside_two_to_a_hundred_are_clamped() {
        let cursor = v(1.0, 0.1);
        assert_eq!(
            divide(&EDGE, DivisionMode::Equal(0), cursor),
            divide(&EDGE, DivisionMode::Equal(2), cursor)
        );
        assert_eq!(
            divide(&EDGE, DivisionMode::Equal(1), cursor),
            divide(&EDGE, DivisionMode::Half, cursor)
        );
        let most = divide(&EDGE, DivisionMode::Equal(255), cursor);
        assert_eq!(most, divide(&EDGE, DivisionMode::Equal(100), cursor));
        assert_eq!(most.len(), 99);
        assert_eq!(DivisionMode::Equal(255).sanitized(), DivisionMode::Equal(100));
    }

    /// INP-010: a percentage is measured from the end nearer the cursor.
    #[test]
    fn a_percentage_is_measured_from_the_nearer_end() {
        let mode = DivisionMode::Percent(25.0);
        assert_near(&xs(&divide(&EDGE, mode, v(0.5, 0.1))), &[0.875]);
        assert_near(&xs(&divide(&EDGE, mode, v(3.0, -0.1))), &[2.625]);
        // The cursor's projection decides, not its distance from the ends.
        assert_near(&xs(&divide(&EDGE, mode, v(1.7, 5.0))), &[0.875]);
        assert_near(&xs(&divide(&EDGE, mode, v(1.8, 5.0))), &[2.625]);
    }

    /// INP-010: a repeated distance runs from the nearer end and stops
    /// before the far end.
    #[test]
    fn a_repeated_distance_runs_from_the_nearer_end() {
        let mode = DivisionMode::Repeated(1.0);
        assert_near(&xs(&divide(&EDGE, mode, v(0.2, 0.0))), &[1.0, 2.0, 3.0]);
        assert_near(&xs(&divide(&EDGE, mode, v(3.3, 0.0))), &[2.5, 1.5, 0.5]);
        // A distance that fits exactly leaves out the far end.
        let points = divide(&EDGE, DivisionMode::Repeated(0.875), v(0.2, 0.0));
        assert_near(&xs(&points), &[0.875, 1.75, 2.625]);
        let tiny = divide(&EDGE, DivisionMode::Repeated(0.001), v(0.2, 0.0));
        assert_eq!(tiny.len(), MAX_DIVISION_POINTS, "bounded per frame");
    }

    /// INP-010: the best equal division near 1.1 m of a 3.5 m edge is three
    /// parts of 1.1667 m.
    #[test]
    fn the_best_equal_division_comes_nearest_the_target() {
        let points = xs(&divide(&EDGE, DivisionMode::BestEqualNear(1.1), v(0.2, 0.0)));
        assert_near(&points, &[3.5 / 3.0, 7.0 / 3.0]);
        assert!((points[0] - 1.1667).abs() < 1e-4);
        assert!(
            divide(&EDGE, DivisionMode::BestEqualNear(5.0), v(0.2, 0.0)).is_empty(),
            "one part"
        );
        assert_eq!(
            divide(&EDGE, DivisionMode::BestEqualNear(1e-3), v(0.2, 0.0)).len(),
            MAX_DIVISION_POINTS
        );
    }

    /// INP-010: circles and arcs are divided by arc length, the nearer end
    /// of an arc found along it.
    #[test]
    fn circles_and_arcs_are_divided_by_arc_length() {
        let centre = v(1.0, 2.0);
        let circle = Curve::Circle { centre, radius: 2.0 };
        assert!((circle.length() - 4.0 * PI).abs() < 1e-12);
        let points = divide(&circle, DivisionMode::Equal(4), centre + v(2.0, 0.1));
        let expected = [v(0.0, 2.0), v(-2.0, 0.0), v(0.0, -2.0)].map(|p| centre + p);
        assert_eq!(points.len(), 3);
        for (p, e) in points.iter().zip(expected) {
            assert!(p.distance(e) < 1e-9, "{points:?}");
        }

        let quarter = |cursor: DVec2| divide(&circle, DivisionMode::Percent(25.0), centre + cursor);
        assert!(
            quarter(v(1.0, 1.0))[0].distance(centre + v(0.0, 2.0)) < 1e-9,
            "upper half"
        );
        assert!(
            quarter(v(1.0, -1.0))[0].distance(centre + v(0.0, -2.0)) < 1e-9,
            "lower half"
        );

        // A quarter arc from 0° to 90°, and the same arc clockwise from 90°.
        for arc in [
            ArcTarget {
                centre,
                radius: 2.0,
                start: 0.0,
                sweep: PI / 2.0,
            },
            ArcTarget {
                centre,
                radius: 2.0,
                start: PI / 2.0,
                sweep: -PI / 2.0,
            },
        ] {
            let curve = Curve::Arc(arc);
            assert!((curve.length() - PI).abs() < 1e-12);
            let mid = divide(&curve, DivisionMode::Half, centre + v(2.0, 0.0));
            assert!(mid[0].distance(centre + DVec2::from_angle(PI / 4.0) * 2.0) < 1e-9);
            // 25 % of π from the end at 0°, nearer the cursor at 10°.
            let near_x = DVec2::from_angle(10f64.to_radians()) * 2.0;
            let quarter = divide(&curve, DivisionMode::Percent(25.0), centre + near_x);
            assert!(
                quarter[0].distance(centre + DVec2::from_angle(PI / 8.0) * 2.0) < 1e-9,
                "{arc:?}"
            );
            // Beyond the arc's end at 0° the projection is that end.
            let beyond = divide(&curve, DivisionMode::Percent(25.0), centre + v(2.0, -0.3));
            assert!(beyond[0].distance(quarter[0]) < 1e-9, "{arc:?}");
        }
    }

    /// INP-010: out-of-range values are sanitised.
    #[test]
    fn out_of_range_values_are_sanitised() {
        use DivisionMode::*;
        let cases = [
            (Equal(1), Equal(2)),
            (Equal(101), Equal(100)),
            (Percent(-5.0), Percent(0.0)),
            (Percent(250.0), Percent(100.0)),
            (Percent(f64::NAN), Percent(50.0)),
            (Repeated(0.0), Repeated(1.0)),
            (Repeated(-2.0), Repeated(1.0)),
            (Repeated(f64::INFINITY), Repeated(1.0)),
            (BestEqualNear(f64::NAN), BestEqualNear(1.0)),
            (Repeated(0.6), Repeated(0.6)),
        ];
        for (given, expected) in cases {
            assert_eq!(given.sanitized(), expected, "{given:?}");
        }
    }
}
