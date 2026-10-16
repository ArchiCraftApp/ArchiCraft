//! Triangle meshes and their validity checks (IFX-032, IFX-038).

use std::collections::{HashMap, HashSet};

use glam::{DVec2, DVec3};

/// A triangle list. Coordinates are in metres.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TriMesh {
    pub points: Vec<DVec3>,
    /// Three indices per triangle, counter-clockwise seen from outside.
    pub indices: Vec<u32>,
}

/// Why a mesh is not a valid closed body (IFX-032).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeshIssue {
    Empty,
    IndexOutOfRange,
    DegenerateTriangle,
    DuplicateTriangle,
    UnreferencedPoint,
    /// An edge is not shared by exactly two triangles with opposite
    /// directions: the mesh is open, non-manifold or inconsistently oriented.
    OpenOrNonManifold,
    /// The mesh is oriented inwards.
    InsideOut,
    TooManyPoints,
}

impl std::fmt::Display for MeshIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            MeshIssue::Empty => "the mesh is empty",
            MeshIssue::IndexOutOfRange => "a triangle refers to a missing point",
            MeshIssue::DegenerateTriangle => "the mesh has a degenerate triangle",
            MeshIssue::DuplicateTriangle => "the mesh has a duplicate triangle",
            MeshIssue::UnreferencedPoint => "the mesh has an unused point",
            MeshIssue::OpenOrNonManifold => "the mesh is not closed and consistently oriented",
            MeshIssue::InsideOut => "the mesh is oriented inwards",
            MeshIssue::TooManyPoints => "the mesh has more than 65,535 points",
        };
        f.write_str(text)
    }
}

/// Largest number of points in one mesh; the reference viewer uses 16-bit
/// indices (IFX-038).
pub const MAX_MESH_POINTS: usize = 65_535;

/// Quantisation step used to merge coincident points when checking a mesh.
/// IFCX coordinates are written rounded to 10⁻⁶ m (IFX-007).
const MERGE_STEP: f64 = 1e-6;

type Key2 = (i64, i64);
type Key3 = (i64, i64, i64);

fn key(p: DVec3) -> Key3 {
    (
        (p.x / MERGE_STEP).round() as i64,
        (p.y / MERGE_STEP).round() as i64,
        (p.z / MERGE_STEP).round() as i64,
    )
}

impl TriMesh {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn triangles(&self) -> impl Iterator<Item = [DVec3; 3]> + '_ {
        self.indices
            .chunks_exact(3)
            .map(|t| [t[0], t[1], t[2]].map(|i| self.points[i as usize]))
    }

    /// Adds a planar face with its own vertices (IFX-038): `points` are the
    /// face's corners and `triangles` index into them.
    pub fn add_face(&mut self, points: &[DVec3], triangles: &[[usize; 3]]) {
        let base = self.points.len() as u32;
        self.points.extend_from_slice(points);
        for t in triangles {
            self.indices.extend(t.iter().map(|&i| base + i as u32));
        }
    }

    pub fn append(&mut self, other: &TriMesh) {
        let base = self.points.len() as u32;
        self.points.extend_from_slice(&other.points);
        self.indices.extend(other.indices.iter().map(|i| base + i));
    }

    pub fn translated(&self, offset: DVec3) -> TriMesh {
        TriMesh {
            points: self.points.iter().map(|p| *p + offset).collect(),
            indices: self.indices.clone(),
        }
    }

    /// Signed volume; positive for a closed mesh oriented outwards.
    pub fn signed_volume(&self) -> f64 {
        self.triangles().map(|[a, b, c]| a.dot(b.cross(c))).sum::<f64>() / 6.0
    }

    /// Axis-aligned bounds, if the mesh has points.
    pub fn bounds(&self) -> Option<(DVec3, DVec3)> {
        let first = *self.points.first()?;
        Some(
            self.points
                .iter()
                .fold((first, first), |(lo, hi), p| (lo.min(*p), hi.max(*p))),
        )
    }

    /// Checks IFX-032 and the point limit of IFX-038: closed and 2-manifold
    /// once coincident points are merged, consistently oriented outwards, with
    /// no degenerate or duplicate triangles and no unreferenced points.
    pub fn check_closed(&self) -> Result<(), MeshIssue> {
        if self.indices.is_empty() || self.points.is_empty() {
            return Err(MeshIssue::Empty);
        }
        if self.points.len() > MAX_MESH_POINTS {
            return Err(MeshIssue::TooManyPoints);
        }
        if self.indices.iter().any(|&i| i as usize >= self.points.len()) {
            return Err(MeshIssue::IndexOutOfRange);
        }
        let mut used = vec![false; self.points.len()];
        let mut merged: HashMap<Key3, u32> = HashMap::new();
        let ids: Vec<u32> = self
            .points
            .iter()
            .map(|p| {
                let next = merged.len() as u32;
                *merged.entry(key(*p)).or_insert(next)
            })
            .collect();
        let mut seen_triangles: HashSet<[u32; 3]> = HashSet::new();
        let mut edges: HashMap<(u32, u32), i32> = HashMap::new();
        for t in self.indices.chunks_exact(3) {
            for &i in t {
                used[i as usize] = true;
            }
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| ids[i as usize]);
            if a == b || b == c || a == c {
                return Err(MeshIssue::DegenerateTriangle);
            }
            let [pa, pb, pc] = [t[0], t[1], t[2]].map(|i| self.points[i as usize]);
            if (pb - pa).cross(pc - pa).length() < 1e-12 {
                return Err(MeshIssue::DegenerateTriangle);
            }
            let mut sorted = [a, b, c];
            sorted.sort_unstable();
            if !seen_triangles.insert(sorted) {
                return Err(MeshIssue::DuplicateTriangle);
            }
            for (from, to) in [(a, b), (b, c), (c, a)] {
                *edges.entry((from, to)).or_insert(0) += 1;
            }
        }
        if used.iter().any(|u| !u) {
            return Err(MeshIssue::UnreferencedPoint);
        }
        for (&(from, to), &count) in &edges {
            if count != 1 || edges.get(&(to, from)) != Some(&1) {
                return Err(MeshIssue::OpenOrNonManifold);
            }
        }
        if self.signed_volume() <= 0.0 {
            return Err(MeshIssue::InsideOut);
        }
        Ok(())
    }

    /// Intersects a ray with the mesh and returns the smallest positive ray
    /// parameter, for picking in 3D.
    pub fn ray_hit(&self, origin: DVec3, direction: DVec3) -> Option<f64> {
        let mut best: Option<f64> = None;
        for [a, b, c] in self.triangles() {
            if let Some(t) = ray_triangle(origin, direction, a, b, c)
                && best.is_none_or(|b| t < b)
            {
                best = Some(t);
            }
        }
        best
    }

    /// The outline of the faces that look up (+Z), as segments in plan. For
    /// extruded bodies this is the footprint. Used for projected lines in plan
    /// views.
    pub fn up_facing_outline(&self) -> Vec<[DVec2; 2]> {
        self.facing_outline(true)
    }

    /// The outline of the faces that look down, for overhead lines.
    pub fn down_facing_outline(&self) -> Vec<[DVec2; 2]> {
        self.facing_outline(false)
    }

    fn facing_outline(&self, up: bool) -> Vec<[DVec2; 2]> {
        let mut edges: HashMap<(Key2, Key2), (DVec2, DVec2, i32)> = HashMap::new();
        let key2 = |p: DVec2| -> Key2 { ((p.x / MERGE_STEP).round() as i64, (p.y / MERGE_STEP).round() as i64) };
        for [a, b, c] in self.triangles() {
            let normal = (b - a).cross(c - a);
            let facing = if up { normal.z } else { -normal.z };
            if facing <= 1e-12 * normal.length().max(1e-12) {
                continue;
            }
            for (p, q) in [(a, b), (b, c), (c, a)] {
                let (p, q) = (p.truncate(), q.truncate());
                let (kp, kq) = (key2(p), key2(q));
                if kp == kq {
                    continue;
                }
                let k = if kp < kq { (kp, kq) } else { (kq, kp) };
                edges.entry(k).or_insert((p, q, 0)).2 += 1;
            }
        }
        let mut out: Vec<[DVec2; 2]> = edges
            .into_values()
            .filter(|(_, _, count)| *count == 1)
            .map(|(p, q, _)| [p, q])
            .collect();
        out.sort_by(|a, b| {
            (a[0].x, a[0].y, a[1].x, a[1].y)
                .partial_cmp(&(b[0].x, b[0].y, b[1].x, b[1].y))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out
    }

    /// Edges between faces that are not coplanar (and open edges), for the
    /// "shaded with edges" display (VIEW-050).
    pub fn feature_edges(&self, min_angle_deg: f64) -> Vec<[DVec3; 2]> {
        let cos_limit = min_angle_deg.to_radians().cos();
        let mut edges: HashMap<(Key3, Key3), (DVec3, DVec3, Vec<DVec3>)> = HashMap::new();
        for [a, b, c] in self.triangles() {
            let normal = (b - a).cross(c - a).normalize_or_zero();
            for (p, q) in [(a, b), (b, c), (c, a)] {
                let (kp, kq) = (key(p), key(q));
                if kp == kq {
                    continue;
                }
                let k = if kp < kq { (kp, kq) } else { (kq, kp) };
                edges.entry(k).or_insert((p, q, Vec::new())).2.push(normal);
            }
        }
        edges
            .into_values()
            .filter(|(_, _, normals)| normals.len() != 2 || normals[0].dot(normals[1]) < cos_limit)
            .map(|(p, q, _)| [p, q])
            .collect()
    }
}

/// Möller–Trumbore ray/triangle intersection, both sides.
fn ray_triangle(origin: DVec3, dir: DVec3, a: DVec3, b: DVec3, c: DVec3) -> Option<f64> {
    let e1 = b - a;
    let e2 = c - a;
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-14 {
        return None;
    }
    let inv = 1.0 / det;
    let s = origin - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 1e-9).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extrude::extrude;

    fn unit_box() -> TriMesh {
        let square = [
            DVec2::new(0.0, 0.0),
            DVec2::new(1.0, 0.0),
            DVec2::new(1.0, 1.0),
            DVec2::new(0.0, 1.0),
        ];
        extrude(&square, &[], 0.0, 1.0).unwrap()
    }

    #[test]
    fn box_is_closed_with_unit_volume() {
        let m = unit_box();
        assert_eq!(m.check_closed(), Ok(()));
        assert!((m.signed_volume() - 1.0).abs() < 1e-12);
        assert_eq!(m.triangle_count(), 12);
        assert_eq!(m.points.len(), 24, "every face has its own vertices (IFX-038)");
    }

    #[test]
    fn detects_open_and_flipped_meshes() {
        let mut open = unit_box();
        open.indices.truncate(open.indices.len() - 3);
        assert_eq!(open.check_closed(), Err(MeshIssue::UnreferencedPoint));
        let mut flipped = unit_box();
        for t in flipped.indices.chunks_exact_mut(3) {
            t.swap(1, 2);
        }
        assert_eq!(flipped.check_closed(), Err(MeshIssue::InsideOut));
        let mut broken = unit_box();
        broken.indices.swap(0, 1);
        assert_eq!(broken.check_closed(), Err(MeshIssue::OpenOrNonManifold));
    }

    #[test]
    fn ray_hits_box() {
        let m = unit_box();
        let t = m.ray_hit(DVec3::new(0.5, 0.5, 5.0), DVec3::NEG_Z).unwrap();
        assert!((t - 4.0).abs() < 1e-12);
        assert!(m.ray_hit(DVec3::new(2.5, 0.5, 5.0), DVec3::NEG_Z).is_none());
    }

    #[test]
    fn outline_and_feature_edges_of_box() {
        let m = unit_box();
        assert_eq!(m.up_facing_outline().len(), 4);
        assert_eq!(m.feature_edges(1.0).len(), 12);
    }
}
