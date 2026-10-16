//! Wall footprints and junctions (WAL-003, WAL-004, subset of WAL-020 and
//! WAL-026).
//!
//! Junctions are detected from reference-line contact only, never from
//! overlapping bodies, and the result does not depend on the order in which
//! walls were drawn:
//!
//! * **L junction**: exactly two walls share an end point. Both ends are
//!   mitred along the diagonal of the overlap of the two wall strips that
//!   separates the walls.
//! * **T junction**: a wall end lies on the reference line of exactly one
//!   other wall, away from its ends. The end is trimmed or extended to the
//!   face of the other wall that is nearest to it.
//! * Anything else (free ends, three or more walls at a node, crossings) is
//!   left square. Full junction handling with layer priorities arrives with
//!   layer structures (SRS Appendix F, increment 1).

use std::collections::HashMap;

use archicraft_model::{ElementId, ReferenceLine, StoreyId};
use glam::DVec2;

/// Two reference lines touch if their points are closer than this: 0.1 mm.
pub const JOIN_TOLERANCE: f64 = 1e-4;

/// The input of junction resolution for one wall.
#[derive(Clone, Debug)]
pub struct WallInput {
    pub id: ElementId,
    pub storey: StoreyId,
    pub start: DVec2,
    pub end: DVec2,
    pub thickness: f64,
    pub reference_line: ReferenceLine,
    /// Absolute bottom and top elevation; walls only join if they overlap
    /// vertically.
    pub bottom: f64,
    pub top: f64,
}

/// A line through `point` with direction `dir`.
#[derive(Clone, Copy, Debug)]
struct Line {
    point: DVec2,
    dir: DVec2,
}

impl Line {
    fn side(&self, p: DVec2) -> f64 {
        self.dir.perp_dot(p - self.point)
    }
}

/// Intersection of two lines, or `None` if they are (nearly) parallel.
fn intersect(a: Line, b: Line) -> Option<DVec2> {
    let denom = a.dir.perp_dot(b.dir);
    if denom.abs() < 1e-12 {
        return None;
    }
    let t = (b.point - a.point).perp_dot(b.dir) / denom;
    Some(a.point + a.dir * t)
}

#[derive(Clone, Copy, Debug)]
struct Faces {
    exterior: Line,
    interior: Line,
}

impl WallInput {
    fn dir(&self) -> DVec2 {
        (self.end - self.start).normalize_or_zero()
    }

    fn faces(&self) -> Faces {
        let dir = self.dir();
        let normal = dir.perp();
        let (ext, int) = self.reference_line.face_offsets(self.thickness);
        Faces {
            exterior: Line {
                point: self.start + normal * ext,
                dir,
            },
            interior: Line {
                point: self.start + normal * int,
                dir,
            },
        }
    }

    fn endpoint(&self, at_end: bool) -> DVec2 {
        if at_end { self.end } else { self.start }
    }

    fn overlaps_vertically(&self, other: &WallInput) -> bool {
        self.storey == other.storey && self.bottom < other.top && other.bottom < self.top
    }
}

/// The corners of a wall in plan, counter-clockwise: start exterior, end
/// exterior, end interior, start interior.
pub type Footprint = [DVec2; 4];

/// The two corners of one wall end: on the exterior and the interior face.
#[derive(Clone, Copy, Debug, PartialEq)]
struct EndCorners {
    exterior: DVec2,
    interior: DVec2,
}

fn square_end(wall: &WallInput, at_end: bool) -> EndCorners {
    let p = wall.endpoint(at_end);
    let normal = wall.dir().perp();
    let (ext, int) = wall.reference_line.face_offsets(wall.thickness);
    EndCorners {
        exterior: p + normal * ext,
        interior: p + normal * int,
    }
}

/// Intersects face lines in a fixed order (by element ID), so that two walls
/// sharing a junction compute bit-identical corner points.
fn ordered_intersect(a_id: ElementId, a: Line, b_id: ElementId, b: Line) -> Option<DVec2> {
    if a_id <= b_id { intersect(a, b) } else { intersect(b, a) }
}

fn l_junction(wall: &WallInput, at_end: bool, other: &WallInput, other_at_end: bool) -> Option<EndCorners> {
    let (f, g) = (wall.faces(), other.faces());
    if wall.dir().perp_dot(other.dir()).abs() < 1e-6 {
        return None;
    }
    let ix = |a: Line, b: Line| ordered_intersect(wall.id, a, other.id, b);
    let ee = ix(f.exterior, g.exterior)?;
    let ii = ix(f.interior, g.interior)?;
    let ei = ix(f.exterior, g.interior)?;
    let ie = ix(f.interior, g.exterior)?;
    let far = wall.endpoint(!at_end);
    let other_far = other.endpoint(!other_at_end);
    let separates = |p: DVec2, q: DVec2| {
        let d = Line {
            point: p,
            dir: (q - p).normalize_or_zero(),
        };
        d.side(far) * d.side(other_far) < 0.0
    };
    let corners = if separates(ee, ii) && !separates(ei, ie) {
        EndCorners {
            exterior: ee,
            interior: ii,
        }
    } else if separates(ei, ie) && !separates(ee, ii) {
        EndCorners {
            exterior: ei,
            interior: ie,
        }
    } else {
        return None;
    };
    // Very acute junctions would produce long spikes; keep those square.
    let p = wall.endpoint(at_end);
    let limit = 10.0 * wall.thickness.max(other.thickness);
    (corners.exterior.distance(p) <= limit && corners.interior.distance(p) <= limit).then_some(corners)
}

fn t_junction(wall: &WallInput, at_end: bool, through: &WallInput) -> Option<EndCorners> {
    if wall.dir().perp_dot(through.dir()).abs() < 1e-6 {
        return None;
    }
    let g = through.faces();
    let reference = Line {
        point: through.start,
        dir: through.dir(),
    };
    let far = wall.endpoint(!at_end);
    // The face of the through wall on the side of this wall's far end; the
    // interior face is on the left of the reference line.
    let near_face = if reference.side(far) > 0.0 {
        g.interior
    } else {
        g.exterior
    };
    let f = wall.faces();
    let corners = EndCorners {
        exterior: intersect(f.exterior, near_face)?,
        interior: intersect(f.interior, near_face)?,
    };
    // The wall must keep a positive length after trimming.
    let length_after = |c: DVec2| (c - far).dot(wall.endpoint(at_end) - far);
    (length_after(corners.exterior) > 0.0 && length_after(corners.interior) > 0.0).then_some(corners)
}

fn point_segment_distance(p: DVec2, a: DVec2, b: DVec2) -> (f64, f64) {
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 > 0.0 {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p.distance(a + ab * t), t)
}

fn resolve_end(walls: &[WallInput], index: usize, at_end: bool) -> EndCorners {
    let wall = &walls[index];
    let p = wall.endpoint(at_end);
    let mut end_contacts: Vec<(usize, bool)> = Vec::new();
    let mut body_contacts: Vec<usize> = Vec::new();
    for (j, other) in walls.iter().enumerate() {
        if j == index || !wall.overlaps_vertically(other) {
            continue;
        }
        if other.start.distance(p) < JOIN_TOLERANCE {
            end_contacts.push((j, false));
        } else if other.end.distance(p) < JOIN_TOLERANCE {
            end_contacts.push((j, true));
        } else {
            let (distance, _) = point_segment_distance(p, other.start, other.end);
            if distance < JOIN_TOLERANCE {
                body_contacts.push(j);
            }
        }
    }
    let joined = match (end_contacts.as_slice(), body_contacts.as_slice()) {
        ([(j, other_at_end)], []) => l_junction(wall, at_end, &walls[*j], *other_at_end),
        ([], [j]) => t_junction(wall, at_end, &walls[*j]),
        _ => None,
    };
    joined.unwrap_or_else(|| square_end(wall, at_end))
}

/// Computes the footprint of every wall, with junctions resolved.
pub fn footprints(walls: &[WallInput]) -> HashMap<ElementId, Footprint> {
    // Index walls by storey so that large models do not compare every pair.
    let mut by_storey: HashMap<StoreyId, Vec<usize>> = HashMap::new();
    for (i, w) in walls.iter().enumerate() {
        by_storey.entry(w.storey).or_default().push(i);
    }
    let mut result = HashMap::with_capacity(walls.len());
    for indices in by_storey.values() {
        let group: Vec<WallInput> = indices.iter().map(|&i| walls[i].clone()).collect();
        for (k, wall) in group.iter().enumerate() {
            let start = resolve_end(&group, k, false);
            let end = resolve_end(&group, k, true);
            result.insert(wall.id, [start.exterior, end.exterior, end.interior, start.interior]);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall(start: (f64, f64), end: (f64, f64), thickness: f64, reference_line: ReferenceLine) -> WallInput {
        WallInput {
            id: ElementId::new(),
            storey: STOREY.with(|s| *s),
            start: DVec2::new(start.0, start.1),
            end: DVec2::new(end.0, end.1),
            thickness,
            reference_line,
            bottom: 0.0,
            top: 3.0,
        }
    }

    thread_local! {
        static STOREY: StoreyId = StoreyId::new();
    }

    fn area(fp: &Footprint) -> f64 {
        (0..4).map(|i| fp[i].perp_dot(fp[(i + 1) % 4])).sum::<f64>() / 2.0
    }

    fn close(a: DVec2, b: (f64, f64)) -> bool {
        a.distance(DVec2::new(b.0, b.1)) < 1e-9
    }

    #[test]
    fn free_wall_is_a_rectangle() {
        let w = wall((0.0, 0.0), (5.0, 0.0), 0.3, ReferenceLine::Outside);
        let fp = footprints(std::slice::from_ref(&w))[&w.id];
        assert!(close(fp[0], (0.0, 0.0)) && close(fp[2], (5.0, 0.3)));
        assert!((area(&fp) - 1.5).abs() < 1e-12);
    }

    #[test]
    fn l_junction_mitres_both_walls_identically() {
        // Counter-clockwise corner at (5, 0); exterior faces outside.
        let a = wall((0.0, 0.0), (5.0, 0.0), 0.3, ReferenceLine::Outside);
        let b = wall((5.0, 0.0), (5.0, 4.0), 0.3, ReferenceLine::Outside);
        let fps = footprints(&[a.clone(), b.clone()]);
        let (fa, fb) = (fps[&a.id], fps[&b.id]);
        // a's end corners: exterior at the outer corner (5, 0), interior at (4.7, 0.3).
        assert!(close(fa[1], (5.0, 0.0)), "{fa:?}");
        assert!(close(fa[2], (4.7, 0.3)), "{fa:?}");
        // b's start corners are the same points, bit for bit.
        assert_eq!(fb[0], fa[1]);
        assert_eq!(fb[3], fa[2]);
    }

    #[test]
    fn l_junction_is_order_independent() {
        let a = wall((0.0, 0.0), (5.0, 0.0), 0.25, ReferenceLine::Center);
        let b = wall((5.0, 0.0), (5.0, 4.0), 0.4, ReferenceLine::Center);
        let one = footprints(&[a.clone(), b.clone()]);
        let two = footprints(&[b.clone(), a.clone()]);
        assert_eq!(one[&a.id], two[&a.id]);
        assert_eq!(one[&b.id], two[&b.id]);
    }

    #[test]
    fn l_junction_with_opposite_directions() {
        // b is drawn towards the corner, so its exterior faces the other way.
        let a = wall((0.0, 0.0), (5.0, 0.0), 0.3, ReferenceLine::Center);
        let b = wall((5.0, 4.0), (5.0, 0.0), 0.3, ReferenceLine::Center);
        let fps = footprints(&[a.clone(), b.clone()]);
        let total = area(&fps[&a.id]) + area(&fps[&b.id]);
        // Centre-line lengths 5 + 4, thickness 0.3, mitred corner: no overlap.
        assert!((total - 9.0 * 0.3).abs() < 1e-9, "{total}");
    }

    #[test]
    fn t_junction_stops_at_near_face() {
        let through = wall((0.0, 0.0), (10.0, 0.0), 0.3, ReferenceLine::Outside);
        // Interior wall starting on the through wall's reference line, going
        // into the building (+Y, the interior side).
        let branch = wall((5.0, 0.0), (5.0, 4.0), 0.2, ReferenceLine::Center);
        let fps = footprints(&[through.clone(), branch.clone()]);
        let fb = fps[&branch.id];
        assert!((fb[0].y - 0.3).abs() < 1e-12 && (fb[3].y - 0.3).abs() < 1e-12, "{fb:?}");
        // The through wall is unchanged.
        assert!((area(&fps[&through.id]) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn walls_on_other_storeys_do_not_join() {
        let a = wall((0.0, 0.0), (5.0, 0.0), 0.3, ReferenceLine::Outside);
        let mut b = wall((5.0, 0.0), (5.0, 4.0), 0.3, ReferenceLine::Outside);
        b.bottom = 3.0;
        b.top = 6.0;
        let fps = footprints(&[a.clone(), b]);
        assert!(close(fps[&a.id][2], (5.0, 0.3)));
    }
}
