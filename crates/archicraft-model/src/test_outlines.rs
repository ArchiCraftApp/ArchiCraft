//! Slab outlines that tests at the slab point limit share (SLB-003,
//! NFR-SEC-003, NFR-PERF-006): the comb and the rectangle with holes, one
//! definition for the model's validation tests and the geometry crate's
//! tests. Behind the `test-kinds` feature, which only the crates'
//! dev-dependencies turn on.

use glam::DVec2;

/// A comb of `teeth` unit teeth on a 1 m deep back, clockwise: `4 * teeth +
/// 3` points, right angles only and no self-intersection ([`long_comb`] with
/// teeth 1 m long).
pub fn comb(teeth: usize) -> Vec<DVec2> {
    long_comb(teeth, 1.0)
}

/// A comb of `teeth` teeth `length` long, 1 m wide and 1 m apart, from
/// x = 0 along y = 0, on a back from y = 0 down to y = -1, clockwise:
/// `4 * teeth + 3` points.
pub fn long_comb(teeth: usize, length: f64) -> Vec<DVec2> {
    let mut ring = Vec::with_capacity(4 * teeth + 3);
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

/// A rectangle from the origin of as many columns of 1 m cells as the
/// square root of `count` rounded up, and as many rows as `count` cells
/// fill, with a 0.5 m square hole in the middle of each of the first
/// `count` cells, row by row: `4 + 4 * count` points, the outline
/// counter-clockwise and the holes clockwise.
pub fn rectangle_with_holes(count: usize) -> (Vec<DVec2>, Vec<Vec<DVec2>>) {
    let columns = (count as f64).sqrt().ceil() as usize;
    let rows = count.div_ceil(columns);
    let size = DVec2::new(columns as f64, rows as f64);
    let outline = vec![DVec2::ZERO, DVec2::new(size.x, 0.0), size, DVec2::new(0.0, size.y)];
    let holes = (0..count)
        .map(|k| {
            let corner = DVec2::new((k % columns) as f64, (k / columns) as f64) + DVec2::splat(0.25);
            [
                DVec2::ZERO,
                DVec2::new(0.0, 0.5),
                DVec2::splat(0.5),
                DVec2::new(0.5, 0.0),
            ]
            .map(|p| corner + p)
            .to_vec()
        })
        .collect();
    (outline, holes)
}
