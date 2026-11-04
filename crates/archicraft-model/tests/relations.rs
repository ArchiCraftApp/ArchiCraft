//! Relations between elements (SRS 3.3): the identities of relations, the
//! store of the explicit ones and the structured integrity problems that
//! report dangling references (UI-012, NFR-REL-004).

#![allow(clippy::unwrap_used)]

mod common;

use archicraft_model::glam::DVec2;
use archicraft_model::relations::EXPLICIT_CODES;
use archicraft_model::template::default_metric_project;
use archicraft_model::{
    Arg, Command, ElementId, History, IntegrityProblem, MaterialId, Project, Relation, RelationStore, StoreyRole,
};
use common::wall;
use uuid::Version;

/// A project with two walls added by command, and their ids.
fn two_walls() -> (Project, ElementId, ElementId) {
    let mut p = default_metric_project();
    let mut h = History::default();
    let ground = p.storeys.ground();
    let walls = [
        wall(&p, (0.0, 0.0), (5.0, 0.0)),
        p.new_wall(ground, DVec2::new(0.0, 1.0), DVec2::new(5.0, 1.0), 1),
    ];
    let [a, b] = walls.each_ref().map(|w| w.id);
    h.execute(&mut p, Command::AddElements(walls.to_vec())).unwrap();
    (p, a, b)
}

/// SRS 3.3: a relation's identity is a version 5 UUID of its variant and
/// its ends, so re-creating a relation gives the same id, while another
/// variant with the same ends, or the ends the other way round, is another
/// relation.
#[test]
fn relation_ids_are_derived_from_the_variant_and_the_ends() {
    let (a, b) = (ElementId::new(), ElementId::new());
    let host = Relation::Host { hosted: a, host: b };
    assert_eq!(host.id(), Relation::Host { hosted: a, host: b }.id());
    assert_eq!(host.id().uuid().get_version(), Some(Version::Sha1));
    assert_ne!(host.id(), Relation::Host { hosted: b, host: a }.id());
    assert_ne!(host.id(), Relation::Trim { target: a, by: b }.id());
    assert_ne!(host.id(), Relation::Junction { a, b }.id());
    assert_ne!(
        Relation::SolidOperation { target: a, operator: b }.id(),
        Relation::AnnotationBinding {
            annotation: a,
            target: b
        }
        .id()
    );
    assert_eq!(host.ends(), Some((a, b)));
    assert_eq!(Relation::Junction { a, b }.ends(), Some((a, b)));
    assert!(!host.is_derived());

    let storey = archicraft_model::StoreyId::new();
    let link = Relation::StoreyLink {
        element: a,
        storey,
        role: StoreyRole::Top,
    };
    assert!(link.is_derived());
    assert_eq!(link.ends(), None);
    assert_ne!(
        link.id(),
        Relation::StoreyLink {
            element: a,
            storey,
            role: StoreyRole::Base,
        }
        .id()
    );

    // The file codes name every explicit variant and come back as the
    // same relation (IFX-060).
    let explicit = [
        host,
        Relation::Junction { a, b },
        Relation::Trim { target: a, by: b },
        Relation::SolidOperation { target: a, operator: b },
        Relation::AnnotationBinding {
            annotation: a,
            target: b,
        },
    ];
    for relation in explicit {
        assert_eq!(
            Relation::from_code(relation.code(), a, b),
            Some(relation),
            "{relation:?}"
        );
    }
    let codes: Vec<&str> = explicit.iter().map(Relation::code).collect();
    assert_eq!(
        codes, EXPLICIT_CODES,
        "the schema's enumeration names every explicit variant"
    );
    assert_eq!(Relation::from_code("storey_link", a, b), None);
    assert_eq!(Relation::from_code("glue", a, b), None);
}

/// SRS 3.3: the store holds each explicit relation once, in the order of
/// their ids whatever the insertion order, never a derived storey link, and
/// serialises as the list of its relations.
#[test]
fn the_store_keeps_each_explicit_relation_once_in_id_order() {
    let (a, b, c) = (ElementId::new(), ElementId::new(), ElementId::new());
    let host = Relation::Host { hosted: a, host: b };
    let trim = Relation::Trim { target: c, by: b };
    let mut store = RelationStore::default();
    assert!(store.is_empty());
    assert!(store.insert(host));
    assert!(!store.insert(host), "a duplicate changes nothing");
    assert!(store.insert(trim));
    assert_eq!(store.len(), 2);
    assert!(store.contains(host.id()));
    assert_eq!(store.get(trim.id()), Some(&trim));
    assert!(
        !store.insert(Relation::StoreyLink {
            element: a,
            storey: archicraft_model::StoreyId::new(),
            role: StoreyRole::Base,
        }),
        "derived relations are not stored"
    );
    assert_eq!(store.len(), 2);

    let mut reversed = RelationStore::default();
    reversed.insert(trim);
    reversed.insert(host);
    assert_eq!(store, reversed);
    let order: Vec<Relation> = store.iter().map(|(_, r)| *r).collect();
    let reversed_order: Vec<Relation> = reversed.iter().map(|(_, r)| *r).collect();
    assert_eq!(order, reversed_order, "the iteration order follows the ids");
    let ids: Vec<_> = store.iter().map(|(id, _)| id).collect();
    assert!(ids.windows(2).all(|w| w[0] < w[1]));

    let json = serde_json::to_value(&store).unwrap();
    assert!(json.is_array(), "a list of relations: {json}");
    let read: RelationStore = serde_json::from_value(json).unwrap();
    assert_eq!(read, store);

    assert_eq!(store.remove(host.id()), Some(host));
    assert_eq!(store.remove(host.id()), None);
    assert_eq!(store.len(), 1);
    let starting_at_c: Vec<&Relation> = store.starting_at(c).collect();
    assert_eq!(starting_at_c, [&trim]);
    assert_eq!(store.starting_at(b).count(), 0, "b is the second end of the trim");
}

/// SRS 3.3: the explicit relations are part of the project, so two
/// projects differ by a relation alone and are equal with the same ones.
#[test]
fn project_equality_compares_the_explicit_relations() {
    let (project, a, b) = two_walls();
    let mut related = project.clone();
    assert!(related.relations.insert(Relation::Host { hosted: a, host: b }));
    assert_ne!(related, project);
    let mut same = project.clone();
    same.relations.insert(Relation::Host { hosted: a, host: b });
    assert_eq!(related, same);
    assert!(project.integrity_problems().is_empty());
    assert!(related.integrity_problems().is_empty());
}

/// UI-012, NFR-REL-004: a relation whose end names no element is reported
/// as a structured problem that carries the element that still exists and
/// the relation, so a problems panel can select the element.
#[test]
fn a_dangling_relation_end_is_reported_with_the_element_that_exists() {
    let (mut p, hosted, host) = two_walls();
    let relation = Relation::Host { hosted, host };
    p.relations.insert(relation);
    p.elements.shift_remove(&host);
    let problems = p.integrity_problems();
    assert_eq!(problems.len(), 1, "{problems:#?}");
    let problem = &problems[0];
    assert_eq!(problem.element, Some(hosted));
    assert_eq!(problem.relation, Some(relation.id()));
    assert_eq!(problem.message.key(), "model-problem-relation-end-missing");
    assert_eq!(problem.message.arg("name"), Some(&Arg::Text("W-001".into())));
    match problem.message.arg("relation") {
        Some(Arg::Nested(kind)) => assert_eq!(kind.key(), "model-relation-host"),
        other => panic!("the relation's kind as a nested message, got {other:?}"),
    }

    // The missing end may be the first one.
    let (mut p, hosted, host) = two_walls();
    p.relations.insert(Relation::Host { hosted, host });
    p.elements.shift_remove(&hosted);
    let problems = p.integrity_problems();
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(problems[0].element, Some(host));
    assert_eq!(problems[0].message.arg("name"), Some(&Arg::Text("W-002".into())));
}

/// UI-012: a relation whose ends are both missing is still reported, with
/// the relation but without an element to select.
#[test]
fn a_relation_with_both_ends_missing_is_reported_without_an_element() {
    let (mut p, hosted, host) = two_walls();
    let relation = Relation::Trim {
        target: hosted,
        by: host,
    };
    p.relations.insert(relation);
    p.elements.clear();
    let problems = p.integrity_problems();
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(
        problems[0],
        IntegrityProblem {
            element: None,
            relation: Some(relation.id()),
            message: problems[0].message.clone(),
        }
    );
    assert_eq!(problems[0].message.key(), "model-problem-relation-ends-missing");
    match problems[0].message.arg("relation") {
        Some(Arg::Nested(kind)) => assert_eq!(kind.key(), "model-relation-trim"),
        other => panic!("the relation's kind as a nested message, got {other:?}"),
    }
}

/// UI-012, NFR-REL-004: the storey, layer and material checks report in the
/// same structured form, naming the element and no relation.
#[test]
fn a_missing_material_is_still_reported_in_the_structured_form() {
    let (mut p, a, _) = two_walls();
    p.elements[&a].kind.set_material(MaterialId::new());
    let problems = p.integrity_problems();
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_eq!(problems[0].element, Some(a));
    assert_eq!(problems[0].relation, None);
    assert_eq!(problems[0].message.key(), "model-problem-material-missing");
    assert_eq!(problems[0].message.arg("name"), Some(&Arg::Text("W-001".into())));
}
