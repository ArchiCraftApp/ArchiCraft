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

/// The extent of an element through the project, which for a wall must be
/// the one its `VerticalLink` resolves.
fn extent(p: &Project, id: archicraft_model::ElementId) -> (f64, f64) {
    let e = p.element(id).unwrap();
    let extent = p.vertical_extent(e).unwrap();
    if let Some(w) = e.as_wall() {
        assert_eq!(Some(extent), w.link().extent(e.storey, &p.storeys), "{}", e.name);
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
/// and without shifting the storeys above, storey- and relative-linked wall
/// tops follow their storey by exactly its change, fixed tops stay at base +
/// height, and wall and slab bases follow their home storey.
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
            let Some(w) = e.as_wall() else {
                assert!((t1 - (t0 + home)).abs() < MODEL_TOLERANCE, "{label}: {} top", e.name);
                continue;
            };
            let expected = match w.top {
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
            if !matches!(w.top, TopLink::Fixed { .. }) && (t1 - t0).abs() > MODEL_TOLERANCE {
                followed += 1;
            }
        }
        assert!(followed > 0, "{label}: some linked top moved");
    }
}
