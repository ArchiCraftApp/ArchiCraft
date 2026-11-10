//! Members swept from a cross-section along a straight axis between two
//! planar end cuts (COL-001, COL-003, BEA-001, BEA-002, IFX-032, IFX-038).
//!
//! A column or beam is the prism of its [`Section`] along its axis, cut at
//! both ends by a plane: horizontal or at a custom angle for a column
//! (COL-003), vertical, horizontal or at a custom angle at each end of a beam
//! (BEA-002). Every lateral edge of a straight prism is parallel to the
//! axis, so a cut plane meets each lateral line exactly once: the end face is
//! the section mapped onto the plane, an affine image that keeps the
//! section's validity and triangulation, and every lateral face is a planar
//! trapezoid. No Boolean kernel and no triangle clipping are needed, and the
//! volumes are exact.

use archicraft_model::MaterialId;
use archicraft_model::Message;
use archicraft_model::tolerance::{ANGLE_TOLERANCE, MODEL_TOLERANCE};
use archicraft_model::validate::MIN_HEIGHT;
use glam::{DVec2, DVec3};

use crate::GeometryError;
use crate::mesh::TriMesh;
use crate::section::{Section, SectionLoop, SectionRegion};
use crate::triangulate::triangulate;

/// The smallest angle between an end cut and the member's axis, in degrees:
/// a plane closer to parallel meets the lateral lines too far out to give a
/// usable end face.
pub const MIN_CUT_ANGLE_DEG: f64 = 1.0;

/// The placement of a member's cross-section in space: the member runs
/// along `axis` through `origin`, where the section's origin lies, with the
/// section's x axis along `u` and its y axis along `v`.
///
/// The three directions are orthonormal and right-handed (`u × v = axis`),
/// so the section appears as drawn when the member is seen from its end
/// looking back along the axis: a column's from above, as in plan.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MemberFrame {
    pub origin: DVec3,
    pub axis: DVec3,
    pub u: DVec3,
    pub v: DVec3,
}

impl MemberFrame {
    /// The point of section point `p` at `s` metres along the axis.
    pub fn point(&self, p: DVec2, s: f64) -> DVec3 {
        self.origin + self.u * p.x + self.v * p.y + self.axis * s
    }

    /// Whether the frame is finite, orthonormal and right-handed, its
    /// directions unit and perpendicular within [`ANGLE_TOLERANCE`].
    pub fn is_valid(&self) -> bool {
        let unit = |d: DVec3| (d.length_squared() - 1.0).abs() <= ANGLE_TOLERANCE;
        let square = |a: DVec3, b: DVec3| a.dot(b).abs() <= ANGLE_TOLERANCE;
        let (axis, u, v) = (self.axis, self.u, self.v);
        self.origin.is_finite()
            && unit(axis)
            && unit(u)
            && unit(v)
            && square(u, v)
            && square(v, axis)
            && square(axis, u)
            && u.cross(v).dot(axis) > 0.0
    }
}

/// A planar end cut: the plane through `point` with the normal `normal`,
/// which may have any length but zero and either orientation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CutPlane {
    pub point: DVec3,
    pub normal: DVec3,
}

/// The body of one region of a swept section: a closed mesh (IFX-032) with
/// the region's material and role, which names its part of the member's
/// body.
#[derive(Clone, Debug, PartialEq)]
pub struct SweptRegion {
    pub mesh: TriMesh,
    pub material: MaterialId,
    pub role: String,
}

/// The frame of a column at `position`, turned by `rotation` radians
/// counter-clockwise about its axis, which points up (+Z, COL-001): with no
/// rotation the section's x and y run along the world's X and Y.
pub fn frame_for_column(position: DVec3, rotation: f64) -> MemberFrame {
    let (sin, cos) = rotation.sin_cos();
    MemberFrame {
        origin: position,
        axis: DVec3::Z,
        u: DVec3::new(cos, sin, 0.0),
        v: DVec3::new(-sin, cos, 0.0),
    }
}

/// The frame of a beam from `start` to `end`, rolled by `roll` radians about
/// its axis (BEA-001), with its origin at `start`.
///
/// Without roll, `v` is the world's up direction made perpendicular to the
/// axis, and `u = v × axis` is horizontal, so the frame is right-handed (see
/// [`MemberFrame`]): `u` points to the left of a viewer at the start looking
/// towards the end. The roll turns `u` and `v` counter-clockwise about the
/// axis, as [`Section::transformed`] turns the section. A beam whose ends
/// coincide in plan within [`MODEL_TOLERANCE`] is vertical or has no length,
/// so it has no up direction and no frame.
pub fn frame_for_beam(start: DVec3, end: DVec3, roll: f64) -> Result<MemberFrame, GeometryError> {
    let delta = end - start;
    let (plan, length) = (delta.truncate().length(), delta.length());
    if !(plan >= MODEL_TOLERANCE && length.is_finite() && roll.is_finite() && start.is_finite()) {
        return Err(axis_undefined());
    }
    // The plan direction and the sine and cosine of the pitch, so that
    // `axis = (h cos, sin)` and `v = (-h sin, cos)` are exact to rounding
    // however steep the beam.
    let h = delta.truncate() / plan;
    let (sin, cos) = (delta.z / length, plan / length);
    let axis = delta / length;
    let u = h.perp().extend(0.0);
    let v = (-h * sin).extend(cos);
    let (roll_sin, roll_cos) = roll.sin_cos();
    Ok(MemberFrame {
        origin: start,
        axis,
        u: u * roll_cos + v * roll_sin,
        v: v * roll_cos - u * roll_sin,
    })
}

/// Sweeps `section` along `frame` from the plane `start` to the plane `end`
/// (COL-001, COL-003, BEA-001, BEA-002): one closed, outward-oriented mesh
/// per region, in the section's order, with the region's material and role.
///
/// Each section point `p` is swept along its lateral line
/// `frame.point(p, s)`, which meets each cut plane at one `s`; the end
/// faces are the region mapped onto the planes, triangulated once and used
/// for both, and the lateral faces are trapezoids between the two images of
/// each edge, split along their shorter diagonal. Every planar face has its
/// own vertices, except that the lateral faces share them across the
/// section's smooth vertices (IFX-038): each loop's lateral faces form
/// strips that break at its hard vertices, starting at the first. A section
/// within the profile limit of 10,000 points
/// ([`MAX_SLAB_POINTS`](archicraft_model::validate::MAX_SLAB_POINTS)) gives
/// meshes of at most 60,000 points, within
/// [`MAX_MESH_POINTS`](crate::mesh::MAX_MESH_POINTS).
///
/// Errors are [`GeometryError::InvalidExtent`] with the message
///
/// * `geometry-end-cuts-cross` when the end plane is less than
///   [`MIN_HEIGHT`] further along the axis than the start plane at any point
///   of the section: the cuts cross within it, lie the wrong way round or
///   leave too short a member;
/// * `geometry-end-cut-parallel` when a plane meets the axis at less than
///   [`MIN_CUT_ANGLE_DEG`];
/// * `geometry-member-axis-undefined` when the frame is not
///   [valid](MemberFrame::is_valid) or a plane is not finite or has a zero
///   normal;
///
/// and the [`triangulate`] error of a region that is no valid polygon.
pub fn sweep(
    section: &Section,
    frame: &MemberFrame,
    start: &CutPlane,
    end: &CutPlane,
) -> Result<Vec<SweptRegion>, GeometryError> {
    if !frame.is_valid() {
        return Err(axis_undefined());
    }
    let (start, end) = (Depth::of(start, frame)?, Depth::of(end, frame)?);
    section
        .regions
        .iter()
        .map(|region| sweep_region(region, frame, &start, &end))
        .collect()
}

fn axis_undefined() -> GeometryError {
    GeometryError::InvalidExtent(Message::new("geometry-member-axis-undefined"))
}

/// Where a cut plane meets the lateral lines: `s(p) = at_origin + slope · p`
/// along the axis for the section point `p`. The planes meet the lines at
/// `s = n · (Q - P₀) / (n · axis)`, with `P₀` the line's point at `s = 0`,
/// which is affine in `p`.
struct Depth {
    at_origin: f64,
    slope: DVec2,
}

impl Depth {
    fn of(plane: &CutPlane, frame: &MemberFrame) -> Result<Self, GeometryError> {
        let length = plane.normal.length();
        if !(plane.point.is_finite() && length.is_finite() && length > 0.0) {
            return Err(axis_undefined());
        }
        let normal = plane.normal / length;
        // The sine of the angle between the plane and the axis, signed.
        let along = normal.dot(frame.axis);
        let min = MIN_CUT_ANGLE_DEG.to_radians();
        if along.abs() < min.sin() {
            return Err(GeometryError::InvalidExtent(
                Message::new("geometry-end-cut-parallel").angle("min", min),
            ));
        }
        Ok(Self {
            at_origin: normal.dot(plane.point - frame.origin) / along,
            slope: -DVec2::new(normal.dot(frame.u), normal.dot(frame.v)) / along,
        })
    }

    fn at(&self, p: DVec2) -> f64 {
        self.at_origin + self.slope.dot(p)
    }
}

/// `ring` running counter-clockwise if `ccw`, clockwise otherwise.
fn oriented(ring: &SectionLoop, ccw: bool) -> SectionLoop {
    if (ring.signed_area() > 0.0) == ccw {
        ring.clone()
    } else {
        ring.clone().reversed()
    }
}

fn sweep_region(
    region: &SectionRegion,
    frame: &MemberFrame,
    start: &Depth,
    end: &Depth,
) -> Result<SweptRegion, GeometryError> {
    // The outer loop counter-clockwise and the holes clockwise, as
    // triangulate expects and as puts the solid left of every edge.
    let loops: Vec<SectionLoop> = std::iter::once(oriented(&region.outer, true))
        .chain(region.holes.iter().map(|h| oriented(h, false)))
        .collect();
    // Per loop and point: its images on the start and the end plane.
    let mut columns: Vec<Vec<[DVec3; 2]>> = Vec::with_capacity(loops.len());
    for ring in &loops {
        let mut images = Vec::with_capacity(ring.points.len());
        for &p in &ring.points {
            let (s0, s1) = (start.at(p), end.at(p));
            let length = s1 - s0;
            if length.is_nan() || length < MIN_HEIGHT {
                return Err(GeometryError::InvalidExtent(
                    Message::new("geometry-end-cuts-cross").length("min", MIN_HEIGHT),
                ));
            }
            images.push([frame.point(p, s0), frame.point(p, s1)]);
        }
        columns.push(images);
    }
    let holes: Vec<Vec<DVec2>> = loops[1..].iter().map(|h| h.points.clone()).collect();
    let triangles = triangulate(&loops[0].points, &holes)?;

    // The frame is right-handed, so a counter-clockwise triangle of the
    // section faces along the axis on either plane: out of the end face,
    // and into the start face, whose triangles are therefore reversed.
    let mut mesh = TriMesh::default();
    let end_face: Vec<DVec3> = columns.iter().flatten().map(|c| c[1]).collect();
    mesh.add_face(&end_face, &triangles);
    let start_face: Vec<DVec3> = columns.iter().flatten().map(|c| c[0]).collect();
    let reversed: Vec<[usize; 3]> = triangles.iter().map(|t| [t[0], t[2], t[1]]).collect();
    mesh.add_face(&start_face, &reversed);
    for (ring, images) in loops.iter().zip(&columns) {
        add_lateral(&mut mesh, images, ring);
    }
    Ok(SweptRegion {
        mesh,
        material: region.material,
        role: region.role.clone(),
    })
}

/// Adds the lateral faces of `ring`, whose points have the start and end
/// images `columns`: the trapezoids of its edges, joined into strips that
/// share their vertices across the smooth points of the ring and break at
/// its hard ones (IFX-038). The first strip starts at the first hard point;
/// a ring without one is a single closed strip.
fn add_lateral(mesh: &mut TriMesh, columns: &[[DVec3; 2]], ring: &SectionLoop) {
    let n = columns.len();
    let smooth = |k: usize| ring.smooth.get(k) == Some(&true);
    let first_hard = (0..n).find(|&k| !smooth(k));
    let first = first_hard.unwrap_or(0);
    let mut strip = vec![first];
    for step in 1..=n {
        let k = (first + step) % n;
        if first_hard.is_none() && step == n {
            add_strip(mesh, columns, &strip, true);
        } else {
            strip.push(k);
            if !smooth(k) {
                add_strip(mesh, columns, &strip, false);
                strip = vec![k];
            }
        }
    }
}

/// Adds the strip of trapezoids between the consecutive ring points
/// `points` (and back to the first if `closed`) with shared vertices.
///
/// The ring has the solid on its left, so the trapezoid of the edge from
/// `a` to `b`, `a₀ b₀ b₁ a₁` (0 on the start plane, 1 on the end plane),
/// faces outwards. Its sides `a₀ a₁` and `b₀ b₁` are parallel to the axis
/// and point the same way, so it is planar and convex and splits along
/// either diagonal; the shorter one gives the better triangles.
fn add_strip(mesh: &mut TriMesh, columns: &[[DVec3; 2]], points: &[usize], closed: bool) {
    let corners: Vec<DVec3> = points.iter().flat_map(|&k| columns[k]).collect();
    let m = points.len();
    let quads = if closed { m } else { m - 1 };
    let mut triangles = Vec::with_capacity(2 * quads);
    for i in 0..quads {
        let j = (i + 1) % m;
        let (a0, a1, b0, b1) = (2 * i, 2 * i + 1, 2 * j, 2 * j + 1);
        if corners[a0].distance_squared(corners[b1]) <= corners[b0].distance_squared(corners[a1]) {
            triangles.extend([[a0, b0, b1], [a0, b1, a1]]);
        } else {
            triangles.extend([[a0, b0, a1], [b0, b1, a1]]);
        }
    }
    mesh.add_face(&corners, &triangles);
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::f64::consts::{FRAC_PI_2, FRAC_PI_6};

    use archicraft_model::message::Arg;
    use archicraft_model::profile::ParametricShape;
    use archicraft_model::project::Tessellation;
    use glam::DQuat;

    use super::*;
    use crate::mesh::MAX_MESH_POINTS;
    use crate::section::parametric_section;

    /// IPE 200 (EN 10365): h 200, b 100, tw 5.6, tf 8.5, r 12 mm.
    const IPE_200: ParametricShape = ParametricShape::IShape {
        height: 0.2,
        width: 0.1,
        web: 0.0056,
        flange: 0.0085,
        root_radius: 0.012,
    };
    /// CHS 168.3 x 8.
    const CHS: ParametricShape = ParametricShape::CircularHollow {
        diameter: 0.1683,
        thickness: 0.008,
    };
    /// RHS 200 x 100 x 8, outer radius 16 mm.
    const RHS: ParametricShape = ParametricShape::RectangularHollow {
        width: 0.2,
        height: 0.1,
        thickness: 0.008,
        outer_radius: 0.016,
    };
    /// L 120 x 80 x 10: unequal legs, so its centroid lies off the origin.
    const ANGLE: ParametricShape = ParametricShape::LShape {
        height: 0.12,
        width: 0.08,
        thickness: 0.01,
        root_radius: 0.013,
        toe_radius: 0.0065,
    };
    /// The D.2 default beam, 0.25 x 0.50 m.
    const BEAM: ParametricShape = ParametricShape::Rectangle {
        width: 0.25,
        height: 0.5,
    };
    const SQUARE: ParametricShape = ParametricShape::Rectangle {
        width: 0.3,
        height: 0.3,
    };

    fn section_with(shape: ParametricShape, tessellation: &Tessellation) -> Section {
        parametric_section(&shape, MaterialId::new(), tessellation).unwrap()
    }

    fn section(shape: ParametricShape) -> Section {
        section_with(shape, &Tessellation::default())
    }

    fn relative(actual: f64, expected: f64) -> f64 {
        ((actual - expected) / expected).abs()
    }

    /// Planes square to the frame's axis at `s0` and `s1` along it.
    fn square_cuts(frame: &MemberFrame, s0: f64, s1: f64) -> (CutPlane, CutPlane) {
        let at = |s: f64| CutPlane {
            point: frame.origin + frame.axis * s,
            normal: frame.axis,
        };
        (at(s0), at(s1))
    }

    /// The mesh of a section of one region, which must be closed and
    /// outward-oriented (IFX-032).
    fn body(section: &Section, frame: &MemberFrame, start: &CutPlane, end: &CutPlane) -> TriMesh {
        let mut regions = sweep(section, frame, start, end).unwrap();
        assert_eq!(regions.len(), 1);
        let region = regions.remove(0);
        assert_eq!(region.role, section.regions[0].role);
        assert_eq!(region.material, section.regions[0].material);
        assert_eq!(region.mesh.check_closed(), Ok(()));
        region.mesh
    }

    /// The tilt of `frame`'s axis by `degrees` about `about`: the normal of
    /// a cut tilted by that angle.
    fn tilted(frame: &MemberFrame, about: DVec3, degrees: f64) -> DVec3 {
        DQuat::from_axis_angle(about, degrees.to_radians()) * frame.axis
    }

    /// The message of an `InvalidExtent` error.
    fn extent_error(result: Result<Vec<SweptRegion>, GeometryError>) -> Message {
        match result {
            Err(GeometryError::InvalidExtent(message)) => message,
            other => panic!("expected InvalidExtent, got {other:?}"),
        }
    }

    /// COL-001, IFX-032: a 0.30 x 0.30 m column 3.00 m high with square
    /// cuts is a closed box of 0.27 m³ standing on its position.
    #[test]
    fn a_square_column_is_a_closed_box() {
        let position = DVec3::new(12.0, -4.0, 2.8);
        let frame = frame_for_column(position, 0.0);
        let (start, end) = square_cuts(&frame, 0.0, 3.0);
        let mesh = body(&section(SQUARE), &frame, &start, &end);
        assert!((mesh.signed_volume() - 0.27).abs() < 1e-9, "{}", mesh.signed_volume());
        let (lo, hi) = mesh.bounds().unwrap();
        assert!(lo.abs_diff_eq(position - DVec3::new(0.15, 0.15, 0.0), 1e-12), "{lo}");
        assert!(hi.abs_diff_eq(position + DVec3::new(0.15, 0.15, 3.0), 1e-12), "{hi}");
    }

    /// BEA-001: IPE 200 and CHS 168.3 x 8 swept 5 m horizontally are closed
    /// with the volume of their area times the length.
    #[test]
    fn profiles_swept_horizontally_have_their_area_times_the_length() {
        let start = DVec3::new(3.0, 1.0, 2.6);
        let frame = frame_for_beam(start, start + DVec3::new(3.0, 4.0, 0.0), 0.0).unwrap();
        for shape in [IPE_200, CHS] {
            let section = section(shape);
            let (a, b) = square_cuts(&frame, 0.0, 5.0);
            let mesh = body(&section, &frame, &a, &b);
            let volume = mesh.signed_volume();
            assert!(relative(volume, section.area() * 5.0) < 1e-9, "{shape:?}: {volume}");
        }
    }

    /// A rectangular hollow section's bore is a lateral surface of its own,
    /// facing into the hole, with the bore's perimeter times the length as
    /// its area.
    #[test]
    fn a_hollow_section_has_an_inner_lateral_surface() {
        let section = section(RHS);
        let frame = frame_for_beam(DVec3::ZERO, DVec3::new(4.0, 0.0, 0.0), 0.0).unwrap();
        let (a, b) = square_cuts(&frame, 0.0, 4.0);
        let mesh = body(&section, &frame, &a, &b);
        assert!(relative(mesh.signed_volume(), section.area() * 4.0) < 1e-9);

        // The section is centred on the axis, so the faces of the bore face
        // towards it.
        let inner: f64 = mesh
            .triangles()
            .filter_map(|[a, b, c]| {
                let normal = (b - a).cross(c - a);
                let centre = (a + b + c) / 3.0 - frame.origin;
                let radial = centre - frame.axis * frame.axis.dot(centre);
                let lateral = normal.dot(frame.axis).abs() < 1e-9 * normal.length();
                (lateral && normal.dot(radial) < 0.0).then(|| normal.length() / 2.0)
            })
            .sum();
        let bore = &section.regions[0].holes[0].points;
        let perimeter: f64 = (0..bore.len())
            .map(|i| bore[i].distance(bore[(i + 1) % bore.len()]))
            .sum();
        assert!(
            relative(inner, perimeter * 4.0) < 1e-9,
            "{inner} vs {}",
            perimeter * 4.0
        );
    }

    /// COL-003, BEA-002: a start cut tilted 30° about u and an end cut
    /// tilted 15° about -u leave a closed body without degenerate
    /// triangles, whose volume is the area times the length of the centroid
    /// line between the planes, exactly, on a column and on a beam.
    #[test]
    fn angled_cuts_keep_the_volume_of_the_centroid_line() {
        let section = section(ANGLE);
        let centroid = section.centroid().unwrap();
        assert!(centroid.length() > 0.01, "the centroid lies off the origin: {centroid}");
        let frames = [
            frame_for_column(DVec3::new(1.0, 2.0, 0.5), 0.4),
            frame_for_beam(DVec3::new(1.0, 2.0, 3.0), DVec3::new(1.0, 6.0, 3.5), 0.3).unwrap(),
        ];
        for frame in frames {
            let start = CutPlane {
                point: frame.origin,
                normal: tilted(&frame, frame.u, 30.0),
            };
            let end = CutPlane {
                point: frame.point(DVec2::ZERO, 3.0),
                normal: tilted(&frame, -frame.u, 15.0),
            };
            let mesh = body(&section, &frame, &start, &end);
            let line = frame.point(centroid, 0.0);
            let at = |plane: &CutPlane| plane.normal.dot(plane.point - line) / plane.normal.dot(frame.axis);
            let length = at(&end) - at(&start);
            assert!((length - 3.0).abs() > 0.01, "the cuts change the length: {length}");
            let volume = mesh.signed_volume();
            assert!(volume > 0.0);
            assert!(relative(volume, section.area() * length) < 1e-9, "{volume}");
        }
    }

    /// BEA-001, BEA-002: a beam pitched 30° with plumb end cuts has
    /// vertical end faces, and its volume is the area times the plan length
    /// over cos 30°.
    #[test]
    fn plumb_cuts_of_an_inclined_beam_give_vertical_end_faces() {
        let section = section(BEAM);
        let (plan, direction) = (6.0, DVec2::new(0.6, 0.8));
        let start = DVec3::new(5.0, -3.0, 2.8);
        let end = start + (direction * plan).extend(plan * FRAC_PI_6.tan());
        let frame = frame_for_beam(start, end, 0.0).unwrap();
        let plumb = |at: DVec3| CutPlane {
            point: at,
            normal: direction.extend(0.0),
        };
        let mesh = body(&section, &frame, &plumb(start), &plumb(end));
        let volume = mesh.signed_volume();
        assert!(
            relative(volume, section.area() * plan / FRAC_PI_6.cos()) < 1e-9,
            "{volume}"
        );
        let mut end_faces = 0;
        for [a, b, c] in mesh.triangles() {
            let normal = (b - a).cross(c - a).normalize();
            if normal.dot(direction.extend(0.0)).abs() > 0.9 {
                end_faces += 1;
                assert!(normal.z.abs() < 1e-12, "{normal}");
            }
        }
        assert_eq!(end_faces, 4, "two triangles on each end");
    }

    /// Each lateral trapezoid splits along its shorter diagonal, which
    /// keeps its two triangles as wide as possible.
    #[test]
    fn lateral_trapezoids_split_along_their_shorter_diagonal() {
        let frame = frame_for_column(DVec3::new(1.0, 1.0, 0.0), 0.2);
        let start = CutPlane {
            point: frame.origin,
            normal: tilted(&frame, frame.v, 50.0),
        };
        let end = CutPlane {
            point: frame.point(DVec2::ZERO, 2.0),
            normal: tilted(&frame, frame.u, -20.0),
        };
        let mesh = body(&section(BEAM), &frame, &start, &end);
        let on = |plane: &CutPlane, x: DVec3| plane.normal.dot(x - plane.point).abs() < 1e-9;
        let image = |plane: &CutPlane, x: DVec3| {
            x + frame.axis * plane.normal.dot(plane.point - x) / plane.normal.dot(frame.axis)
        };
        let mut diagonals = 0;
        for corners in mesh.triangles() {
            for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                let (p, q) = match (corners[i], corners[j]) {
                    (p, q) if on(&start, p) && on(&end, q) => (p, q),
                    (q, p) if on(&start, p) && on(&end, q) => (p, q),
                    _ => continue,
                };
                if (q - p).cross(frame.axis).length() < 1e-9 {
                    continue; // a lateral edge, parallel to the axis
                }
                diagonals += 1;
                let other = image(&start, q).distance(image(&end, p));
                assert!(p.distance(q) < other, "{} vs {other}", p.distance(q));
            }
        }
        assert_eq!(
            diagonals, 8,
            "the diagonal of each of the four trapezoids, in both its triangles"
        );
    }

    /// BEA-001: a roll of 90° about the beam's axis swaps the extents of
    /// its body along the unrolled frame's u and v.
    #[test]
    fn a_roll_of_90_degrees_swaps_the_extents_along_u_and_v() {
        let (start, end) = (DVec3::new(1.0, 1.0, 3.0), DVec3::new(5.0, 4.0, 3.5));
        let unrolled = frame_for_beam(start, end, 0.0).unwrap();
        let extents = |roll: f64| {
            let frame = frame_for_beam(start, end, roll).unwrap();
            let (a, b) = square_cuts(&frame, 0.0, 5.0);
            let mesh = body(&section(BEAM), &frame, &a, &b);
            let span = |d: DVec3| {
                let along = mesh.points.iter().map(|p| d.dot(*p - start));
                along.clone().fold(f64::MIN, f64::max) - along.fold(f64::MAX, f64::min)
            };
            (span(unrolled.u), span(unrolled.v))
        };
        let (u, v) = extents(0.0);
        assert!((u - 0.25).abs() < 1e-12 && (v - 0.5).abs() < 1e-12, "{u} {v}");
        let (u, v) = extents(FRAC_PI_2);
        assert!((u - 0.5).abs() < 1e-12 && (v - 0.25).abs() < 1e-12, "{u} {v}");
    }

    /// The column and beam frames are orthonormal and right-handed; a beam
    /// frame without roll has a horizontal u and a v that points up, and a
    /// column's rotation turns its section counter-clockwise.
    #[test]
    fn member_frames_are_right_handed() {
        let column = frame_for_column(DVec3::new(1.0, 2.0, 3.0), FRAC_PI_2);
        assert!(column.is_valid());
        assert!(column.u.abs_diff_eq(DVec3::Y, 1e-15) && column.v.abs_diff_eq(DVec3::NEG_X, 1e-15));
        let along_x = frame_for_beam(DVec3::ZERO, DVec3::new(4.0, 0.0, 0.0), 0.0).unwrap();
        assert_eq!((along_x.axis, along_x.u, along_x.v), (DVec3::X, DVec3::Y, DVec3::Z));
        for (end, roll) in [
            (DVec3::new(3.0, -4.0, 2.0), 0.0),
            (DVec3::new(-0.01, 0.0, 30.0), 0.0),
            (DVec3::new(2.0, 1.0, -1.0), 1.2),
        ] {
            let start = DVec3::new(90_000.0, -90_000.0, 5.0);
            let frame = frame_for_beam(start, start + end, roll).unwrap();
            assert!(frame.is_valid(), "{frame:?}");
            assert!(frame.axis.abs_diff_eq(end.normalize(), 1e-12));
            assert!((frame.u.cross(frame.v) - frame.axis).length() < 1e-12);
            if roll == 0.0 {
                assert_eq!(frame.u.z, 0.0);
                assert!(frame.v.z > 0.0);
            }
        }
    }

    /// IFX-038: a circle of 360 segments shares the vertices of its lateral
    /// quads with their neighbours, 2n points for n segments, while both
    /// caps have their own n points; the 4n points stay below
    /// MAX_MESH_POINTS.
    #[test]
    fn a_circular_prism_shares_its_lateral_vertices() {
        let fine = Tessellation {
            chord_deviation: 0.0,
            max_angle_deg: 1.0,
        };
        let section = section_with(ParametricShape::Circle { diameter: 0.3 }, &fine);
        let n = section.regions[0].outer.points.len();
        assert_eq!(n, 360);
        let frame = frame_for_column(DVec3::new(2.0, 3.0, 0.0), 0.3);
        let (a, b) = square_cuts(&frame, 0.0, 3.0);
        let mesh = body(&section, &frame, &a, &b);
        assert_eq!(mesh.points.len(), 4 * n);
        assert!(mesh.points.len() <= MAX_MESH_POINTS);
        let (mut caps, mut lateral) = (HashSet::new(), HashSet::new());
        for (t, [a, b, c]) in mesh.indices.chunks_exact(3).zip(mesh.triangles()) {
            let normal = (b - a).cross(c - a).normalize();
            let set = if normal.dot(frame.axis).abs() > 0.5 {
                &mut caps
            } else {
                &mut lateral
            };
            set.extend(t.iter().copied());
        }
        assert_eq!((caps.len(), lateral.len()), (2 * n, 2 * n));
        assert!(caps.is_disjoint(&lateral));
    }

    /// The normals of the triangles that use each point of `mesh`.
    fn normals_at_points(mesh: &TriMesh) -> Vec<Vec<DVec3>> {
        let mut normals = vec![Vec::new(); mesh.points.len()];
        for (t, [a, b, c]) in mesh.indices.chunks_exact(3).zip(mesh.triangles()) {
            for &i in t {
                normals[i as usize].push((b - a).cross(c - a).normalize());
            }
        }
        normals
    }

    /// IFX-038: a rectangular prism shares no vertex between its faces, and
    /// in every prism the triangles at a shared vertex meet at a smooth edge
    /// (their normals differ by at most one tessellation step), never at a
    /// hard one.
    #[test]
    fn vertices_are_shared_across_smooth_edges_only() {
        let frame = frame_for_beam(DVec3::ZERO, DVec3::new(2.0, 1.0, 0.5), 0.2).unwrap();
        let (a, b) = square_cuts(&frame, 0.0, 2.0);
        let rectangle = body(&section(BEAM), &frame, &a, &b);
        assert_eq!(rectangle.points.len(), 24);
        for normals in normals_at_points(&rectangle) {
            assert!(normals.iter().all(|n| n.dot(normals[0]) > 1.0 - 1e-12), "{normals:?}");
        }
        let step = Tessellation::default().max_angle_deg.to_radians().cos();
        for shape in [IPE_200, CHS, RHS, ANGLE, ParametricShape::Circle { diameter: 0.3 }] {
            let section = section(shape);
            let mesh = body(&section, &frame, &a, &b);
            for normals in normals_at_points(&mesh) {
                for n in &normals {
                    assert!(n.dot(normals[0]) >= step - 1e-9, "{shape:?}: {normals:?}");
                }
            }
            // The smooth points are shared, so the mesh has fewer points
            // than one face per edge would give.
            let loops: usize = section.regions[0].holes.iter().map(|h| h.points.len()).sum::<usize>()
                + section.regions[0].outer.points.len();
            assert!(mesh.points.len() < 6 * loops, "{shape:?}");
        }
    }

    /// COL-003, BEA-002: end cuts that cross within the section, lie the
    /// wrong way round or leave less than MIN_HEIGHT are errors with a
    /// message, as is a cut within 1° of parallel to the axis.
    #[test]
    fn crossing_and_parallel_cuts_are_rejected() {
        let section = section(SQUARE);
        let frame = frame_for_column(DVec3::new(4.0, 5.0, 0.0), 0.0);
        let crossing = CutPlane {
            point: frame.origin,
            normal: tilted(&frame, frame.u, 45.0),
        };
        let (_, end) = square_cuts(&frame, 0.0, 0.1);
        let message = extent_error(sweep(&section, &frame, &crossing, &end));
        assert_eq!(message.key(), "geometry-end-cuts-cross");
        assert_eq!(message.arg("min"), Some(&Arg::Length(MIN_HEIGHT)));
        let (a, b) = square_cuts(&frame, 0.0, 3.0);
        assert_eq!(
            extent_error(sweep(&section, &frame, &b, &a)).key(),
            "geometry-end-cuts-cross"
        );
        let (a, b) = square_cuts(&frame, 0.0, 0.9 * MIN_HEIGHT);
        assert_eq!(
            extent_error(sweep(&section, &frame, &a, &b)).key(),
            "geometry-end-cuts-cross"
        );
        let (a, b) = square_cuts(&frame, 0.0, 1.1 * MIN_HEIGHT);
        body(&section, &frame, &a, &b);

        // A plane meeting the axis at `degrees`, through the origin, and a
        // square end cut far enough out not to cross it.
        let (_, far) = square_cuts(&frame, 0.0, 20.0);
        let steep = |degrees: f64| CutPlane {
            point: frame.origin,
            normal: tilted(&frame, frame.v, 90.0 - degrees),
        };
        for degrees in [0.0, 0.5, 0.99] {
            let message = extent_error(sweep(&section, &frame, &steep(degrees), &far));
            assert_eq!(message.key(), "geometry-end-cut-parallel", "{degrees}°");
            assert_eq!(message.arg("min"), Some(&Arg::Angle(MIN_CUT_ANGLE_DEG.to_radians())));
            let (start, _) = square_cuts(&frame, -20.0, 0.0);
            let message = extent_error(sweep(&section, &frame, &start, &steep(degrees)));
            assert_eq!(message.key(), "geometry-end-cut-parallel", "{degrees}° at the end");
        }
        let mesh = body(&section, &frame, &steep(1.01), &far);
        assert!(mesh.signed_volume() > 0.0);
    }

    /// Undefined input is an error, never a panic: beams that are vertical
    /// or have no length, frames that are not finite, orthonormal and
    /// right-handed, and planes that are not finite or have no normal.
    #[test]
    fn undefined_placements_are_errors() {
        let key = |result: Result<MemberFrame, GeometryError>| match result {
            Err(GeometryError::InvalidExtent(message)) => message.key(),
            other => panic!("{other:?}"),
        };
        let p = DVec3::new(1.0, 2.0, 3.0);
        for (end, roll) in [
            (p, 0.0),
            (p + DVec3::Z * 3.0, 0.0),
            (p + DVec3::new(6e-6, 0.0, 3.0), 0.0),
            (p + DVec3::new(f64::NAN, 0.0, 0.0), 0.0),
            (p + DVec3::new(f64::INFINITY, 0.0, 0.0), 0.0),
            (p + DVec3::X, f64::NAN),
        ] {
            assert_eq!(
                key(frame_for_beam(p, end, roll)),
                "geometry-member-axis-undefined",
                "{end} {roll}"
            );
        }

        let section = section(SQUARE);
        let good = frame_for_column(p, 0.0);
        let (a, b) = square_cuts(&good, 0.0, 3.0);
        let frames = [
            MemberFrame {
                u: good.v,
                v: good.u,
                ..good
            },
            MemberFrame {
                axis: good.axis * 2.0,
                ..good
            },
            MemberFrame {
                u: (good.u + good.axis * 0.1).normalize(),
                ..good
            },
            MemberFrame {
                origin: DVec3::NAN,
                ..good
            },
            frame_for_column(p, f64::NAN),
            frame_for_column(p, f64::INFINITY),
        ];
        for frame in frames {
            let message = extent_error(sweep(&section, &frame, &a, &b));
            assert_eq!(message.key(), "geometry-member-axis-undefined", "{frame:?}");
        }
        let planes = [
            CutPlane {
                normal: DVec3::ZERO,
                ..b
            },
            CutPlane {
                normal: DVec3::NAN,
                ..b
            },
            CutPlane {
                normal: DVec3::splat(f64::INFINITY),
                ..b
            },
            CutPlane { point: DVec3::NAN, ..b },
            CutPlane {
                point: DVec3::new(0.0, f64::INFINITY, 0.0),
                ..b
            },
        ];
        for plane in planes {
            let message = extent_error(sweep(&section, &good, &a, &plane));
            assert_eq!(message.key(), "geometry-member-axis-undefined", "{plane:?}");
            let message = extent_error(sweep(&section, &good, &plane, &b));
            assert_eq!(message.key(), "geometry-member-axis-undefined", "{plane:?}");
        }
        // Normals of any length and orientation describe the same plane.
        let scaled = CutPlane {
            normal: -good.axis * 1e-3,
            ..b
        };
        let mesh = body(&section, &good, &a, &scaled);
        assert!((mesh.signed_volume() - 0.27).abs() < 1e-9);
    }

    /// Several regions give one mesh each, in the section's order, with
    /// their materials and roles; an empty section gives none.
    #[test]
    fn every_region_gets_its_own_body() {
        let left = section(SQUARE).transformed(DVec2::new(-0.2, 0.0), 0.0);
        let right = section(BEAM).transformed(DVec2::new(0.3, 0.0), 0.0);
        let mut regions = left.regions;
        regions.extend(right.regions);
        regions[0].role = "Region_1".to_owned();
        regions[1].role = "Region_2".to_owned();
        let combined = Section { regions };
        let frame = frame_for_column(DVec3::ZERO, 0.0);
        let (a, b) = square_cuts(&frame, 0.0, 2.0);
        let swept = sweep(&combined, &frame, &a, &b).unwrap();
        assert_eq!(swept.len(), 2);
        for (body, region) in swept.iter().zip(&combined.regions) {
            assert_eq!((&body.role, body.material), (&region.role, region.material));
            assert_eq!(body.mesh.check_closed(), Ok(()));
            assert!(relative(body.mesh.signed_volume(), region.area() * 2.0) < 1e-9);
        }
        assert!(sweep(&Section::default(), &frame, &a, &b).unwrap().is_empty());
        // A loop given the wrong way round is swept as if it were right.
        let mut reversed = section(RHS);
        let region = &mut reversed.regions[0];
        region.outer = region.outer.clone().reversed();
        region.holes = region.holes.iter().map(|h| h.clone().reversed()).collect();
        let mesh = body(&reversed, &frame, &a, &b);
        assert!(relative(mesh.signed_volume(), section(RHS).area() * 2.0) < 1e-9);
    }
}
