//! Vertical extrusion of plan polygons into closed meshes.

use glam::{DVec2, DVec3};

use crate::GeometryError;
use crate::mesh::TriMesh;
use crate::triangulate::triangulate;

fn signed_area(ring: &[DVec2]) -> f64 {
    let n = ring.len();
    (0..n).map(|i| ring[i].perp_dot(ring[(i + 1) % n])).sum::<f64>() / 2.0
}

/// Extrudes a polygon with holes between `z0` and `z1` (`z0 < z1`). The
/// outer ring may have either orientation; holes must lie inside it. Every
/// planar face gets its own vertices (IFX-038), and the result is closed and
/// oriented outwards (IFX-032).
pub fn extrude(outer: &[DVec2], holes: &[Vec<DVec2>], z0: f64, z1: f64) -> Result<TriMesh, GeometryError> {
    if z1 - z0 <= 1e-9 || !z0.is_finite() || !z1.is_finite() {
        return Err(GeometryError::InvalidExtent);
    }
    let mut outer = outer.to_vec();
    if signed_area(&outer) < 0.0 {
        outer.reverse();
    }
    let holes: Vec<Vec<DVec2>> = holes
        .iter()
        .map(|h| {
            let mut h = h.clone();
            if signed_area(&h) > 0.0 {
                h.reverse();
            }
            h
        })
        .collect();

    let triangles = triangulate(&outer, &holes)?;
    let plan: Vec<DVec2> = outer.iter().chain(holes.iter().flatten()).copied().collect();

    let mut mesh = TriMesh::default();
    let top: Vec<DVec3> = plan.iter().map(|p| p.extend(z1)).collect();
    mesh.add_face(&top, &triangles);
    let bottom: Vec<DVec3> = plan.iter().map(|p| p.extend(z0)).collect();
    let reversed: Vec<[usize; 3]> = triangles.iter().map(|t| [t[0], t[2], t[1]]).collect();
    mesh.add_face(&bottom, &reversed);

    // Outer ring counter-clockwise and holes clockwise: the solid is on the
    // left of every edge, so (a0, b0, b1, a1) faces outwards.
    for ring in std::iter::once(&outer).chain(holes.iter()) {
        let n = ring.len();
        for i in 0..n {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            let quad = [a.extend(z0), b.extend(z0), b.extend(z1), a.extend(z1)];
            mesh.add_face(&quad, &[[0, 1, 2], [0, 2, 3]]);
        }
    }
    Ok(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slab_with_hole_is_closed() {
        let outer = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(0.0, 5.0),
            DVec2::new(8.0, 5.0),
            DVec2::new(8.0, 0.0),
        ];
        let hole = vec![
            DVec2::new(2.0, 2.0),
            DVec2::new(3.0, 2.0),
            DVec2::new(3.0, 3.0),
            DVec2::new(2.0, 3.0),
        ];
        let m = extrude(&outer, &[hole], -0.2, 0.0).unwrap();
        assert_eq!(m.check_closed(), Ok(()));
        assert!((m.signed_volume() - (40.0 - 1.0) * 0.2).abs() < 1e-9);
    }

    #[test]
    fn rejects_empty_extent() {
        let outer = vec![DVec2::new(0.0, 0.0), DVec2::new(1.0, 0.0), DVec2::new(0.0, 1.0)];
        assert!(extrude(&outer, &[], 1.0, 1.0).is_err());
    }
}
