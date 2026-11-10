//! Closed cross-sections of profiles (PRF-004, PRF-005, COL-001, IFX-034,
//! IFX-038).
//!
//! A [`Section`] is the cross-section a column or beam is swept from: one or
//! more regions, each a closed outer loop with holes and a material. The
//! loops are polygons in the profile plane (x to the right, y up, metres;
//! see [`archicraft_model::profile`]). They are closed without a repeated
//! last point, outer loops counter-clockwise and holes clockwise, and have no
//! edge shorter than [`MIN_EDGE_LENGTH`], so they pass
//! [`validate_ring`](archicraft_model::validate::validate_ring) as they are,
//! and every hole lies inside its outer loop. Curved edges (fillets and
//! circles) are tessellated by [`arc_segments`] (IFX-034), and every vertex
//! records whether the boundary is smooth there, so that a sweep shares
//! vertices across smooth edges only (IFX-038).
//!
//! [`profile_section`] builds the section of a [`Profile`] resource
//! (PRF-004): the parametric section of its shape or its custom regions.
//! A member places it with [`Section::transformed`], anchored at one of
//! the [`Section::nine_point`]s of its bounding box (COL-001).
//!
//! [`parametric_section`] builds the nine standard shapes from their
//! dimensions, centred on their bounding box. Orientation conventions, which
//! the profile editor's preview shows: an I/H section is symmetric about
//! both axes; a T section has its flange on top; an L section has its heel
//! at the bottom left, with legs along +x and +y and the toe radii on the
//! inner edge of each toe; a U/C section has its web on the left and opens
//! towards +x; a Z section has its top flange towards +x and its bottom
//! flange towards -x.

use std::f64::consts::{FRAC_PI_2, TAU};

use archicraft_model::outline::Ring;
use archicraft_model::profile::{ParametricShape, Profile, ProfileRegion, ProfileShape};
use archicraft_model::project::Tessellation;
use archicraft_model::validate::{MIN_EDGE_LENGTH, check_parametric_shape, validate_slab_polygon};
use archicraft_model::{MaterialId, Message};
use glam::DVec2;

use crate::GeometryError;

/// Most segments a full turn is tessellated into, whatever the settings
/// (NFR-SEC-003): tessellation values read from a file are not range
/// checked, and a deviation of nearly zero would otherwise ask for unbounded
/// work. 1024 segments per turn deviate at most 24 µm from the largest
/// circle a profile may have (10 m across, MAX_PROFILE_DIMENSION).
pub const MAX_SEGMENTS_PER_TURN: usize = 1024;

/// The role of the one region of a parametric section, which a member
/// made of one region names its body part by.
pub const BODY_ROLE: &str = "Body";

/// The role of region `index` (from zero) of a section of `count` regions:
/// [`BODY_ROLE`] for the only one, `Region_1`, `Region_2`, ... in order for
/// several. A member names the parts of its body by them.
fn region_role(index: usize, count: usize) -> String {
    if count == 1 {
        BODY_ROLE.to_owned()
    } else {
        format!("Region_{}", index + 1)
    }
}

/// A closed loop of a section region.
#[derive(Clone, Debug, PartialEq)]
pub struct SectionLoop {
    pub points: Vec<DVec2>,
    /// Per point: whether the boundary is smooth there, that is the point
    /// lies on an arc (its tangent points included) rather than on a sharp
    /// corner (IFX-038). Same length as `points`.
    pub smooth: Vec<bool>,
}

impl SectionLoop {
    /// The shoelace area: positive for a counter-clockwise loop.
    pub fn signed_area(&self) -> f64 {
        Ring::<()>::from_points(self.points.iter().copied()).signed_area()
    }

    /// A loop of hard vertices only.
    fn hard(points: Vec<DVec2>) -> Self {
        let smooth = vec![false; points.len()];
        Self { points, smooth }
    }

    /// The signed area and the signed first moment of area of the loop,
    /// with its points taken relative to `origin`: the centroid of the
    /// loop is `origin + moment / area`.
    fn moments(&self, origin: DVec2) -> (f64, DVec2) {
        let n = self.points.len();
        (0..n).fold((0.0, DVec2::ZERO), |(area, moment), i| {
            let (p, q) = (self.points[i] - origin, self.points[(i + 1) % n] - origin);
            let cross = p.perp_dot(q);
            (area + cross / 2.0, moment + (p + q) * cross / 6.0)
        })
    }

    fn transformed(&self, offset: DVec2, turn: DVec2) -> Self {
        Self {
            points: self.points.iter().map(|&p| turn.rotate(p + offset)).collect(),
            smooth: self.smooth.clone(),
        }
    }

    fn reversed(mut self) -> Self {
        self.points.reverse();
        self.smooth.reverse();
        self
    }
}

/// One region of a section: an outer loop (counter-clockwise) with holes
/// (clockwise), the material it is made of and the role that names its part
/// of a member's body.
#[derive(Clone, Debug, PartialEq)]
pub struct SectionRegion {
    pub outer: SectionLoop,
    pub holes: Vec<SectionLoop>,
    pub material: MaterialId,
    pub role: String,
}

impl SectionRegion {
    /// The area of the outer loop minus that of the holes.
    pub fn area(&self) -> f64 {
        self.outer.signed_area().abs() - self.holes.iter().map(|h| h.signed_area().abs()).sum::<f64>()
    }

    /// The area and the first moment of area of the region about `origin`,
    /// the holes subtracted whichever way the loops run.
    fn moments(&self, origin: DVec2) -> (f64, DVec2) {
        let unsigned = |ring: &SectionLoop| {
            let (area, moment) = ring.moments(origin);
            if area < 0.0 { (-area, -moment) } else { (area, moment) }
        };
        let (area, moment) = unsigned(&self.outer);
        self.holes
            .iter()
            .map(unsigned)
            .fold((area, moment), |(a, m), (ha, hm)| (a - ha, m - hm))
    }
}

/// A closed cross-section: regions that do not overlap (PRF-004, PRF-005).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Section {
    pub regions: Vec<SectionRegion>,
}

impl Section {
    /// The area of all regions, holes subtracted.
    pub fn area(&self) -> f64 {
        self.regions.iter().map(SectionRegion::area).sum()
    }

    /// The bounding box of the outer loops as (min, max), or `None` for a
    /// section without points. For a parametric section it is the box of
    /// its dimensions, (-b/2, -h/2)..(b/2, h/2), exactly.
    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        self.regions
            .iter()
            .flat_map(|r| r.outer.points.iter())
            .fold(None, |b, &p| {
                Some(match b {
                    None => (p, p),
                    Some((min, max)) => (min.min(p), max.max(p)),
                })
            })
    }

    /// The centroid of all regions, each weighted by its area with the
    /// holes subtracted (COL-001), or `None` for a section without area.
    /// The origin for a shape symmetric about both axes.
    pub fn centroid(&self) -> Option<DVec2> {
        // Moments about the centre of the box keep the products small.
        let (min, max) = self.bounds()?;
        let origin = (min + max) / 2.0;
        let (area, moment) = self
            .regions
            .iter()
            .map(|r| r.moments(origin))
            .fold((0.0, DVec2::ZERO), |(a, m), (ra, rm)| (a + ra, m + rm));
        (area > 0.0).then(|| origin + moment / area).filter(|c| c.is_finite())
    }

    /// One of the nine anchor points of the cross-section (COL-001): the
    /// corners, edge midpoints and centre of [`Section::bounds`], numbered
    /// row by row from the top left, so 0 is (min.x, max.y), 1 the middle of
    /// the top edge, 4 the centre and 8 (max.x, min.y). `None` for an index
    /// from 9 on and for a section without points.
    pub fn nine_point(&self, index: usize) -> Option<DVec2> {
        if index >= 9 {
            return None;
        }
        let (min, max) = self.bounds()?;
        let centre = (min + max) / 2.0;
        let x = [min.x, centre.x, max.x][index % 3];
        let y = [max.y, centre.y, min.y][index / 3];
        Some(DVec2::new(x, y))
    }

    /// The section moved by `offset` and then turned by `rotation` radians
    /// counter-clockwise about the origin (COL-001): every point p moves to
    /// rotate(p + offset). A member anchored at the point a turns about its
    /// axis with `transformed(-a, rotation)`. A rotation keeps every loop's
    /// orientation, so outer loops stay counter-clockwise and holes
    /// clockwise; the smooth flags, materials and roles stay as they are.
    #[must_use]
    pub fn transformed(&self, offset: DVec2, rotation: f64) -> Section {
        let turn = DVec2::from_angle(rotation);
        Section {
            regions: self
                .regions
                .iter()
                .map(|r| SectionRegion {
                    outer: r.outer.transformed(offset, turn),
                    holes: r.holes.iter().map(|h| h.transformed(offset, turn)).collect(),
                    material: r.material,
                    role: r.role.clone(),
                })
                .collect(),
        }
    }
}

/// The closed section of a profile (PRF-004, PRF-005), in profile
/// coordinates (the insertion [`Profile::origin`] is not applied; a member
/// moves the section with [`Section::transformed`]).
///
/// A parametric profile has the [`parametric_section`] of its shape and
/// material. A custom profile has one region per profile region, in order,
/// with its material and holes, every vertex hard (custom profiles have no
/// arcs yet), and the role [`BODY_ROLE`] when it is the only region,
/// otherwise `Region_1`, `Region_2`, ... Each region is checked and
/// normalised as [`validate_profile`] does, outer loops counter-clockwise
/// and holes clockwise, so a validated profile's rings are taken as they
/// are; a profile without regions or with a region that is no valid
/// polygon with holes has no section, and the error carries the message
/// [`validate_profile`] gives. Whether regions overlap is left to
/// [`validate_profile`].
///
/// [`validate_profile`]: archicraft_model::validate::validate_profile
pub fn profile_section(profile: &Profile, tessellation: &Tessellation) -> Result<Section, GeometryError> {
    match &profile.shape {
        ProfileShape::Parametric { shape, material } => parametric_section(shape, *material, tessellation),
        ProfileShape::Custom { regions } => custom_section(regions),
    }
}

/// The section of the regions of a custom profile, see [`profile_section`].
fn custom_section(regions: &[ProfileRegion]) -> Result<Section, GeometryError> {
    if regions.is_empty() {
        return Err(GeometryError::InvalidPolygon(Message::new(
            "model-error-profile-no-regions",
        )));
    }
    let count = regions.len();
    let regions = regions
        .iter()
        .enumerate()
        .map(|(i, region)| {
            let (outer, holes) = validate_slab_polygon(&region.outer, &region.holes).map_err(|e| {
                GeometryError::InvalidPolygon(
                    Message::new("model-error-profile-region")
                        .count("number", i + 1)
                        .nested("reason", e),
                )
            })?;
            Ok(SectionRegion {
                outer: SectionLoop::hard(outer),
                holes: holes.into_iter().map(SectionLoop::hard).collect(),
                material: region.material,
                role: region_role(i, count),
            })
        })
        .collect::<Result<Vec<_>, GeometryError>>()?;
    Ok(Section { regions })
}

/// The number of segments an arc of `radius` sweeping `sweep` radians (of
/// either sign) is tessellated into (IFX-034): the larger of the counts the
/// two rules need, so that no segment spans more than
/// `tessellation.max_angle_deg` and no chord lies more than
/// `tessellation.chord_deviation` from the arc, and at least one. A rule
/// whose value is not positive is left out, a deviation as large as the
/// radius allows any chord, and no full turn has more than
/// [`MAX_SEGMENTS_PER_TURN`] segments.
///
/// The shared `tessellate(curve, settings)` of 2D construction geometry
/// (roadmap feature `drafting-trim-split`) implements the same rule; when it
/// lands, the two are merged.
pub fn arc_segments(radius: f64, sweep: f64, tessellation: &Tessellation) -> usize {
    let sweep = sweep.abs();
    if !(radius > 0.0 && radius.is_finite() && sweep.is_finite()) {
        return 1;
    }
    let max_angle = tessellation.max_angle_deg.to_radians();
    let by_angle = if max_angle > 0.0 { steps(sweep, max_angle) } else { 0.0 };
    let deviation = tessellation.chord_deviation;
    let by_chord = if deviation > 0.0 && deviation < radius {
        steps(sweep, 2.0 * (1.0 - deviation / radius).acos())
    } else {
        0.0
    };
    let most = steps(sweep, TAU / MAX_SEGMENTS_PER_TURN as f64);
    // `max` and `min` pass over a NaN, so the count stays within 1..=most.
    by_angle.max(by_chord).min(most).max(1.0) as usize
}

/// `span / step` rounded up to a whole number, after removing the few units
/// in the last place of rounding error that radians carry: 90° in steps of
/// 10° is 9 steps, not 10.
fn steps(span: f64, step: f64) -> f64 {
    (span / step * (1.0 - 8.0 * f64::EPSILON)).ceil()
}

/// The most segments an arc may have without a chord shorter than
/// [`MIN_EDGE_LENGTH`] (EDT-019): zero when even the chord of the whole arc
/// is shorter.
fn longest_count(radius: f64, sweep: f64) -> usize {
    let half_chord = MIN_EDGE_LENGTH / (2.0 * radius);
    if half_chord > 1.0 {
        return 0;
    }
    (sweep.abs() / (2.0 * half_chord.asin())).floor() as usize
}

/// The segments of an arc of `radius`: [`arc_segments`] for an arc of
/// `count_radius` (at least `radius`) over the same angle, but no chord
/// shorter than [`MIN_EDGE_LENGTH`]; zero when the arc is too small to draw.
///
/// A hole takes the counts of the outer loop's arc about the same centre,
/// so its vertices lie on the same rays and every chord of the hole is a
/// scaled copy of the outer one, nearer the centre: the hole stays inside
/// however thin the wall, where counts of its own could put a vertex of
/// the hole beyond a chord of the outer loop. The smaller radius gets the
/// finer chords, so IFX-034 still holds.
fn segments(radius: f64, count_radius: f64, sweep: f64, tessellation: &Tessellation) -> usize {
    arc_segments(count_radius, sweep, tessellation).min(longest_count(radius, sweep))
}

/// The closed section of a parametric shape (PRF-005): one region of
/// `material` with the role [`BODY_ROLE`], centred on the shape's bounding
/// box, in the orientation the module documentation describes, its arcs
/// tessellated with `tessellation` (IFX-034).
///
/// The fillets are quarter circles: the root radii of I/H, T and U/C
/// sections, the root and toe radii of an L section and the outer corner
/// radius of a rectangular hollow section, whose inner radius is the outer
/// one less the thickness, or a sharp corner when that is not positive.
/// Circles and circular hollow sections are rings of four quarter arcs that
/// start at angle 0, so equal diameters tessellate equally and every circle
/// reaches its bounding box. A Z section has sharp corners.
///
/// A shape [`check_parametric_shape`] rejects has no section, and nor does
/// one so close to degenerate that its loops would not make a valid polygon
/// with holes ([`validate_slab_polygon`]; for example a bore narrower than
/// [`MIN_EDGE_LENGTH`]); the error carries the validation's message.
pub fn parametric_section(
    shape: &ParametricShape,
    material: MaterialId,
    tessellation: &Tessellation,
) -> Result<Section, GeometryError> {
    check_parametric_shape(shape).map_err(GeometryError::InvalidPolygon)?;
    let (outer, holes) = shape_loops(shape, tessellation);
    let hole_points: Vec<Vec<DVec2>> = holes.iter().map(|h| h.points.clone()).collect();
    validate_slab_polygon(&outer.points, &hole_points).map_err(GeometryError::InvalidPolygon)?;
    Ok(Section {
        regions: vec![SectionRegion {
            outer,
            holes,
            material,
            role: BODY_ROLE.to_owned(),
        }],
    })
}

/// The outer loop and the holes of a valid parametric shape.
fn shape_loops(shape: &ParametricShape, tessellation: &Tessellation) -> (SectionLoop, Vec<SectionLoop>) {
    let rounded = |corners: &[Corner]| rounded_loop(corners, tessellation);
    let size = shape.size() / 2.0;
    let (x, y) = (size.x, size.y);
    match *shape {
        ParametricShape::Rectangle { .. } => (rounded(&rectangle(x, y, 0.0, 0.0)), Vec::new()),
        ParametricShape::Circle { diameter } => {
            let radius = diameter / 2.0;
            (circle_loop(radius, radius, tessellation), Vec::new())
        }
        ParametricShape::RectangularHollow {
            thickness,
            outer_radius,
            ..
        } => {
            // The inner fillets share the outer ones' centres and counts.
            let inner_radius = (outer_radius - thickness).max(0.0);
            let hole = rounded(&rectangle(x - thickness, y - thickness, inner_radius, outer_radius));
            let outer = rounded(&rectangle(x, y, outer_radius, outer_radius));
            (outer, vec![hole.reversed()])
        }
        ParametricShape::CircularHollow { diameter, thickness } => {
            // The bore takes the outer circle's count, so the rings nest.
            let radius = diameter / 2.0;
            let hole = circle_loop(radius - thickness, radius, tessellation);
            (circle_loop(radius, radius, tessellation), vec![hole.reversed()])
        }
        ParametricShape::IShape {
            web,
            flange,
            root_radius: r,
            ..
        } => {
            // f: the inner face of the flanges, w: the faces of the web.
            let (w, f) = (web / 2.0, y - flange);
            let corners = [
                Corner::sharp(-x, -y),
                Corner::sharp(x, -y),
                Corner::sharp(x, -f),
                Corner::round(w, -f, r),
                Corner::round(w, f, r),
                Corner::sharp(x, f),
                Corner::sharp(x, y),
                Corner::sharp(-x, y),
                Corner::sharp(-x, f),
                Corner::round(-w, f, r),
                Corner::round(-w, -f, r),
                Corner::sharp(-x, -f),
            ];
            (rounded(&corners), Vec::new())
        }
        ParametricShape::TShape {
            web,
            flange,
            root_radius: r,
            ..
        } => {
            let (w, f) = (web / 2.0, y - flange);
            let corners = [
                Corner::sharp(-w, -y),
                Corner::sharp(w, -y),
                Corner::round(w, f, r),
                Corner::sharp(x, f),
                Corner::sharp(x, y),
                Corner::sharp(-x, y),
                Corner::sharp(-x, f),
                Corner::round(-w, f, r),
            ];
            (rounded(&corners), Vec::new())
        }
        ParametricShape::LShape {
            thickness,
            root_radius,
            toe_radius,
            ..
        } => {
            // The inner faces of the vertical and the horizontal leg.
            let (ix, iy) = (-x + thickness, -y + thickness);
            let corners = [
                Corner::sharp(-x, -y),
                Corner::sharp(x, -y),
                Corner::round(x, iy, toe_radius),
                Corner::round(ix, iy, root_radius),
                Corner::round(ix, y, toe_radius),
                Corner::sharp(-x, y),
            ];
            (rounded(&corners), Vec::new())
        }
        ParametricShape::UShape {
            web,
            flange,
            root_radius: r,
            ..
        } => {
            // w: the inner face of the web, f: that of the flanges.
            let (w, f) = (-x + web, y - flange);
            let corners = [
                Corner::sharp(-x, -y),
                Corner::sharp(x, -y),
                Corner::sharp(x, -f),
                Corner::round(w, -f, r),
                Corner::round(w, f, r),
                Corner::sharp(x, f),
                Corner::sharp(x, y),
                Corner::sharp(-x, y),
            ];
            (rounded(&corners), Vec::new())
        }
        ParametricShape::ZShape { web, flange, .. } => {
            let (w, f) = (web / 2.0, y - flange);
            let corners = [
                Corner::sharp(-x, -y),
                Corner::sharp(w, -y),
                Corner::sharp(w, f),
                Corner::sharp(x, f),
                Corner::sharp(x, y),
                Corner::sharp(-w, y),
                Corner::sharp(-w, -f),
                Corner::sharp(-x, -f),
            ];
            (rounded(&corners), Vec::new())
        }
    }
}

/// A corner of a polygon and the radius of the fillet that rounds it (zero
/// for a sharp corner), tessellated with the count of an arc of
/// `count_radius` ([`segments`]).
#[derive(Clone, Copy, Debug)]
struct Corner {
    point: DVec2,
    radius: f64,
    count_radius: f64,
}

impl Corner {
    fn sharp(x: f64, y: f64) -> Self {
        Self::round(x, y, 0.0)
    }

    fn round(x: f64, y: f64, radius: f64) -> Self {
        Self {
            point: DVec2::new(x, y),
            radius,
            count_radius: radius,
        }
    }
}

/// The rectangle (-x, -y)..(x, y), counter-clockwise from the bottom left,
/// every corner rounded with `radius` and tessellated with the count of an
/// arc of `count_radius`.
fn rectangle(x: f64, y: f64, radius: f64, count_radius: f64) -> [Corner; 4] {
    [(-x, -y), (x, -y), (x, y), (-x, y)].map(|(x, y)| Corner {
        count_radius,
        ..Corner::round(x, y, radius)
    })
}

/// The loop through `corners` (counter-clockwise, no repeated last corner)
/// with every rounded corner replaced by the arc of its radius tangent to
/// both adjacent edges, [`fillet`]. The fillets must fit their edges, as
/// [`check_parametric_shape`] ensures; a fillet that uses up an edge makes
/// its tangent point meet the next corner, and the two become one point.
fn rounded_loop(corners: &[Corner], tessellation: &Tessellation) -> SectionLoop {
    let n = corners.len();
    let mut points = Vec::with_capacity(n);
    let mut smooth = Vec::with_capacity(n);
    for (i, corner) in corners.iter().enumerate() {
        let prev = corners[(i + n - 1) % n].point;
        let next = corners[(i + 1) % n].point;
        match fillet(prev, *corner, next, tessellation) {
            Some(arc) => {
                smooth.resize(smooth.len() + arc.len(), true);
                points.extend(arc);
            }
            None => {
                points.push(corner.point);
                smooth.push(false);
            }
        }
    }
    cleaned(points, smooth)
}

/// The points of the arc that rounds `corner` between the edge from `prev`
/// and the edge to `next`, from tangent point to tangent point, or `None`
/// when the corner stays sharp: it has no radius, or the arc is too small
/// for a chord of [`MIN_EDGE_LENGTH`].
///
/// The tangent points are computed along the edges, so they lie on them
/// exactly; the points between are `centre + r (cos, sin)` of the start
/// angle plus `i` times the step, without accumulated rotation.
fn fillet(prev: DVec2, corner: Corner, next: DVec2, tessellation: &Tessellation) -> Option<Vec<DVec2>> {
    let Corner {
        point,
        radius,
        count_radius,
    } = corner;
    if radius <= 0.0 {
        return None;
    }
    let (incoming, outgoing) = ((point - prev).normalize(), (next - point).normalize());
    // Positive for a left turn (a convex corner of a counter-clockwise
    // loop), negative for a right turn (a concave one).
    let turn = incoming.perp_dot(outgoing).atan2(incoming.dot(outgoing));
    let count = segments(radius, count_radius, turn, tessellation);
    if count == 0 {
        return None;
    }
    let tangent = radius * (turn.abs() / 2.0).tan();
    let (start, end) = (point - incoming * tangent, point + outgoing * tangent);
    let centre = start + incoming.perp() * radius * turn.signum();
    let from = start - centre;
    let start_angle = from.y.atan2(from.x);
    let mut arc = Vec::with_capacity(count + 1);
    arc.push(start);
    for i in 1..count {
        let angle = start_angle + turn * i as f64 / count as f64;
        arc.push(centre + radius * DVec2::new(angle.cos(), angle.sin()));
    }
    arc.push(end);
    Some(arc)
}

/// A circle of `radius` about the origin, counter-clockwise from angle 0:
/// four quarter arcs of the count of an arc of `count_radius` over 90°
/// ([`segments`]). The other quarters are the first turned by exact quarter
/// turns, so the circle is symmetric and its four extreme points lie on its
/// bounding box.
fn circle_loop(radius: f64, count_radius: f64, tessellation: &Tessellation) -> SectionLoop {
    // A circle too small for chords of MIN_EDGE_LENGTH collapses when
    // cleaned and is rejected by validate_slab_polygon.
    let count = segments(radius, count_radius, FRAC_PI_2, tessellation).max(1);
    let quarter: Vec<DVec2> = (0..count)
        .map(|i| {
            let angle = FRAC_PI_2 * i as f64 / count as f64;
            radius * DVec2::new(angle.cos(), angle.sin())
        })
        .collect();
    let points: Vec<DVec2> = [DVec2::X, DVec2::Y, DVec2::NEG_X, DVec2::NEG_Y]
        .into_iter()
        .flat_map(|turn| quarter.iter().map(move |&p| turn.rotate(p)))
        .collect();
    let smooth = vec![true; points.len()];
    cleaned(points, smooth)
}

/// The loop without points closer than [`MIN_EDGE_LENGTH`] to the point
/// before them, the last point included, as [`clean_ring`] removes them, so
/// that the result is a fixed point of `clean_ring`. A removed point merges
/// into the point it follows (or, for the last, the first point), which
/// stays smooth only if both were: a tangent point that meets a sharp corner
/// is that corner, two tangent points that meet are smooth (IFX-038).
///
/// [`clean_ring`]: archicraft_model::validate::clean_ring
fn cleaned(points: Vec<DVec2>, smooth: Vec<bool>) -> SectionLoop {
    let mut out = SectionLoop {
        points: Vec::with_capacity(points.len()),
        smooth: Vec::with_capacity(smooth.len()),
    };
    for (p, s) in points.into_iter().zip(smooth) {
        match (out.points.last(), out.smooth.last_mut()) {
            (Some(q), Some(merged)) if q.distance(p) < MIN_EDGE_LENGTH => *merged &= s,
            _ => {
                out.points.push(p);
                out.smooth.push(s);
            }
        }
    }
    while out.points.len() > 1 && out.points[0].distance(out.points[out.points.len() - 1]) < MIN_EDGE_LENGTH {
        out.points.pop();
        if let Some(s) = out.smooth.pop() {
            out.smooth[0] &= s;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> Tessellation {
        Tessellation::default()
    }

    /// A convex corner of a counter-clockwise loop is rounded by an arc
    /// turning left, a concave one by an arc turning right; both end on the
    /// edges at the tangent points, one radius from the corner for a right
    /// angle.
    #[test]
    fn fillets_are_tangent_to_both_edges() {
        let tessellation = defaults();
        let r = 0.1;
        let convex = fillet(
            DVec2::new(1.0, 0.0),
            Corner::round(1.0, 1.0, r),
            DVec2::new(0.0, 1.0),
            &tessellation,
        )
        .unwrap_or_default();
        assert_eq!(convex.len(), 10);
        assert_eq!(convex[0].x, 1.0);
        assert!((convex[0].y - 0.9).abs() < 1e-15);
        assert_eq!(convex[9].y, 1.0);
        let centre = DVec2::new(0.9, 0.9);
        assert!(convex.iter().all(|p| (p.distance(centre) - r).abs() < 1e-15));
        // Right of the chord from the first to the last point, outside the
        // loop: the arc bulges out.
        assert!((convex[9] - convex[0]).perp_dot(convex[5] - convex[0]) < 0.0);

        let concave = fillet(
            DVec2::new(1.0, 0.0),
            Corner::round(0.0, 0.0, r),
            DVec2::new(0.0, 1.0),
            &tessellation,
        )
        .unwrap_or_default();
        let centre = DVec2::new(0.1, 0.1);
        assert!(concave.iter().all(|p| (p.distance(centre) - r).abs() < 1e-15));
        assert_eq!(concave[0].y, 0.0);
        assert_eq!(concave[concave.len() - 1].x, 0.0);
    }

    /// EDT-019: no chord is shorter than MIN_EDGE_LENGTH; an arc too small
    /// for one chord is a sharp corner.
    #[test]
    fn small_arcs_have_fewer_segments_or_none() {
        let fine = Tessellation {
            chord_deviation: 0.002,
            max_angle_deg: 1.0,
        };
        let r = 3.0 * MIN_EDGE_LENGTH;
        let count = segments(r, r, FRAC_PI_2, &fine);
        assert_eq!(count, 4);
        assert!(2.0 * r * (FRAC_PI_2 / (2.0 * count as f64)).sin() >= MIN_EDGE_LENGTH);
        assert!(2.0 * r * (FRAC_PI_2 / (2.0 * (count + 1) as f64)).sin() < MIN_EDGE_LENGTH);
        assert_eq!(
            segments(MIN_EDGE_LENGTH / 2.0, MIN_EDGE_LENGTH / 2.0, FRAC_PI_2, &fine),
            0
        );
        // A hole takes the count of the larger arc around it, but not chords
        // shorter than MIN_EDGE_LENGTH.
        assert_eq!(segments(0.012, 1.0, FRAC_PI_2, &fine), 90);
        assert_eq!(segments(r, 1.0, FRAC_PI_2, &fine), 4);
        let corner = Corner::round(0.0, 0.0, MIN_EDGE_LENGTH / 2.0);
        assert_eq!(fillet(DVec2::X, corner, DVec2::Y, &fine), None);
    }

    /// IFX-038: a point that falls on the one before it is merged into it,
    /// which stays smooth only if both were; the last point merges into the
    /// first.
    #[test]
    fn merged_points_are_smooth_only_if_both_were() {
        let p = |x| DVec2::new(x, 0.0);
        let near = MIN_EDGE_LENGTH / 2.0;
        let ring = cleaned(
            vec![p(0.0), p(1.0), p(1.0 + near), p(2.0), p(2.0 + near), p(3.0), p(near)],
            vec![false, true, false, true, true, true, true],
        );
        assert_eq!(ring.points, [p(0.0), p(1.0), p(2.0), p(3.0)]);
        assert_eq!(ring.smooth, [false, false, true, true]);
    }
}
