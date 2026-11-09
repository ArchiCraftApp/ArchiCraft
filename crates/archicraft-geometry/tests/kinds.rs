//! The geometry registry (SRS 3.2, IFX-016, NFR-REL-006): every kind has a
//! generator, part bodies carry the roles the model declares, and a
//! generator that panics takes only its own element down. Uses the test
//! kind, which the crate's `test-kinds` feature provides to its tests.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use archicraft_geometry::kinds::GENERATORS;
use archicraft_geometry::{ElementGeometry, ModelGeometry, ViewRange, plan_drawing};
use archicraft_model::glam::DVec2;
use archicraft_model::kinds::{Dummy, Note};
use archicraft_model::template::default_metric_project;
use archicraft_model::{Command, Element, ElementId, History, KindTag, Message, Project, TopLink, samples};

/// Elements of `kind` on the ground storey: one of each shape the kind
/// has, a simple and a compound test element for the test kind. A new kind
/// gets a sample here, so that the registry tests cover it.
fn sample_elements(p: &Project, kind: KindTag) -> Vec<Element> {
    let g = p.storeys.ground();
    let material = p.materials[0].id;
    match kind {
        KindTag::Wall => vec![p.new_wall(g, DVec2::ZERO, DVec2::new(4.0, 0.0), 0)],
        KindTag::Slab => {
            let outline = vec![
                DVec2::new(0.0, 2.0),
                DVec2::new(4.0, 2.0),
                DVec2::new(4.0, 5.0),
                DVec2::new(0.0, 5.0),
            ];
            vec![p.new_slab(g, outline, Vec::new(), 0)]
        }
        KindTag::Space => {
            let boundary = vec![
                DVec2::new(0.0, 6.0),
                DVec2::new(4.0, 6.0),
                DVec2::new(4.0, 9.0),
                DVec2::new(0.0, 9.0),
            ];
            vec![p.new_space(g, boundary, Vec::new(), 0)]
        }
        KindTag::Text => vec![p.new_text(g, DVec2::new(1.0, 7.0), "Kitchen", 0)],
        KindTag::Dummy => {
            let simple = Dummy::new(DVec2::new(6.0, 0.0), DVec2::ONE, material);
            let mut compound = Dummy::new(DVec2::new(8.0, 0.0), DVec2::new(2.0, 1.0), material);
            compound.parts = true;
            vec![simple.into_element(p, g, 0), compound.into_element(p, g, 1)]
        }
        KindTag::Note => vec![Note::new(g, DVec2::new(1.0, 1.0), "Hall").into_element(p, g, 0)],
        other => panic!("no sample element for the kind {other:?}: add one to sample_elements"),
    }
}

/// A project with the sample elements of every registered kind.
fn with_every_kind() -> Project {
    let mut p = default_metric_project();
    let mut h = History::default();
    let elements: Vec<Element> = GENERATORS.iter().flat_map(|kind| sample_elements(&p, *kind)).collect();
    h.execute(&mut p, Command::AddElements(elements)).unwrap();
    p
}

/// SRS 3.2: every kind of this build has a generator, every generator
/// belongs to a kind of this build, and none is listed twice.
#[test]
fn every_kind_has_a_generator_and_every_generator_a_kind() {
    let kinds: BTreeSet<KindTag> = KindTag::ALL.iter().copied().collect();
    let generators: BTreeSet<KindTag> = GENERATORS.iter().copied().collect();
    assert_eq!(generators.len(), GENERATORS.len(), "a kind is registered twice");
    assert_eq!(generators, kinds, "generators {GENERATORS:?}, kinds {:?}", KindTag::ALL);
}

/// IFX-016: for every registered kind, the roles of the evaluated part
/// bodies equal the roles of the model's `Kind::parts`, in order, so the
/// writer can join the part classes and the bodies by role.
#[test]
fn part_body_roles_equal_the_kinds_parts() {
    let p = with_every_kind();
    let geometry = ModelGeometry::evaluate(&p);
    assert!(geometry.problems.is_empty(), "{:?}", geometry.problems);
    let mut compound = 0;
    for kind in GENERATORS {
        let elements: Vec<&Element> = p.elements.values().filter(|e| e.tag() == *kind).collect();
        assert!(!elements.is_empty(), "{kind:?} has no sample element");
        for element in elements {
            let declared: Vec<String> = element.kind.parts().into_iter().map(|part| part.role).collect();
            let evaluated: Vec<String> = geometry
                .get(element.id)
                .unwrap()
                .children
                .iter()
                .map(|c| c.role.clone())
                .collect();
            assert_eq!(evaluated, declared, "{}", element.name);
            compound += usize::from(!declared.is_empty());
        }
    }
    assert!(compound >= 1, "no sample element has parts");
}

/// SRS 3.2: `evaluate.rs` dispatches through the registry; it has no arm per
/// kind and no wildcard for kinds without a generator.
#[test]
fn evaluate_rs_has_no_arm_per_kind() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/evaluate.rs");
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    for (n, line) in text.lines().enumerate() {
        for pattern in ["ElementKind::Wall", "ElementKind::Slab", "UnsupportedKind"] {
            assert!(
                !line.contains(pattern),
                "evaluate.rs:{}: `{pattern}` in {line:?}",
                n + 1
            );
        }
    }
}

/// IFX-016: a compound test element is a closed body plus one closed body
/// per part, its bounds and volume include the parts, and a simple one has
/// no children. The test kind stacks its parts on top of its box, half as
/// high, each over an equal strip of the plan rectangle.
#[test]
fn a_compound_test_element_has_closed_part_bodies_that_count() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let g = p.storeys.ground();
    let material = p.materials[0].id;
    let simple = Dummy::new(DVec2::new(1.0, 1.0), DVec2::new(2.0, 1.0), material).into_element(&p, g, 0);
    let mut compound = Dummy::new(DVec2::new(1.0, 1.0), DVec2::new(2.0, 1.0), material);
    compound.parts = true;
    let compound = compound.into_element(&p, g, 1);
    let (simple_id, compound_id) = (simple.id, compound.id);
    h.execute(&mut p, Command::AddElements(vec![simple, compound])).unwrap();
    let geometry = ModelGeometry::evaluate(&p);
    assert!(geometry.problems.is_empty(), "{:?}", geometry.problems);

    let simple = geometry.get(simple_id).unwrap();
    assert!(simple.children.is_empty());
    assert_eq!(simple.parts.len(), 1);
    assert!((simple.volume() - 2.0).abs() < 1e-9, "{}", simple.volume());

    let compound = geometry.get(compound_id).unwrap();
    assert_eq!(compound.parts.len(), 1);
    assert_eq!(compound.children.len(), 2);
    for part in &compound.parts {
        assert_eq!(part.mesh.check_closed(), Ok(()), "{}", part.name);
    }
    for child in &compound.children {
        assert_eq!(child.parts.len(), 1, "{}", child.role);
        assert!(child.axis.is_empty(), "{}", child.role);
        for part in &child.parts {
            assert_eq!(part.mesh.check_closed(), Ok(()), "{}", child.role);
            assert_eq!(part.material, Some(material));
        }
    }
    // The body is 2 × 1 × 1 m; each part is 1 × 1 × 0.5 m on top of it.
    assert!((compound.volume() - 3.0).abs() < 1e-9, "{}", compound.volume());
    let (lo, hi) = compound.bounds().unwrap();
    assert!((lo.z).abs() < 1e-9 && (hi.z - 1.5).abs() < 1e-9, "{lo} {hi}");
    let (plain_lo, plain_hi) = simple.bounds().unwrap();
    assert_eq!((plain_lo, plain_hi.z), (lo, 1.0));
    let roles: Vec<&str> = compound.children.iter().map(|c| c.role.as_str()).collect();
    assert_eq!(roles, ["Part_1", "Part_2"]);
}

/// NFR-REL-006: a generator that panics (the test kind asked to) lists its
/// element as a problem with the `geometry-evaluation-panicked` message and
/// does not stop the other elements, and the plan leaves it out.
#[test]
fn a_panicking_generator_lists_its_element_and_keeps_the_others() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let g = p.storeys.ground();
    let wall = p.new_wall(g, DVec2::ZERO, DVec2::new(4.0, 0.0), 0);
    let mut dummy = Dummy::new(DVec2::new(1.0, 1.0), DVec2::ONE, p.materials[0].id);
    dummy.panic_on_evaluate = true;
    let dummy = dummy.into_element(&p, g, 0);
    let (wall_id, dummy_id) = (wall.id, dummy.id);
    h.execute(&mut p, Command::AddElements(vec![wall, dummy])).unwrap();

    let geometry = ModelGeometry::evaluate(&p);
    assert!(geometry.get(wall_id).is_some(), "the wall still evaluates");
    assert!(geometry.get(dummy_id).is_none());
    assert_eq!(
        geometry.problems.get(&dummy_id),
        Some(&Message::new("geometry-evaluation-panicked"))
    );
    assert_eq!(geometry.problems.len(), 1, "{:?}", geometry.problems);

    let plan = plan_drawing(&p, &geometry, g, ViewRange::for_storey(&p, g), |_| true);
    let drawn: Vec<_> = plan.elements.iter().map(|e| e.id).collect();
    assert_eq!(drawn, [wall_id]);
}

/// SPC-001, ANN-007 (preparation): a test element without a material, as a
/// space will be, evaluates to a closed body whose part has no material,
/// without a problem, and the plan cuts no region from it and draws nothing
/// of it where the cut plane passes through it, but still its outline below
/// the cut plane.
#[test]
fn a_material_less_test_element_evaluates_without_a_cut_region() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let g = p.storeys.ground();
    let mut dummy = Dummy::without_material(DVec2::new(1.0, 1.0), DVec2::new(2.0, 1.0));
    dummy.link.top = TopLink::Fixed { height: 3.0 };
    let dummy = dummy.into_element(&p, g, 0);
    let id = dummy.id;
    h.execute(&mut p, Command::AddElements(vec![dummy])).unwrap();

    let geometry = ModelGeometry::evaluate(&p);
    assert!(geometry.problems.is_empty(), "{:?}", geometry.problems);
    let body = geometry.get(id).unwrap();
    assert_eq!(body.parts.len(), 1);
    assert_eq!(body.parts[0].material, None);
    assert_eq!(body.parts[0].mesh.check_closed(), Ok(()));
    assert!((body.volume() - 6.0).abs() < 1e-9, "{}", body.volume());

    let range = ViewRange::for_storey(&p, g);
    let (lo, hi) = body.bounds().unwrap();
    assert!(lo.z < range.cut && hi.z > range.cut, "the cut plane passes through it");
    let plan = plan_drawing(&p, &geometry, g, range, |_| true);
    assert!(plan.cut_regions.is_empty(), "{:?}", plan.cut_regions);
    let drawn = plan.elements.iter().find(|e| e.id == id).unwrap();
    assert!(drawn.cut.is_empty() && drawn.projected.is_empty() && drawn.overhead.is_empty());

    // Below the cut plane it is still drawn as an uncut outline.
    h.undo(&mut p).unwrap();
    let low = Dummy::without_material(DVec2::new(1.0, 1.0), DVec2::new(2.0, 1.0)).into_element(&p, g, 0);
    let low_id = low.id;
    h.execute(&mut p, Command::AddElements(vec![low])).unwrap();
    let geometry = ModelGeometry::evaluate(&p);
    let plan = plan_drawing(&p, &geometry, g, range, |_| true);
    assert!(plan.cut_regions.is_empty(), "{:?}", plan.cut_regions);
    let drawn = plan.elements.iter().find(|e| e.id == low_id).unwrap();
    assert!(drawn.cut.is_empty());
    assert!(!drawn.projected.is_empty());
}

/// ANN-002, ANN-007: texts and notes, the body-less test kind standing in
/// for markers and dimensions, evaluate to an empty geometry (no body, no
/// axis, no parts) without a problem, on the ground storey and on an upper
/// storey alike, while a wall beside them keeps its body; the plan slices
/// nothing of them and lists them among no drawn elements.
#[test]
fn texts_and_notes_evaluate_to_no_body_and_appear_in_no_plan() {
    let mut p = samples::two_storey_shell();
    let mut h = History::default();
    let g = p.storeys.ground();
    let upper = p.storeys.relative(g, 1).unwrap().id;
    let notes = vec![
        Note::new(g, DVec2::new(2.0, 2.0), "Kitchen").into_element(&p, g, 0),
        Note::new(upper, DVec2::new(3.0, 1.0), "Bedroom").into_element(&p, upper, 1),
        p.new_text(g, DVec2::new(2.0, 3.0), "Kitchen\n12 m²", 0),
        p.new_text(upper, DVec2::new(3.0, 2.0), "Bedroom", 1),
    ];
    let ids: Vec<ElementId> = notes.iter().map(|e| e.id).collect();
    h.execute(&mut p, Command::AddElements(notes)).unwrap();

    let geometry = ModelGeometry::evaluate(&p);
    assert!(geometry.problems.is_empty(), "{:?}", geometry.problems);
    for id in &ids {
        assert_eq!(geometry.get(*id), Some(&ElementGeometry::default()), "{id}");
        assert_eq!(geometry.get(*id).unwrap().bounds(), None);
    }
    assert!(p.elements.values().any(|e| e.as_wall().is_some()));
    for wall in p.elements.values().filter(|e| e.as_wall().is_some()) {
        assert!(!geometry.get(wall.id).unwrap().parts.is_empty(), "{}", wall.name);
    }

    for storey in [g, upper] {
        let plan = plan_drawing(&p, &geometry, storey, ViewRange::for_storey(&p, storey), |_| true);
        assert!(!plan.elements.is_empty(), "the walls are drawn");
        assert!(
            plan.elements.iter().all(|e| !ids.contains(&e.id)),
            "{:?}",
            plan.elements
        );
        let without: Project = {
            let mut q = p.clone();
            q.elements.retain(|id, _| !ids.contains(id));
            q
        };
        let reference = plan_drawing(
            &without,
            &ModelGeometry::evaluate(&without),
            storey,
            ViewRange::for_storey(&without, storey),
            |_| true,
        );
        assert_eq!(plan.cut_regions.len(), reference.cut_regions.len());
        for (with, without) in plan.cut_regions.iter().zip(&reference.cut_regions) {
            assert_eq!(with.polygons, without.polygons, "the notes add no cut region");
        }
    }
}
