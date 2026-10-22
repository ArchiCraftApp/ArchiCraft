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

/// The straight segments a full circle is drawn with.
const CIRCLE_SEGMENTS: f64 = 96.0;

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

    /// The part of the guide inside the rectangle from `min` to `max`, as a
    /// polyline to draw: the clipped piece of a line, a ray or a segment,
    /// empty when it misses the rectangle; a circle or an arc as a whole,
    /// in straight pieces, when it reaches into the rectangle, for the
    /// painter to clip.
    pub fn clip(&self, min: DVec2, max: DVec2) -> Vec<DVec2> {
        match *self {
            GuideShape::Line { point, dir } => clip_line(point, dir, f64::NEG_INFINITY, f64::INFINITY, min, max),
            GuideShape::Ray { origin, dir } => clip_line(origin, dir, 0.0, f64::INFINITY, min, max),
            GuideShape::Segment([a, b]) => {
                let length = a.distance(b);
                clip_line(a, (b - a).normalize_or_zero(), 0.0, length, min, max)
            }
            GuideShape::Circle { centre, radius } => curve_in(&ArcTarget::circle(centre, radius), min, max),
            GuideShape::Arc(arc) => curve_in(&arc, min, max),
        }
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

/// An arc or a circle in straight pieces when its bounding box overlaps the
/// rectangle, else nothing.
fn curve_in(arc: &ArcTarget, min: DVec2, max: DVec2) -> Vec<DVec2> {
    let reach = DVec2::splat(arc.radius);
    let (lo, hi) = (arc.centre - reach, arc.centre + reach);
    if hi.x < min.x || lo.x > max.x || hi.y < min.y || lo.y > max.y {
        return Vec::new();
    }
    let sweep = arc.sweep.clamp(-TAU, TAU);
    let pieces = (CIRCLE_SEGMENTS * sweep.abs() / TAU).ceil().max(1.0) as usize;
    (0..=pieces)
        .map(|i| arc.centre + DVec2::from_angle(arc.start + sweep * i as f64 / pieces as f64) * arc.radius)
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
        assert_points(diagonal.clip(min, max), &[v(0.0, 0.0), v(2.0, 2.0)]);
        let horizontal = GuideShape::line(v(-10.0, 1.5), v(-1.0, 0.0));
        assert_points(horizontal.clip(min, max), &[v(0.0, 1.5), v(4.0, 1.5)]);
        let outside = GuideShape::line(v(0.0, 3.0), v(1.0, 0.0));
        assert!(outside.clip(min, max).is_empty());
        let missing = GuideShape::line(v(6.0, 0.0), v(1.0, 1.0));
        assert!(missing.clip(min, max).is_empty());
        let along_edge = GuideShape::line(v(0.0, 7.0), v(0.0, 1.0));
        assert_points(along_edge.clip(min, max), &[v(0.0, 0.0), v(0.0, 2.0)]);
    }

    #[test]
    fn rays_segments_and_circles_are_clipped_to_their_extent() {
        let (min, max) = (v(0.0, 0.0), v(4.0, 2.0));
        let ray = GuideShape::ray(v(1.0, 1.0), v(1.0, 0.0));
        assert_points(ray.clip(min, max), &[v(1.0, 1.0), v(4.0, 1.0)]);
        let segment = GuideShape::Segment([v(-1.0, 1.0), v(2.0, 1.0)]);
        assert_points(segment.clip(min, max), &[v(0.0, 1.0), v(2.0, 1.0)]);
        let c = circle(2.0, 1.0, 0.5);
        let drawn = c.clip(min, max);
        assert!(drawn.len() > 8);
        assert!(drawn.iter().all(|p| (p.distance(v(2.0, 1.0)) - 0.5).abs() < 1e-12));
        assert!(circle(9.0, 9.0, 1.0).clip(min, max).is_empty());
    }
}
