//! Vertical extrusion of plan polygons into closed meshes.

use archicraft_model::MODEL_TOLERANCE;
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
/// oriented outwards (IFX-032). Heights below the model tolerance
/// (NFR-ACC-003) are rejected as zero; the 1 mm minimum height of elements is
/// enforced earlier by validation.
pub fn extrude(outer: &[DVec2], holes: &[Vec<DVec2>], z0: f64, z1: f64) -> Result<TriMesh, GeometryError> {
    if z1 - z0 < MODEL_TOLERANCE || !z0.is_finite() || !z1.is_finite() {
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
        // Heights within the model tolerance are zero (NFR-ACC-003).
        assert!(extrude(&outer, &[], 1.0, 1.0 + 0.000_005).is_err());
        assert!(extrude(&outer, &[], 1.0, 1.0 + 0.000_020).is_ok());
    }

    /// EDT-019 limits with NFR-ACC-003: two 0.1 mm edges meet at `a`, where
    /// the outline turns by 0.1°, so the triangle b–a–c has the sine of a
    /// 0.1° corner and an area of ½ · (0.1 mm)² · sin 0.1° ≈ 8.7e-12 m²,
    /// below the square of the model tolerance (1e-10 m²). It must be kept,
    /// or the cap opens. The far corners keep it a Delaunay triangle.
    #[test]
    fn ring_at_the_edt_019_limits_triangulates_completely() {
        use archicraft_model::validate::{MIN_CORNER_ANGLE_DEG, MIN_EDGE_LENGTH, validate_ring};

        let edge = MIN_EDGE_LENGTH * 1.000_001;
        let turn = MIN_CORNER_ANGLE_DEG.to_radians();
        let a = DVec2::new(0.0, 0.0);
        let b = a - DVec2::X * edge;
        let c = a + DVec2::from_angle(turn) * edge;
        let ring = vec![b, a, c, DVec2::new(0.5, 0.5), DVec2::new(-0.5, 0.5)];
        assert_eq!(
            validate_ring(&ring).unwrap(),
            ring,
            "the ring is valid and counter-clockwise"
        );

        let triangles = triangulate(&ring, &[]).unwrap();
        let area = |t: &[usize; 3]| (ring[t[1]] - ring[t[0]]).perp_dot(ring[t[2]] - ring[t[0]]) / 2.0;
        let sliver = triangles.iter().map(area).fold(f64::INFINITY, f64::min);
        assert!(sliver > 8.0e-12 && sliver < 9.0e-12, "the sliver is kept: {sliver}");
        let total: f64 = triangles.iter().map(area).sum();
        assert!((total - signed_area(&ring)).abs() < 1e-12, "{total}");
        assert_eq!(triangles.len(), ring.len() - 2);

        let body = extrude(&ring, &[], 0.0, 3.0).unwrap();
        assert_eq!(body.check_closed(), Ok(()));
        assert!((body.signed_volume() - 3.0 * signed_area(&ring)).abs() < 1e-12);
    }
}
