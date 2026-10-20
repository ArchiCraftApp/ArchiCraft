//! The geometric coincidence tolerance and the helpers that apply it
//! (NFR-ACC-003, SRS Appendix D.1: model tolerance 0.01 mm).
//!
//! [`MODEL_TOLERANCE`] is the only tolerance literal of the model and
//! geometry crates; every other item of this module is derived from it, so
//! that the tolerance can later become a project setting (EDT-019) by
//! threading it through these helpers. A source scan
//! (`tests/tolerance_scan.rs`) keeps it that way. The snapping code still
//! has its own epsilons and joins the scan once they are retired.
//!
//! Comparisons fall into three groups:
//!
//! * **Lengths** (distances between points, elevations, gaps between
//!   reference lines) are compared with [`MODEL_TOLERANCE`]: points closer
//!   than the tolerance are the same point in snapping, joining and Boolean
//!   operations ([`coincident`], [`coincident3`], [`JOIN_TOLERANCE`],
//!   [`Welder`]).
//! * **Directions** are compared by the sine of the angle between them with
//!   [`ANGLE_TOLERANCE`] ([`is_parallel`]).
//! * **Dimensionless robustness guards** (determinants, cross products, ray
//!   parameters) are not tolerances. [`near_zero`] compares them with the
//!   rounding error of `f64` at the scale of their operands, so that a guard
//!   never decides about real geometry, however small.
//!
//! Boolean operations (SOL-020): the Boolean wrapper of the solid kernel
//! must pass [`MODEL_TOLERANCE`] to the kernel as its merge tolerance, so
//! that the Boolean clause of NFR-ACC-003 holds as well. The repair
//! thresholds of EDT-019 (0.1 mm edges, 0.1° corners) are validation ranges,
//! not coincidence tolerances; they stay in [`crate::validate`].

use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;

use glam::{DVec2, DVec3};

/// Geometric coincidence tolerance: 0.01 mm (NFR-ACC-003, Appendix D.1).
/// Points closer than this are treated as identical in snapping, joining
/// and Boolean operations.
pub const MODEL_TOLERANCE: f64 = 1e-5;

/// Two reference lines touch, and their walls join, if their points are
/// closer than this (decision D-6: junctions from reference-line contact
/// only; WAL-020).
pub const JOIN_TOLERANCE: f64 = MODEL_TOLERANCE;

/// Two directions are parallel if the sine of the angle between them is at
/// most this: the angle by which a direction turns to deviate by
/// [`MODEL_TOLERANCE`] over one metre (about 0.00057°). Over one metre this
/// keeps every angle that can be drawn distinct from a straight line.
pub const ANGLE_TOLERANCE: f64 = MODEL_TOLERANCE / 1.0;

/// True if two plan points coincide within [`MODEL_TOLERANCE`].
pub fn coincident(a: DVec2, b: DVec2) -> bool {
    a.distance(b) < MODEL_TOLERANCE
}

/// True if two points coincide within [`MODEL_TOLERANCE`].
pub fn coincident3(a: DVec3, b: DVec3) -> bool {
    a.distance(b) < MODEL_TOLERANCE
}

/// A cell of the plan grid with [`MODEL_TOLERANCE`] spacing.
pub type Cell2 = (i64, i64);
/// A cell of the spatial grid with [`MODEL_TOLERANCE`] spacing.
pub type Cell3 = (i64, i64, i64);

fn cell_index(value: f64) -> i64 {
    (value / MODEL_TOLERANCE).floor() as i64
}

/// The cell of the [`MODEL_TOLERANCE`] grid that contains `p`, as a hash key.
///
/// Bit-identical points always get the same key, and coincident points lie
/// in the same or in adjacent cells. Points that straddle a cell boundary
/// get different keys although they coincide, so an exact key lookup merges
/// only points that are computed identically; [`Welder`] merges every pair
/// of coincident points.
pub fn quantise(p: DVec2) -> Cell2 {
    (cell_index(p.x), cell_index(p.y))
}

/// The cell of the [`MODEL_TOLERANCE`] grid that contains `p`; see
/// [`quantise`].
pub fn quantise3(p: DVec3) -> Cell3 {
    (cell_index(p.x), cell_index(p.y), cell_index(p.z))
}

/// True if the directions `a` and `b` (of any length) are parallel or
/// opposite within [`ANGLE_TOLERANCE`]. A zero vector is parallel to every
/// direction.
pub fn is_parallel(a: DVec2, b: DVec2) -> bool {
    a.perp_dot(b).abs() <= ANGLE_TOLERANCE * a.length() * b.length()
}

/// True if `value` is zero up to the rounding error of a computation whose
/// operands have the magnitude `scale`: `|value| <= 64 ε |scale|`, with ε
/// the machine epsilon of `f64`. The factor leaves room for the rounding of
/// a few chained operations.
///
/// This is the robustness guard for dimensionless quantities (determinants,
/// cross products, ray parameters), not a length tolerance: for a cross
/// product of two coordinate differences `u × v`, pass the largest
/// coordinate magnitude times `|u| + |v|`, which bounds its rounding error.
pub fn near_zero(value: f64, scale: f64) -> bool {
    value.abs() <= 64.0 * f64::EPSILON * scale.abs()
}

/// A point type that [`Welder`] can merge: [`DVec2`] or [`DVec3`].
pub trait WeldPoint: Copy {
    /// A cell of the [`MODEL_TOLERANCE`] grid.
    type Cell: Copy + Eq + Hash + Debug;

    /// The cell that contains the point ([`quantise`], [`quantise3`]).
    fn cell(self) -> Self::Cell;

    /// The cell itself followed by its adjacent cells: every point that
    /// coincides with a point of `cell` lies in one of them.
    fn neighbourhood(cell: Self::Cell) -> impl Iterator<Item = Self::Cell>;

    /// [`coincident`] or [`coincident3`].
    fn coincides(self, other: Self) -> bool;
}

/// An index of the grid followed by its two neighbours. Indices saturate at
/// the ends of `i64` (infinite coordinates or beyond about 9.2e13 m), where
/// the neighbours repeat the index itself.
const fn around(index: i64) -> [i64; 3] {
    [index, index.saturating_sub(1), index.saturating_add(1)]
}

impl WeldPoint for DVec2 {
    type Cell = Cell2;

    fn cell(self) -> Cell2 {
        quantise(self)
    }

    fn neighbourhood((x, y): Cell2) -> impl Iterator<Item = Cell2> {
        around(x).into_iter().flat_map(move |x| around(y).map(|y| (x, y)))
    }

    fn coincides(self, other: Self) -> bool {
        coincident(self, other)
    }
}

impl WeldPoint for DVec3 {
    type Cell = Cell3;

    fn cell(self) -> Cell3 {
        quantise3(self)
    }

    fn neighbourhood((x, y, z): Cell3) -> impl Iterator<Item = Cell3> {
        around(x)
            .into_iter()
            .flat_map(move |x| around(y).into_iter().flat_map(move |y| around(z).map(|z| (x, y, z))))
    }

    fn coincides(self, other: Self) -> bool {
        coincident3(self, other)
    }
}

/// Merges points that coincide within [`MODEL_TOLERANCE`] (NFR-ACC-003),
/// for mesh welding and snap-target deduplication.
///
/// Every point gets the number of a group whose representative is the first
/// point of that group. A later point joins the group of the first
/// representative it coincides with, searching its own cell first and then
/// the adjacent cells, or founds a new group. Grouping is greedy: two points
/// that each coincide with a representative but not with each other end up
/// in one group, and the result depends on the order in which points are
/// welded, which is deterministic for a given input.
#[derive(Clone, Debug)]
pub struct Welder<P: WeldPoint> {
    cells: HashMap<P::Cell, Vec<usize>>,
    points: Vec<P>,
}

impl<P: WeldPoint> Default for Welder<P> {
    fn default() -> Self {
        Self {
            cells: HashMap::new(),
            points: Vec::new(),
        }
    }
}

impl<P: WeldPoint> Welder<P> {
    pub fn new() -> Self {
        Self::default()
    }

    /// The group of `p`, if an earlier point coincides with it.
    pub fn find(&self, p: P) -> Option<usize> {
        P::neighbourhood(p.cell()).find_map(|cell| {
            self.cells
                .get(&cell)?
                .iter()
                .copied()
                .find(|&group| p.coincides(self.points[group]))
        })
    }

    /// The group of `p`; founds a new group with `p` as its representative
    /// if no earlier point coincides with it.
    pub fn weld(&mut self, p: P) -> usize {
        if let Some(group) = self.find(p) {
            return group;
        }
        let group = self.points.len();
        self.points.push(p);
        self.cells.entry(p.cell()).or_default().push(group);
        group
    }

    /// The representatives, indexed by group.
    pub fn points(&self) -> &[P] {
        &self.points
    }

    /// The number of groups.
    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// The representatives, indexed by group.
    pub fn into_points(self) -> Vec<P> {
        self.points
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MICROMETRE: f64 = 1e-6;

    #[test]
    fn model_tolerance_is_a_hundredth_of_a_millimetre() {
        // NFR-ACC-003 and Appendix D.1: 0.01 mm.
        assert_eq!(MODEL_TOLERANCE, 1e-5);
        assert_eq!(MODEL_TOLERANCE, 0.01 / 1000.0);
    }

    #[test]
    fn derived_tolerances() {
        assert_eq!(JOIN_TOLERANCE, MODEL_TOLERANCE);
        // A direction turned by the angle tolerance deviates by the model
        // tolerance over one metre.
        let turned = DVec2::from_angle(ANGLE_TOLERANCE.asin());
        assert!((turned.y - MODEL_TOLERANCE).abs() < 1e-18);
        assert!((ANGLE_TOLERANCE.asin().to_degrees() - 0.00057).abs() < 0.000_01);
    }

    #[test]
    fn points_closer_than_the_tolerance_coincide() {
        let a = DVec2::new(3.0, 4.0);
        assert!(coincident(a, a));
        assert!(coincident(a, a + DVec2::new(3.0, 4.0) * MICROMETRE), "5 µm apart");
        assert!(!coincident(a, a + DVec2::new(0.0, 20.0 * MICROMETRE)), "20 µm apart");
        let b = DVec3::new(1.0, -2.0, 3.0);
        assert!(coincident3(b, b + DVec3::new(3.0, 0.0, 4.0) * MICROMETRE));
        assert!(!coincident3(b, b + DVec3::new(0.0, 0.0, 20.0 * MICROMETRE)));
    }

    #[test]
    fn coincident_points_lie_in_neighbouring_cells() {
        for k in 0..200 {
            let a = DVec2::new(k as f64 * 0.7 * MICROMETRE - 50.0, 12.345 + k as f64 * MICROMETRE);
            let b = a + DVec2::new(7.0, -7.0) * MICROMETRE;
            assert!(coincident(a, b));
            assert!(DVec2::neighbourhood(quantise(a)).any(|c| c == quantise(b)), "{a} {b}");
            let a3 = a.extend(-7.0);
            let b3 = a3 + DVec3::new(5.0, -5.0, 5.0) * MICROMETRE;
            assert!(coincident3(a3, b3));
            assert!(
                DVec3::neighbourhood(quantise3(a3)).any(|c| c == quantise3(b3)),
                "{a3} {b3}"
            );
        }
        assert_eq!(quantise(DVec2::new(1.0, 1.0)), quantise(DVec2::new(1.0, 1.0)));
        assert_eq!(DVec2::neighbourhood((0, 0)).count(), 9);
        assert_eq!(DVec3::neighbourhood((0, 0, 0)).count(), 27);
        assert_eq!(DVec2::neighbourhood((4, 5)).next(), Some((4, 5)), "own cell first");
    }

    #[test]
    fn parallel_directions() {
        let x = DVec2::new(2.0, 0.0);
        assert!(is_parallel(x, DVec2::new(5.0, 0.0)));
        assert!(
            is_parallel(x, DVec2::new(-0.1, 0.0)),
            "opposite directions are parallel"
        );
        assert!(is_parallel(x, DVec2::from_angle(0.0001_f64.to_radians())));
        assert!(!is_parallel(x, DVec2::from_angle(0.001_f64.to_radians())));
        assert!(!is_parallel(x, DVec2::from_angle(0.001_f64.to_radians()) * 1000.0));
        assert!(!is_parallel(x, DVec2::Y));
        assert!(is_parallel(x, DVec2::ZERO));
    }

    #[test]
    fn near_zero_is_relative_to_the_operand_scale() {
        assert!(near_zero(0.0, 0.0));
        assert!(near_zero(f64::EPSILON, 1.0));
        assert!(near_zero(-1e-12, 1e4));
        assert!(!near_zero(1e-12, 1.0));
        assert!(!near_zero(f64::NAN, 1.0));
        // With the documented scale for a cross product of coordinate
        // differences, an EDT-019 sliver (two 0.1 mm edges at 0.1°) is far
        // from zero although its area is below the square of the model
        // tolerance, and a collinear triple is zero, from the origin out to
        // the coordinate limit of ±100 km (NFR-ACC-001).
        for origin in [0.0, 100.0, 100_000.0] {
            let a = DVec2::splat(origin);
            let b = a + DVec2::X * 1e-4;
            let c = a + DVec2::from_angle(0.1_f64.to_radians()) * 1e-4;
            let midpoint = a + (b - a) * 0.5;
            let cross = |p: DVec2, q: DVec2| {
                let (u, v) = (p - a, q - a);
                let magnitude = a.abs().max(p.abs()).max(q.abs()).max_element();
                (u.perp_dot(v), magnitude * (u.length() + v.length()))
            };
            let (sliver, scale) = cross(b, c);
            assert!(sliver / 2.0 < MODEL_TOLERANCE * MODEL_TOLERANCE);
            assert!(!near_zero(sliver, scale), "sliver at {origin}");
            let (flat, scale) = cross(b, midpoint);
            assert!(near_zero(flat, scale), "collinear at {origin}: {flat}");
        }
    }

    #[test]
    fn welder_merges_coincident_points_across_cell_boundaries() {
        // A pair 5 µm apart that straddles a cell boundary.
        let (a, b) = (0..20)
            .map(|k| {
                let a = DVec2::new(1.0 + k as f64 * MICROMETRE, 2.0);
                (a, a + DVec2::new(5.0 * MICROMETRE, 0.0))
            })
            .find(|(a, b)| quantise(*a) != quantise(*b))
            .expect("a pair across a cell boundary");
        let mut welder = Welder::new();
        assert!(welder.is_empty());
        assert_eq!(welder.weld(a), 0);
        assert_eq!(welder.weld(b), 0, "5 µm apart: one point");
        let apart = a + DVec2::new(0.0, 20.0 * MICROMETRE);
        assert_eq!(welder.weld(apart), 1, "20 µm apart: two points");
        assert_eq!(welder.find(b), Some(0));
        assert_eq!(welder.find(DVec2::ZERO), None);
        assert_eq!(welder.points(), &[a, apart]);
        assert_eq!(welder.len(), 2);

        let mut welder3 = Welder::new();
        let p = DVec3::new(1.0, 2.0, 3.0);
        assert_eq!(welder3.weld(p), 0);
        assert_eq!(welder3.weld(p - DVec3::splat(2.0 * MICROMETRE)), 0);
        assert_eq!(welder3.weld(p + DVec3::Z * 20.0 * MICROMETRE), 1);
        assert_eq!(welder3.into_points(), [p, p + DVec3::Z * 20.0 * MICROMETRE]);
    }

    #[test]
    fn welder_accepts_points_beyond_the_grid() {
        // Cell indices saturate for infinite coordinates and beyond about
        // 9.2e13 m; the neighbourhood of a saturated cell must not overflow.
        // A non-finite point coincides with nothing, not even itself.
        let mut welder = Welder::new();
        let huge = DVec2::new(1e20, -1e20);
        assert_eq!(welder.weld(DVec2::new(f64::INFINITY, 0.0)), 0);
        assert_eq!(welder.weld(DVec2::new(f64::NEG_INFINITY, 1e20)), 1);
        assert_eq!(welder.weld(huge), 2);
        assert_eq!(welder.find(huge), Some(2));
        assert_eq!(welder.find(DVec2::new(f64::INFINITY, 0.0)), None);
        let mut welder3 = Welder::new();
        assert_eq!(welder3.weld(DVec3::new(f64::INFINITY, f64::NEG_INFINITY, 1e20)), 0);
        assert_eq!(DVec2::neighbourhood((i64::MAX, i64::MIN)).count(), 9);
    }
}
