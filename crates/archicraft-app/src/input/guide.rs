//! Guides of point input (INP-006, INP-007, INP-011, INP-012): the lines
//! and circles the cursor can be held on and snaps along.
//!
//! Locks (INP-007, INP-023), the angle constraint (INP-006), inference
//! guides (INP-011) and persistent guide lines (INP-012) all produce a
//! [`Guide`]: a [`GuideShape`] and the [`GuideOrigin`] it came from.
//! Snapping intersects guides with the edges near the cursor and with each
//! other, and projects the cursor onto them (INP-009); views draw the part
//! of each guide inside their rectangle.

use std::f64::consts::TAU;

use archicraft_model::MODEL_TOLERANCE;
use archicraft_model::glam::DVec2;
use archicraft_model::tolerance::{coincident, is_parallel};

use super::snap::{ArcTarget, closest_on_segment};

/// The most straight pieces one visible stretch of a circle or an arc is
/// drawn with, whatever the deviation asked for.
const MAX_CURVE_PIECES: f64 = 4096.0;

/// Where a guide comes from, which decides how it is drawn and how long it
/// lasts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GuideOrigin {
    /// A locked coordinate, distance or angle (INP-007, INP-023).
    Lock,
    /// The direction of the angle constraint (INP-006).
    Constraint,
    /// An inference guide from a marked reference (INP-011).
    Inference,
    /// A guide line placed in a plan view (INP-012).
    Persistent,
}

/// The shape of a guide. Directions are unit vectors (see
/// [`GuideShape::line`] and [`GuideShape::ray`]); lengths are metres and
/// angles radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GuideShape {
    /// An infinite line through `point`.
    Line {
        point: DVec2,
        dir: DVec2,
    },
    /// A half-line from `origin`.
    Ray {
        origin: DVec2,
        dir: DVec2,
    },
    Segment([DVec2; 2]),
    Circle {
        centre: DVec2,
        radius: f64,
    },
    Arc(ArcTarget),
}

/// A guide shown while resolving a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Guide {
    pub shape: GuideShape,
    pub origin: GuideOrigin,
}

impl Guide {
    pub fn new(shape: GuideShape, origin: GuideOrigin) -> Self {
        Self { shape, origin }
    }
}

/// The straight carrier or the circle of a shape, which intersections are
/// computed on before they are restricted to the shape.
enum Carrier {
    Line { point: DVec2, dir: DVec2 },
    Circle { centre: DVec2, radius: f64 },
}

impl GuideShape {
    /// The line through `point` along `dir`, normalised.
    pub fn line(point: DVec2, dir: DVec2) -> Self {
        GuideShape::Line {
            point,
            dir: dir.normalize_or_zero(),
        }
    }

    /// The half-line from `origin` along `dir`, normalised.
    pub fn ray(origin: DVec2, dir: DVec2) -> Self {
        GuideShape::Ray {
            origin,
            dir: dir.normalize_or_zero(),
        }
    }

    fn carrier(&self) -> Carrier {
        match *self {
            GuideShape::Line { point, dir } => Carrier::Line { point, dir },
            GuideShape::Ray { origin, dir } => Carrier::Line { point: origin, dir },
            GuideShape::Segment([a, b]) => Carrier::Line {
                point: a,
                dir: (b - a).normalize_or_zero(),
            },
            GuideShape::Circle { centre, radius } => Carrier::Circle { centre, radius },
            GuideShape::Arc(arc) => Carrier::Circle {
                centre: arc.centre,
                radius: arc.radius,
            },
        }
    }

    /// True if `p`, a point of the carrier, lies on the shape, its ends
    /// within the model tolerance (NFR-ACC-003).
    fn contains(&self, p: DVec2) -> bool {
        match *self {
            GuideShape::Line { .. } | GuideShape::Circle { .. } => true,
            GuideShape::Ray { origin, dir } => (p - origin).dot(dir) > -MODEL_TOLERANCE,
            GuideShape::Segment(segment) => coincident(closest_on_segment(p, segment), p),
            GuideShape::Arc(arc) => arc.spans((p - arc.centre).to_angle()),
        }
    }

    /// The point of the guide nearest `p`. A point on the centre of a circle
    /// projects to the point at angle 0; outside its span an arc projects to
    /// its nearer end.
    pub fn project(&self, p: DVec2) -> DVec2 {
        match *self {
            GuideShape::Line { point, dir } => point + dir * (p - point).dot(dir),
            GuideShape::Ray { origin, dir } => origin + dir * (p - origin).dot(dir).max(0.0),
            GuideShape::Segment(segment) => closest_on_segment(p, segment),
            GuideShape::Circle { centre, radius } => on_circle(centre, radius, p),
            GuideShape::Arc(arc) => {
                let on = on_circle(arc.centre, arc.radius, p);
                if arc.spans((on - arc.centre).to_angle()) {
                    return on;
                }
                let [start, end] = arc_ends(&arc);
                if p.distance(start) <= p.distance(end) {
                    start
                } else {
                    end
                }
            }
        }
    }

    /// The distance from `p` to the guide.
    pub fn distance(&self, p: DVec2) -> f64 {
        self.project(p).distance(p)
    }

    /// The points where two guides meet, within the extent of each (rays,
    /// segments and arcs end). Parallel lines, also coincident ones, and
    /// concentric circles have none; a line touching a circle and circles
    /// touching each other within the model tolerance have one.
    pub fn intersect(&self, other: &GuideShape) -> Vec<DVec2> {
        let points = match (self.carrier(), other.carrier()) {
            (Carrier::Line { point: p, dir: r }, Carrier::Line { point: q, dir: s }) => {
                if is_parallel(r, s) {
                    Vec::new()
                } else {
                    vec![p + r * ((q - p).perp_dot(s) / r.perp_dot(s))]
                }
            }
            (Carrier::Line { point, dir }, Carrier::Circle { centre, radius })
            | (Carrier::Circle { centre, radius }, Carrier::Line { point, dir }) => {
                line_circle(point, dir, centre, radius)
            }
            (Carrier::Circle { centre: c1, radius: r1 }, Carrier::Circle { centre: c2, radius: r2 }) => {
                circle_circle(c1, r1, c2, r2)
            }
        };
        points
            .into_iter()
            .filter(|p| self.contains(*p) && other.contains(*p))
            .collect()
    }

    /// The part of the guide inside the rectangle from `min` to `max`, as
    /// polylines to draw (INP-007): the clipped piece of a line, a ray or a
    /// segment; the stretches of a circle or an arc whose angles the
    /// rectangle covers, in straight pieces that deviate at most
    /// `max_deviation` from the curve, for the painter to clip. Empty when
    /// the guide misses the rectangle.
    pub fn clip(&self, min: DVec2, max: DVec2, max_deviation: f64) -> Vec<Vec<DVec2>> {
        let piece = match *self {
            GuideShape::Line { point, dir } => clip_line(point, dir, f64::NEG_INFINITY, f64::INFINITY, min, max),
            GuideShape::Ray { origin, dir } => clip_line(origin, dir, 0.0, f64::INFINITY, min, max),
            GuideShape::Segment([a, b]) => {
                let length = a.distance(b);
                clip_line(a, (b - a).normalize_or_zero(), 0.0, length, min, max)
            }
            GuideShape::Circle { centre, radius } => {
                return curve_in(&ArcTarget::circle(centre, radius), min, max, max_deviation);
            }
            GuideShape::Arc(arc) => return curve_in(&arc, min, max, max_deviation),
        };
        if piece.is_empty() { Vec::new() } else { vec![piece] }
    }
}

/// The point of the circle nearest `p`.
fn on_circle(centre: DVec2, radius: f64, p: DVec2) -> DVec2 {
    let offset = p - centre;
    let dir = if coincident(offset, DVec2::ZERO) {
        DVec2::X
    } else {
        offset.normalize()
    };
    centre + dir * radius
}

/// The start and the end point of an arc.
fn arc_ends(arc: &ArcTarget) -> [DVec2; 2] {
    [arc.start, arc.start + arc.sweep].map(|angle| arc.centre + DVec2::from_angle(angle) * arc.radius)
}

/// The points where the line through `point` along the unit `dir` meets
/// the circle: none when it passes further than the tolerance outside, the
/// foot of the centre when it touches within the tolerance, else two.
fn line_circle(point: DVec2, dir: DVec2, centre: DVec2, radius: f64) -> Vec<DVec2> {
    let foot = point + dir * (centre - point).dot(dir);
    let h = foot.distance(centre);
    if h > radius + MODEL_TOLERANCE {
        Vec::new()
    } else if (radius - h).abs() < MODEL_TOLERANCE {
        vec![foot]
    } else {
        let half = (radius * radius - h * h).sqrt();
        vec![foot - dir * half, foot + dir * half]
    }
}

/// The points where two circles meet: none for concentric or separate
/// circles, one where they touch within the tolerance, else two.
fn circle_circle(c1: DVec2, r1: f64, c2: DVec2, r2: f64) -> Vec<DVec2> {
    let d = c1.distance(c2);
    if coincident(c1, c2) || d > r1 + r2 + MODEL_TOLERANCE || d < (r1 - r2).abs() - MODEL_TOLERANCE {
        return Vec::new();
    }
    let u = (c2 - c1) / d;
    let a = (d * d + r1 * r1 - r2 * r2) / (2.0 * d);
    let h = (r1 * r1 - a * a).max(0.0).sqrt();
    let base = c1 + u * a;
    if h < MODEL_TOLERANCE {
        vec![base]
    } else {
        vec![base + u.perp() * h, base - u.perp() * h]
    }
}

/// The piece of the line `point + t·dir`, `t` in `[t0, t1]`, inside the
/// rectangle (Liang–Barsky): its two ends, or nothing.
fn clip_line(point: DVec2, dir: DVec2, mut t0: f64, mut t1: f64, min: DVec2, max: DVec2) -> Vec<DVec2> {
    for (p, d, lo, hi) in [(point.x, dir.x, min.x, max.x), (point.y, dir.y, min.y, max.y)] {
        if d == 0.0 {
            if p < lo || p > hi {
                return Vec::new();
            }
            continue;
        }
        let (a, b) = ((lo - p) / d, (hi - p) / d);
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
    }
    if t0 > t1 || !t0.is_finite() || !t1.is_finite() {
        return Vec::new();
    }
    vec![point + dir * t0, point + dir * t1]
}

/// The stretches of an arc or a circle whose angles the rectangle covers,
/// each in straight pieces that deviate at most `max_deviation` from it;
/// nothing when it misses the rectangle. Seen from a centre outside the
/// rectangle, the rectangle covers a wedge of less than π, so a huge
/// circle through a small rectangle is sampled only along that wedge.
fn curve_in(arc: &ArcTarget, min: DVec2, max: DVec2, max_deviation: f64) -> Vec<Vec<DVec2>> {
    let corners = [min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)];
    let nearest = arc.centre.clamp(min, max);
    let farthest = corners.iter().map(|c| c.distance(arc.centre)).fold(0.0, f64::max);
    if arc.radius < nearest.distance(arc.centre) || arc.radius > farthest {
        return Vec::new();
    }
    // The arc counter-clockwise from `start` over `sweep`.
    let (mut start, sweep) = if arc.sweep < 0.0 {
        (arc.start + arc.sweep, -arc.sweep)
    } else {
        (arc.start, arc.sweep)
    };
    let sweep = sweep.min(TAU);
    let spans = if nearest == arc.centre {
        vec![(0.0, sweep)]
    } else {
        // The whole rectangle lies beyond its nearest point, so every corner
        // is within π/2 of the direction towards it.
        let towards = nearest - arc.centre;
        let offsets = corners.map(|c| {
            let d = c - arc.centre;
            towards.perp_dot(d).atan2(towards.dot(d))
        });
        let low = offsets.into_iter().fold(f64::INFINITY, f64::min);
        let high = offsets.into_iter().fold(f64::NEG_INFINITY, f64::max);
        let wedge = towards.to_angle() + low;
        if sweep >= TAU {
            start = wedge;
        }
        let offset = (wedge - start).rem_euclid(TAU);
        [offset, offset - TAU]
            .into_iter()
            .map(|a| (a.max(0.0), (a + high - low).min(sweep)))
            .filter(|(a, b)| b > a)
            .collect()
    };
    let step = 2.0 * (1.0 - max_deviation / arc.radius).max(-1.0).acos();
    spans
        .into_iter()
        .map(|(a, b)| {
            let pieces = ((b - a) / step).ceil().clamp(1.0, MAX_CURVE_PIECES) as usize;
            (0..=pieces)
                .map(|i| {
                    let angle = start + a + (b - a) * i as f64 / pieces as f64;
                    arc.centre + DVec2::from_angle(angle) * arc.radius
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, PI};

    use super::*;

    fn v(x: f64, y: f64) -> DVec2 {
        DVec2::new(x, y)
    }

    fn close(a: DVec2, b: DVec2) -> bool {
        a.distance(b) < 1e-12
    }

    /// The points sorted by x, then y, for comparison.
    fn sorted(mut points: Vec<DVec2>) -> Vec<DVec2> {
        points.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
        points
    }

    fn assert_points(found: Vec<DVec2>, expected: &[DVec2]) {
        let found = sorted(found);
        let expected = sorted(expected.to_vec());
        assert_eq!(found.len(), expected.len(), "{found:?} != {expected:?}");
        for (f, e) in found.iter().zip(&expected) {
            assert!(close(*f, *e), "{found:?} != {expected:?}");
        }
    }

    fn circle(x: f64, y: f64, radius: f64) -> GuideShape {
        GuideShape::Circle {
            centre: v(x, y),
            radius,
        }
    }

    /// The quarter arc of the unit circle from angle 0 to π/2.
    fn quarter() -> GuideShape {
        GuideShape::Arc(ArcTarget {
            centre: DVec2::ZERO,
            radius: 1.0,
            start: 0.0,
            sweep: FRAC_PI_2,
        })
    }

    #[test]
    fn crossing_lines_meet_in_one_point() {
        let a = GuideShape::line(v(0.0, 1.0), v(1.0, 0.0));
        let b = GuideShape::line(v(3.0, -2.0), v(1.0, 1.0));
        assert_points(a.intersect(&b), &[v(6.0, 1.0)]);
        assert_points(b.intersect(&a), &[v(6.0, 1.0)]);
    }

    #[test]
    fn parallel_and_coincident_lines_do_not_meet() {
        let a = GuideShape::line(v(0.0, 1.0), v(1.0, 0.0));
        assert!(a.intersect(&GuideShape::line(v(0.0, 2.0), v(-3.0, 0.0))).is_empty());
        assert!(a.intersect(&GuideShape::line(v(5.0, 1.0), v(1.0, 0.0))).is_empty());
    }

    #[test]
    fn rays_and_segments_end() {
        let ray = GuideShape::ray(v(0.0, 0.0), v(1.0, 0.0));
        let ahead = GuideShape::line(v(2.0, -1.0), v(0.0, 1.0));
        let behind = GuideShape::line(v(-2.0, -1.0), v(0.0, 1.0));
        assert_points(ray.intersect(&ahead), &[v(2.0, 0.0)]);
        assert!(ray.intersect(&behind).is_empty());
        let segment = GuideShape::Segment([v(1.0, -1.0), v(1.0, 1.0)]);
        assert_points(segment.intersect(&ray), &[v(1.0, 0.0)]);
        let short = GuideShape::Segment([v(1.0, 0.5), v(1.0, 1.0)]);
        assert!(short.intersect(&ray).is_empty());
    }

    #[test]
    fn a_line_meets_a_circle_in_zero_one_or_two_points() {
        let c = circle(1.0, 1.0, 2.0);
        assert!(GuideShape::line(v(0.0, 4.0), v(1.0, 0.0)).intersect(&c).is_empty());
        assert_points(GuideShape::line(v(0.0, 3.0), v(1.0, 0.0)).intersect(&c), &[v(1.0, 3.0)]);
        assert_points(
            GuideShape::line(v(0.0, 1.0), v(1.0, 0.0)).intersect(&c),
            &[v(-1.0, 1.0), v(3.0, 1.0)],
        );
        // Diagonal, through the centre.
        let h = 2.0 / 2f64.sqrt();
        assert_points(
            c.intersect(&GuideShape::line(v(0.0, 0.0), v(1.0, 1.0))),
            &[v(1.0 - h, 1.0 - h), v(1.0 + h, 1.0 + h)],
        );
    }

    #[test]
    fn a_line_meets_an_arc_only_within_its_span() {
        let horizontal = GuideShape::line(v(0.0, 0.5), v(1.0, 0.0));
        let x = 0.75f64.sqrt();
        assert_points(quarter().intersect(&horizontal), &[v(x, 0.5)]);
    }

    #[test]
    fn circles_meet_in_zero_one_or_two_points() {
        let a = circle(0.0, 0.0, 1.0);
        assert!(a.intersect(&circle(3.0, 0.0, 1.0)).is_empty(), "apart");
        assert!(a.intersect(&circle(0.1, 0.0, 0.2)).is_empty(), "inside");
        assert!(a.intersect(&circle(0.0, 0.0, 2.0)).is_empty(), "concentric");
        assert!(a.intersect(&circle(0.0, 0.0, 1.0)).is_empty(), "identical");
        assert_points(a.intersect(&circle(2.0, 0.0, 1.0)), &[v(1.0, 0.0)]);
        assert_points(a.intersect(&circle(0.5, 0.0, 0.5)), &[v(1.0, 0.0)]);
        let y = 0.75f64.sqrt();
        assert_points(a.intersect(&circle(1.0, 0.0, 1.0)), &[v(0.5, y), v(0.5, -y)]);
        assert_points(quarter().intersect(&circle(1.0, 0.0, 1.0)), &[v(0.5, y)]);
    }

    #[test]
    fn points_project_onto_lines_rays_and_segments() {
        let line = GuideShape::line(v(0.0, 1.0), v(2.0, 0.0));
        assert!(close(line.project(v(-3.0, 4.0)), v(-3.0, 1.0)));
        assert!((line.distance(v(-3.0, 4.0)) - 3.0).abs() < 1e-12);
        let ray = GuideShape::ray(v(0.0, 1.0), v(1.0, 0.0));
        assert!(close(ray.project(v(-3.0, 4.0)), v(0.0, 1.0)));
        assert!(close(ray.project(v(3.0, 4.0)), v(3.0, 1.0)));
        let segment = GuideShape::Segment([v(0.0, 0.0), v(2.0, 0.0)]);
        assert!(close(segment.project(v(5.0, 1.0)), v(2.0, 0.0)));
        assert!(close(segment.project(v(1.0, 1.0)), v(1.0, 0.0)));
    }

    #[test]
    fn points_project_onto_circles_and_arcs() {
        let c = circle(1.0, 1.0, 2.0);
        assert!(close(c.project(v(1.0, 5.0)), v(1.0, 3.0)));
        assert!(close(c.project(v(1.5, 1.0)), v(3.0, 1.0)));
        assert!((c.distance(v(1.5, 1.0)) - 1.5).abs() < 1e-12);
        let arc = quarter();
        let s = 0.5f64.sqrt();
        assert!(close(arc.project(v(2.0, 2.0)), v(s, s)), "within the span");
        assert!(close(arc.project(v(2.0, -0.5)), v(1.0, 0.0)), "beyond the start");
        assert!(close(arc.project(v(-0.5, 2.0)), v(0.0, 1.0)), "beyond the end");
        let (sin, cos) = (PI * 1.2).sin_cos();
        let behind = v(cos, sin) * 3.0;
        let p = arc.project(behind);
        assert!(close(p, v(1.0, 0.0)) || close(p, v(0.0, 1.0)), "an end: {p}");
    }

    #[test]
    fn an_infinite_line_is_clipped_to_the_rectangle() {
        let (min, max) = (v(0.0, 0.0), v(4.0, 2.0));
        let diagonal = GuideShape::line(v(1.0, 1.0), v(1.0, 1.0));
        assert_points(diagonal.clip(min, max, 1e-6).concat(), &[v(0.0, 0.0), v(2.0, 2.0)]);
        let horizontal = GuideShape::line(v(-10.0, 1.5), v(-1.0, 0.0));
        assert_points(horizontal.clip(min, max, 1e-6).concat(), &[v(0.0, 1.5), v(4.0, 1.5)]);
        let outside = GuideShape::line(v(0.0, 3.0), v(1.0, 0.0));
        assert!(outside.clip(min, max, 1e-6).concat().is_empty());
        let missing = GuideShape::line(v(6.0, 0.0), v(1.0, 1.0));
        assert!(missing.clip(min, max, 1e-6).concat().is_empty());
        let along_edge = GuideShape::line(v(0.0, 7.0), v(0.0, 1.0));
        assert_points(along_edge.clip(min, max, 1e-6).concat(), &[v(0.0, 0.0), v(0.0, 2.0)]);
    }

    #[test]
    fn rays_segments_and_circles_are_clipped_to_their_extent() {
        let (min, max) = (v(0.0, 0.0), v(4.0, 2.0));
        let ray = GuideShape::ray(v(1.0, 1.0), v(1.0, 0.0));
        assert_points(ray.clip(min, max, 1e-6).concat(), &[v(1.0, 1.0), v(4.0, 1.0)]);
        let segment = GuideShape::Segment([v(-1.0, 1.0), v(2.0, 1.0)]);
        assert_points(segment.clip(min, max, 1e-6).concat(), &[v(0.0, 1.0), v(2.0, 1.0)]);
        let c = circle(2.0, 1.0, 0.5);
        let drawn = c.clip(min, max, 1e-6).concat();
        assert!(drawn.len() > 8);
        assert!(drawn.iter().all(|p| (p.distance(v(2.0, 1.0)) - 0.5).abs() < 1e-12));
        assert!(circle(9.0, 9.0, 1.0).clip(min, max, 1e-6).concat().is_empty());
    }

    /// Asserts that every point of the polylines lies on the circle and that
    /// no chord between neighbours strays further than `deviation` from it.
    fn assert_follows(polylines: &[Vec<DVec2>], centre: DVec2, radius: f64, deviation: f64) {
        assert!(!polylines.is_empty());
        for polyline in polylines {
            assert!(polyline.len() >= 2, "{polyline:?}");
            for p in polyline {
                assert!(
                    (p.distance(centre) - radius).abs() < 1e-9 * radius.max(1.0),
                    "{p} is off the circle"
                );
            }
            for pair in polyline.windows(2) {
                let sagitta = radius - ((pair[0] + pair[1]) / 2.0).distance(centre);
                assert!(sagitta <= deviation, "a chord strays {sagitta} m from the circle");
            }
        }
    }

    /// INP-007: a 30 m distance-lock circle drawn with 0.5 mm deviation,
    /// whole and through a window on its edge, stays within 0.5 mm of the
    /// true circle (the fixed 96 pieces strayed about 16 mm).
    #[test]
    fn circles_are_drawn_within_the_deviation() {
        let (deviation, radius) = (0.5e-3, 30.0);
        let lock = circle(5.0, -3.0, radius);
        let whole = lock.clip(v(-40.0, -40.0), v(40.0, 40.0), deviation);
        assert_eq!(whole.len(), 1);
        assert_follows(&whole, v(5.0, -3.0), radius, deviation);
        let ends = (whole[0][0], whole[0][whole[0].len() - 1]);
        assert!(close(ends.0, ends.1), "closed: {ends:?}");
        let window = lock.clip(v(30.0, -10.0), v(40.0, 0.0), deviation);
        assert_follows(&window, v(5.0, -3.0), radius, deviation);
        assert!(window.iter().map(Vec::len).sum::<usize>() < whole[0].len() / 4);
        let arc = GuideShape::Arc(ArcTarget {
            centre: v(5.0, -3.0),
            radius,
            start: 1.0,
            sweep: -2.5,
        });
        let drawn = arc.clip(v(-40.0, -40.0), v(40.0, 40.0), deviation);
        assert_follows(&drawn, v(5.0, -3.0), radius, deviation);
        let ends = [drawn[0][0], drawn[0][drawn[0].len() - 1]];
        assert_points(
            ends.to_vec(),
            &arc_ends(&ArcTarget {
                centre: v(5.0, -3.0),
                radius,
                start: 1.0,
                sweep: -2.5,
            }),
        );
    }

    /// INP-007: a 1000 km circle through a 2 m window is sampled only along
    /// the angles the window covers, in a handful of pieces that still reach
    /// across the window.
    #[test]
    fn a_huge_circle_through_a_small_rectangle_takes_few_pieces() {
        let (min, max) = (v(-1.0, -1.0), v(1.0, 1.0));
        let radius = 1.0e6;
        let centre = v(0.0, -radius);
        let drawn = circle(centre.x, centre.y, radius).clip(min, max, 1e-6);
        assert_eq!(drawn.len(), 1);
        assert!(drawn[0].len() <= 64, "{} points", drawn[0].len());
        assert_follows(&drawn, centre, radius, 1e-6);
        let xs = drawn[0].iter().map(|p| p.x);
        let (left, right) = xs.fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(x), b.max(x)));
        assert!(left <= -1.0 && right >= 1.0, "reaches across: {left} to {right}");
        let inside = circle(0.0, 0.0, radius).clip(min, max, 1e-6);
        assert!(inside.is_empty(), "the window lies inside the circle");
    }

    /// INP-007: an arc whose gap lies inside the rectangle is drawn as its
    /// two visible stretches, never across the gap.
    #[test]
    fn an_arc_around_the_rectangle_is_drawn_in_two_stretches() {
        let arc = GuideShape::Arc(ArcTarget {
            centre: v(0.0, -10.0),
            radius: 10.0,
            start: FRAC_PI_2 + 0.05,
            sweep: TAU - 0.1,
        });
        let drawn = arc.clip(v(-1.0, -1.0), v(1.0, 1.0), 1e-4);
        assert_eq!(drawn.len(), 2, "{drawn:?}");
        assert_follows(&drawn, v(0.0, -10.0), 10.0, 1e-4);
        for polyline in &drawn {
            assert!(polyline.iter().all(|p| arc.contains(*p)), "{polyline:?} leaves the arc");
        }
    }
}
