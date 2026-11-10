//! Cross-sections of the parametric standard profiles (PRF-005): closed
//! polygons with holes, curved edges tessellated by the IFX-034 rule and
//! smooth vertices marked for IFX-038.

#![allow(clippy::unwrap_used)]

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use archicraft_geometry::GeometryError;
use archicraft_geometry::section::{
    BODY_ROLE, MAX_SEGMENTS_PER_TURN, Section, SectionLoop, arc_segments, parametric_section,
};
use archicraft_model::MaterialId;
use archicraft_model::glam::DVec2;
use archicraft_model::profile::ParametricShape;
use archicraft_model::project::{ProjectSettings, Tessellation};
use archicraft_model::validate::{MIN_EDGE_LENGTH, clean_ring, validate_ring, validate_slab_polygon};

fn v(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn defaults() -> Tessellation {
    ProjectSettings::default().tessellation
}

fn section(shape: ParametricShape) -> Section {
    section_with(shape, &defaults())
}

fn section_with(shape: ParametricShape, tessellation: &Tessellation) -> Section {
    parametric_section(&shape, MaterialId::new(), tessellation).unwrap_or_else(|e| panic!("{shape:?}: {e}"))
}

/// The one loop of a section without holes.
fn outer(section: &Section) -> &SectionLoop {
    assert_eq!(section.regions.len(), 1);
    &section.regions[0].outer
}

fn relative(actual: f64, expected: f64) -> f64 {
    ((actual - expected) / expected).abs()
}

/// The area of the regular polygon with `n` vertices on a circle of `r`.
fn inscribed(n: usize, r: f64) -> f64 {
    n as f64 / 2.0 * r * r * (TAU / n as f64).sin()
}

/// The area a quarter-circle fillet of radius `r` adds to a concave corner
/// or removes from a convex one.
fn fillet(r: f64) -> f64 {
    (1.0 - PI / 4.0) * r * r
}

// Typical dimensions, in metres, named after the sections they come from.

/// IPE 200 (EN 10365): h 200, b 100, tw 5.6, tf 8.5, r 12 mm.
const IPE_200: ParametricShape = ParametricShape::IShape {
    height: 0.2,
    width: 0.1,
    web: 0.0056,
    flange: 0.0085,
    root_radius: 0.012,
};
/// A rectangle of 300 x 500 mm.
const RECTANGLE: ParametricShape = ParametricShape::Rectangle {
    width: 0.3,
    height: 0.5,
};
const CIRCLE: ParametricShape = ParametricShape::Circle { diameter: 0.3 };
/// RHS 200 x 100 x 8, outer radius 16 mm (inner radius 8 mm).
const RHS: ParametricShape = ParametricShape::RectangularHollow {
    width: 0.2,
    height: 0.1,
    thickness: 0.008,
    outer_radius: 0.016,
};
/// CHS 168.3 x 8.
const CHS: ParametricShape = ParametricShape::CircularHollow {
    diameter: 0.1683,
    thickness: 0.008,
};
/// T 100 (EN 10055 without the toe radii).
const TEE: ParametricShape = ParametricShape::TShape {
    height: 0.1,
    width: 0.1,
    web: 0.011,
    flange: 0.011,
    root_radius: 0.011,
};
/// L 120 x 80 x 10, r1 13 mm, r2 6.5 mm: unequal legs, so a swapped width
/// and height would show.
const ANGLE: ParametricShape = ParametricShape::LShape {
    height: 0.12,
    width: 0.08,
    thickness: 0.01,
    root_radius: 0.013,
    toe_radius: 0.0065,
};
/// UPE 200: h 200, b 80, tw 6, tf 11, r 13 mm.
const CHANNEL: ParametricShape = ParametricShape::UShape {
    height: 0.2,
    width: 0.08,
    web: 0.006,
    flange: 0.011,
    root_radius: 0.013,
};
const ZED: ParametricShape = ParametricShape::ZShape {
    height: 0.2,
    width: 0.15,
    web: 0.008,
    flange: 0.01,
};

/// CHS 1052 x 1: outer radius 0.526 m, bore radius 0.525 m.
const THIN_CHS: ParametricShape = ParametricShape::CircularHollow {
    diameter: 1.052,
    thickness: 0.001,
};
/// A square tube with fillets of 0.526 m outside and 0.525 m inside.
const THIN_RHS: ParametricShape = ParametricShape::RectangularHollow {
    width: 2.0,
    height: 2.0,
    thickness: 0.001,
    outer_radius: 0.526,
};

/// One shape of each kind with typical dimensions.
const TYPICAL: [ParametricShape; 9] = [RECTANGLE, CIRCLE, RHS, CHS, IPE_200, TEE, ANGLE, CHANNEL, ZED];

/// Shapes whose radii sit at the limits `check_parametric_shape` accepts,
/// or at zero, where tangent points meet corners or each other.
fn limits() -> Vec<ParametricShape> {
    vec![
        // Outer radius at most the thickness: sharp inner corners.
        ParametricShape::RectangularHollow {
            width: 0.2,
            height: 0.1,
            thickness: 0.01,
            outer_radius: 0.006,
        },
        ParametricShape::RectangularHollow {
            width: 0.2,
            height: 0.1,
            thickness: 0.01,
            outer_radius: 0.01,
        },
        // Outer radius half the smaller side: semicircular ends.
        ParametricShape::RectangularHollow {
            width: 0.2,
            height: 0.1,
            thickness: 0.01,
            outer_radius: 0.05,
        },
        // A square tube rounded into a ring.
        ParametricShape::RectangularHollow {
            width: 0.1,
            height: 0.1,
            thickness: 0.01,
            outer_radius: 0.05,
        },
        ParametricShape::RectangularHollow {
            width: 0.2,
            height: 0.1,
            thickness: 0.01,
            outer_radius: 0.0,
        },
        // Root radius filling the whole flange outstand.
        ParametricShape::IShape {
            height: 0.2,
            width: 0.1,
            web: 0.006,
            flange: 0.01,
            root_radius: 0.047,
        },
        // Root radius filling the whole web height (the arcs meet).
        ParametricShape::IShape {
            height: 0.1,
            width: 0.2,
            web: 0.006,
            flange: 0.01,
            root_radius: 0.04,
        },
        ParametricShape::IShape {
            height: 0.2,
            width: 0.1,
            web: 0.006,
            flange: 0.01,
            root_radius: 0.0,
        },
        ParametricShape::TShape {
            height: 0.1,
            width: 0.1,
            web: 0.01,
            flange: 0.01,
            root_radius: 0.045,
        },
        ParametricShape::UShape {
            height: 0.2,
            width: 0.08,
            web: 0.006,
            flange: 0.011,
            root_radius: 0.074,
        },
        ParametricShape::UShape {
            height: 0.1,
            width: 0.1,
            web: 0.006,
            flange: 0.011,
            root_radius: 0.039,
        },
        // Toe radius equal to the thickness: the arc ends at the outer
        // corner of the toe.
        ParametricShape::LShape {
            height: 0.1,
            width: 0.1,
            thickness: 0.01,
            root_radius: 0.012,
            toe_radius: 0.01,
        },
        // Root and toe radii together the whole inner face of a leg.
        ParametricShape::LShape {
            height: 0.1,
            width: 0.08,
            thickness: 0.01,
            root_radius: 0.06,
            toe_radius: 0.01,
        },
        ParametricShape::LShape {
            height: 0.1,
            width: 0.1,
            thickness: 0.01,
            root_radius: 0.0,
            toe_radius: 0.0,
        },
        // A radius whose arc is shorter than the shortest edge.
        ParametricShape::IShape {
            height: 0.2,
            width: 0.1,
            web: 0.006,
            flange: 0.01,
            root_radius: MIN_EDGE_LENGTH / 4.0,
        },
        // A radius whose segments would be shorter than the shortest edge.
        ParametricShape::LShape {
            height: 0.1,
            width: 0.1,
            thickness: 0.01,
            root_radius: 3.0 * MIN_EDGE_LENGTH,
            toe_radius: 2.0 * MIN_EDGE_LENGTH,
        },
        ParametricShape::Circle {
            diameter: 4.0 * MIN_EDGE_LENGTH,
        },
        // Root radius filling the whole web height of a T: the arc ends at
        // the web's bottom corner. Binary fractions, so the limit is exact.
        ParametricShape::TShape {
            height: 0.0625,
            width: 0.25,
            web: 0.015625,
            flange: 0.015625,
            root_radius: 0.046875,
        },
        // Root radius filling the inner face of both legs, no toe radius.
        ParametricShape::LShape {
            height: 0.125,
            width: 0.125,
            thickness: 0.03125,
            root_radius: 0.09375,
            toe_radius: 0.0,
        },
        // Thin walls around radii on either side of 0.5256 m, where the
        // chord rule starts to need more than the angle rule's 9 segments
        // per quarter.
        THIN_CHS,
        THIN_RHS,
        // The largest shapes the validation accepts.
        ParametricShape::Circle { diameter: 10.0 },
        ParametricShape::CircularHollow {
            diameter: 10.0,
            thickness: 0.01,
        },
    ]
}

/// Tessellation settings from coarse to fine, including the extremes
/// `SetSettings` accepts.
fn settings() -> Vec<Tessellation> {
    let t = |chord_deviation, max_angle_deg| Tessellation {
        chord_deviation,
        max_angle_deg,
    };
    vec![defaults(), t(1.0, 90.0), t(0.0001, 1.0), t(0.002, 45.0), t(1e-9, 10.0)]
}

/// PRF-005: a rectangle is its four corners, so its area is w x h exactly.
#[test]
fn a_rectangle_has_the_exact_area() {
    let section = section(RECTANGLE);
    assert_eq!(outer(&section).points.len(), 4);
    assert!(relative(section.area(), 0.3 * 0.5) < 1e-12);
}

/// PRF-005, IFX-034: circles and circular hollow sections are inscribed
/// regular polygons with the segment count of the IFX-034 rule; at the
/// default 2 mm / 10° settings they lie within 0.6 % of the true area (36
/// segments give 0.51 %).
#[test]
fn circles_are_inscribed_polygons() {
    let r = 0.15;
    let circle = section(CIRCLE);
    let n = outer(&circle).points.len();
    assert_eq!(n, 4 * arc_segments(r, FRAC_PI_2, &defaults()));
    assert!(relative(circle.area(), inscribed(n, r)) < 1e-12);
    assert!(relative(circle.area(), PI * r * r) < 0.006);

    let (ro, ri) = (0.1683 / 2.0, 0.1683 / 2.0 - 0.008);
    let chs = section(CHS);
    let region = &chs.regions[0];
    assert_eq!(region.holes.len(), 1);
    let (n_outer, n_inner) = (region.outer.points.len(), region.holes[0].points.len());
    assert_eq!(n_outer, 36);
    assert_eq!(n_inner, 36);
    let polygon = inscribed(n_outer, ro) - inscribed(n_inner, ri);
    assert!(relative(chs.area(), polygon) < 1e-12);
    assert!(relative(chs.area(), PI * (ro * ro - ri * ri)) < 0.006);

    // A large circle: the chord rule dominates.
    let large = section(ParametricShape::Circle { diameter: 2.0 });
    let n = outer(&large).points.len();
    assert_eq!(n, 52);
    assert!(relative(large.area(), inscribed(n, 1.0)) < 1e-12);
}

/// PRF-005: the filleted shapes match their closed-form areas within 0.2 %
/// (each quarter-circle fillet adds or removes (1 - π/4) r²), and the
/// sharp-cornered Z exactly.
#[test]
fn filleted_shapes_match_their_closed_form_areas() {
    let check = |shape: ParametricShape, expected: f64, bound: f64| {
        let area = section(shape).area();
        assert!(
            relative(area, expected) < bound,
            "{}: {area} against {expected}",
            shape.code()
        );
    };
    // RHS: w*h - (w-2t)(h-2t) - (4-π)(ro² - ri²) with ri = ro - t.
    let (w, h, t, ro) = (0.2, 0.1, 0.008, 0.016);
    let ri = ro - t;
    check(
        RHS,
        w * h - (w - 2.0 * t) * (h - 2.0 * t) - 4.0 * (fillet(ro) - fillet(ri)),
        0.002,
    );
    // An inner radius clamped at zero.
    let rhs = ParametricShape::RectangularHollow {
        width: w,
        height: h,
        thickness: 0.01,
        outer_radius: 0.006,
    };
    check(rhs, w * h - (w - 0.02) * (h - 0.02) - 4.0 * fillet(0.006), 0.002);

    // I/H: 2*b*tf + (h-2tf)*tw + (4-π)*r².
    let (h, b, tw, tf, r) = (0.2, 0.1, 0.0056, 0.0085, 0.012);
    check(IPE_200, 2.0 * b * tf + (h - 2.0 * tf) * tw + 4.0 * fillet(r), 0.002);
    // T: b*tf + (h-tf)*tw + 2 fillets.
    let (h, b, tw, tf, r) = (0.1, 0.1, 0.011, 0.011, 0.011);
    check(TEE, b * tf + (h - tf) * tw + 2.0 * fillet(r), 0.002);
    // U/C: 2*b*tf + (h-2tf)*tw + 2 fillets.
    let (h, b, tw, tf, r) = (0.2, 0.08, 0.006, 0.011, 0.013);
    check(CHANNEL, 2.0 * b * tf + (h - 2.0 * tf) * tw + 2.0 * fillet(r), 0.002);
    // L: t(b + h - t) + the root fillet - the two toe fillets.
    let (h, b, t, r1, r2) = (0.12, 0.08, 0.01, 0.013, 0.0065);
    check(ANGLE, t * (b + h - t) + fillet(r1) - 2.0 * fillet(r2), 0.002);
    // Z: both flanges (b + tw)/2 long, the web between them.
    let (h, b, tw, tf) = (0.2, 0.15, 0.008, 0.01);
    check(ZED, tf * (b + tw) + (h - 2.0 * tf) * tw, 1e-12);
}

/// PRF-005: IPE 200 has 28.48 cm², within 0.5 % of the 28.5 cm² that
/// EN 10365 tabulates. 28.48 cm² is the exact area of the shape; at the
/// default 2 mm / 10° the chords of the four concave root fillets add
/// 0.08 % (28.51 cm²), and at 1° per segment the section gives 28.48 cm².
#[test]
fn ipe_200_has_the_tabulated_area() {
    let (h, b, tw, tf, r) = (0.2, 0.1, 0.0056, 0.0085, 0.012);
    let exact = (2.0 * b * tf + (h - 2.0 * tf) * tw + 4.0 * fillet(r)) * 1e4;
    assert!((exact - 28.48).abs() < 0.005, "{exact}");

    let cm2 = section(IPE_200).area() * 1e4;
    assert!(relative(cm2, exact) < 0.001, "{cm2}");
    assert!(relative(cm2, 28.5) < 0.005, "{cm2}");

    let fine = Tessellation {
        max_angle_deg: 1.0,
        ..defaults()
    };
    let cm2 = section_with(IPE_200, &fine).area() * 1e4;
    assert!((cm2 - 28.48).abs() < 0.005, "{cm2}");
}

/// The cleaning `clean_ring` and `validate_ring` would do, done already:
/// every loop is closed without a repeated last point, has no edge shorter
/// than MIN_EDGE_LENGTH and passes `validate_ring` as it is, outer loops
/// counter-clockwise and holes clockwise, and every hole lies inside its
/// outer loop.
fn assert_valid(section: &Section, what: &str) {
    for region in &section.regions {
        let all = std::iter::once(&region.outer).chain(&region.holes);
        for ring in all {
            assert_eq!(ring.points.len(), ring.smooth.len(), "{what}");
            assert_eq!(clean_ring(&ring.points), ring.points, "{what}: not clean");
            let n = ring.points.len();
            for i in 0..n {
                let edge = ring.points[i].distance(ring.points[(i + 1) % n]);
                assert!(edge >= MIN_EDGE_LENGTH, "{what}: edge {i} is {edge} long");
            }
        }
        let outer = &region.outer.points;
        assert_eq!(validate_ring(outer).as_ref(), Ok(outer), "{what}: outer loop");
        assert!(region.outer.signed_area() > 0.0, "{what}");
        for hole in &region.holes {
            assert!(hole.signed_area() < 0.0, "{what}");
            let reversed: Vec<DVec2> = hole.points.iter().rev().copied().collect();
            assert_eq!(validate_ring(&hole.points), Ok(reversed), "{what}: hole");
            for &p in &hole.points {
                assert!(point_in_ring(p, outer), "{what}: hole point {p} outside");
            }
        }
        let holes: Vec<Vec<DVec2>> = region.holes.iter().map(|h| h.points.clone()).collect();
        assert_eq!(
            validate_slab_polygon(outer, &holes),
            Ok((outer.clone(), holes)),
            "{what}: holes"
        );
    }
}

fn point_in_ring(p: DVec2, ring: &[DVec2]) -> bool {
    let mut inside = false;
    let mut j = ring.len() - 1;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// PRF-005, EDT-018, EDT-019: every loop of every shape, at typical
/// dimensions and at the limits of the validation, under coarse and fine
/// tessellation, is a valid ring as it is; holes lie inside their outer
/// loop.
#[test]
fn every_loop_is_a_valid_ring() {
    for tessellation in settings() {
        for shape in TYPICAL.into_iter().chain(ParametricShape::ALL).chain(limits()) {
            let what = format!("{shape:?} at {tessellation:?}");
            assert_valid(&section_with(shape, &tessellation), &what);
        }
    }
}

/// IFX-034: the angle rule and the chord rule, whichever needs more
/// segments, and at least one.
#[test]
fn arc_segments_follow_the_angle_and_the_chord_rule() {
    let tessellation = defaults();
    // 90° / 10° = 9; the chord rule needs 2 for 12 mm.
    assert_eq!(arc_segments(0.012, FRAC_PI_2, &tessellation), 9);
    // 2 mm deviation on 1 m: 2 acos(0.998) = 7.25° per segment, 50 in all.
    assert_eq!(arc_segments(1.0, TAU, &tessellation), 50);
    assert_eq!(arc_segments(0.15, TAU, &tessellation), 36);
    // The sign of the sweep does not matter, and any arc has a segment.
    assert_eq!(arc_segments(0.012, -FRAC_PI_2, &tessellation), 9);
    assert_eq!(arc_segments(0.012, 0.0, &tessellation), 1);
    assert_eq!(arc_segments(0.0, FRAC_PI_2, &tessellation), 1);
    // A deviation larger than the radius leaves only the angle rule.
    assert_eq!(arc_segments(0.001, PI, &tessellation), 18);
}

/// IFX-034: the tolerances are adjustable per project, and changing the
/// project's settings changes the segment counts and the sections.
#[test]
fn the_project_tessellation_settings_change_the_counts() {
    let mut settings = ProjectSettings::default();
    let before = arc_segments(1.0, TAU, &settings.tessellation);
    let circle = section_with(CIRCLE, &settings.tessellation);
    settings.tessellation.max_angle_deg = 5.0;
    assert_eq!(arc_segments(0.012, FRAC_PI_2, &settings.tessellation), 18);
    assert_eq!(arc_segments(0.15, TAU, &settings.tessellation), 72);
    settings.tessellation.chord_deviation = 0.0005;
    assert!(arc_segments(1.0, TAU, &settings.tessellation) > before);
    assert_eq!(arc_segments(1.0, TAU, &settings.tessellation), 100);
    let finer = section_with(CIRCLE, &settings.tessellation);
    assert_eq!(outer(&circle).points.len(), 36);
    assert_eq!(outer(&finer).points.len(), 72);
    let ipe = |tessellation: &Tessellation| {
        let section = section_with(IPE_200, tessellation);
        outer(&section).points.len()
    };
    assert_eq!(ipe(&defaults()), 8 + 4 * 10);
    assert_eq!(ipe(&settings.tessellation), 8 + 4 * 19);
}

/// NFR-SEC-003: tessellation values a crafted file may carry neither
/// produce zero segments nor unbounded work.
#[test]
fn unusable_tessellation_values_stay_bounded() {
    let t = |chord_deviation, max_angle_deg| Tessellation {
        chord_deviation,
        max_angle_deg,
    };
    for tessellation in [
        t(0.0, 0.0),
        t(-1.0, -10.0),
        t(f64::NAN, f64::NAN),
        t(f64::MIN_POSITIVE, 1e-300),
        t(f64::INFINITY, f64::INFINITY),
    ] {
        for (radius, sweep) in [(1.0, TAU), (5.0, FRAC_PI_2), (0.012, FRAC_PI_2)] {
            let n = arc_segments(radius, sweep, &tessellation);
            assert!((1..=MAX_SEGMENTS_PER_TURN).contains(&n), "{tessellation:?}: {n}");
        }
        for shape in TYPICAL {
            assert_valid(&section_with(shape, &tessellation), &format!("{shape:?}"));
        }
    }
    assert_eq!(arc_segments(5.0, TAU, &t(1e-300, 10.0)), MAX_SEGMENTS_PER_TURN);
    assert_eq!(
        arc_segments(5.0, FRAC_PI_2, &t(1e-300, 10.0)),
        MAX_SEGMENTS_PER_TURN / 4
    );
}

/// What a parametric shape is made of: its sharp corners and its fillets
/// (centre and radius), in profile coordinates.
struct Features {
    corners: Vec<DVec2>,
    fillets: Vec<(DVec2, f64)>,
}

/// The features of the typical filleted shapes, from the orientation
/// conventions: I/H symmetric, T flange on top, U/C web on the left, L heel
/// at the bottom left with the toe radii on the inner edge of each toe.
fn features(shape: ParametricShape) -> Features {
    let mirror = |p: DVec2| [p, v(-p.x, p.y), v(-p.x, -p.y), v(p.x, -p.y)];
    match shape {
        ParametricShape::RectangularHollow {
            width,
            height,
            thickness,
            outer_radius,
        } => {
            let (x, y) = (width / 2.0, height / 2.0);
            let ri = outer_radius - thickness;
            let centre = v(x - outer_radius, y - outer_radius);
            let mut fillets: Vec<_> = mirror(centre).map(|c| (c, outer_radius)).to_vec();
            fillets.extend(mirror(centre).map(|c| (c, ri)));
            Features {
                corners: Vec::new(),
                fillets,
            }
        }
        ParametricShape::IShape {
            height,
            width,
            web,
            flange,
            root_radius: r,
        } => {
            let (x, y, w, f) = (width / 2.0, height / 2.0, web / 2.0, height / 2.0 - flange);
            let mut corners = mirror(v(x, y)).to_vec();
            corners.extend(mirror(v(x, f)));
            Features {
                corners,
                fillets: mirror(v(w + r, f - r)).map(|c| (c, r)).to_vec(),
            }
        }
        ParametricShape::TShape {
            height,
            width,
            web,
            flange,
            root_radius: r,
        } => {
            let (x, y, w, f) = (width / 2.0, height / 2.0, web / 2.0, height / 2.0 - flange);
            Features {
                corners: vec![v(-w, -y), v(w, -y), v(x, f), v(x, y), v(-x, y), v(-x, f)],
                fillets: vec![(v(w + r, f - r), r), (v(-w - r, f - r), r)],
            }
        }
        ParametricShape::UShape {
            height,
            width,
            web,
            flange,
            root_radius: r,
        } => {
            let (x, y, f) = (width / 2.0, height / 2.0, height / 2.0 - flange);
            let inner = -x + web;
            Features {
                corners: vec![v(-x, -y), v(x, -y), v(x, -f), v(x, f), v(x, y), v(-x, y)],
                fillets: vec![(v(inner + r, -f + r), r), (v(inner + r, f - r), r)],
            }
        }
        ParametricShape::LShape {
            height,
            width,
            thickness: t,
            root_radius: r1,
            toe_radius: r2,
        } => {
            let (x, y) = (width / 2.0, height / 2.0);
            let (ix, iy) = (-x + t, -y + t);
            Features {
                corners: vec![v(-x, -y), v(x, -y), v(-x, y)],
                fillets: vec![
                    (v(x - r2, iy - r2), r2),
                    (v(ix + r1, iy + r1), r1),
                    (v(ix - r2, y - r2), r2),
                ],
            }
        }
        _ => unreachable!("{shape:?} has no fillets"),
    }
}

/// IFX-038: the smooth flags are true exactly on arc vertices, the tangent
/// points included, and false on sharp corners. A fillet of radius r has
/// arc_segments(r, 90°) + 1 vertices on its arc.
#[test]
fn smooth_flags_mark_exactly_the_arc_vertices() {
    let tessellation = defaults();
    for shape in [RHS, IPE_200, TEE, CHANNEL, ANGLE] {
        let section = section(shape);
        let Features { corners, fillets } = features(shape);
        let region = &section.regions[0];
        let mut on_fillet = vec![0; fillets.len()];
        let mut hard = Vec::new();
        for ring in std::iter::once(&region.outer).chain(&region.holes) {
            for (&p, &smooth) in ring.points.iter().zip(&ring.smooth) {
                if smooth {
                    let j = fillets
                        .iter()
                        .position(|&(c, r)| (p.distance(c) - r).abs() < 1e-12)
                        .unwrap_or_else(|| panic!("{}: smooth {p} on no fillet", shape.code()));
                    on_fillet[j] += 1;
                } else {
                    hard.push(p);
                }
            }
        }
        for (j, &(_, r)) in fillets.iter().enumerate() {
            assert_eq!(
                on_fillet[j],
                arc_segments(r, FRAC_PI_2, &tessellation) + 1,
                "{}: fillet {j}",
                shape.code()
            );
        }
        assert_eq!(hard.len(), corners.len(), "{}: {hard:?}", shape.code());
        for corner in corners {
            assert!(
                hard.iter().any(|p| p.distance(corner) < 1e-12),
                "{}: {corner} is not a hard vertex",
                shape.code()
            );
        }
    }
    // Circles are smooth all round; the rectangle and the Z are all corners.
    for shape in [CIRCLE, CHS] {
        let section = section(shape);
        let region = &section.regions[0];
        assert!(
            region
                .outer
                .smooth
                .iter()
                .chain(region.holes.iter().flat_map(|h| &h.smooth))
                .all(|&s| s)
        );
    }
    for shape in [RECTANGLE, ZED] {
        assert!(outer(&section(shape)).smooth.iter().all(|&s| !s));
    }
}

/// IFX-038: where a radius sits at its limit, a tangent point meets a sharp
/// corner, which stays hard, or the tangent point of the next arc, which
/// stays smooth; a radius too small to draw is a sharp corner.
#[test]
fn merged_tangent_points_keep_the_right_flag() {
    // Semicircular ends: the arcs of each end meet at the end's middle.
    let rhs = section(ParametricShape::RectangularHollow {
        width: 0.2,
        height: 0.1,
        thickness: 0.01,
        outer_radius: 0.05,
    });
    let region = &rhs.regions[0];
    assert!(region.outer.smooth.iter().all(|&s| s));
    assert!(region.holes[0].smooth.iter().all(|&s| s));
    let ends = region.outer.points.iter().filter(|p| p.y.abs() < 1e-12).count();
    assert_eq!(ends, 2, "one vertex at the middle of each end");

    // The toe arc ends at the outer corner of the toe, which stays hard.
    let angle = section(ParametricShape::LShape {
        height: 0.1,
        width: 0.1,
        thickness: 0.01,
        root_radius: 0.012,
        toe_radius: 0.01,
    });
    let ring = outer(&angle);
    let hard: Vec<DVec2> = ring
        .points
        .iter()
        .zip(&ring.smooth)
        .filter(|(_, s)| !**s)
        .map(|(p, _)| *p)
        .collect();
    assert_eq!(hard.len(), 3, "{hard:?}");
    for corner in [v(-0.05, -0.05), v(0.05, -0.05), v(-0.05, 0.05)] {
        assert!(hard.iter().any(|p| p.distance(corner) < 1e-12), "{corner}");
    }

    // A root radius below the shortest edge leaves the corner sharp.
    let tiny = section(ParametricShape::IShape {
        height: 0.2,
        width: 0.1,
        web: 0.006,
        flange: 0.01,
        root_radius: MIN_EDGE_LENGTH / 4.0,
    });
    let ring = outer(&tiny);
    assert_eq!(ring.points.len(), 12);
    assert!(ring.smooth.iter().all(|&s| !s));
}

/// PRF-005: every section spans exactly its bounding box,
/// (-b/2, -h/2)..(b/2, h/2), a circle's the diameter both ways.
#[test]
fn bounds_are_the_bounding_box_of_the_dimensions() {
    for tessellation in settings() {
        for shape in TYPICAL.into_iter().chain(ParametricShape::ALL).chain(limits()) {
            let half = shape.size() / 2.0;
            assert_eq!(
                section_with(shape, &tessellation).bounds(),
                Some((-half, half)),
                "{shape:?} at {tessellation:?}"
            );
        }
    }
    assert_eq!(Section::default().bounds(), None);
}

/// PRF-005: a parametric section is one region of the given material with
/// the role Body; the same input gives the same section.
#[test]
fn a_parametric_section_is_one_body_region() {
    let material = MaterialId::new();
    for shape in TYPICAL {
        let section = parametric_section(&shape, material, &defaults()).unwrap();
        assert_eq!(section.regions.len(), 1);
        assert_eq!(section.regions[0].material, material);
        assert_eq!(section.regions[0].role, BODY_ROLE);
        assert_eq!(BODY_ROLE, "Body");
        assert_eq!(parametric_section(&shape, material, &defaults()).unwrap(), section);
    }
    // Shapes without holes have none; hollow sections have one.
    for (shape, holes) in [(RECTANGLE, 0), (CIRCLE, 0), (RHS, 1), (CHS, 1), (IPE_200, 0), (ZED, 0)] {
        assert_eq!(section(shape).regions[0].holes.len(), holes, "{}", shape.code());
    }
}

/// UI-012: a shape the validation rejects has no section, and the error
/// carries the validation's message.
#[test]
fn an_invalid_shape_has_no_section() {
    let shape = ParametricShape::IShape {
        height: 0.2,
        width: 0.1,
        web: 0.2,
        flange: 0.01,
        root_radius: 0.0,
    };
    let error = parametric_section(&shape, MaterialId::new(), &defaults()).unwrap_err();
    let GeometryError::InvalidPolygon(reason) = error else {
        panic!("{error:?}");
    };
    assert_eq!(reason.key(), "model-error-profile-dimension-limit");
    let negative = ParametricShape::Circle { diameter: -1.0 };
    assert!(matches!(
        parametric_section(&negative, MaterialId::new(), &defaults()),
        Err(GeometryError::InvalidPolygon(m)) if m.key() == "model-error-profile-dimension-range"
    ));
    // A bore narrower than the shortest edge is no ring.
    let pinhole = ParametricShape::CircularHollow {
        diameter: 0.1,
        thickness: 0.05 - MIN_EDGE_LENGTH / 4.0,
    };
    assert!(matches!(
        parametric_section(&pinhole, MaterialId::new(), &defaults()),
        Err(GeometryError::InvalidPolygon(m)) if m.key() == "model-error-hole"
    ));
}

/// EDT-018: a hole takes the segment counts of the outer loop's arcs about
/// the same centres, so its vertices lie on the same rays and it stays
/// inside however thin the wall. With counts of its own, the bore of a CHS
/// 1052 x 1 would have 9 segments per quarter against the outer ring's 10,
/// and its vertex at 20° would lie 0.12 mm outside the outer ring.
#[test]
fn thin_walls_keep_the_hole_inside() {
    let tessellation = defaults();
    assert_eq!(arc_segments(0.526, FRAC_PI_2, &tessellation), 10);
    assert_eq!(arc_segments(0.525, FRAC_PI_2, &tessellation), 9);
    // Four quarter circles of 10 segments; four fillets of 11 points.
    for (shape, points) in [(THIN_CHS, 40), (THIN_RHS, 44)] {
        let section = section(shape);
        assert_valid(&section, shape.code());
        let region = &section.regions[0];
        let outer = &region.outer.points;
        let hole: Vec<DVec2> = region.holes[0].points.iter().rev().copied().collect();
        assert_eq!(outer.len(), points, "{}", shape.code());
        assert_eq!(hole.len(), outer.len(), "{}", shape.code());
        // Counter-clockwise, each hole vertex lies one wall thickness
        // inside the outer vertex of the same index.
        for (q, p) in outer.iter().zip(&hole) {
            assert!((q.distance(*p) - 0.001).abs() < 1e-12, "{}: {q} {p}", shape.code());
        }
    }
}
