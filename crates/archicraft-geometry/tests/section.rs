//! Cross-sections of the parametric standard profiles (PRF-005): closed
//! polygons with holes, curved edges tessellated by the IFX-034 rule and
//! smooth vertices marked for IFX-038.

#![allow(clippy::unwrap_used)]

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use archicraft_geometry::GeometryError;
use archicraft_geometry::section::{
    BODY_ROLE, MAX_SEGMENTS_PER_TURN, Section, SectionLoop, arc_segments, parametric_section, profile_section,
};
use archicraft_model::glam::DVec2;
use archicraft_model::message::Arg;
use archicraft_model::outline::Ring;
use archicraft_model::profile::{ParametricShape, Profile, ProfileDimension, ProfileRegion};
use archicraft_model::project::{ProjectSettings, Tessellation};
use archicraft_model::validate::{
    MAX_CHORD_DEVIATION, MAX_PROFILE_DIMENSION, MAX_SEGMENT_ANGLE_DEG, MIN_CHORD_DEVIATION, MIN_EDGE_LENGTH,
    MIN_PROFILE_DIMENSION, MIN_SEGMENT_ANGLE_DEG, check_parametric_shape, clean_ring, validate_ring,
    validate_slab_polygon,
};
use archicraft_model::{MaterialId, Message};

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
    use ProfileDimension as D;
    let mut shapes = vec![
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
    ];
    // Plates leaving the smallest gap the validation accepts,
    // MIN_PROFILE_DIMENSION, to the last bit: the bore of a CHS, small and
    // in a large tube, the hole of an RHS, sharp and rounded into a slot or
    // a circle, the web heights and flange outstands of I/H, T, U/C and Z,
    // and an angle's inner leg face; then the radius filling the gap.
    let rhs = |width, height, outer_radius| ParametricShape::RectangularHollow {
        width,
        height,
        thickness: 0.01,
        outer_radius,
    };
    let at_gap = [
        (CHS, D::Thickness),
        (THIN_CHS, D::Thickness),
        (
            ParametricShape::CircularHollow {
                diameter: 10.0,
                thickness: 0.01,
            },
            D::Thickness,
        ),
        (rhs(0.2, 0.1, 0.0), D::Thickness),
        (rhs(0.2, 0.1, 0.05), D::Thickness),
        (rhs(0.1, 0.1, 0.05), D::Thickness),
        (IPE_200, D::Flange),
        (IPE_200, D::Web),
        (TEE, D::Flange),
        (TEE, D::Web),
        (CHANNEL, D::Flange),
        (CHANNEL, D::Web),
        (ZED, D::Flange),
        (ZED, D::Web),
        (ANGLE, D::Thickness),
    ];
    for (shape, plate) in at_gap {
        let thick = accepted_limit(shape, plate, MAX_PROFILE_DIMENSION);
        shapes.push(thick);
        for (radius, _) in thick.dimensions().into_iter().filter(|(d, _)| d.is_radius()) {
            shapes.push(accepted_limit(thick, radius, MAX_PROFILE_DIMENSION));
        }
    }
    shapes
}

/// Tessellation settings from coarse to fine, including the extremes
/// `SetSettings` accepts.
fn settings() -> Vec<Tessellation> {
    let t = |chord_deviation, max_angle_deg| Tessellation {
        chord_deviation,
        max_angle_deg,
    };
    vec![
        defaults(),
        t(MAX_CHORD_DEVIATION, MAX_SEGMENT_ANGLE_DEG),
        t(0.0001, 1.0),
        t(0.002, 45.0),
        t(MIN_CHORD_DEVIATION, MIN_SEGMENT_ANGLE_DEG),
    ]
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
            let ring = Ring::<()>::from_points(outer.iter().copied());
            for &p in &hole.points {
                assert!(ring.contains(p), "{what}: hole point {p} outside");
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

/// The value of `dimension` in `shape` that `check_parametric_shape`
/// accepts nearest `towards`, by bisection from the accepted value it has:
/// the limit itself to the last bit, since the accepted values of a
/// dimension are an interval.
fn accepted_limit(shape: ParametricShape, dimension: ProfileDimension, towards: f64) -> ParametricShape {
    let with = |value: f64| {
        let mut shape = shape;
        *shape.dimension_mut(dimension).unwrap() = value;
        shape
    };
    let accepted = |value: f64| check_parametric_shape(&with(value)).is_ok();
    let mut good = *{ shape }.dimension_mut(dimension).unwrap();
    assert!(accepted(good), "{shape:?}");
    if accepted(towards) {
        return with(towards);
    }
    let mut bad = towards;
    loop {
        let mid = good + (bad - good) / 2.0;
        if mid == good || mid == bad {
            return with(good);
        }
        if accepted(mid) {
            good = mid;
        } else {
            bad = mid;
        }
    }
}

/// PRF-005, EDT-018: every shape the validation accepts has a section, also
/// at the limits of its dimensions: from each typical, default and limit
/// shape, each dimension in turn moved to the largest and the smallest
/// value accepted, to the last bit, and from there each radius to its
/// largest. Plates then leave gaps of MIN_PROFILE_DIMENSION, radii fill
/// those gaps, and the shape's checks meet in every combination of one
/// dimension and one radius. A smallest value that is the range minimum
/// itself, MIN_PROFILE_DIMENSION, is left out: a circle of that diameter
/// and some plates of that thickness have no section yet (archicraft-ymqhf).
#[test]
fn every_accepted_limit_has_a_section() {
    let tessellations = [
        defaults(),
        Tessellation {
            chord_deviation: MIN_CHORD_DEVIATION,
            max_angle_deg: MIN_SEGMENT_ANGLE_DEG,
        },
    ];
    let mut checked = 0;
    for start in TYPICAL.into_iter().chain(ParametricShape::ALL).chain(limits()) {
        for (dimension, _) in start.dimensions() {
            let min = if dimension.is_radius() {
                0.0
            } else {
                MIN_PROFILE_DIMENSION
            };
            for towards in [MAX_PROFILE_DIMENSION, min] {
                let edge = accepted_limit(start, dimension, towards);
                if towards == min && min > 0.0 && *{ edge }.dimension_mut(dimension).unwrap() == min {
                    continue;
                }
                let radii = edge.dimensions().into_iter().filter(|(d, _)| d.is_radius());
                let shapes = std::iter::once(edge)
                    .chain(radii.map(|(radius, _)| accepted_limit(edge, radius, MAX_PROFILE_DIMENSION)));
                for shape in shapes {
                    for tessellation in &tessellations {
                        let section = parametric_section(&shape, MaterialId::new(), tessellation)
                            .unwrap_or_else(|e| panic!("{shape:?} at {tessellation:?}: {e}"));
                        assert_valid(&section, &format!("{shape:?} at {tessellation:?}"));
                        checked += 1;
                    }
                }
            }
        }
    }
    assert!(checked > 1000, "{checked}");
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

/// IFX-034: the segment cap and the smallest tolerances `SetSettings`
/// accepts agree: at either minimum the largest circle a profile may have
/// (10 m across) needs no more than MAX_SEGMENTS_PER_TURN segments, so the
/// cap never overrides an accepted setting.
#[test]
fn the_smallest_accepted_tolerances_stay_below_the_cap() {
    let radius = MAX_PROFILE_DIMENSION / 2.0;
    let t = |chord_deviation, max_angle_deg| Tessellation {
        chord_deviation,
        max_angle_deg,
    };
    let by_chord = arc_segments(radius, TAU, &t(MIN_CHORD_DEVIATION, MAX_SEGMENT_ANGLE_DEG));
    let by_angle = arc_segments(radius, TAU, &t(MAX_CHORD_DEVIATION, MIN_SEGMENT_ANGLE_DEG));
    let both = arc_segments(radius, TAU, &t(MIN_CHORD_DEVIATION, MIN_SEGMENT_ANGLE_DEG));
    assert_eq!((by_chord, by_angle, both), (994, 720, 994));
    assert!(both <= MAX_SEGMENTS_PER_TURN);
    // The chord rule itself, not the cap, gives the count: the chords lie
    // within the minimum deviation.
    let half_angle = PI / by_chord as f64;
    assert!(radius * (1.0 - half_angle.cos()) <= MIN_CHORD_DEVIATION);
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
/// (centre, radius and the radius whose segment count the fillet takes), in
/// profile coordinates.
struct Features {
    corners: Vec<DVec2>,
    fillets: Vec<(DVec2, f64, f64)>,
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
            let mut fillets: Vec<_> = mirror(centre).map(|c| (c, outer_radius, outer_radius)).to_vec();
            // The hole's fillets take the outer fillets' counts.
            fillets.extend(mirror(centre).map(|c| (c, ri, outer_radius)));
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
                fillets: mirror(v(w + r, f - r)).map(|c| (c, r, r)).to_vec(),
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
                fillets: vec![(v(w + r, f - r), r, r), (v(-w - r, f - r), r, r)],
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
                fillets: vec![(v(inner + r, -f + r), r, r), (v(inner + r, f - r), r, r)],
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
                    (v(x - r2, iy - r2), r2, r2),
                    (v(ix + r1, iy + r1), r1, r1),
                    (v(ix - r2, y - r2), r2, r2),
                ],
            }
        }
        _ => unreachable!("{shape:?} has no fillets"),
    }
}

/// IFX-038: the smooth flags are true exactly on arc vertices, the tangent
/// points included, and false on sharp corners. A fillet of radius r has
/// arc_segments(r, 90°) + 1 vertices on its arc; a fillet of a hole takes
/// the count of the outer loop's fillet about the same centre, so it has
/// arc_segments(ro, 90°) + 1.
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
                        .position(|&(c, r, _)| (p.distance(c) - r).abs() < 1e-12)
                        .unwrap_or_else(|| panic!("{}: smooth {p} on no fillet", shape.code()));
                    on_fillet[j] += 1;
                } else {
                    hard.push(p);
                }
            }
        }
        for (j, &(_, _, count_radius)) in fillets.iter().enumerate() {
            assert_eq!(
                on_fillet[j],
                arc_segments(count_radius, FRAC_PI_2, &tessellation) + 1,
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

/// PRF-005, EDT-018: a shape the validation rejects has no section, and the error
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
    // A bore narrower than the smallest gap is rejected naming the wall,
    // not as a hole that is no ring.
    let pinhole = ParametricShape::CircularHollow {
        diameter: 0.1,
        thickness: 0.05 - MIN_EDGE_LENGTH / 4.0,
    };
    assert!(matches!(
        parametric_section(&pinhole, MaterialId::new(), &defaults()),
        Err(GeometryError::InvalidPolygon(m)) if m.key() == "model-error-profile-dimension-limit"
            && m.arg("dimension") == Some(&Arg::Nested(Box::new(Message::new("model-profile-dimension-thickness"))))
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

fn square(min: DVec2, side: f64) -> Vec<DVec2> {
    vec![min, min + v(side, 0.0), min + v(side, side), min + v(0.0, side)]
}

/// A custom profile of two touching regions: a 0.2 m square with a
/// clockwise 0.1 m square hole, and a 0.1 m square to its right.
fn two_regions(first: MaterialId, second: MaterialId) -> Profile {
    let mut hole = square(v(-0.05, -0.05), 0.1);
    hole.reverse();
    Profile::custom(
        "Two regions",
        vec![
            ProfileRegion {
                outer: square(v(-0.1, -0.1), 0.2),
                holes: vec![hole],
                material: first,
            },
            ProfileRegion {
                outer: square(v(0.1, -0.1), 0.1),
                holes: Vec::new(),
                material: second,
            },
        ],
    )
}

fn sections_of(profile: &Profile) -> Section {
    profile_section(profile, &defaults()).unwrap_or_else(|e| panic!("{}: {e}", profile.name))
}

/// PRF-004, PRF-005: the section of a parametric profile is the parametric
/// section of its shape and material; a custom profile keeps its regions in
/// order with their materials and holes, all vertices hard, the roles
/// Region_1, Region_2, ... for several regions and Body for one.
#[test]
fn a_profile_section_is_its_shape_or_its_regions() {
    let material = MaterialId::new();
    for shape in TYPICAL {
        let profile = Profile::parametric(shape.code(), shape, material);
        assert_eq!(
            profile_section(&profile, &defaults()),
            parametric_section(&shape, material, &defaults()),
            "{}",
            shape.code()
        );
    }

    let (first, second) = (MaterialId::new(), MaterialId::new());
    let profile = two_regions(first, second);
    let section = sections_of(&profile);
    let archicraft_model::profile::ProfileShape::Custom { regions } = &profile.shape else {
        unreachable!()
    };
    assert_eq!(section.regions.len(), 2);
    for (i, (actual, given)) in section.regions.iter().zip(regions).enumerate() {
        assert_eq!(actual.role, format!("Region_{}", i + 1));
        assert_eq!(actual.material, given.material);
        assert_eq!(actual.outer.points, given.outer);
        let holes: Vec<Vec<DVec2>> = actual.holes.iter().map(|h| h.points.clone()).collect();
        assert_eq!(holes, given.holes);
        for ring in std::iter::once(&actual.outer).chain(&actual.holes) {
            assert_eq!(ring.smooth, vec![false; ring.points.len()]);
        }
    }
    assert_eq!(section.regions[0].material, first);
    assert_eq!(section.regions[1].material, second);
    assert_eq!(section.regions[0].holes.len(), 1);
    assert_valid(&section, &profile.name);
    assert!(relative(section.area(), 0.04 - 0.01 + 0.01) < 1e-12);

    let one = Profile::custom(
        "One region",
        vec![ProfileRegion {
            outer: square(v(0.0, 0.0), 0.3),
            holes: Vec::new(),
            material,
        }],
    );
    let section = sections_of(&one);
    assert_eq!(section.regions.len(), 1);
    assert_eq!(section.regions[0].role, BODY_ROLE);
}

/// PRF-004: a custom profile that is not a valid set of regions has no
/// section; one whose rings run the wrong way is normalised, outer loops
/// counter-clockwise and holes clockwise.
#[test]
fn custom_regions_are_checked_and_normalised() {
    let empty = Profile::custom("Empty", Vec::new());
    assert!(matches!(
        profile_section(&empty, &defaults()),
        Err(GeometryError::InvalidPolygon(m)) if m.key() == "model-error-profile-no-regions"
    ));
    let line = Profile::custom(
        "Line",
        vec![ProfileRegion {
            outer: vec![v(0.0, 0.0), v(1.0, 0.0), v(2.0, 0.0)],
            holes: Vec::new(),
            material: MaterialId::new(),
        }],
    );
    assert!(matches!(
        profile_section(&line, &defaults()),
        Err(GeometryError::InvalidPolygon(m)) if m.key() == "model-error-profile-region"
    ));

    let mut outer = square(v(-0.1, -0.1), 0.2);
    outer.reverse();
    let reversed = Profile::custom(
        "Reversed",
        vec![ProfileRegion {
            outer,
            holes: vec![square(v(-0.05, -0.05), 0.1)],
            material: MaterialId::new(),
        }],
    );
    let section = sections_of(&reversed);
    assert_valid(&section, "reversed");
    assert!(relative(section.area(), 0.03) < 1e-12);
}

/// PRF-004, PRF-005: the section spans the profile's bounding box, for
/// every parametric shape and for custom regions.
#[test]
fn profile_sections_span_the_profile_bounds() {
    let material = MaterialId::new();
    let profiles = TYPICAL
        .into_iter()
        .chain(ParametricShape::ALL)
        .map(|shape| Profile::parametric(shape.code(), shape, material))
        .chain([two_regions(material, material)]);
    for profile in profiles {
        assert_eq!(sections_of(&profile).bounds(), profile.bounds(), "{}", profile.name);
    }
}

/// COL-001: the nine anchor points of a cross-section are the 3 x 3 points
/// of its bounding box in row-major order from the top left: 0 is the top
/// left corner, 4 the centre, 8 the bottom right corner.
#[test]
fn nine_points_run_row_major_from_the_top_left() {
    let custom = sections_of(&two_regions(MaterialId::new(), MaterialId::new()));
    // The bounds are (-0.1, -0.1)..(0.2, 0.1).
    let (xs, ys) = ([-0.1, 0.05, 0.2], [0.1, 0.0, -0.1]);
    for i in 0..9 {
        let expected = v(xs[i % 3], ys[i / 3]);
        let point = custom.nine_point(i).unwrap();
        assert!(point.distance(expected) < 1e-15, "{i}: {point} against {expected}");
    }
    assert_eq!(custom.nine_point(0), Some(v(-0.1, 0.1)));
    assert_eq!(custom.nine_point(8), Some(v(0.2, -0.1)));
    assert_eq!(custom.nine_point(9), None);
    assert_eq!(Section::default().nine_point(4), None);

    let ipe = section(IPE_200);
    assert_eq!(ipe.nine_point(4), Some(DVec2::ZERO));
    assert_eq!(ipe.nine_point(2), Some(v(0.05, 0.1)));
    assert_eq!(ipe.nine_point(3), Some(v(-0.05, 0.0)));
}

/// The centroid of the region between a convex corner at the origin and a
/// quarter circle of radius `r` tangent to both edges lies `spandrel(r)`
/// from the corner along each edge.
fn spandrel(r: f64) -> f64 {
    r * (10.0 - 3.0 * PI) / (12.0 - 3.0 * PI)
}

/// COL-001: the centroid is area-weighted over the regions with the holes
/// subtracted: the origin for the symmetric shapes, the closed-form centroid
/// of an L section within 0.5 %, and off the centre of a custom profile
/// with a hole and a second region.
#[test]
fn the_centroid_weights_regions_and_subtracts_holes() {
    for shape in [RECTANGLE, CIRCLE, RHS, CHS, IPE_200, ZED] {
        let centroid = section(shape).centroid().unwrap();
        assert!(centroid.length() < 1e-15, "{}: {centroid}", shape.code());
    }
    for shape in [TEE, CHANNEL] {
        // Symmetric about one axis only.
        let centroid = section(shape).centroid().unwrap();
        let symmetric = if shape == TEE { centroid.x } else { centroid.y };
        assert!(symmetric.abs() < 1e-15, "{}: {centroid}", shape.code());
    }

    // L 120 x 80 x 10: two legs, the root fillet added and the two toe
    // fillets removed, each a spandrel with a known centroid.
    let (h, b, t, r1, r2) = (0.12, 0.08, 0.01, 0.013, 0.0065);
    let (x, y) = (b / 2.0, h / 2.0);
    let (ix, iy) = (-x + t, -y + t);
    let parts = [
        (b * t, v(0.0, -y + t / 2.0)),
        (t * (h - t), v(-x + t / 2.0, (iy + y) / 2.0)),
        (fillet(r1), v(ix, iy) + DVec2::splat(spandrel(r1))),
        (-fillet(r2), v(x, iy) - DVec2::splat(spandrel(r2))),
        (-fillet(r2), v(ix, y) - DVec2::splat(spandrel(r2))),
    ];
    let area: f64 = parts.iter().map(|(a, _)| a).sum();
    let expected = parts.iter().map(|&(a, c)| a * c).sum::<DVec2>() / area;
    // Measured from the heel, as tables give it.
    let heel = v(-x, -y);
    let actual = section(ANGLE).centroid().unwrap() - heel;
    let expected = expected - heel;
    assert!(relative(actual.x, expected.x) < 0.005, "{actual} against {expected}");
    assert!(relative(actual.y, expected.y) < 0.005, "{actual} against {expected}");
    // Without radii the closed form is exact.
    let sharp = ParametricShape::LShape {
        height: h,
        width: b,
        thickness: t,
        root_radius: 0.0,
        toe_radius: 0.0,
    };
    let (area, moment) = (
        parts[0].0 + parts[1].0,
        parts[0].0 * parts[0].1 + parts[1].0 * parts[1].1,
    );
    let centroid = section(sharp).centroid().unwrap();
    assert!(centroid.distance(moment / area) < 1e-15, "{centroid}");

    // The square of 0.04 m² with a hole of 0.01 m² centred on the origin,
    // and a square of 0.01 m² centred on (0.15, -0.05).
    let custom = sections_of(&two_regions(MaterialId::new(), MaterialId::new()));
    let expected = (0.03 * DVec2::ZERO + 0.01 * v(0.15, -0.05)) / 0.04;
    let centroid = custom.centroid().unwrap();
    assert!(centroid.distance(expected) < 1e-15, "{centroid} against {expected}");
    assert_eq!(Section::default().centroid(), None);
}

/// COL-001: transformed(offset, rotation) translates every point by the
/// offset and then rotates it about the origin, so an anchor point a moves
/// to rotate(a + offset); it keeps the area, the orientation of every loop
/// (validate_ring leaves it unchanged), the smooth flags, the materials and
/// the roles.
#[test]
fn transformed_translates_then_rotates() {
    let material = MaterialId::new();
    let offset = v(0.1, -0.05);
    let sections: Vec<Section> = TYPICAL
        .into_iter()
        .map(section)
        .chain([sections_of(&two_regions(material, MaterialId::new()))])
        .collect();
    for (n, original) in sections.iter().enumerate() {
        for rotation in [0.0, 0.3, FRAC_PI_2, -2.0, PI] {
            let moved = original.transformed(offset, rotation);
            let what = format!("section {n} at {rotation}");
            let turn = DVec2::from_angle(rotation);
            assert_eq!(moved.regions.len(), original.regions.len());
            for (a, b) in original.regions.iter().zip(&moved.regions) {
                assert_eq!((a.material, &a.role), (b.material, &b.role));
                assert_eq!(a.holes.len(), b.holes.len());
                for (from, to) in std::iter::once(&a.outer)
                    .chain(&a.holes)
                    .zip(std::iter::once(&b.outer).chain(&b.holes))
                {
                    assert_eq!(from.smooth, to.smooth, "{what}");
                    assert_eq!(from.points.len(), to.points.len(), "{what}");
                    for (&p, &q) in from.points.iter().zip(&to.points) {
                        assert!(q.distance(turn.rotate(p + offset)) < 1e-15, "{what}: {p} to {q}");
                    }
                }
                assert_eq!(validate_ring(&b.outer.points).as_ref(), Ok(&b.outer.points), "{what}");
                for hole in &b.holes {
                    let reversed: Vec<DVec2> = hole.points.iter().rev().copied().collect();
                    assert_eq!(validate_ring(&hole.points), Ok(reversed), "{what}: hole");
                }
            }
            assert!(relative(moved.area(), original.area()) < 1e-12, "{what}");
        }
        // Without a rotation the nine points move by the offset.
        let moved = original.transformed(offset, 0.0);
        for i in 0..9 {
            let (a, b) = (original.nine_point(i).unwrap(), moved.nine_point(i).unwrap());
            assert!(b.distance(a + offset) < 1e-15, "{i}: {a} to {b}");
        }
    }

    // Anchored at its top left corner and turned a quarter: the rectangle's
    // corner lies on the axis, and the rectangle, 0.3 wide and 0.5 high,
    // extends 0.5 along +x and 0.3 along +y.
    let rectangle = section(RECTANGLE);
    let anchor = rectangle.nine_point(0).unwrap();
    let moved = rectangle.transformed(-anchor, FRAC_PI_2);
    assert!(outer(&moved).points.iter().any(|p| p.length() < 1e-15));
    let (min, max) = moved.bounds().unwrap();
    assert!(min.length() < 1e-15 && max.distance(v(0.5, 0.3)) < 1e-15, "{min} {max}");
}
