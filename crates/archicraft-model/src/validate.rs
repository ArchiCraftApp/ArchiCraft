//! Validation of element parameters and polygons (ELM-017, EDT-018, EDT-019,
//! NFR-REL-005, NFR-ACC-003).

use glam::DVec2;

use crate::element::{Element, ElementKind, TopLink};
use crate::kinds::{ReferenceLine, Slab, Wall};
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
/// model tolerance.
pub fn segments_touch(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> bool {
    let d1 = orient(c, d, a);
    let d2 = orient(c, d, b);
    let d3 = orient(a, b, c);
    let d4 = orient(a, b, d);
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0)) {
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

fn check_coordinates(points: &[DVec2]) -> Result<(), String> {
    if points
        .iter()
        .any(|p| !p.is_finite() || p.x.abs() > MAX_COORDINATE || p.y.abs() > MAX_COORDINATE)
    {
        return Err("Coordinates must be finite and within 100 km of the project origin.".into());
    }
    Ok(())
}

/// Cleans and validates a closed ring (EDT-018). Returns the cleaned ring,
/// counter-clockwise.
pub fn validate_ring(ring: &[DVec2]) -> Result<Vec<DVec2>, String> {
    check_coordinates(ring)?;
    let mut ring = clean_ring(ring);
    if ring.len() < 3 {
        return Err("A polygon needs at least three distinct points.".into());
    }
    let area = signed_area(&ring);
    if area.abs() < MIN_EDGE_LENGTH * MIN_EDGE_LENGTH {
        return Err("The polygon has no area.".into());
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
            return Err("The polygon has a sliver corner sharper than 0.1°.".into());
        }
    }
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        for j in (i + 1)..n {
            // Skip adjacent edges, which share an endpoint.
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            let (c, d) = (ring[j], ring[(j + 1) % n]);
            if segments_touch(a, b, c, d) {
                return Err("The polygon outline intersects itself.".into());
            }
        }
    }
    Ok(ring)
}

fn rings_touch(a: &[DVec2], b: &[DVec2]) -> bool {
    let (na, nb) = (a.len(), b.len());
    (0..na).any(|i| (0..nb).any(|j| segments_touch(a[i], a[(i + 1) % na], b[j], b[(j + 1) % nb])))
}

/// Validates a slab outline with holes. Returns the outline counter-clockwise
/// and the holes clockwise (SLB-001, SLB-003, EDT-018).
pub fn validate_slab_polygon(outline: &[DVec2], holes: &[Vec<DVec2>]) -> Result<(Vec<DVec2>, Vec<Vec<DVec2>>), String> {
    let outline = validate_ring(outline)?;
    let mut cleaned_holes: Vec<Vec<DVec2>> = Vec::with_capacity(holes.len());
    for hole in holes {
        let mut hole = validate_ring(hole).map_err(|e| format!("Hole: {e}"))?;
        if !point_in_ring(hole[0], &outline) || rings_touch(&hole, &outline) {
            return Err("A hole must lie inside the slab outline without touching it.".into());
        }
        if cleaned_holes
            .iter()
            .any(|other| rings_touch(&hole, other) || point_in_ring(hole[0], other) || point_in_ring(other[0], &hole))
        {
            return Err("Holes must not overlap each other.".into());
        }
        hole.reverse();
        cleaned_holes.push(hole);
    }
    Ok((outline, cleaned_holes))
}

fn check_thickness(thickness: f64) -> Result<(), String> {
    if !thickness.is_finite() || !(MIN_THICKNESS..=MAX_THICKNESS).contains(&thickness) {
        return Err("Thickness must be between 1 mm and 10 m.".into());
    }
    Ok(())
}

fn check_offset(value: f64, what: &str) -> Result<(), String> {
    if !value.is_finite() || value.abs() > 1000.0 {
        return Err(format!("{what} must be a finite value within ±1000 m."));
    }
    Ok(())
}

/// Range checks for a wall (ELM-017, NFR-REL-005).
pub fn validate_wall(wall: &Wall) -> Result<(), String> {
    check_coordinates(&[wall.start, wall.end])?;
    if wall.length() < MIN_EDGE_LENGTH {
        return Err("A wall must be at least 0.1 mm long.".into());
    }
    check_thickness(wall.thickness)?;
    if let ReferenceLine::Custom(offset) = wall.reference_line
        && (!offset.is_finite() || offset.abs() > MAX_THICKNESS)
    {
        return Err("The reference-line offset is out of range.".into());
    }
    check_offset(wall.base_offset, "The base offset")?;
    match wall.top {
        TopLink::Fixed { height } => {
            if !height.is_finite() || !(MIN_HEIGHT..=1000.0).contains(&height) {
                return Err("The wall height must be between 1 mm and 1000 m.".into());
            }
        }
        TopLink::Storey { offset, .. } => check_offset(offset, "The top offset")?,
        TopLink::Relative { storeys, offset } => {
            if storeys.abs() > 200 {
                return Err("The linked storey is out of range.".into());
            }
            check_offset(offset, "The top offset")?;
        }
    }
    Ok(())
}

/// Range checks for a slab, returning the slab with a normalised polygon.
pub fn validate_slab(slab: &Slab) -> Result<Slab, String> {
    check_thickness(slab.thickness)?;
    check_offset(slab.offset, "The slab offset")?;
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
pub fn validate_element(project: &Project, element: &Element) -> Result<Element, String> {
    if element.name.trim().is_empty() {
        return Err("An element needs a name.".into());
    }
    if project.storeys.get(element.storey).is_none() {
        return Err("The element's home storey does not exist.".into());
    }
    if project.layer(element.layer).is_none() {
        return Err("The element's layer does not exist.".into());
    }
    if project.material(element.kind.material()).is_none() {
        return Err("The element's material does not exist.".into());
    }
    if !element.tag().ifc_classes().contains(&element.ifc_class) {
        return Err(format!(
            "{} is not an allowed IFC class for a {}.",
            element.ifc_class.code(),
            element.tag().label().to_lowercase()
        ));
    }
    let mut element = element.clone();
    match &mut element.kind {
        ElementKind::Wall(wall) => {
            validate_wall(wall)?;
            if let TopLink::Storey { storey, .. } = wall.top
                && project.storeys.get(storey).is_none()
            {
                return Err("The storey the wall top is linked to does not exist.".into());
            }
        }
        ElementKind::Slab(slab) => *slab = validate_slab(slab)?,
    }
    let (bottom, top) = project
        .vertical_extent(&element)
        .ok_or("The element's vertical extent cannot be determined.")?;
    if top - bottom < MIN_HEIGHT {
        return Err("The top of the element must be above its base.".into());
    }
    Ok(element)
}

#[cfg(test)]
mod tests {
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
}
