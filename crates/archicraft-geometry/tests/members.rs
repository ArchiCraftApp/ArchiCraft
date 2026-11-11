//! The bodies of columns (COL-001, COL-003, PRF-004, IFX-035): the
//! cross-section, a rectangle, a circle or a profile, anchored and turned,
//! swept up the axis between the end cuts, with the axis from the bottom cut
//! to the top cut, in storey coordinates.

#![allow(clippy::unwrap_used)]

use std::f64::consts::PI;

use archicraft_geometry::{ElementGeometry, ModelGeometry};
use archicraft_model::glam::{DVec2, DVec3};
use archicraft_model::kinds::{Anchor, Column, ColumnCut, CrossSection, VerticalLink};
use archicraft_model::template::default_metric_project;
use archicraft_model::{
    Command, Element, History, MaterialId, ParametricShape, Profile, ProfileRegion, Project, TopLink,
};

/// A project with the D.2 column at `position` on `storey` (0 the ground
/// storey, 1 the first floor), changed by `change`, and the column's
/// evaluated geometry.
fn column_on(storey: usize, position: DVec2, change: impl FnOnce(&mut Column)) -> (Project, Element, ElementGeometry) {
    let mut p = default_metric_project();
    let home = p.storeys.as_slice()[storey].id;
    let mut element = p.new_column(home, position, 0.0, 0);
    change(element.as_column_mut().unwrap());
    History::default()
        .execute(&mut p, Command::AddElements(vec![element.clone()]))
        .unwrap();
    let geometry = ModelGeometry::evaluate(&p);
    assert!(geometry.problems.is_empty(), "{:?}", geometry.problems);
    let body = geometry.get(element.id).unwrap().clone();
    (p, element, body)
}

/// The D.2 column at `position` on the ground storey, changed by `change`.
fn column(position: DVec2, change: impl FnOnce(&mut Column)) -> ElementGeometry {
    column_on(0, position, change).2
}

fn relative(actual: f64, expected: f64) -> f64 {
    ((actual - expected) / expected).abs()
}

fn assert_near(actual: DVec3, expected: DVec3) {
    assert!(actual.abs_diff_eq(expected, 1e-12), "{actual} is not {expected}");
}

/// COL-001, Appendix D.2: the default column (0.30 × 0.30 m reinforced
/// concrete, centre anchor, top at the storey above) on the ground floor is
/// one closed part `Body` of its material with the volume 0.27 m³, centred
/// on its position from z 0 to 3.
#[test]
fn the_default_column_is_a_closed_box_of_0_27_cubic_metres() {
    let (p, element, body) = column_on(0, DVec2::new(4.0, 2.0), |_| {});
    assert_eq!(body.parts.len(), 1);
    let part = &body.parts[0];
    assert_eq!(part.name, "Body");
    assert_eq!(part.material, element.kind.material());
    assert_eq!(p.material(part.material.unwrap()).unwrap().name, "Reinforced concrete");
    part.mesh.check_closed().unwrap();
    assert!(relative(body.volume(), 0.27) < 1e-12, "{}", body.volume());
    let (lo, hi) = body.bounds().unwrap();
    assert_near(lo, DVec3::new(3.85, 1.85, 0.0));
    assert_near(hi, DVec3::new(4.15, 2.15, 3.0));
    assert!(body.children.is_empty());
}

/// COL-001: a rotation turns the section about the axis, so a square
/// 0.30 m column turned by 30° covers 0.15 · (cos 30° + sin 30°) on each
/// side of its position.
#[test]
fn a_rotated_column_has_the_analytic_bounding_box() {
    let position = DVec2::new(-2.0, 5.0);
    let body = column(position, |c| c.rotation = 30_f64.to_radians());
    let half = 0.15 * (30_f64.to_radians().cos() + 30_f64.to_radians().sin());
    let (lo, hi) = body.bounds().unwrap();
    assert_near(lo, DVec3::new(position.x - half, position.y - half, 0.0));
    assert_near(hi, DVec3::new(position.x + half, position.y + half, 3.0));
    assert!(relative(body.volume(), 0.27) < 1e-12);
}

/// COL-001: the anchor is the point of the section on the position, so
/// with `BottomLeft` the section's bottom-left corner lies on it; turned,
/// that corner stays on the position and the section turns about it.
#[test]
fn the_bottom_left_anchor_puts_that_corner_on_the_position() {
    let position = DVec2::new(1.0, 1.0);
    let section = CrossSection::Rectangular {
        width: 0.4,
        height: 0.2,
    };
    let body = column(position, |c| {
        c.anchor = Anchor::BottomLeft;
        c.section = section;
    });
    let (lo, hi) = body.bounds().unwrap();
    assert_near(lo, DVec3::new(1.0, 1.0, 0.0));
    assert_near(hi, DVec3::new(1.4, 1.2, 3.0));

    let body = column(position, |c| {
        c.anchor = Anchor::BottomLeft;
        c.section = section;
        c.rotation = PI / 2.0;
    });
    let (lo, hi) = body.bounds().unwrap();
    assert_near(lo, DVec3::new(0.8, 1.0, 0.0));
    assert_near(hi, DVec3::new(1.0, 1.4, 3.0));
}

/// COL-001, IFX-034: a circular column of 0.40 m has 36 segments at the
/// default tessellation (10° per segment), and its volume is the area of the
/// inscribed 36-gon times its height, within 0.6 % of the circle's.
#[test]
fn a_circular_column_is_an_inscribed_polygon_swept_up() {
    let position = DVec2::new(3.0, -1.0);
    let body = column(position, |c| c.section = CrossSection::Circular { diameter: 0.4 });
    let mesh = &body.parts[0].mesh;
    mesh.check_closed().unwrap();
    let mut floor: Vec<DVec2> = mesh
        .points
        .iter()
        .filter(|q| q.z.abs() < 1e-12)
        .map(|q| q.truncate())
        .collect();
    floor.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    floor.dedup_by(|a, b| a.distance(*b) < 1e-12);
    assert_eq!(floor.len(), 36, "{floor:?}");
    for q in &floor {
        assert!((q.distance(position) - 0.2).abs() < 1e-12, "{q} is on the circle");
    }
    let inscribed = 18.0 * 0.2_f64.powi(2) * (2.0 * PI / 36.0).sin();
    assert!(relative(body.volume(), inscribed * 3.0) < 1e-9, "{}", body.volume());
    assert!(relative(body.volume(), PI * 0.2_f64.powi(2) * 3.0) < 0.006);
    let (lo, hi) = body.bounds().unwrap();
    assert_near(lo, DVec3::new(2.8, -1.2, 0.0));
    assert_near(hi, DVec3::new(3.2, -0.8, 3.0));
}

/// ELM-005, ELM-006: a fixed height of 2.50 m with a base offset of 0.20 m
/// spans z 0.20 to 2.70 above the storey, also on the first floor, whose
/// elevation the storey coordinates leave out; the axis runs between them.
#[test]
fn a_fixed_height_with_a_base_offset_spans_from_the_offset_up() {
    for storey in [0, 1] {
        let (_, _, body) = column_on(storey, DVec2::new(1.0, 2.0), |c| {
            c.link = VerticalLink {
                base_offset: 0.2,
                top: TopLink::Fixed { height: 2.5 },
            };
        });
        let (lo, hi) = body.bounds().unwrap();
        assert_eq!((lo.z, hi.z), (0.2, 0.2 + 2.5), "storey {storey}");
        assert_eq!(body.axis, [DVec3::new(1.0, 2.0, 0.2), DVec3::new(1.0, 2.0, 2.7)]);
        assert!(relative(body.volume(), 0.09 * 2.5) < 1e-12);
    }
}

/// COL-003: an end cut at an angle is a plane through the axis that rises
/// by tan(angle) per metre along the column's local y axis, so every top
/// vertex lies on it, and the volume is the section's area times the length
/// along the axis through its centroid. With the centre anchor that length
/// is the height; anchored at the bottom left, the centroid lies 0.15 m
/// along y from the axis. Cuts at both ends work alike, also turned.
#[test]
fn an_angled_cut_gives_the_area_times_the_centroid_length() {
    let top = 20_f64.to_radians();
    let body = column(DVec2::ZERO, |c| c.top_cut = ColumnCut::Angle(top));
    body.parts[0].mesh.check_closed().unwrap();
    assert!(relative(body.volume(), 0.09 * 3.0) < 1e-12, "{}", body.volume());
    let (_, hi) = body.bounds().unwrap();
    assert!((hi.z - (3.0 + 0.15 * top.tan())).abs() < 1e-12, "{hi}");

    let bottom = -10_f64.to_radians();
    let rotation = 30_f64.to_radians();
    let position = DVec2::new(2.0, 3.0);
    let body = column(position, |c| {
        c.anchor = Anchor::BottomLeft;
        c.rotation = rotation;
        c.top_cut = ColumnCut::Angle(top);
        c.bottom_cut = ColumnCut::Angle(bottom);
    });
    let mesh = &body.parts[0].mesh;
    mesh.check_closed().unwrap();
    let length = 3.0 + 0.15 * (top.tan() - bottom.tan());
    assert!(relative(body.volume(), 0.09 * length) < 1e-12, "{}", body.volume());
    let v = DVec2::new(-rotation.sin(), rotation.cos());
    for q in &mesh.points {
        let y = (q.truncate() - position).dot(v);
        let on_top = (q.z - (3.0 + y * top.tan())).abs() < 1e-9;
        let on_bottom = (q.z - y * bottom.tan()).abs() < 1e-9;
        assert!(on_top || on_bottom, "{q} lies on neither cut");
    }
    assert_eq!(body.axis, [position.extend(0.0), position.extend(3.0)]);
}

/// IFX-035: the axis runs through the position from the bottom cut to the
/// top cut, in storey coordinates, whatever the anchor and the storey.
#[test]
fn the_axis_runs_from_the_bottom_to_the_top_through_the_position() {
    let (x, y) = (5.5, -3.25);
    let body = column(DVec2::new(x, y), |_| {});
    assert_eq!(body.axis, [DVec3::new(x, y, 0.0), DVec3::new(x, y, 3.0)]);
    let (_, _, upper) = column_on(1, DVec2::new(x, y), |c| {
        c.anchor = Anchor::TopRight;
        c.link.top = TopLink::Fixed { height: 3.0 };
    });
    assert_eq!(upper.axis, [DVec3::new(x, y, 0.0), DVec3::new(x, y, 3.0)]);
    let (lo, hi) = upper.bounds().unwrap();
    assert_near(lo, DVec3::new(x - 0.3, y - 0.3, 0.0));
    assert_near(hi, DVec3::new(x, y, 3.0));
}

/// The project `p` with `profile`, made of its materials, and the D.2
/// column at `position` on the ground storey using it as its section,
/// changed by `change`, and the column's evaluated geometry (PRF-004).
fn profile_column(
    mut p: Project,
    profile: &Profile,
    position: DVec2,
    change: impl FnOnce(&mut Column),
) -> (Project, History, Element, ElementGeometry) {
    let mut h = History::default();
    h.execute(&mut p, Command::AddProfile(profile.clone())).unwrap();
    let mut element = p.new_column(p.storeys.ground(), position, 0.0, 0);
    element.as_column_mut().unwrap().section = CrossSection::Profile(profile.id);
    change(element.as_column_mut().unwrap());
    h.execute(&mut p, Command::AddElements(vec![element.clone()])).unwrap();
    let geometry = ModelGeometry::evaluate(&p);
    assert!(geometry.problems.is_empty(), "{:?}", geometry.problems);
    let body = geometry.get(element.id).unwrap().clone();
    (p, h, element, body)
}

/// IPE 300 (EN 10365): h 300, b 150, tw 7.1, tf 10.7, r 15 mm.
fn ipe_300() -> ParametricShape {
    ParametricShape::IShape {
        height: 0.3,
        width: 0.15,
        web: 0.0071,
        flange: 0.0107,
        root_radius: 0.015,
    }
}

/// A custom region: the rectangle from `min` to `max` of `material`.
fn plate(min: DVec2, max: DVec2, material: MaterialId) -> ProfileRegion {
    ProfileRegion {
        outer: vec![min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)],
        holes: Vec::new(),
        material,
    }
}

/// The area of the profile's section at the default tessellation.
fn section_area(p: &Project, profile: &Profile) -> f64 {
    archicraft_geometry::section::profile_section(profile, &p.settings.tessellation)
        .unwrap()
        .area()
}

/// COL-001, PRF-004: with `Anchor::ProfileOrigin` the profile's insertion
/// origin, here (0.05, 0.10) of a 0.20 × 0.40 m plate from (0, 0), lies on
/// the column's position; turned by 90°, the section turns about it.
#[test]
fn the_profile_origin_anchor_puts_that_profile_point_on_the_position() {
    let p = default_metric_project();
    let material = p.materials[0].id;
    let profile = Profile {
        origin: DVec2::new(0.05, 0.10),
        ..Profile::custom("Plate", vec![plate(DVec2::ZERO, DVec2::new(0.2, 0.4), material)])
    };
    let position = DVec2::new(1.0, 1.0);
    let (_, _, _, body) = profile_column(p.clone(), &profile, position, |c| c.anchor = Anchor::ProfileOrigin);
    let (lo, hi) = body.bounds().unwrap();
    assert_near(lo, DVec3::new(0.95, 0.90, 0.0));
    assert_near(hi, DVec3::new(1.15, 1.30, 3.0));
    assert_eq!(body.axis, [position.extend(0.0), position.extend(3.0)]);

    let (_, _, _, turned) = profile_column(p.clone(), &profile, position, |c| {
        c.anchor = Anchor::ProfileOrigin;
        c.rotation = PI / 2.0;
    });
    let (lo, hi) = turned.bounds().unwrap();
    assert_near(lo, DVec3::new(0.70, 0.95, 0.0));
    assert_near(hi, DVec3::new(1.10, 1.15, 3.0));

    let (_, _, _, corner) = profile_column(p, &profile, position, |c| c.anchor = Anchor::BottomLeft);
    let (lo, _) = corner.bounds().unwrap();
    assert_near(lo, DVec3::new(1.0, 1.0, 0.0));
}

/// COL-001, PRF-004, PRF-005: an IPE 300 profile column is one closed part
/// `Body` of the profile's material, not the column's, whose volume is the
/// section's area times the height.
#[test]
fn an_ipe_300_profile_column_is_one_closed_body() {
    let p = default_metric_project();
    let steel = p.materials.iter().find(|m| m.name == "Steel").unwrap().id;
    let profile = Profile::parametric("IPE 300", ipe_300(), steel);
    let (p, _, element, body) = profile_column(p, &profile, DVec2::new(4.0, 2.0), |_| {});
    assert_ne!(
        element.kind.material(),
        Some(steel),
        "the column keeps its own material"
    );
    assert_eq!(body.parts.len(), 1);
    let part = &body.parts[0];
    assert_eq!((part.name.as_str(), part.material), ("Body", Some(steel)));
    part.mesh.check_closed().unwrap();
    let area = section_area(&p, &profile);
    assert!((area - 0.00538).abs() < 0.0001, "IPE 300 is about 53.8 cm²: {area}");
    assert!(relative(body.volume(), area * 3.0) < 1e-9, "{}", body.volume());
    let (lo, hi) = body.bounds().unwrap();
    assert_near(lo, DVec3::new(3.925, 1.85, 0.0));
    assert_near(hi, DVec3::new(4.075, 2.15, 3.0));
}

/// COL-001, PRF-004: a custom profile of two regions makes a column of two
/// closed parts `Region_1` and `Region_2` in the profile's region order,
/// each of its region's material.
#[test]
fn a_two_region_profile_column_has_a_part_per_region() {
    let p = default_metric_project();
    let (concrete, steel) = (p.materials[0].id, p.materials[1].id);
    let profile = Profile::custom(
        "Composite",
        vec![
            plate(DVec2::new(-0.2, -0.2), DVec2::new(0.2, 0.0), concrete),
            plate(DVec2::new(-0.2, 0.0), DVec2::new(0.2, 0.1), steel),
        ],
    );
    let (_, _, _, body) = profile_column(p, &profile, DVec2::ZERO, |_| {});
    let parts: Vec<(&str, Option<MaterialId>)> = body.parts.iter().map(|q| (q.name.as_str(), q.material)).collect();
    assert_eq!(parts, [("Region_1", Some(concrete)), ("Region_2", Some(steel))]);
    for part in &body.parts {
        part.mesh.check_closed().unwrap();
    }
    assert!(
        relative(body.volume(), (0.08 + 0.04) * 3.0) < 1e-12,
        "{}",
        body.volume()
    );
}

/// PRF-004: editing a profile updates every element that uses it, so
/// UpdateProfile raising an I-profile from IPE 300 to 400 mm changes the
/// evaluated volume of the column using it, incrementally as in full, and
/// undo brings the old volume back.
#[test]
fn updating_a_profile_changes_the_columns_that_use_it() {
    let p = default_metric_project();
    let steel = p.materials.iter().find(|m| m.name == "Steel").unwrap().id;
    let profile = Profile::parametric("IPE 300", ipe_300(), steel);
    let (mut p, mut h, element, body) = profile_column(p, &profile, DVec2::ZERO, |_| {});
    let before = ModelGeometry::evaluate(&p);
    let mut taller = profile.clone();
    if let archicraft_model::ProfileShape::Parametric {
        shape: ParametricShape::IShape { height, .. },
        ..
    } = &mut taller.shape
    {
        *height = 0.4;
    }
    h.execute(&mut p, Command::UpdateProfile(taller.clone())).unwrap();
    let updated = ModelGeometry::evaluate_incremental(&p, &before);
    assert!(updated.changed.contains(&element.id), "the column is re-evaluated");
    let volume = updated.get(element.id).unwrap().volume();
    assert!(relative(volume, section_area(&p, &taller) * 3.0) < 1e-9, "{volume}");
    assert!(volume > body.volume() * 1.1, "{volume} is more than {}", body.volume());
    assert_eq!(updated.get(element.id), ModelGeometry::evaluate(&p).get(element.id));
    h.undo(&mut p).unwrap();
    let undone = ModelGeometry::evaluate_incremental(&p, &updated);
    assert!(relative(undone.get(element.id).unwrap().volume(), body.volume()) < 1e-12);
}
