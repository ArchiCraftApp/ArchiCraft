//! The storey-link and plan-transform hooks of the element kinds (SRS 3.2,
//! ELM-006, STR-006, EDT-001…EDT-003): walls resolve their base and top
//! through the shared `VerticalLink`, linked tops follow storey edits, and
//! every kind moves, rotates and mirrors its own parameters.

#![allow(clippy::unwrap_used)]

mod common;

use std::f64::consts::FRAC_PI_2;

use archicraft_model::glam::DVec2;
use archicraft_model::kinds::{StoreyLinkRef, VerticalLink};
use archicraft_model::template::default_metric_project;
use archicraft_model::transform::PlanTransform;
use archicraft_model::{
    Command, Element, ElementKind, History, MODEL_TOLERANCE, Project, ReferenceLine, Storey, StoreyId, StoreyList,
    TopLink,
};
use common::{ground, wall};

fn close(a: DVec2, b: DVec2) -> bool {
    a.distance(b) < MODEL_TOLERANCE
}

fn signed_area(ring: &[DVec2]) -> f64 {
    let n = ring.len();
    (0..n).map(|i| ring[i].perp_dot(ring[(i + 1) % n])).sum::<f64>() / 2.0
}

/// EDT-002: a wall rotated 90° counter-clockwise about a point keeps its
/// thickness and reference line, and its ends turn about that point.
#[test]
fn rotating_a_wall_quarter_turn_about_a_point() {
    let p = default_metric_project();
    let mut w = wall(&p, (2.0, 1.0), (5.0, 1.0));
    let before = w.as_wall().unwrap().clone();
    w.kind
        .transform(&PlanTransform::rotation(DVec2::new(1.0, 1.0), FRAC_PI_2))
        .unwrap();
    let after = w.as_wall().unwrap();
    assert!(close(after.start, DVec2::new(1.0, 2.0)), "{:?}", after.start);
    assert!(close(after.end, DVec2::new(1.0, 5.0)), "{:?}", after.end);
    assert_eq!(
        (after.thickness, after.reference_line, after.base_offset, after.top),
        (before.thickness, before.reference_line, before.base_offset, before.top)
    );
}

/// EDT-003, WAL-003: a mirrored wall swaps its ends, so its exterior face
/// is still on the right of the reference line, now on the mirrored side.
#[test]
fn mirroring_a_wall_keeps_its_exterior_side() {
    let p = default_metric_project();
    let mut w = wall(&p, (1.0, 0.0), (4.0, 0.0));
    if let ElementKind::Wall(wall) = &mut w.kind {
        wall.reference_line = ReferenceLine::Outside;
    }
    w.kind
        .transform(&PlanTransform::mirror(DVec2::ZERO, DVec2::X).unwrap())
        .unwrap();
    let after = w.as_wall().unwrap();
    assert_eq!((after.start, after.end), (DVec2::new(4.0, 0.0), DVec2::new(1.0, 0.0)));
    assert!(PlanTransform::mirror(DVec2::ONE, DVec2::ONE).is_none());
}

/// EDT-003, SLB-001: a mirrored slab has the mirrored coordinates, its
/// outline stays counter-clockwise and its holes clockwise, and the result
/// is a valid slab.
#[test]
fn mirroring_a_slab_keeps_outline_and_hole_orientation() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let outline = vec![
        DVec2::new(0.0, 0.0),
        DVec2::new(4.0, 0.0),
        DVec2::new(4.0, 3.0),
        DVec2::new(0.0, 3.0),
    ];
    let hole = vec![
        DVec2::new(1.0, 1.0),
        DVec2::new(1.0, 2.0),
        DVec2::new(2.0, 2.0),
        DVec2::new(2.0, 1.0),
    ];
    let mut s = p.new_slab(ground(&p), outline, vec![hole], 0);
    let id = s.id;
    h.execute(&mut p, Command::AddElements(vec![s.clone()])).unwrap();

    // Mirror about the vertical line x = 5.
    let mirror = PlanTransform::mirror(DVec2::new(5.0, 0.0), DVec2::new(5.0, 1.0)).unwrap();
    s.kind.transform(&mirror).unwrap();
    let slab = s.as_slab().unwrap();
    assert_eq!(
        slab.outline,
        [
            DVec2::new(10.0, 0.0),
            DVec2::new(10.0, 3.0),
            DVec2::new(6.0, 3.0),
            DVec2::new(6.0, 0.0)
        ]
    );
    assert_eq!(
        slab.holes,
        [vec![
            DVec2::new(9.0, 1.0),
            DVec2::new(8.0, 1.0),
            DVec2::new(8.0, 2.0),
            DVec2::new(9.0, 2.0)
        ]]
    );
    assert!(signed_area(&slab.outline) > 0.0, "outline counter-clockwise");
    assert!(signed_area(&slab.holes[0]) < 0.0, "hole clockwise");

    h.execute(&mut p, Command::UpdateElements(vec![s.clone()])).unwrap();
    assert_eq!(
        p.element(id).unwrap().kind,
        s.kind,
        "validation keeps the mirrored slab"
    );
}

/// EDT-001: MoveElements translates walls and slabs exactly by the
/// distance, through the transform hook, and undo restores them.
#[test]
fn move_elements_translates_exactly() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let w = wall(&p, (0.1, 0.2), (5.3, 0.2));
    let s = p.new_slab(
        ground(&p),
        vec![
            DVec2::ZERO,
            DVec2::new(2.0, 0.0),
            DVec2::new(2.0, 1.5),
            DVec2::new(0.0, 1.5),
        ],
        Vec::new(),
        0,
    );
    let ids = vec![w.id, s.id];
    h.execute(&mut p, Command::AddElements(vec![w.clone(), s.clone()]))
        .unwrap();
    let before = p.clone();
    let delta = DVec2::new(0.7, -0.3);
    h.execute(
        &mut p,
        Command::MoveElements {
            ids: ids.clone(),
            delta,
        },
    )
    .unwrap();
    let moved = p.element(ids[0]).unwrap().as_wall().unwrap();
    let original = w.as_wall().unwrap();
    assert_eq!((moved.start, moved.end), (original.start + delta, original.end + delta));
    let moved = p.element(ids[1]).unwrap().as_slab().unwrap();
    let expected: Vec<DVec2> = s.as_slab().unwrap().outline.iter().map(|q| *q + delta).collect();
    assert_eq!(moved.outline, expected);
    let mut translated = s.kind.clone();
    translated.translate(delta);
    assert_eq!(&p.element(ids[1]).unwrap().kind, &translated);
    h.undo(&mut p).unwrap();
    assert_eq!(p.elements, before.elements);
}

/// The three top links of ELM-006, each with a non-zero offset, on a wall
/// with a non-zero base offset.
fn top_links(p: &Project) -> [TopLink; 3] {
    let first = p.storeys.as_slice()[1].id;
    [
        TopLink::Fixed { height: 2.6 },
        TopLink::Storey {
            storey: first,
            offset: 0.15,
        },
        TopLink::Relative {
            storeys: 1,
            offset: -0.1,
        },
    ]
}

fn linked_wall(p: &Project, home: StoreyId, top: TopLink) -> Element {
    let mut w = p.new_wall(home, DVec2::ZERO, DVec2::new(4.0, 0.0), 0);
    if let ElementKind::Wall(wall) = &mut w.kind {
        wall.base_offset = -0.2;
        wall.top = top;
    }
    w
}

/// The base offset and top of a vertical element: a wall, or the test kind
/// in builds with the feature `test-kinds`.
fn vertical_link(e: &Element) -> Option<VerticalLink> {
    #[cfg(feature = "test-kinds")]
    if let Some(d) = e.as_dummy() {
        return Some(d.link);
    }
    e.as_wall().map(|w| w.link())
}

/// The extent of an element through the project, which for a vertical
/// element must be the one its `VerticalLink` resolves.
fn extent(p: &Project, id: archicraft_model::ElementId) -> (f64, f64) {
    let e = p.element(id).unwrap();
    let extent = p.vertical_extent(e).unwrap();
    if let Some(link) = vertical_link(e) {
        assert_eq!(Some(extent), link.extent(e.storey, &p.storeys), "{}", e.name);
        assert_eq!(Some(extent), e.kind.vertical_extent(e.storey, &p.storeys));
    }
    extent
}

/// ELM-006: for each top link a wall's extent resolves through
/// `VerticalLink`: a fixed height above the base, a storey's elevation plus
/// the offset, or the elevation `n` storeys above home plus the offset.
#[test]
fn wall_extents_resolve_through_vertical_link() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let g = ground(&p);
    let first = p.storeys.as_slice()[1].id;
    let mut ids = Vec::new();
    for home in [g, first] {
        for top in top_links(&p) {
            let w = linked_wall(&p, home, top);
            ids.push(w.id);
            h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
        }
    }
    let expected = [
        (-0.2, 2.4),
        (-0.2, 3.15),
        (-0.2, 2.9),
        (2.8, 5.4),
        (2.8, 3.15),
        // Relative +1 from the top storey extrapolates its nominal height.
        (2.8, 5.9),
    ];
    for (id, (bottom, top)) in ids.iter().zip(expected) {
        let (b, t) = extent(&p, *id);
        assert!(
            (b - bottom).abs() < MODEL_TOLERANCE && (t - top).abs() < MODEL_TOLERANCE,
            "{b}, {t}"
        );
    }

    let link = VerticalLink {
        base_offset: -0.2,
        top: top_links(&p)[1],
    };
    assert_eq!(
        link.storey_links(),
        [StoreyLinkRef {
            role: "top",
            storey: first
        }]
    );
    let wall = p.element(ids[1]).unwrap();
    assert_eq!(wall.kind.storey_links(), link.storey_links());
    assert!(p.element(ids[0]).unwrap().kind.storey_links().is_empty());
    assert!(p.element(ids[2]).unwrap().kind.storey_links().is_empty());
}

/// Elevation change of `storey` between two storey lists.
fn change(before: &StoreyList, after: &StoreyList, storey: StoreyId) -> f64 {
    after.get(storey).unwrap().elevation - before.get(storey).unwrap().elevation
}

/// STR-006 (ELM-006): after SetStoreyElevation and SetStoreyHeight, with
/// and without shifting the storeys above, storey- and relative-linked tops
/// of walls (and of the test kind, with `test-kinds`) follow their storey by
/// exactly its change, fixed tops stay at base + height, and every base
/// follows its home storey.
#[test]
fn linked_tops_and_bases_follow_storey_changes() {
    let setup = || {
        let mut p = default_metric_project();
        let mut h = History::default();
        let g = ground(&p);
        let first = p.storeys.as_slice()[1].id;
        let second = Storey::new("02 Second floor", 6.0, 3.0);
        h.execute(
            &mut p,
            Command::InsertStorey {
                reference: first,
                above: true,
                storey: second,
            },
        )
        .unwrap();
        let mut elements = Vec::new();
        for (i, home) in [g, first].into_iter().enumerate() {
            for top in top_links(&p) {
                elements.push(linked_wall(&p, home, top));
                #[cfg(feature = "test-kinds")]
                elements.push(linked_dummy(&p, home, top, elements.len() as u32));
            }
            let outline = vec![DVec2::ZERO, DVec2::X, DVec2::ONE, DVec2::Y];
            elements.push(p.new_slab(home, outline, Vec::new(), i as u32));
        }
        h.execute(&mut p, Command::AddElements(elements)).unwrap();
        (p, h, g, first)
    };
    let commands = |g: StoreyId, first: StoreyId| {
        [
            Command::SetStoreyHeight {
                storey: g,
                height: 3.4,
                shift_above: true,
            },
            Command::SetStoreyHeight {
                storey: g,
                height: 3.4,
                shift_above: false,
            },
            Command::SetStoreyElevation {
                storey: first,
                elevation: 3.5,
                shift_above: true,
            },
            Command::SetStoreyElevation {
                storey: first,
                elevation: 3.5,
                shift_above: false,
            },
        ]
    };
    for i in 0..4 {
        let (mut p, mut h, g, first) = setup();
        let command = commands(g, first)[i].clone();
        let before = p.clone();
        let label = format!("{command:?}");
        h.execute(&mut p, command).unwrap();
        let (old, new) = (&before.storeys, &p.storeys);
        let mut followed = 0;
        for (id, old_id) in p.elements.keys().zip(before.elements.keys()) {
            assert_eq!(id, old_id);
            let e = p.element(*id).unwrap();
            let (b0, t0) = extent(&before, *id);
            let (b1, t1) = extent(&p, *id);
            let home = change(old, new, e.storey);
            assert!((b1 - (b0 + home)).abs() < MODEL_TOLERANCE, "{label}: {} base", e.name);
            let Some(link) = vertical_link(e) else {
                assert!((t1 - (t0 + home)).abs() < MODEL_TOLERANCE, "{label}: {} top", e.name);
                continue;
            };
            let expected = match link.top {
                TopLink::Fixed { height } => b1 + height,
                TopLink::Storey { storey, .. } => t0 + change(old, new, storey),
                // Both homes have a storey above them, so the link names it.
                TopLink::Relative { storeys, .. } => t0 + change(old, new, old.relative(e.storey, storeys).unwrap().id),
            };
            assert!(
                (t1 - expected).abs() < MODEL_TOLERANCE,
                "{label}: {} top {t1} vs {expected}",
                e.name
            );
            if !matches!(link.top, TopLink::Fixed { .. }) && (t1 - t0).abs() > MODEL_TOLERANCE {
                followed += 1;
            }
        }
        assert!(followed > 0, "{label}: some linked top moved");
    }
}

/// The plan-bounds hook: a wall's bounds cover both faces at both ends, a
/// slab's its outline.
#[test]
fn plan_bounds_cover_the_footprint() {
    let p = default_metric_project();
    let mut w = wall(&p, (1.0, 2.0), (5.0, 2.0));
    if let ElementKind::Wall(wall) = &mut w.kind {
        wall.thickness = 0.3;
        wall.reference_line = ReferenceLine::Center;
    }
    let (lo, hi) = w.kind.plan_bounds().unwrap();
    assert!(
        close(lo, DVec2::new(1.0, 1.85)) && close(hi, DVec2::new(5.0, 2.15)),
        "{lo:?} {hi:?}"
    );
    let outline = vec![DVec2::new(-1.0, 0.0), DVec2::new(3.0, -2.0), DVec2::new(2.0, 4.0)];
    let s = p.new_slab(ground(&p), outline, Vec::new(), 0);
    assert_eq!(
        s.kind.plan_bounds(),
        Some((DVec2::new(-1.0, -2.0), DVec2::new(3.0, 4.0)))
    );
    assert!(w.kind.parts().is_empty() && s.kind.parts().is_empty());
    assert_eq!(w.kind.container(), archicraft_model::kinds::Container::Storey);
    assert_eq!(s.kind.container(), archicraft_model::kinds::Container::Storey);
}

/// A test-kind element on `home` with the wall fixture's base offset and the
/// given top.
#[cfg(feature = "test-kinds")]
fn linked_dummy(p: &Project, home: StoreyId, top: TopLink, nth: u32) -> Element {
    let mut d = archicraft_model::kinds::Dummy::new(DVec2::new(nth as f64, 1.0), DVec2::ONE, p.materials[0].id)
        .into_element(p, home, nth);
    d.as_dummy_mut().unwrap().link = VerticalLink { base_offset: -0.2, top };
    d
}

/// ELM-006: for each top link (fixed height, a storey, home + 1, each with
/// a non-zero offset) and on two home storeys, the test kind's extent
/// resolves through the shared `VerticalLink` exactly like a wall's with
/// the same base offset and top.
#[cfg(feature = "test-kinds")]
#[test]
fn test_kind_extents_equal_wall_extents() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let first = p.storeys.as_slice()[1].id;
    let mut pairs = Vec::new();
    for home in [ground(&p), first] {
        for top in top_links(&p) {
            let w = linked_wall(&p, home, top);
            let d = linked_dummy(&p, home, top, pairs.len() as u32);
            pairs.push((w.id, d.id));
            h.execute(&mut p, Command::AddElements(vec![w, d])).unwrap();
        }
    }
    for (w, d) in pairs {
        assert_eq!(extent(&p, d), extent(&p, w));
        let (wall, dummy) = (p.element(w).unwrap(), p.element(d).unwrap());
        assert_eq!(dummy.kind.storey_links(), wall.kind.storey_links());
    }
}

/// STR-004, ELM-006: deleting the storey a test-kind top is linked to keeps
/// that top where it is as a fixed height, and undo restores the link.
#[cfg(feature = "test-kinds")]
#[test]
fn deleting_the_linked_storey_fixes_the_test_kind_top() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let g = ground(&p);
    let first = p.storeys.as_slice()[1].id;
    let second = Storey::new("02 Second floor", 6.0, 3.0);
    let second_id = second.id;
    h.execute(
        &mut p,
        Command::InsertStorey {
            reference: first,
            above: true,
            storey: second,
        },
    )
    .unwrap();
    let top = TopLink::Storey {
        storey: second_id,
        offset: 0.15,
    };
    let d = linked_dummy(&p, g, top, 0);
    let id = d.id;
    h.execute(&mut p, Command::AddElements(vec![d])).unwrap();
    let before = extent(&p, id);
    assert_eq!(before, (-0.2, 6.15));
    h.execute(
        &mut p,
        Command::DeleteStorey {
            storey: second_id,
            elements: archicraft_model::command::StoreyElements::Delete,
        },
    )
    .unwrap();
    let link = p.element(id).unwrap().as_dummy().unwrap().link;
    assert_eq!(link.base_offset, -0.2);
    match link.top {
        TopLink::Fixed { height } => assert!((height - 6.35).abs() < MODEL_TOLERANCE, "{height}"),
        other => panic!("expected a fixed height, got {other:?}"),
    }
    let after = extent(&p, id);
    assert!((after.0 - before.0).abs() < MODEL_TOLERANCE && (after.1 - before.1).abs() < MODEL_TOLERANCE);
    h.undo(&mut p).unwrap();
    assert_eq!(p.element(id).unwrap().as_dummy().unwrap().link.top, top);
}

/// ELM-005, ELM-006: relinking a test-kind element to another storey keeps
/// its absolute base and top for every top link.
#[cfg(feature = "test-kinds")]
#[test]
fn relinking_the_test_kind_keeps_its_position() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let g = ground(&p);
    let first = p.storeys.as_slice()[1].id;
    for (nth, top) in top_links(&p).into_iter().enumerate() {
        let d = linked_dummy(&p, g, top, nth as u32);
        let id = d.id;
        h.execute(&mut p, Command::AddElements(vec![d])).unwrap();
        let before = extent(&p, id);
        let moved = archicraft_model::command::relink_to_storey(&p, p.element(id).unwrap(), first).unwrap();
        h.execute(&mut p, Command::UpdateElements(vec![moved])).unwrap();
        let after = p.element(id).unwrap();
        assert_eq!(after.storey, first);
        let (b, t) = extent(&p, id);
        assert!(
            (b - before.0).abs() < MODEL_TOLERANCE && (t - before.1).abs() < MODEL_TOLERANCE,
            "{top:?}: {before:?} -> {:?}",
            (b, t)
        );
    }
}

/// EDT-001, EDT-003: the test kind translates, but refuses a mirror (and a
/// rotation) with `TransformError::Unsupported` and stays unchanged.
#[cfg(feature = "test-kinds")]
#[test]
fn the_test_kind_refuses_mirrors() {
    use archicraft_model::transform::TransformError;

    let p = default_metric_project();
    let mut d = linked_dummy(&p, ground(&p), TopLink::Fixed { height: 2.0 }, 0);
    let before = d.kind.clone();
    let mirror = PlanTransform::mirror(DVec2::ZERO, DVec2::Y).unwrap();
    assert_eq!(d.kind.transform(&mirror), Err(TransformError::Unsupported));
    let rotation = PlanTransform::rotation(DVec2::ZERO, FRAC_PI_2);
    assert_eq!(d.kind.transform(&rotation), Err(TransformError::Unsupported));
    assert_eq!(d.kind, before);
    d.kind.translate(DVec2::new(2.0, 0.5));
    assert_eq!(d.as_dummy().unwrap().origin, DVec2::new(2.0, 1.5));
}

/// The test kind's remaining hooks: no parts yet, the site container when
/// asked for, plan bounds of its rectangle, and validation of the rectangle
/// and the linked storey.
#[cfg(feature = "test-kinds")]
#[test]
fn the_test_kind_hooks() {
    use archicraft_model::kinds::Container;

    let mut p = default_metric_project();
    let mut h = History::default();
    let mut d = linked_dummy(&p, ground(&p), TopLink::Fixed { height: 2.0 }, 0);
    assert!(d.kind.parts().is_empty());
    assert_eq!(d.kind.container(), Container::Storey);
    d.as_dummy_mut().unwrap().site_level = true;
    assert_eq!(d.kind.container(), Container::Site);
    assert_eq!(d.kind.plan_bounds(), Some((DVec2::new(0.0, 1.0), DVec2::new(1.0, 2.0))));

    let before = p.clone();
    let mut flat = d.clone();
    flat.as_dummy_mut().unwrap().size = DVec2::new(1.0, 0.0);
    let mut unlinked = d.clone();
    unlinked.as_dummy_mut().unwrap().link.top = TopLink::Storey {
        storey: StoreyId::new(),
        offset: 0.0,
    };
    for (bad, reason) in [
        (flat, "model-error-polygon-area"),
        (unlinked, "model-error-vertical-extent"),
    ] {
        match h.execute(&mut p, Command::AddElements(vec![bad])) {
            Err(archicraft_model::CommandError::Invalid(m)) => {
                let nested = match m.arg("reason") {
                    Some(archicraft_model::Arg::Nested(nested)) => nested.key(),
                    _ => m.key(),
                };
                assert_eq!(nested, reason, "{m}");
            }
            other => panic!("expected CommandError::Invalid, got {other:?}"),
        }
    }
    assert_eq!(p, before);
    h.execute(&mut p, Command::AddElements(vec![d])).unwrap();
}
