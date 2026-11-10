//! Relations between elements (SRS 3.3): the identities of relations, the
//! store of the explicit ones and the structured integrity problems that
//! report dangling references (UI-012, NFR-REL-004); the relation commands
//! with their validation and exact undo, the derived storey-link index and
//! the dependency queries (STR-006).

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::{BTreeMap, BTreeSet};

use archicraft_model::command::{Edit, StoreyElements};
use archicraft_model::glam::DVec2;
use archicraft_model::relations::EXPLICIT_CODES;
use archicraft_model::template::default_metric_project;
use archicraft_model::tolerance::MODEL_TOLERANCE;
use archicraft_model::{
    Arg, Command, CommandError, Element, ElementId, History, IntegrityProblem, MaterialId, Message, Project, Relation,
    RelationId, RelationStore, Storey, StoreyId, StoreyRole, TopLink,
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

/// Three walls W-001, W-002 and W-003 on the ground storey, added by
/// command, with the history and their ids.
fn three_walls() -> (Project, History, [ElementId; 3]) {
    let mut p = default_metric_project();
    let mut h = History::default();
    let ground = p.storeys.ground();
    let walls: Vec<Element> = (0..3)
        .map(|n| {
            let y = f64::from(n);
            p.new_wall(ground, DVec2::new(0.0, y), DVec2::new(5.0, y), n)
        })
        .collect();
    let ids = [walls[0].id, walls[1].id, walls[2].id];
    h.execute(&mut p, Command::AddElements(walls)).unwrap();
    (p, h, ids)
}

/// The elements with these ids, as a set.
fn set(ids: &[ElementId]) -> BTreeSet<ElementId> {
    ids.iter().copied().collect()
}

/// The message of a rejected command, with the variant it came in.
fn rejection(result: Result<(), CommandError>) -> (&'static str, Message) {
    match result {
        Err(e @ CommandError::Invalid(_)) => ("Invalid", e.message()),
        Err(e @ CommandError::Locked(_)) => ("Locked", e.message()),
        Err(e @ CommandError::NotFound(_)) => ("NotFound", e.message()),
        Err(e @ CommandError::NoChange) => ("NoChange", e.message()),
        Ok(()) => panic!("the command was accepted"),
    }
}

/// The key of a nested message argument.
fn nested_key(message: &Message, name: &str) -> String {
    match message.arg(name) {
        Some(Arg::Nested(inner)) => inner.key().to_owned(),
        other => panic!("{message}: {name} is not a nested message: {other:?}"),
    }
}

/// Asserts that `p` is `expected` apart from the cache revision, with the
/// elements and the relations in the same order and the same derived index
/// (UND-001).
#[track_caller]
fn assert_exact(p: &Project, expected: &Project) {
    assert_eq!(
        *p,
        Project {
            revision: p.revision,
            ..expected.clone()
        }
    );
    let elements = |p: &Project| p.elements.keys().copied().collect::<Vec<_>>();
    assert_eq!(elements(p), elements(expected));
    let relations = |p: &Project| p.relations.iter().map(|(id, r)| (id, *r)).collect::<Vec<_>>();
    assert_eq!(relations(p), relations(expected), "the store's order");
    assert!(p.relations.same_index(&expected.relations), "the derived index");
}

/// The explicit relations of `p` in store order.
fn explicit(p: &Project) -> Vec<Relation> {
    p.relations.iter().map(|(_, r)| *r).collect()
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

/// SRS 3.3, UND-001: recording a host relation and removing it again are a
/// command each; undo and redo give back the exact project, with the
/// relations in the same order, and `host_of` names the host while the
/// relation exists.
#[test]
fn inserting_and_removing_a_host_relation_undo_and_redo_exactly() {
    let (mut p, mut h, [a, b, c]) = three_walls();
    // A second relation, so that the store's order is checked as well.
    let junction = Relation::Junction { a: b, b: c };
    h.execute(&mut p, Command::InsertRelation(junction)).unwrap();
    let host = Relation::Host { hosted: a, host: b };
    let empty = p.clone();
    h.execute(&mut p, Command::InsertRelation(host)).unwrap();
    assert_eq!(h.undo_label(), Some(Message::new("model-label-insert-relation")));
    assert_eq!(p.relations.host_of(a), Some(b), "the hosted element names its host");
    assert_eq!(p.relations.host_of(b), None, "the host itself has none");
    assert_eq!(p.relations.host_of(c), None);
    let mut expected = vec![junction, host];
    expected.sort_by_key(Relation::id);
    assert_eq!(explicit(&p), expected);
    let hosted = p.clone();

    h.execute(&mut p, Command::RemoveRelation(host.id())).unwrap();
    assert_eq!(h.undo_label(), Some(Message::new("model-label-remove-relation")));
    assert_eq!(p.relations.host_of(a), None);
    assert_eq!(explicit(&p), [junction]);
    let removed = p.clone();

    h.undo(&mut p).unwrap();
    assert_exact(&p, &hosted);
    assert_eq!(p.relations.host_of(a), Some(b));
    h.undo(&mut p).unwrap();
    assert_exact(&p, &empty);
    h.redo(&mut p).unwrap();
    assert_exact(&p, &hosted);
    h.redo(&mut p).unwrap();
    assert_exact(&p, &removed);
}

/// SRS 3.3, NFR-REL-005: a relation with a missing end, a relation of an
/// element with itself, one recorded already (a junction also the other way
/// round), a second host for a hosted element, a host relation that would
/// close a cycle of hosts and a derived storey link are refused as invalid,
/// and the project and its history stay as they were. Removing a relation
/// that does not exist is refused as not found, a storey link as invalid.
#[test]
fn invalid_relations_are_refused_and_leave_the_project_unchanged() {
    let (mut p, mut h, [a, b, c]) = three_walls();
    // W-001 is hosted by W-002, which is hosted by W-003; W-001 and W-003 meet.
    for relation in [
        Relation::Host { hosted: a, host: b },
        Relation::Host { hosted: b, host: c },
        Relation::Junction { a, b: c },
    ] {
        h.execute(&mut p, Command::InsertRelation(relation)).unwrap();
    }
    let missing = ElementId::new();
    let ground = p.storeys.ground();
    let cases = [
        (
            Relation::Trim { target: a, by: missing },
            "model-error-relation-end-missing",
        ),
        (
            Relation::Trim { target: missing, by: a },
            "model-error-relation-end-missing",
        ),
        (Relation::Junction { a: c, b: c }, "model-error-relation-self"),
        (Relation::Host { hosted: a, host: b }, "model-error-relation-duplicate"),
        (Relation::Junction { a: c, b: a }, "model-error-relation-duplicate"),
        (Relation::Host { hosted: a, host: c }, "model-error-host-twice"),
        (Relation::Host { hosted: c, host: a }, "model-error-host-cycle"),
        (
            Relation::StoreyLink {
                element: a,
                storey: ground,
                role: StoreyRole::Base,
            },
            "model-error-relation-derived",
        ),
    ];
    let mut messages = BTreeMap::new();
    for (relation, key) in cases {
        let before = p.clone();
        let steps = h.undo_len();
        let (variant, message) = rejection(h.execute(&mut p, Command::InsertRelation(relation)));
        assert_eq!((variant, message.key()), ("Invalid", key), "{relation:?}");
        assert_exact(&p, &before);
        assert_eq!(h.undo_len(), steps, "{relation:?}: nothing was recorded");
        messages.insert(key, message);
    }
    // The messages name the element and the kind of relation.
    let w = |n: &str| Some(Arg::Text(n.to_owned()));
    let end_missing = &messages["model-error-relation-end-missing"];
    assert_eq!(nested_key(end_missing, "relation"), "model-relation-trim");
    let own = &messages["model-error-relation-self"];
    assert_eq!(own.arg("name").cloned(), w("W-003"));
    assert_eq!(nested_key(own, "relation"), "model-relation-junction");
    let duplicate = &messages["model-error-relation-duplicate"];
    assert_eq!(duplicate.arg("name").cloned(), w("W-003"), "the junction's first end");
    assert_eq!(nested_key(duplicate, "relation"), "model-relation-junction");
    assert_eq!(messages["model-error-host-twice"].arg("name").cloned(), w("W-001"));
    assert_eq!(messages["model-error-host-cycle"].arg("name").cloned(), w("W-003"));

    // The shortest cycle: W-002 hosted by the W-001 it hosts.
    let (mut q, mut hq, [x, y, _]) = three_walls();
    hq.execute(&mut q, Command::InsertRelation(Relation::Host { hosted: x, host: y }))
        .unwrap();
    let before = q.clone();
    let (variant, message) =
        rejection(hq.execute(&mut q, Command::InsertRelation(Relation::Host { hosted: y, host: x })));
    assert_eq!((variant, message.key()), ("Invalid", "model-error-host-cycle"));
    assert_exact(&q, &before);

    // Removing: an unknown identity is not found, a storey link is derived.
    let before = p.clone();
    let unknown = Relation::Trim { target: c, by: b }.id();
    let (variant, message) = rejection(h.execute(&mut p, Command::RemoveRelation(unknown)));
    assert_eq!((variant, message.key()), ("NotFound", "model-error-relation-not-found"));
    let link = Relation::StoreyLink {
        element: a,
        storey: ground,
        role: StoreyRole::Base,
    };
    assert!(p.relations.relations_of(a).any(|r| r == link), "the link exists");
    let (variant, message) = rejection(h.execute(&mut p, Command::RemoveRelation(link.id())));
    assert_eq!((variant, message.key()), ("Invalid", "model-error-relation-derived"));
    assert_exact(&p, &before);
}

/// SRS 3.3, UND-001: deleting an element removes every explicit relation it
/// takes part in through RemoveRelation edits, which the transaction's
/// forward edits hold before the element removals, each relation once; the
/// other relations stay, and undo restores the exact project, putting the
/// elements back before their relations.
#[test]
fn deleting_an_element_removes_its_relations_and_undo_restores_them() {
    let (mut p, mut h, [a, b, c]) = three_walls();
    let host = Relation::Host { hosted: a, host: b };
    let junction = Relation::Junction { a: b, b: c };
    let trim = Relation::Trim { target: c, by: a };
    for relation in [host, junction, trim] {
        h.execute(&mut p, Command::InsertRelation(relation)).unwrap();
    }
    let before = p.clone();

    h.execute(&mut p, Command::DeleteElements(vec![b])).unwrap();
    let transaction = h.transactions().last().unwrap();
    let removed: Vec<RelationId> = transaction
        .forward()
        .iter()
        .filter_map(|edit| match edit {
            Edit::RemoveRelation(id) => Some(*id),
            _ => None,
        })
        .collect();
    let mut expected = vec![host.id(), junction.id()];
    expected.sort();
    assert_eq!(removed, expected, "the relations of W-002, in identity order");
    let first_removal = transaction
        .forward()
        .iter()
        .position(|edit| matches!(edit, Edit::RemoveElement { .. }))
        .unwrap();
    assert_eq!(first_removal, removed.len(), "the relations go first");
    let inverse = transaction.inverse();
    let last_insert = inverse
        .iter()
        .rposition(|edit| matches!(edit, Edit::InsertElement { .. }))
        .unwrap();
    let first_relation = inverse
        .iter()
        .position(|edit| matches!(edit, Edit::InsertRelation(_)))
        .unwrap();
    assert!(
        last_insert < first_relation,
        "undo puts the element back first: {inverse:?}"
    );
    assert_eq!(explicit(&p), [trim]);
    assert!(p.integrity_problems().is_empty(), "no relation is left dangling");
    assert_eq!(p.relations.dependents_of(c), BTreeSet::new());
    let deleted = p.clone();

    h.undo(&mut p).unwrap();
    assert_exact(&p, &before);
    assert_eq!(p.relations.host_of(a), Some(b));
    h.redo(&mut p).unwrap();
    assert_exact(&p, &deleted);
    h.undo(&mut p).unwrap();

    // Deleting both ends removes their shared relation once.
    h.execute(&mut p, Command::DeleteElements(vec![a, b])).unwrap();
    let removed = h
        .transactions()
        .last()
        .unwrap()
        .forward()
        .iter()
        .filter(|edit| matches!(edit, Edit::RemoveRelation(_)))
        .count();
    assert_eq!(removed, 3, "the host, the junction and the trim");
    assert!(p.relations.is_empty());
    h.undo(&mut p).unwrap();
    assert_exact(&p, &before);
}

/// STR-004, SRS 3.3: deleting a storey with its elements removes their
/// relations the same way, those to elements on other storeys too, and undo
/// restores them; moving the elements to another storey keeps them.
#[test]
fn deleting_a_storey_with_its_elements_removes_their_relations() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let (ground, first) = (p.storeys.ground(), p.storeys.as_slice()[1].id);
    let lower = wall(&p, (0.0, 0.0), (5.0, 0.0));
    let mut upper = p.new_wall(first, DVec2::ZERO, DVec2::new(5.0, 0.0), 1);
    if let Some(w) = upper.as_wall_mut() {
        w.top = TopLink::Fixed { height: 2.5 };
    }
    let ids = (lower.id, upper.id);
    h.execute(&mut p, Command::AddElements(vec![lower, upper])).unwrap();
    let (lower, upper) = ids;
    let trim = Relation::Trim {
        target: upper,
        by: lower,
    };
    h.execute(&mut p, Command::InsertRelation(trim)).unwrap();
    let before = p.clone();

    h.execute(
        &mut p,
        Command::DeleteStorey {
            storey: first,
            elements: StoreyElements::Delete,
        },
    )
    .unwrap();
    let forward = h.transactions().last().unwrap().forward();
    assert_eq!(forward[0], Edit::RemoveRelation(trim.id()));
    assert!(forward.contains(&Edit::RemoveElement { id: upper }));
    assert!(p.relations.is_empty());
    assert!(p.integrity_problems().is_empty());
    h.undo(&mut p).unwrap();
    assert_exact(&p, &before);

    h.execute(
        &mut p,
        Command::DeleteStorey {
            storey: first,
            elements: StoreyElements::MoveTo(ground),
        },
    )
    .unwrap();
    assert_eq!(explicit(&p), [trim], "a moved element keeps its relations");
    assert_eq!(p.relations.storey_dependents(ground), set(&[lower, upper]));
}

/// SRS 3.3: each relation says which end depends on which. A hosted element
/// and its host depend on each other (the hosted element is placed on its
/// host and cuts it), as do the two elements of a junction; a trimmed
/// element depends on what trims it, the target of a solid operation on the
/// operator and an annotation on what it refers to. `neighbours` lists the
/// other end whichever way it depends.
#[test]
fn dependents_follow_the_direction_of_each_relation() {
    let (x, y) = (ElementId::new(), ElementId::new());
    // The relation from x to y, the dependents of x and those of y.
    let cases = [
        (Relation::Host { hosted: x, host: y }, set(&[y]), set(&[x])),
        (Relation::Junction { a: x, b: y }, set(&[y]), set(&[x])),
        (Relation::Trim { target: x, by: y }, set(&[]), set(&[x])),
        (Relation::SolidOperation { target: x, operator: y }, set(&[]), set(&[x])),
        (
            Relation::AnnotationBinding {
                annotation: x,
                target: y,
            },
            set(&[]),
            set(&[x]),
        ),
    ];
    for (relation, of_x, of_y) in cases {
        let mut store = RelationStore::default();
        assert!(store.insert(relation));
        assert_eq!(
            store.dependents_of(x),
            of_x,
            "{relation:?}: dependents of the first end"
        );
        assert_eq!(
            store.dependents_of(y),
            of_y,
            "{relation:?}: dependents of the second end"
        );
        assert_eq!(store.neighbours(x), set(&[y]), "{relation:?}");
        assert_eq!(store.neighbours(y), set(&[x]), "{relation:?}");
        assert_eq!(store.relations_of(x).collect::<Vec<_>>(), [relation]);
        assert_eq!(store.relations_of(y).collect::<Vec<_>>(), [relation]);
        assert_eq!(store.remove(relation.id()), Some(relation));
        for end in [x, y] {
            assert!(store.dependents_of(end).is_empty(), "{relation:?}: removed");
            assert!(store.neighbours(end).is_empty(), "{relation:?}: removed");
            assert_eq!(store.relations_of(end).count(), 0, "{relation:?}: removed");
        }
    }
}

/// STR-006, SRS 3.3: a storey link makes its element depend on the storey,
/// never the other way round: the element is a storey dependent of its home
/// storey (its base) and of the storey its top is linked to, and the link
/// makes no element depend on it. `relations_of` lists its explicit
/// relations first, then the links, the home storey first.
#[test]
fn a_storey_link_makes_the_element_depend_on_the_storey() {
    let p = default_metric_project();
    let (ground, first) = (p.storeys.ground(), p.storeys.as_slice()[1].id);
    let mut w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    if let Some(wall) = w.as_wall_mut() {
        wall.top = TopLink::Storey {
            storey: first,
            offset: 0.0,
        };
    }
    let mut store = RelationStore::default();
    store.update_element(w.id, Some(&w));
    assert_eq!(store.storey_dependents(ground), set(&[w.id]));
    assert_eq!(store.storey_dependents(first), set(&[w.id]));
    assert!(
        store.dependents_of(w.id).is_empty(),
        "a storey link makes no element depend on it"
    );
    assert!(store.neighbours(w.id).is_empty());
    let base = Relation::StoreyLink {
        element: w.id,
        storey: ground,
        role: StoreyRole::Base,
    };
    let top = Relation::StoreyLink {
        element: w.id,
        storey: first,
        role: StoreyRole::Top,
    };
    assert_eq!(store.relations_of(w.id).collect::<Vec<_>>(), [base, top]);

    let other = ElementId::new();
    let junction = Relation::Junction { a: other, b: w.id };
    store.insert(junction);
    assert_eq!(store.relations_of(w.id).collect::<Vec<_>>(), [junction, base, top]);

    // A fixed top follows no storey; the base still follows the home storey.
    if let Some(wall) = w.as_wall_mut() {
        wall.top = TopLink::Fixed { height: 2.5 };
    }
    store.update_element(w.id, Some(&w));
    assert_eq!(store.storey_dependents(ground), set(&[w.id]));
    assert!(store.storey_dependents(first).is_empty());
    assert_eq!(store.relations_of(w.id).collect::<Vec<_>>(), [junction, base]);

    // A removed element has no links; its explicit relation is the
    // integrity check's to report.
    store.update_element(w.id, None);
    assert!(store.storey_dependents(ground).is_empty());
    assert_eq!(store.relations_of(w.id).collect::<Vec<_>>(), [junction]);
}

/// STR-006: `storey_dependents` finds every element linked to a storey:
/// those homed on it, those whose top is linked to it by its identity, and,
/// through its home storey, an element whose top is linked relative to it
/// ("home + 1", the template's default), also above the top storey, where
/// the link reaches by extrapolation. So a storey edit finds the elements
/// that follow it among the dependents of the storeys whose elevation or
/// height it changes.
#[test]
fn storey_dependents_find_every_element_that_follows_a_storey() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let (ground, first) = (p.storeys.ground(), p.storeys.as_slice()[1].id);
    let second = Storey::new("02 Second floor", 0.0, 3.0);
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
    let with_top = |home: StoreyId, nth: u32, top: TopLink| {
        let mut e = p.new_wall(
            home,
            DVec2::new(0.0, f64::from(nth)),
            DVec2::new(5.0, f64::from(nth)),
            nth,
        );
        if let Some(w) = e.as_wall_mut() {
            w.top = top;
        }
        e
    };
    let relative = TopLink::Relative {
        storeys: 1,
        offset: 0.0,
    };
    let a = with_top(ground, 0, relative);
    let b = with_top(
        ground,
        1,
        TopLink::Storey {
            storey: second_id,
            offset: 0.0,
        },
    );
    let c = with_top(first, 2, TopLink::Fixed { height: 2.5 });
    // On the top storey, "home + 1" reaches above it by extrapolation.
    let d = with_top(second_id, 3, relative);
    let slab = p.new_slab(
        first,
        vec![
            DVec2::ZERO,
            DVec2::new(5.0, 0.0),
            DVec2::new(5.0, 4.0),
            DVec2::new(0.0, 4.0),
        ],
        Vec::new(),
        0,
    );
    let ids = [a.id, b.id, c.id, d.id, slab.id];
    h.execute(&mut p, Command::AddElements(vec![a, b, c, d, slab])).unwrap();
    let [a, b, c, d, slab] = ids;
    assert_eq!(p.relations.storey_dependents(ground), set(&[a, b]));
    assert_eq!(p.relations.storey_dependents(first), set(&[c, slab]));
    assert_eq!(p.relations.storey_dependents(second_id), set(&[b, d]));

    let commands = [
        Command::SetStoreyElevation {
            storey: first,
            elevation: 3.2,
            shift_above: false,
        },
        Command::SetStoreyElevation {
            storey: second_id,
            elevation: 6.5,
            shift_above: true,
        },
        Command::SetStoreyHeight {
            storey: ground,
            height: 3.3,
            shift_above: true,
        },
        // The top storey's nominal height moves the extrapolated top only.
        Command::SetStoreyHeight {
            storey: second_id,
            height: 3.5,
            shift_above: false,
        },
    ];
    for command in commands {
        let label = format!("{command:?}");
        let before = p.clone();
        h.execute(&mut p, command).unwrap();
        let changed: Vec<StoreyId> = before
            .storeys
            .iter()
            .filter(|old| {
                p.storeys
                    .get(old.id)
                    .is_none_or(|new| new.elevation != old.elevation || new.height != old.height)
            })
            .map(|s| s.id)
            .collect();
        let candidates: BTreeSet<ElementId> = changed
            .iter()
            .flat_map(|s| before.relations.storey_dependents(*s))
            .collect();
        let moved: BTreeSet<ElementId> = before
            .elements
            .values()
            .filter(|e| {
                let (b0, t0) = before.vertical_extent(e).unwrap();
                let (b1, t1) = p.vertical_extent(p.element(e.id).unwrap()).unwrap();
                (b1 - b0).abs() > MODEL_TOLERANCE || (t1 - t0).abs() > MODEL_TOLERANCE
            })
            .map(|e| e.id)
            .collect();
        assert!(!moved.is_empty(), "{label}: something follows");
        assert!(
            moved.is_subset(&candidates),
            "{label}: moved {moved:?}, found {candidates:?}"
        );
        h.undo(&mut p).unwrap();
    }
}

/// SRS 3.3: the derived index is not part of a project's equality: two
/// projects with the same explicit relations are equal whatever their
/// indexes hold, and the index kept up to date edit by edit is the one a
/// rebuild from scratch gives.
#[test]
fn project_equality_ignores_the_derived_index() {
    let (mut p, mut h, [a, b, _]) = three_walls();
    h.execute(&mut p, Command::InsertRelation(Relation::Host { hosted: a, host: b }))
        .unwrap();
    let ground = p.storeys.ground();
    let mut stale = p.clone();
    stale.relations.rebuild(std::iter::empty());
    assert!(!stale.relations.same_index(&p.relations), "the indexes differ");
    assert!(stale.relations.storey_dependents(ground).is_empty());
    assert_eq!(p.relations.storey_dependents(ground).len(), 3);
    assert_eq!(stale, p, "projects with the same explicit relations are equal");
    assert_eq!(stale.relations, p.relations);
    assert_eq!(
        stale.relations.host_of(a),
        Some(b),
        "the explicit relations' ends stay indexed"
    );

    let mut rebuilt = p.clone();
    rebuilt.rebuild_indexes();
    assert!(rebuilt.relations.same_index(&p.relations));
    stale.rebuild_indexes();
    assert!(stale.relations.same_index(&p.relations));
}

/// A small seeded generator (SplitMix64), so that the random sequence of
/// the brute-force test is the same on every run.
#[derive(Debug)]
struct Seeded(u64);

impl Seeded {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number below `n`, which is positive.
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).unwrap()
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())]
    }
}

/// STR-006 by brute force: the elements homed on `storey` and the walls
/// whose top is linked to it by its identity. A top linked relative to the
/// home storey counts the home storey. Only walls and slabs are expected.
fn linked_to(p: &Project, storey: StoreyId) -> BTreeSet<ElementId> {
    p.elements
        .values()
        .filter(|e| {
            let top = match e.as_wall() {
                Some(wall) => match wall.top {
                    TopLink::Storey { storey, .. } => Some(storey),
                    TopLink::Fixed { .. } | TopLink::Relative { .. } => None,
                },
                None => {
                    assert!(e.as_slab().is_some(), "{} is a wall or a slab", e.name);
                    None
                }
            };
            e.storey == storey || top == Some(storey)
        })
        .map(|e| e.id)
        .collect()
}

/// A top for a wall on `home`: a fixed height, a link to `home` or a storey
/// above it by its identity, or a relative link one or two storeys up.
fn random_top(rng: &mut Seeded, p: &Project, home: StoreyId) -> TopLink {
    let at = p.storeys.index_of(home).unwrap();
    match rng.below(3) {
        0 => TopLink::Fixed { height: 2.5 },
        1 => {
            let above: Vec<StoreyId> = p.storeys.as_slice()[at..].iter().map(|s| s.id).collect();
            let storey = rng.pick(&above);
            let offset = if storey == home { 2.5 } else { 0.0 };
            TopLink::Storey { storey, offset }
        }
        _ => TopLink::Relative {
            storeys: 1 + i32::from(rng.below(2) == 1),
            offset: 0.0,
        },
    }
}

/// A new element on a random storey: mostly a wall with a random top,
/// sometimes a slab.
fn random_element(rng: &mut Seeded, p: &Project, nth: u32) -> Element {
    let storeys: Vec<StoreyId> = p.storeys.iter().map(|s| s.id).collect();
    let home = rng.pick(&storeys);
    let x = f64::from(nth % 40);
    if rng.below(5) == 0 {
        let square = vec![
            DVec2::new(x, 0.0),
            DVec2::new(x + 1.0, 0.0),
            DVec2::new(x + 1.0, 1.0),
            DVec2::new(x, 1.0),
        ];
        return p.new_slab(home, square, Vec::new(), nth);
    }
    let mut e = p.new_wall(home, DVec2::new(x, 2.0), DVec2::new(x, 6.0), nth);
    let top = random_top(rng, p, home);
    if let Some(wall) = e.as_wall_mut() {
        wall.top = top;
    }
    e
}

/// A random step: adds, top-link and home changes, deletions of elements
/// and storeys, inserted storeys and explicit relations. `None` for undo
/// and redo, which the caller does.
fn random_command(rng: &mut Seeded, p: &Project, step: u32) -> (&'static str, Option<Command>) {
    let ids: Vec<ElementId> = p.elements.keys().copied().collect();
    let storeys: Vec<StoreyId> = p.storeys.iter().map(|s| s.id).collect();
    let choice = rng.below(16);
    if ids.len() < 2 && (3..11).contains(&choice) {
        return ("add", Some(Command::AddElements(vec![random_element(rng, p, 0)])));
    }
    match choice {
        0..=2 => {
            let elements = (0..=rng.below(3) as u32).map(|n| random_element(rng, p, n)).collect();
            ("add", Some(Command::AddElements(elements)))
        }
        3..=5 => {
            let mut e = p.element(rng.pick(&ids)).unwrap().clone();
            if e.as_wall().is_none() {
                return (
                    "update",
                    Some(Command::MoveElements {
                        ids: vec![e.id],
                        delta: DVec2::X,
                    }),
                );
            }
            if rng.below(3) == 0 {
                e.storey = rng.pick(&storeys);
            }
            let top = random_top(rng, p, e.storey);
            if let Some(wall) = e.as_wall_mut() {
                wall.top = top;
            }
            ("update", Some(Command::UpdateElements(vec![e])))
        }
        6 => {
            let doomed = (0..=rng.below(2)).map(|_| rng.pick(&ids)).collect();
            ("delete", Some(Command::DeleteElements(doomed)))
        }
        7 if storeys.len() > 2 => {
            let storey = rng.pick(&storeys);
            let elements = match rng.below(2) {
                0 => StoreyElements::Delete,
                _ => StoreyElements::MoveTo(rng.pick(&storeys)),
            };
            ("delete storey", Some(Command::DeleteStorey { storey, elements }))
        }
        7 | 8 => (
            "insert storey",
            Some(Command::InsertStorey {
                reference: rng.pick(&storeys),
                above: rng.below(2) == 0,
                storey: Storey::new(format!("S{step}"), 0.0, 2.8),
            }),
        ),
        9 | 10 => {
            let (first, second) = (rng.pick(&ids), rng.pick(&ids));
            let relation = match rng.below(3) {
                0 => Relation::Host {
                    hosted: first,
                    host: second,
                },
                1 => Relation::Junction { a: first, b: second },
                _ => Relation::Trim {
                    target: first,
                    by: second,
                },
            };
            let stored = p.relations.iter().nth(rng.below(p.relations.len() + 1));
            match (stored, rng.below(2)) {
                (Some((id, _)), 0) => ("remove relation", Some(Command::RemoveRelation(id))),
                _ => ("insert relation", Some(Command::InsertRelation(relation))),
            }
        }
        11..=13 => ("undo", None),
        _ => ("redo", None),
    }
}

/// STR-006, SRS 3.3: after a seeded random sequence of a few hundred
/// element and storey commands with undo and redo, the storey dependents of
/// every storey ever created equal the set computed from the elements' home
/// storeys and top links, the explicit relations' ends are indexed exactly,
/// and the index kept up to date edit by edit equals one rebuilt from
/// scratch. A refused command leaves the project unchanged.
#[test]
fn relation_index_matches_brute_force() {
    let mut rng = Seeded(0x1965_0002);
    let mut p = default_metric_project();
    let mut h = History::default();
    let mut storeys: BTreeSet<StoreyId> = p.storeys.iter().map(|s| s.id).collect();
    let mut done: BTreeMap<&str, usize> = BTreeMap::new();
    let mut refused = 0;
    for step in 0..400 {
        let (what, command) = random_command(&mut rng, &p, step);
        let applied = match (what, command) {
            ("undo", _) => h.undo(&mut p).is_some(),
            (_, None) => h.redo(&mut p).is_some(),
            (_, Some(command)) => {
                if let Command::InsertStorey { storey, .. } = &command {
                    storeys.insert(storey.id);
                }
                let before = p.clone();
                let result = h.execute(&mut p, command);
                if result.is_err() {
                    assert_exact(&p, &before);
                    refused += 1;
                }
                result.is_ok()
            }
        };
        if applied {
            *done.entry(what).or_default() += 1;
        }
        for &storey in &storeys {
            assert_eq!(
                p.relations.storey_dependents(storey),
                linked_to(&p, storey),
                "step {step} ({what}): the dependents of a storey"
            );
        }
        for id in p.elements.keys().copied().chain([ElementId::new()]) {
            let neighbours: BTreeSet<ElementId> = p
                .relations
                .iter()
                .filter_map(|(_, r)| r.ends())
                .filter_map(|(first, second)| match (first == id, second == id) {
                    (true, false) => Some(second),
                    (false, true) => Some(first),
                    _ => None,
                })
                .collect();
            assert_eq!(p.relations.neighbours(id), neighbours, "step {step} ({what})");
        }
        let mut rebuilt = p.relations.clone();
        rebuilt.rebuild(p.elements.values());
        assert!(
            p.relations.same_index(&rebuilt),
            "step {step} ({what}): the index kept edit by edit equals a rebuilt one"
        );
        assert!(p.integrity_problems().is_empty(), "step {step} ({what})");
    }
    for what in [
        "add",
        "update",
        "delete",
        "delete storey",
        "insert storey",
        "insert relation",
        "remove relation",
        "undo",
        "redo",
    ] {
        let n = done.get(what).copied().unwrap_or(0);
        assert!(n >= 5, "{what} ran {n} times: {done:?}, {refused} refused");
    }
}
