//! Validation of element parameters and polygons (ELM-017, EDT-018, EDT-019,
//! NFR-REL-005, NFR-ACC-003).
//!
//! Errors are [`Message`]s (NFR-L10N-001): numeric limits are length or angle
//! arguments, so the application shows them in the project units.

mod sweep;

use glam::DVec2;

use crate::element::{Element, TopLink};
use crate::kinds::{ReferenceLine, Slab, VerticalLink, Wall};
use crate::message::Message;
use crate::project::Project;

/// Geometric coincidence tolerance: 0.01 mm (NFR-ACC-003), defined in
/// [`crate::tolerance`] and re-exported here for existing users.
pub use crate::tolerance::MODEL_TOLERANCE;

/// Edges shorter than this are removed or reported: 0.1 mm (EDT-019).
pub const MIN_EDGE_LENGTH: f64 = 1e-4;
/// Corners sharper than this are rejected as slivers: 0.1° (EDT-019).
pub const MIN_CORNER_ANGLE_DEG: f64 = 0.1;
/// Model coordinates must stay within ±100 km of the project origin
/// (NFR-ACC-001, NFR-ACC-004).
pub const MAX_COORDINATE: f64 = 100_000.0;
/// Smallest wall or slab thickness: 1 mm.
pub const MIN_THICKNESS: f64 = 0.001;
/// Largest wall or slab thickness: 10 m.
pub const MAX_THICKNESS: f64 = 10.0;
/// Smallest wall height: 1 mm.
pub const MIN_HEIGHT: f64 = 0.001;
/// Largest wall height: 1000 m.
pub const MAX_HEIGHT: f64 = 1000.0;
/// Largest base, top or slab offset: ±1000 m.
pub const MAX_OFFSET: f64 = 1000.0;
/// Largest number of storeys a wall top may be linked across.
pub const MAX_LINKED_STOREYS: usize = 200;
/// Largest number of decimals the length and angle display may use.
pub const MAX_DECIMALS: u8 = 8;

/// Most points a slab may have in its outline and holes together (SLB-003,
/// SRS Appendix D.9). The polygon checks take O(n log n) steps (a sweep
/// line), so a slab at the limit validates in milliseconds; the limit keeps a
/// crafted file from loading one slab of millions of points (NFR-SEC-003).
pub const MAX_SLAB_POINTS: usize = 10_000;

/// Largest step or spacing of a drafting grid, in metres (INP-013).
pub const MAX_GRID_SPACING: f64 = 1000.0;

/// Whether both components of a grid step or spacing are finite, positive and
/// at most [`MAX_GRID_SPACING`] (INP-013).
pub fn valid_grid_spacing(spacing: DVec2) -> bool {
    spacing.is_finite() && spacing.cmpgt(DVec2::ZERO).all() && spacing.cmple(DVec2::splat(MAX_GRID_SPACING)).all()
}

/// Whether a drafting grid origin lies within [`MAX_COORDINATE`] (INP-013).
pub fn valid_grid_origin(origin: DVec2) -> bool {
    origin.is_finite() && origin.abs().cmple(DVec2::splat(MAX_COORDINATE)).all()
}

/// Signed area of a ring (positive for counter-clockwise).
pub fn signed_area(ring: &[DVec2]) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let a = ring[i];
            let b = ring[(i + 1) % n];
            a.perp_dot(b)
        })
        .sum::<f64>()
        / 2.0
}

/// Removes consecutive duplicates (closer than [`MIN_EDGE_LENGTH`]),
/// including a repeated first point at the end.
pub fn clean_ring(ring: &[DVec2]) -> Vec<DVec2> {
    let mut out: Vec<DVec2> = Vec::with_capacity(ring.len());
    for &p in ring {
        if out.last().is_none_or(|q| q.distance(p) >= MIN_EDGE_LENGTH) {
            out.push(p);
        }
    }
    while out.len() > 1 && out[0].distance(out[out.len() - 1]) < MIN_EDGE_LENGTH {
        out.pop();
    }
    out
}

fn orient(a: DVec2, b: DVec2, c: DVec2) -> f64 {
    (b - a).perp_dot(c - a)
}

fn on_segment(a: DVec2, b: DVec2, p: DVec2) -> bool {
    p.x >= a.x.min(b.x) - MODEL_TOLERANCE
        && p.x <= a.x.max(b.x) + MODEL_TOLERANCE
        && p.y >= a.y.min(b.y) - MODEL_TOLERANCE
        && p.y <= a.y.max(b.y) + MODEL_TOLERANCE
}

fn point_segment_distance(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let len2 = ab.length_squared();
    if len2 == 0.0 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// True if the closed segments `ab` and `cd` intersect or touch within the
/// model tolerance. Segments cross only where their bounding boxes overlap:
/// for collinear segments, as on a straight wall turned by 30°, the signs of
/// the orientations are rounding noise (EDT-018).
pub fn segments_touch(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> bool {
    let d1 = orient(c, d, a);
    let d2 = orient(c, d, b);
    let d3 = orient(a, b, c);
    let d4 = orient(a, b, d);
    let boxes_overlap = a.min(b).cmple(c.max(d)).all() && c.min(d).cmple(a.max(b)).all();
    if boxes_overlap
        && ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    point_segment_distance(a, c, d) < MODEL_TOLERANCE && on_segment(c, d, a)
        || point_segment_distance(b, c, d) < MODEL_TOLERANCE && on_segment(c, d, b)
        || point_segment_distance(c, a, b) < MODEL_TOLERANCE && on_segment(a, b, c)
        || point_segment_distance(d, a, b) < MODEL_TOLERANCE && on_segment(a, b, d)
}

/// Point-in-polygon test (even-odd rule).
pub fn point_in_ring(p: DVec2, ring: &[DVec2]) -> bool {
    let mut inside = false;
    let n = ring.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (ring[i], ring[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn check_coordinates(points: &[DVec2]) -> Result<(), Message> {
    if points
        .iter()
        .any(|p| !p.is_finite() || p.x.abs() > MAX_COORDINATE || p.y.abs() > MAX_COORDINATE)
    {
        return Err(Message::new("model-error-coordinates-range").length("max", MAX_COORDINATE));
    }
    Ok(())
}

/// Cleans and validates a closed ring (EDT-018). Returns the cleaned ring,
/// counter-clockwise.
pub fn validate_ring(ring: &[DVec2]) -> Result<Vec<DVec2>, Message> {
    let ring = prepare_ring(ring)?;
    // Edges that do not follow each other must not touch (NFR-SEC-003).
    if sweep::nesting(&sweep::Rings::new(&[&ring])).is_err() {
        return Err(Message::new("model-error-polygon-self-intersection"));
    }
    Ok(ring)
}

/// [`validate_ring`] up to the test for touching edges.
fn prepare_ring(ring: &[DVec2]) -> Result<Vec<DVec2>, Message> {
    check_coordinates(ring)?;
    let mut ring = clean_ring(ring);
    if ring.len() < 3 {
        return Err(Message::new("model-error-polygon-points"));
    }
    let area = signed_area(&ring);
    if area.abs() < MIN_EDGE_LENGTH * MIN_EDGE_LENGTH {
        return Err(Message::new("model-error-polygon-area"));
    }
    if area < 0.0 {
        ring.reverse();
    }
    let n = ring.len();
    let min_angle = MIN_CORNER_ANGLE_DEG.to_radians();
    for i in 0..n {
        let prev = ring[(i + n - 1) % n];
        let p = ring[i];
        let next = ring[(i + 1) % n];
        let angle = (prev - p).angle_to(next - p).abs();
        if angle < min_angle {
            return Err(Message::new("model-error-polygon-sliver").angle("min", min_angle));
        }
    }
    Ok(ring)
}

/// Validates a slab outline with holes. Returns the outline counter-clockwise
/// and the holes clockwise (SLB-001, SLB-003, EDT-018). It has at most
/// [`MAX_SLAB_POINTS`] points (NFR-SEC-003).
///
/// The outline is checked first, then each hole on its own, then how they
/// lie: no hole may touch the outline or lie outside it, and no two holes may
/// touch or lie in each other. When several holes are wrong, the message
/// names touching rings before a hole outside and that before nested holes.
pub fn validate_slab_polygon(
    outline: &[DVec2],
    holes: &[Vec<DVec2>],
) -> Result<(Vec<DVec2>, Vec<Vec<DVec2>>), Message> {
    if outline.len() + holes.iter().map(Vec::len).sum::<usize>() > MAX_SLAB_POINTS {
        return Err(Message::new("model-error-slab-points").count("max", MAX_SLAB_POINTS));
    }
    let outline = validate_ring(outline)?;
    let holes = holes
        .iter()
        .map(|hole| validate_ring(hole).map_err(|e| Message::new("model-error-hole").nested("reason", e)))
        .collect::<Result<Vec<_>, _>>()?;
    if holes.is_empty() {
        return Ok((outline, holes));
    }
    let rings: Vec<&[DVec2]> = std::iter::once(&outline).chain(&holes).map(Vec::as_slice).collect();
    let rings = sweep::Rings::new(&rings);
    // Ring 0 is the outline, ring i the hole i - 1.
    let parents = sweep::nesting(&rings).map_err(|(e, f)| {
        Message::new(if rings.ring(e) == 0 || rings.ring(f) == 0 {
            "model-error-hole-outside"
        } else {
            "model-error-holes-overlap"
        })
    })?;
    // A hole around nothing lies outside the outline (or around it). If no
    // hole does, all lie in the outline, and one that lies in another hole
    // overlaps it.
    if parents[1..].contains(&None) {
        return Err(Message::new("model-error-hole-outside"));
    }
    if parents[1..].iter().any(|&parent| parent != Some(0)) {
        return Err(Message::new("model-error-holes-overlap"));
    }
    let holes = holes
        .into_iter()
        .map(|mut hole| {
            hole.reverse();
            hole
        })
        .collect();
    Ok((outline, holes))
}

/// Checks a wall or slab thickness against [`MIN_THICKNESS`] and
/// [`MAX_THICKNESS`].
pub fn check_thickness(thickness: f64) -> Result<(), Message> {
    if !thickness.is_finite() || !(MIN_THICKNESS..=MAX_THICKNESS).contains(&thickness) {
        return Err(Message::new("model-error-thickness-range")
            .length("min", MIN_THICKNESS)
            .length("max", MAX_THICKNESS));
    }
    Ok(())
}

/// Checks an offset against [`MAX_OFFSET`]; `key` names the offset in the
/// message.
pub fn check_offset(value: f64, key: &'static str) -> Result<(), Message> {
    if !value.is_finite() || value.abs() > MAX_OFFSET {
        return Err(Message::new(key).length("max", MAX_OFFSET));
    }
    Ok(())
}

/// Range checks for a wall (ELM-017, NFR-REL-005).
pub fn validate_wall(wall: &Wall) -> Result<(), Message> {
    check_coordinates(&[wall.start, wall.end])?;
    if wall.length() < MIN_EDGE_LENGTH {
        return Err(Message::new("model-error-wall-length").length("min", MIN_EDGE_LENGTH));
    }
    check_thickness(wall.thickness)?;
    if let ReferenceLine::Custom(offset) = wall.reference_line
        && (!offset.is_finite() || offset.abs() > MAX_THICKNESS)
    {
        return Err(Message::new("model-error-reference-line-range").length("max", MAX_THICKNESS));
    }
    check_vertical_link(&wall.link())
}

/// Range checks for the base offset and top of a vertical element (ELM-006).
/// A storey the top is linked to is checked by the kind, and the resulting
/// extent by [`validate_element`]. The fixed-height message names walls, the
/// only vertical kind with a tool so far.
pub fn check_vertical_link(link: &VerticalLink) -> Result<(), Message> {
    check_offset(link.base_offset, "model-error-base-offset-range")?;
    match link.top {
        TopLink::Fixed { height } => {
            if !height.is_finite() || !(MIN_HEIGHT..=MAX_HEIGHT).contains(&height) {
                return Err(Message::new("model-error-wall-height-range")
                    .length("min", MIN_HEIGHT)
                    .length("max", MAX_HEIGHT));
            }
        }
        TopLink::Storey { offset, .. } => check_offset(offset, "model-error-top-offset-range")?,
        TopLink::Relative { storeys, offset } => {
            if storeys.unsigned_abs() as usize > MAX_LINKED_STOREYS {
                return Err(Message::new("model-error-linked-storey-range").count("max", MAX_LINKED_STOREYS));
            }
            check_offset(offset, "model-error-top-offset-range")?;
        }
    }
    Ok(())
}

/// Range checks for a slab, returning the slab with a normalised polygon.
pub fn validate_slab(slab: &Slab) -> Result<Slab, Message> {
    check_thickness(slab.thickness)?;
    check_offset(slab.offset, "model-error-slab-offset-range")?;
    let (outline, holes) = validate_slab_polygon(&slab.outline, &slab.holes)?;
    Ok(Slab {
        outline,
        holes,
        ..slab.clone()
    })
}

/// Validates an element against the project it is placed in: references to
/// storey, layer and material, its parameters, and its vertical extent. On
/// success, returns the element with normalised geometry.
pub fn validate_element(project: &Project, element: &Element) -> Result<Element, Message> {
    let element = validate_prototype(project, element)?;
    match project.vertical_extent(&element) {
        Some((bottom, top)) if top - bottom < MIN_HEIGHT => Err(Message::new("model-error-top-below-base")),
        _ => Ok(element),
    }
}

/// [`validate_element`] for a tool's prototype (ELM-001): everything but the
/// height, which depends on the storey the tool draws on. The extent must be
/// determinable, so linked storeys must exist (ELM-006).
pub fn validate_prototype(project: &Project, element: &Element) -> Result<Element, Message> {
    if element.name.trim().is_empty() {
        return Err(Message::new("model-error-element-name"));
    }
    if project.storeys.get(element.storey).is_none() {
        return Err(Message::new("model-error-home-storey-missing"));
    }
    if project.layer(element.layer).is_none() {
        return Err(Message::new("model-error-element-layer-missing"));
    }
    if project.material(element.kind.material()).is_none() {
        return Err(Message::new("model-error-element-material-missing"));
    }
    // ELM-011: the class must be one of the kind's selectable classes.
    let descriptor = element.kind.descriptor();
    if !descriptor.ifc_classes.contains(&element.ifc_class) {
        return Err(Message::new("model-error-ifc-class-not-allowed")
            .text("class", element.ifc_class.code())
            .kind("kind", element.tag()));
    }
    let element = Element {
        kind: element.kind.validate(&project.storeys)?,
        ..element.clone()
    };
    if project.vertical_extent(&element).is_none() {
        return Err(Message::new("model-error-vertical-extent"));
    }
    Ok(element)
}

#[cfg(test)]
mod tests {
    use std::f64::consts::TAU;
    use std::time::{Duration, Instant};

    use super::*;

    fn square(size: f64) -> Vec<DVec2> {
        vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(size, 0.0),
            DVec2::new(size, size),
            DVec2::new(0.0, size),
        ]
    }

    #[test]
    fn ring_cleanup_and_orientation() {
        let mut ring = square(1.0);
        ring.insert(1, DVec2::new(0.0, 0.0));
        ring.push(DVec2::new(0.0, 0.0));
        ring.reverse();
        let cleaned = validate_ring(&ring).unwrap();
        assert_eq!(cleaned.len(), 4);
        assert!(signed_area(&cleaned) > 0.0);
    }

    #[test]
    fn rejects_degenerate_rings() {
        assert!(validate_ring(&[DVec2::ZERO, DVec2::X]).is_err());
        assert!(validate_ring(&[DVec2::ZERO, DVec2::X, DVec2::new(2.0, 0.0)]).is_err());
        let bow_tie = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(1.0, 1.0),
            DVec2::new(1.0, 0.0),
            DVec2::new(0.0, 1.0),
        ];
        assert!(validate_ring(&bow_tie).is_err());
    }

    #[test]
    fn holes_must_be_inside() {
        let hole: Vec<DVec2> = square(1.0).into_iter().map(|p| p + DVec2::splat(1.0)).collect();
        let (outline, holes) = validate_slab_polygon(&square(4.0), &[hole]).unwrap();
        assert!(signed_area(&outline) > 0.0);
        assert!(signed_area(&holes[0]) < 0.0);
        let outside: Vec<DVec2> = square(1.0).into_iter().map(|p| p + DVec2::splat(5.0)).collect();
        assert!(validate_slab_polygon(&square(4.0), &[outside]).is_err());
        let touching: Vec<DVec2> = square(1.0);
        assert!(validate_slab_polygon(&square(4.0), &[touching]).is_err());
    }

    /// A comb with `teeth` teeth: `4 * teeth + 3` points, right angles only
    /// and no self-intersection.
    fn comb(teeth: usize) -> Vec<DVec2> {
        long_comb(teeth, 1.0)
    }

    /// [`comb`] with teeth `length` long, 1 m wide and 1 m apart.
    fn long_comb(teeth: usize, length: f64) -> Vec<DVec2> {
        let mut ring = Vec::new();
        for i in 0..teeth {
            let x = 2.0 * i as f64;
            ring.extend([
                DVec2::new(x, 0.0),
                DVec2::new(x, length),
                DVec2::new(x + 1.0, length),
                DVec2::new(x + 1.0, 0.0),
            ]);
        }
        let end = 2.0 * teeth as f64;
        ring.extend([DVec2::new(end, 0.0), DVec2::new(end, -1.0), DVec2::new(0.0, -1.0)]);
        ring
    }

    /// `ring` turned by `degrees` about the origin.
    fn turned(ring: &[DVec2], degrees: f64) -> Vec<DVec2> {
        let rotation = DVec2::from_angle(degrees.to_radians());
        ring.iter().map(|&p| rotation.rotate(p)).collect()
    }

    /// `ring` moved by `offset`.
    fn moved(ring: &[DVec2], offset: DVec2) -> Vec<DVec2> {
        ring.iter().map(|&p| p + offset).collect()
    }

    /// NFR-SEC-003, SLB-003: a slab may have up to 10,000 points in its
    /// outline and holes together (SRS Appendix D.9); a slab with more is
    /// rejected with a message before the polygon checks run.
    #[test]
    fn slab_polygons_are_limited_to_max_slab_points() {
        assert_eq!(MAX_SLAB_POINTS, 10_000);
        let largest = comb((MAX_SLAB_POINTS - 3) / 4);
        assert!(largest.len() <= MAX_SLAB_POINTS);
        assert!(validate_slab_polygon(&largest, &[]).is_ok());
        let too_large = comb((MAX_SLAB_POINTS - 3) / 4 + 1);
        assert!(too_large.len() > MAX_SLAB_POINTS);
        let error = validate_slab_polygon(&too_large, &[]).unwrap_err();
        assert_eq!(error, Message::new("model-error-slab-points").count("max", 10_000));
        // The holes count too.
        let hole = moved(&square(0.5), DVec2::new(0.25, -0.75));
        let holes = vec![hole; MAX_SLAB_POINTS / 4];
        let error = validate_slab_polygon(&comb(10), &holes).unwrap_err();
        assert_eq!(error.key(), "model-error-slab-points");
    }

    type Holes = Vec<Vec<DVec2>>;

    /// Valid slabs at the point limit that are hard for the polygon checks:
    /// the comb outline, a rectangle with 2,499 holes, a comb with holes in
    /// its base, and combs with teeth 1 km long turned by 30° and by 90°, so
    /// that thousands of long edges are side by side.
    fn slabs_at_the_limit() -> Vec<(&'static str, Vec<DVec2>, Holes)> {
        let teeth = (MAX_SLAB_POINTS - 3) / 4;
        let grid_holes: Vec<Vec<DVec2>> = (0..teeth)
            .map(|i| moved(&square(0.5), DVec2::new((i % 50) as f64, (i / 50) as f64) + 0.25))
            .collect();
        let base_holes: Vec<Vec<DVec2>> = (0..teeth / 2)
            .map(|i| moved(&square(0.5), DVec2::new(2.0 * i as f64 + 0.25, -0.75)))
            .collect();
        vec![
            ("comb", comb(teeth), Vec::new()),
            ("rectangle with holes", square(50.0), grid_holes),
            ("comb with holes", comb(teeth / 2 + 1), base_holes),
            (
                "long comb turned by 30°",
                turned(&long_comb(teeth, 1000.0), 30.0),
                Vec::new(),
            ),
            (
                "long comb turned by 90°",
                turned(&long_comb(teeth, 1000.0), 90.0),
                Vec::new(),
            ),
        ]
    }

    /// NFR-SEC-003, SLB-003: the polygon checks grow little faster than the
    /// points, not with their square, so each slab of [`slabs_at_the_limit`]
    /// validates in under 100 ms in a release build (about 10 ms; testing
    /// every pair of edges took about 0.6 s for one). Test builds, optimised
    /// less, get 250 ms.
    #[test]
    fn slabs_at_the_point_limit_validate_quickly() {
        let budget = Duration::from_millis(if cfg!(debug_assertions) { 250 } else { 100 });
        for (name, outline, holes) in slabs_at_the_limit() {
            let points = outline.len() + holes.iter().map(Vec::len).sum::<usize>();
            assert!(
                (MAX_SLAB_POINTS - 10..=MAX_SLAB_POINTS).contains(&points),
                "{name}: {points} points"
            );
            let start = Instant::now();
            let result = validate_slab_polygon(&outline, &holes);
            let elapsed = start.elapsed();
            assert!(result.is_ok(), "{name}: {result:?}");
            assert!(elapsed < budget, "{name} took {elapsed:?}");
        }
    }

    /// NFR-SEC-003: the sweep tests a few pairs of edges per point, whatever
    /// the shape: at most five for each slab of [`slabs_at_the_limit`],
    /// where testing every pair takes about 50 million.
    #[test]
    fn the_sweep_tests_a_few_pairs_of_edges_per_point() {
        for (name, outline, holes) in slabs_at_the_limit() {
            let rings: Vec<Vec<DVec2>> = std::iter::once(&outline)
                .chain(&holes)
                .map(|ring| validate_ring(ring).unwrap())
                .collect();
            let points: usize = rings.iter().map(Vec::len).sum();
            let rings: Vec<&[DVec2]> = rings.iter().map(Vec::as_slice).collect();
            let rings = sweep::Rings::new(&rings);
            assert!(sweep::by_sweep(&rings).is_ok(), "{name}");
            let tested = rings.tested.get();
            assert!(
                tested <= 5 * points,
                "{name}: {tested} pairs tested for {points} points"
            );
        }
    }

    /// A ring with a spike from the left whose tip, at the origin, points
    /// into a slot between x = -1 and the right side `right` (points listed
    /// downwards). Counter-clockwise, simple apart from what `right` brings
    /// close to the tip.
    fn spike_into_slot(right: &[DVec2]) -> Vec<DVec2> {
        let mut ring = vec![DVec2::new(-3.0, -3.0), DVec2::new(3.0, -3.0), DVec2::new(3.0, 3.0)];
        ring.extend_from_slice(right);
        ring.extend([
            DVec2::new(-1.0, -2.0),
            DVec2::new(-1.0, -0.5),
            DVec2::ZERO,
            DVec2::new(-1.0, 0.5),
            DVec2::new(-1.0, 3.0),
            DVec2::new(-3.0, 3.0),
        ]);
        ring
    }

    /// Runs the sweep and the test of every pair on `rings` (as
    /// [`prepare_ring`] leaves them), asserts that they agree on whether
    /// edges touch and, if none do, on how the rings nest, and returns
    /// whether edges touch.
    fn sweep_agrees(rings: &[Vec<DVec2>]) -> bool {
        let slices: Vec<&[DVec2]> = rings.iter().map(Vec::as_slice).collect();
        let rings = sweep::Rings::new(&slices);
        let (swept, every) = (sweep::by_sweep(&rings), sweep::every_pair(&rings));
        match (&swept, &every) {
            (Ok(a), Ok(b)) => assert_eq!(a, b, "nesting of {slices:?}"),
            _ => assert_eq!(swept.is_err(), every.is_err(), "touching edges in {slices:?}"),
        }
        swept.is_err()
    }

    /// EDT-018, NFR-ACC-003: an edge within the model tolerance of a point
    /// it does not end at is found whatever its direction: a steep edge that
    /// does not reach the vertical through the point, an edge ending next to
    /// the point that reaches neither the vertical nor the horizontal through
    /// it, and two long edges crossing far from every point. A little further
    /// away than the tolerance, the same rings are valid. The sweep finds
    /// them as testing every pair does, here and far from the origin.
    #[test]
    fn touching_edges_are_found_in_every_direction() {
        for away in [0.9, 1.1] {
            let d = away * MODEL_TOLERANCE;
            let touch = away < 1.0;
            // The slot's right side is a steep edge from (d, -2) to (d + tol/100, 3).
            let steep = spike_into_slot(&[DVec2::new(d + MODEL_TOLERANCE / 100.0, 3.0), DVec2::new(d, -2.0)]);
            // A second spike from the right ends d from the first one's tip, diagonally.
            let tip = DVec2::splat(d / 2.0_f64.sqrt());
            let tips = spike_into_slot(&[
                DVec2::new(1.0, 3.0),
                DVec2::new(1.0, 1.0),
                tip,
                DVec2::new(1.0, 0.5),
                DVec2::new(1.0, -2.0),
            ]);
            for ring in [steep, tips] {
                assert_eq!(validate_ring(&ring).is_err(), touch, "{away}: {ring:?}");
                assert_eq!(sweep_agrees(&[prepare_ring(&ring).unwrap()]), touch, "{away}: {ring:?}");
                let far = moved(&turned(&ring, 37.0), DVec2::new(-54_321.0, 76_543.0));
                sweep_agrees(&[prepare_ring(&far).unwrap()]);
            }
        }
        let crossing = [
            DVec2::new(0.0, 0.0),
            DVec2::new(10.0, 10.0),
            DVec2::new(10.0, 0.0),
            DVec2::new(0.0, 4.0),
        ];
        assert_eq!(
            validate_ring(&crossing),
            Err(Message::new("model-error-polygon-self-intersection"))
        );
        assert!(sweep_agrees(&[prepare_ring(&crossing).unwrap()]));
    }

    /// SLB-003: holes must not touch or lie in each other, whichever comes
    /// first, and a hole around the outline lies outside it.
    #[test]
    fn holes_must_not_touch_or_contain_each_other() {
        let outline = square(10.0);
        let left = moved(&square(2.0), DVec2::splat(1.0));
        for (away, valid) in [(0.9, false), (1.1, true)] {
            let right = moved(&square(2.0), DVec2::new(3.0 + away * MODEL_TOLERANCE, 1.0));
            let result = validate_slab_polygon(&outline, &[left.clone(), right]);
            assert_eq!(result.is_ok(), valid, "{away}: {result:?}");
            if !valid {
                assert_eq!(result.unwrap_err().key(), "model-error-holes-overlap");
            }
        }
        let big = moved(&square(6.0), DVec2::splat(1.0));
        let small = moved(&square(1.0), DVec2::splat(3.0));
        // The sweep finds that the small hole lies in the big one.
        let rings = [&outline, &big, &small].map(|ring| prepare_ring(ring).unwrap());
        let slices: Vec<&[DVec2]> = rings.iter().map(Vec::as_slice).collect();
        let nesting = sweep::by_sweep(&sweep::Rings::new(&slices));
        assert_eq!(nesting, Ok(vec![None, Some(0), Some(1)]));
        for holes in [[big.clone(), small.clone()], [small, big]] {
            let error = validate_slab_polygon(&outline, &holes).unwrap_err();
            assert_eq!(error.key(), "model-error-holes-overlap");
        }
        let error = validate_slab_polygon(&moved(&square(1.0), DVec2::splat(5.0)), &[outline]).unwrap_err();
        assert_eq!(error.key(), "model-error-hole-outside");
    }

    /// A small deterministic random generator (splitmix64) for the random
    /// rings below.
    struct Random(u64);

    impl Random {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        }

        /// A number in [0, 1).
        fn unit(&mut self) -> f64 {
            (self.next() >> 11) as f64 / (1_u64 << 53) as f64
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }

        fn pick<T: Copy>(&mut self, items: &[T]) -> T {
            items[self.below(items.len())]
        }

        /// A direction at a random angle.
        fn direction(&mut self) -> DVec2 {
            DVec2::from_angle(self.unit() * TAU)
        }
    }

    /// Distances around the model tolerance, as multiples of it.
    const SPACINGS: [f64; 12] = [0.0, 0.3, 0.7, 0.9, 0.99, 0.999, 1.001, 1.01, 1.1, 1.3, 1.45, 1.6];

    /// A star of 4 to 123 points about the origin, `scale` in radius at most,
    /// valid but for the rare crossing; sometimes two of its points are
    /// swapped, which makes edges cross.
    fn star(random: &mut Random, scale: f64) -> Vec<DVec2> {
        let most = random.pick(&[8, 28, 120]);
        let n = 4 + random.below(most);
        let mut angles: Vec<f64> = (0..n).map(|_| random.unit() * TAU).collect();
        angles.sort_by(f64::total_cmp);
        let mut ring: Vec<DVec2> = angles
            .iter()
            .map(|&a| DVec2::from_angle(a) * scale * (0.2 + 0.8 * random.unit()))
            .collect();
        if random.below(4) == 0 {
            let (i, j) = (random.below(n), random.below(n));
            ring.swap(i, j);
        }
        ring
    }

    /// A [`star`] at a random scale, from 1 mm to 1 km.
    fn random_star(random: &mut Random) -> Vec<DVec2> {
        let scale = random.pick(&[0.001, 0.1, 10.0, 1000.0]);
        star(random, scale)
    }

    /// A ring with a spike whose tip lies a distance around the model
    /// tolerance from the slot's right side: a straight side leaning by a
    /// random amount (steep to shallow), or a second spike whose tip lies in
    /// a random direction from the first one's.
    fn random_spike(random: &mut Random) -> Vec<DVec2> {
        let away = random.pick(&SPACINGS) * MODEL_TOLERANCE;
        let mut ring = if random.below(2) == 0 {
            let lean = random.pick(&[0.0, MODEL_TOLERANCE / 100.0, 0.01, 0.2, 0.45]);
            let (bottom, top) = (DVec2::new(0.5 - lean, -2.0), DVec2::new(0.5 + lean, 3.0));
            let height = -1.5 + 4.0 * random.unit();
            let side = bottom.lerp(top, (height + 2.0) / 5.0);
            let into_slot = (top - bottom).perp().normalize();
            let tip = side + into_slot * away;
            let mut ring = spike_into_slot(&[top, bottom]);
            // The spike's base and tip follow the height of the tip.
            ring[6] = DVec2::new(-1.0, height - 0.5);
            ring[7] = tip;
            ring[8] = DVec2::new(-1.0, height + 0.5);
            ring
        } else {
            let mut direction = random.direction();
            direction.x = direction.x.abs();
            let tip = direction * away;
            spike_into_slot(&[
                DVec2::new(1.0, 3.0),
                DVec2::new(1.0, tip.y + 0.5 + random.unit()),
                tip,
                DVec2::new(1.0, tip.y - 0.5 - random.unit()),
                DVec2::new(1.0, -2.0),
            ])
        };
        if random.below(3) == 0 {
            ring.reverse();
        }
        ring
    }

    /// A ring of 4 to 11 points on a coarse integer grid: often crossing
    /// itself, with points on other edges, edges lying on each other and
    /// points repeated.
    fn random_grid_ring(random: &mut Random) -> Vec<DVec2> {
        let n = 4 + random.below(8);
        (0..n)
            .map(|_| DVec2::new(random.below(5) as f64, random.below(5) as f64))
            .collect()
    }

    /// `ring` turned by a random angle (often a right one) and moved, often
    /// far from the origin, where rounding is coarser.
    fn randomly_placed(random: &mut Random, ring: &[DVec2]) -> Vec<DVec2> {
        let any = random.unit() * 360.0;
        let degrees = random.pick(&[0.0, 90.0, 180.0, 30.0, 45.0, any]);
        let offset = random.pick(&[
            DVec2::ZERO,
            DVec2::new(54_321.125, -76_543.375),
            DVec2::new(-98_765.5, 87_654.25),
        ]);
        moved(&turned(ring, degrees), offset)
    }

    /// The polygon checks as they were before the sweep line
    /// (archicraft-91jj), testing every pair of edges: the reference the
    /// sweep must agree with.
    mod pairwise {
        use super::super::*;

        pub fn validate_ring(ring: &[DVec2]) -> Result<Vec<DVec2>, Message> {
            check_coordinates(ring)?;
            let mut ring = clean_ring(ring);
            if ring.len() < 3 {
                return Err(Message::new("model-error-polygon-points"));
            }
            let area = signed_area(&ring);
            if area.abs() < MIN_EDGE_LENGTH * MIN_EDGE_LENGTH {
                return Err(Message::new("model-error-polygon-area"));
            }
            if area < 0.0 {
                ring.reverse();
            }
            let n = ring.len();
            let min_angle = MIN_CORNER_ANGLE_DEG.to_radians();
            for i in 0..n {
                let prev = ring[(i + n - 1) % n];
                let p = ring[i];
                let next = ring[(i + 1) % n];
                let angle = (prev - p).angle_to(next - p).abs();
                if angle < min_angle {
                    return Err(Message::new("model-error-polygon-sliver").angle("min", min_angle));
                }
            }
            for i in 0..n {
                let (a, b) = (ring[i], ring[(i + 1) % n]);
                for j in (i + 1)..n {
                    if j == i + 1 || (i == 0 && j == n - 1) {
                        continue;
                    }
                    let (c, d) = (ring[j], ring[(j + 1) % n]);
                    if segments_touch(a, b, c, d) {
                        return Err(Message::new("model-error-polygon-self-intersection"));
                    }
                }
            }
            Ok(ring)
        }

        fn rings_touch(a: &[DVec2], b: &[DVec2]) -> bool {
            let (na, nb) = (a.len(), b.len());
            (0..na).any(|i| (0..nb).any(|j| segments_touch(a[i], a[(i + 1) % na], b[j], b[(j + 1) % nb])))
        }

        pub fn validate_slab_polygon(
            outline: &[DVec2],
            holes: &[Vec<DVec2>],
        ) -> Result<(Vec<DVec2>, Vec<Vec<DVec2>>), Message> {
            let outline = validate_ring(outline)?;
            let mut cleaned_holes: Vec<Vec<DVec2>> = Vec::with_capacity(holes.len());
            for hole in holes {
                let mut hole = validate_ring(hole).map_err(|e| Message::new("model-error-hole").nested("reason", e))?;
                if !point_in_ring(hole[0], &outline) || rings_touch(&hole, &outline) {
                    return Err(Message::new("model-error-hole-outside"));
                }
                if cleaned_holes.iter().any(|other| {
                    rings_touch(&hole, other) || point_in_ring(hole[0], other) || point_in_ring(other[0], &hole)
                }) {
                    return Err(Message::new("model-error-holes-overlap"));
                }
                hole.reverse();
                cleaned_holes.push(hole);
            }
            Ok((outline, cleaned_holes))
        }
    }

    /// EDT-018, NFR-SEC-003: the sweep finds touching edges exactly where
    /// testing every pair does, and rings validate as they did with every
    /// pair tested: random stars, spikes whose tips lie around the model
    /// tolerance from another edge or end point, and rings on a coarse grid
    /// that touch themselves in every way, at random angles and often far
    /// from the origin.
    #[test]
    fn rings_validate_as_with_every_pair_of_edges_tested() {
        let mut random = Random(91);
        let (mut apart, mut touching) = (0, 0);
        for case in 0..6000 {
            let ring = match case % 3 {
                0 => random_star(&mut random),
                1 => random_spike(&mut random),
                _ => random_grid_ring(&mut random),
            };
            let ring = randomly_placed(&mut random, &ring);
            assert_eq!(
                validate_ring(&ring),
                pairwise::validate_ring(&ring),
                "case {case}: {ring:?}"
            );
            if let Ok(prepared) = prepare_ring(&ring) {
                if sweep_agrees(&[prepared]) {
                    touching += 1;
                } else {
                    apart += 1;
                }
            }
        }
        // Both outcomes are common, so the comparison means something.
        assert!(apart > 1500 && touching > 1500, "{apart} apart, {touching} touching");
    }

    /// SLB-003, NFR-SEC-003: the sweep finds touching edges and nested rings
    /// as testing every pair does, and slabs validate as they did with every
    /// pair tested: an outline with up to three holes, each placed at random, next
    /// to the outline or another hole at a distance around the model
    /// tolerance, or around another hole. With one hole the message is the
    /// same; with several, a slab with two problems may name the other one.
    #[test]
    fn slabs_validate_as_with_every_pair_of_edges_tested() {
        let mut random = Random(9_100);
        let (mut valid, mut invalid, mut apart, mut touching) = (0, 0, 0, 0);
        for case in 0..3000 {
            let outline = if random.below(3) > 0 {
                square(10.0)
            } else {
                moved(&star(&mut random, 6.0), DVec2::splat(5.0))
            };
            let mut holes: Vec<Vec<DVec2>> = Vec::new();
            for _ in 0..1 + random.below(3) {
                let size = random.pick(&[0.5, 1.0, 2.0, 12.0]);
                let corner = match random.below(4) {
                    // Anywhere.
                    0 | 1 => DVec2::new(random.unit(), random.unit()) * 6.0 + 1.0,
                    // Next to an outline point.
                    2 => {
                        outline[random.below(outline.len())]
                            + random.direction() * random.pick(&SPACINGS) * MODEL_TOLERANCE
                    }
                    // Next to the last hole's right side.
                    _ => {
                        holes.last().map_or(DVec2::splat(1.0), |h: &Vec<DVec2>| h[1])
                            + DVec2::new(random.pick(&SPACINGS) * MODEL_TOLERANCE, 0.0)
                    }
                };
                holes.push(moved(&square(size), corner));
            }
            let degrees = random.pick(&[0.0, 30.0, 90.0]);
            let outline = turned(&outline, degrees);
            let holes: Vec<Vec<DVec2>> = holes.iter().map(|h| turned(h, degrees)).collect();
            let expected = pairwise::validate_slab_polygon(&outline, &holes);
            let result = validate_slab_polygon(&outline, &holes);
            if holes.len() == 1 {
                assert_eq!(result, expected, "case {case}: {outline:?} {holes:?}");
            } else {
                assert_eq!(result.is_ok(), expected.is_ok(), "case {case}: {outline:?} {holes:?}");
            }
            if expected.is_ok() {
                valid += 1;
            } else {
                invalid += 1;
            }
            let rings: Result<Vec<_>, _> = std::iter::once(&outline)
                .chain(&holes)
                .map(|r| prepare_ring(r))
                .collect();
            if let Ok(rings) = rings {
                if sweep_agrees(&rings) {
                    touching += 1;
                } else {
                    apart += 1;
                }
            }
        }
        assert!(valid > 500 && invalid > 500, "{valid} valid, {invalid} invalid");
        assert!(apart > 500 && touching > 500, "{apart} apart, {touching} touching");
    }
}
