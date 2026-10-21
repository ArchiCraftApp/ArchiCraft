//! Command and undo behaviour (NFR-MNT-001, UND-001, UND-002, STR-004,
//! STR-005, ELM-013, LAY-002, DSP-003, INP-013).

#![allow(clippy::unwrap_used)]

mod common;

use archicraft_model::command::{StoreyElements, replay};
use archicraft_model::glam::DVec2;
use archicraft_model::storey::MIN_STOREY_HEIGHT;
use archicraft_model::template::default_metric_project;
use archicraft_model::validate::{MAX_GRID_SPACING, MAX_THICKNESS, MIN_THICKNESS};
use archicraft_model::{
    Arg, Command, CommandError, Element, ElementKind, History, KindTag, Layer, Material, Message, PlanGrid,
    ProjectSettings, Storey, TopLink,
};
use common::{ground, wall};

#[test]
fn add_undo_redo_keeps_identity() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    let id = w.id;
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    assert_eq!(p.elements.len(), 1);
    assert_eq!(p.element(id).unwrap().name, "W-001");
    assert_eq!(p.counters.get(KindTag::Wall), 1);
    assert_ne!(h.state_id(), 0);

    let label = h.undo(&mut p).unwrap();
    assert_eq!(label.key(), "model-label-create-kind");
    assert_eq!(label.arg("kind"), Some(&Arg::Kind(KindTag::Wall)));
    assert!(p.elements.is_empty());
    assert_eq!(p.counters.get(KindTag::Wall), 0);
    assert_eq!(h.state_id(), 0);

    h.redo(&mut p).unwrap();
    assert_eq!(p.element(id).unwrap().name, "W-001");
}

#[test]
fn undo_restores_the_exact_project() {
    let mut p = default_metric_project();
    let start = p.clone();
    let mut h = History::default();
    let w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    let wid = w.id;
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    let ground = ground(&p);
    h.execute(
        &mut p,
        Command::InsertStorey {
            reference: ground,
            above: false,
            storey: Storey::new("-01 Basement", 0.0, 2.6),
        },
    )
    .unwrap();
    h.execute(
        &mut p,
        Command::MoveElements {
            ids: vec![wid],
            delta: DVec2::new(1.0, 2.0),
        },
    )
    .unwrap();
    let layer = p.layers[0].clone();
    h.execute(
        &mut p,
        Command::UpdateLayer(Layer {
            visible: false,
            ..layer
        }),
    )
    .unwrap();
    assert_eq!(h.undo_len(), 4);
    while h.undo(&mut p).is_some() {}
    let mut expected = start;
    expected.revision = p.revision;
    assert_eq!(p, expected);
}

#[test]
fn invalid_commands_leave_the_project_unchanged() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let before = p.clone();
    let mut w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    if let ElementKind::Wall(wall) = &mut w.kind {
        wall.thickness = 0.0;
    }
    assert!(matches!(
        h.execute(&mut p, Command::AddElements(vec![w])),
        Err(CommandError::Invalid(_))
    ));
    let zero_length = wall(&p, (1.0, 1.0), (1.0, 1.0));
    assert!(h.execute(&mut p, Command::AddElements(vec![zero_length])).is_err());
    assert_eq!(p, before);
    assert!(!h.can_undo());
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

/// NFR-L10N-001: command errors are message keys with typed arguments, not
/// English text. Numeric limits are lengths in metres or counts, so the
/// application shows them in the user's units; names are text arguments.
#[test]
fn command_errors_report_message_keys_and_arguments() {
    let mut p = default_metric_project();
    let mut h = History::default();

    // An out-of-range wall thickness reports the limits as lengths.
    let mut thick = wall(&p, (0.0, 0.0), (5.0, 0.0));
    if let ElementKind::Wall(w) = &mut thick.kind {
        w.thickness = 12.0;
    }
    let (variant, message) = rejection(h.execute(&mut p, Command::AddElements(vec![thick])));
    assert_eq!(variant, "Invalid");
    assert_eq!(message.key(), "model-error-thickness-range");
    assert_eq!(message.arg("min"), Some(&Arg::Length(MIN_THICKNESS)));
    assert_eq!(message.arg("max"), Some(&Arg::Length(MAX_THICKNESS)));

    // An edit names the element and nests the reason.
    let w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    let id = w.id;
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    let mut edited = p.element(id).unwrap().clone();
    if let ElementKind::Wall(w) = &mut edited.kind {
        w.thickness = 0.0;
    }
    let (variant, message) = rejection(h.execute(&mut p, Command::UpdateElements(vec![edited])));
    assert_eq!(variant, "Invalid");
    assert_eq!(message.key(), "model-error-element-invalid");
    assert_eq!(message.arg("name"), Some(&Arg::Text("W-001".into())));
    match message.arg("reason") {
        Some(Arg::Nested(reason)) => assert_eq!(reason.key(), "model-error-thickness-range"),
        other => panic!("expected a nested reason, got {other:?}"),
    }

    // Unchanged commands map to their own key.
    let unchanged = p.element(id).unwrap().clone();
    let (variant, message) = rejection(h.execute(&mut p, Command::UpdateElements(vec![unchanged])));
    assert_eq!((variant, message.key()), ("NoChange", "model-error-no-change"));

    // A missing element.
    h.execute(&mut p, Command::DeleteElements(vec![id])).unwrap();
    let (variant, message) = rejection(h.execute(&mut p, Command::DeleteElements(vec![id])));
    assert_eq!((variant, message.key()), ("NotFound", "model-error-element-not-found"));
    h.undo(&mut p).unwrap();

    // Locked elements and layers name the element (and the layer).
    let mut locked = p.element(id).unwrap().clone();
    locked.locked = true;
    h.execute(&mut p, Command::UpdateElements(vec![locked])).unwrap();
    let (variant, message) = rejection(h.execute(&mut p, Command::DeleteElements(vec![id])));
    assert_eq!((variant, message.key()), ("Locked", "model-error-element-locked"));
    assert_eq!(message.arg("name"), Some(&Arg::Text("W-001".into())));
    h.undo(&mut p).unwrap();
    let layer = p.layer(p.element(id).unwrap().layer).unwrap().clone();
    let layer_name = layer.name.clone();
    h.execute(&mut p, Command::UpdateLayer(Layer { locked: true, ..layer }))
        .unwrap();
    let (variant, message) = rejection(h.execute(&mut p, Command::DeleteElements(vec![id])));
    assert_eq!((variant, message.key()), ("Locked", "model-error-layer-locked"));
    assert_eq!(message.arg("name"), Some(&Arg::Text("W-001".into())));
    assert_eq!(message.arg("layer"), Some(&Arg::Text(layer_name)));

    // Storey errors flow through the command with their limits.
    let storey = ground(&p);
    let (variant, message) = rejection(h.execute(
        &mut p,
        Command::SetStoreyHeight {
            storey,
            height: 0.001,
            shift_above: true,
        },
    ));
    assert_eq!((variant, message.key()), ("Invalid", "model-error-storey-height-min"));
    assert_eq!(message.arg("min"), Some(&Arg::Length(MIN_STOREY_HEIGHT)));

    // Material priorities are plain numbers.
    let material = Material {
        priority: 0,
        ..p.materials[0].clone()
    };
    let (variant, message) = rejection(h.execute(&mut p, Command::UpdateMaterial(material)));
    assert_eq!((variant, message.key()), ("Invalid", "model-error-priority-range"));
    assert_eq!(message.arg("min"), Some(&Arg::Count(1)));
    assert_eq!(message.arg("max"), Some(&Arg::Count(999)));

    // Plan grid limits are lengths.
    let mut settings = p.settings.clone();
    settings.plan_grid.snap_step.x = 0.0;
    let (variant, message) = rejection(h.execute(&mut p, Command::SetSettings(settings)));
    assert_eq!((variant, message.key()), ("Invalid", "model-error-snap-grid-range"));
    assert_eq!(message.arg("max"), Some(&Arg::Length(MAX_GRID_SPACING)));

    // The error's Display is the key form, for logs.
    let error = CommandError::Invalid(Message::new("model-error-layer-name"));
    assert_eq!(error.to_string(), "model-error-layer-name");
    assert_eq!(CommandError::NoChange.to_string(), "model-error-no-change");
}

#[test]
fn locked_elements_reject_edits_but_can_be_unlocked() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let mut w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    w.locked = true;
    let id = w.id;
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    assert!(matches!(
        h.execute(&mut p, Command::DeleteElements(vec![id])),
        Err(CommandError::Locked(_))
    ));
    assert!(matches!(
        h.execute(
            &mut p,
            Command::MoveElements {
                ids: vec![id],
                delta: DVec2::X
            }
        ),
        Err(CommandError::Locked(_))
    ));
    let unlocked = Element {
        locked: false,
        ..p.element(id).unwrap().clone()
    };
    h.execute(&mut p, Command::UpdateElements(vec![unlocked])).unwrap();
    h.execute(&mut p, Command::DeleteElements(vec![id])).unwrap();
}

#[test]
fn locked_layer_blocks_edits() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    let id = w.id;
    let layer = p.layer(w.layer).unwrap().clone();
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    h.execute(&mut p, Command::UpdateLayer(Layer { locked: true, ..layer }))
        .unwrap();
    assert!(matches!(
        h.execute(&mut p, Command::DeleteElements(vec![id])),
        Err(CommandError::Locked(_))
    ));
}

#[test]
fn storey_changes_move_linked_walls() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    let id = w.id;
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    assert_eq!(p.vertical_extent(p.element(id).unwrap()), Some((0.0, 3.0)));
    let g = ground(&p);
    h.execute(
        &mut p,
        Command::SetStoreyHeight {
            storey: g,
            height: 3.4,
            shift_above: true,
        },
    )
    .unwrap();
    assert_eq!(p.vertical_extent(p.element(id).unwrap()), Some((0.0, 3.4)));
    h.undo(&mut p);
    assert_eq!(p.vertical_extent(p.element(id).unwrap()), Some((0.0, 3.0)));
}

#[test]
fn deleting_a_storey_can_keep_its_elements() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let first = p.storeys.as_slice()[1].id;
    let g = ground(&p);
    let mut w = p.new_wall(first, DVec2::ZERO, DVec2::X * 4.0, 0);
    if let ElementKind::Wall(wall) = &mut w.kind {
        wall.top = TopLink::Fixed { height: 2.5 };
    }
    let id = w.id;
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    h.execute(
        &mut p,
        Command::DeleteStorey {
            storey: first,
            elements: StoreyElements::MoveTo(g),
        },
    )
    .unwrap();
    let moved = p.element(id).unwrap();
    assert_eq!(moved.storey, g);
    assert_eq!(p.vertical_extent(moved), Some((3.0, 5.5)));
    h.undo(&mut p);
    assert_eq!(p.element(id).unwrap().storey, first);
    assert_eq!(p.storeys.len(), 2);

    h.execute(
        &mut p,
        Command::DeleteStorey {
            storey: first,
            elements: StoreyElements::Delete,
        },
    )
    .unwrap();
    assert!(p.element(id).is_none());
    assert!(h.can_undo(), "storey operations keep the undo history (STR-005)");
}

#[test]
fn deleting_resources_requires_a_replacement() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    let id = w.id;
    let brick = w.kind.material();
    let concrete = p.materials[0].id;
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    assert!(
        h.execute(
            &mut p,
            Command::DeleteMaterial {
                material: brick,
                replacement: brick
            }
        )
        .is_err()
    );
    h.execute(
        &mut p,
        Command::DeleteMaterial {
            material: brick,
            replacement: concrete,
        },
    )
    .unwrap();
    assert_eq!(p.element(id).unwrap().kind.material(), concrete);
    assert_eq!(p.tool_defaults.wall.material, concrete);
    assert!(p.material(brick).is_none());

    let walls_layer = p.element(id).unwrap().layer;
    let other = p.layers[0].id;
    h.execute(
        &mut p,
        Command::DeleteLayer {
            layer: walls_layer,
            reassign_to: other,
        },
    )
    .unwrap();
    assert_eq!(p.element(id).unwrap().layer, other);
    assert!(p.integrity_problems().is_empty());
}

#[test]
fn history_is_bounded_and_replayable() {
    let mut p = default_metric_project();
    let start = p.clone();
    let mut h = History::new(3);
    for i in 0..5 {
        let w = p.new_wall(
            p.storeys.ground(),
            DVec2::new(0.0, i as f64),
            DVec2::new(4.0, i as f64),
            0,
        );
        h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    }
    assert_eq!(h.undo_len(), 3);
    assert_eq!(p.element(*p.elements.keys().last().unwrap()).unwrap().name, "W-005");

    // Commands serialise for logs and replay onto the starting project.
    let log = serde_json::to_string(&h.applied_commands().collect::<Vec<_>>()).unwrap();
    let commands: Vec<Command> = serde_json::from_str(&log).unwrap();
    let mut replayed = start;
    for i in 0..2 {
        let w = replayed.new_wall(
            replayed.storeys.ground(),
            DVec2::new(0.0, i as f64),
            DVec2::new(4.0, i as f64),
            0,
        );
        History::default()
            .execute(&mut replayed, Command::AddElements(vec![w]))
            .unwrap();
    }
    replay(&mut replayed, &commands).unwrap();
    assert_eq!(replayed.elements.len(), 5);
    assert_eq!(h.journal().count(), 5);
}

#[test]
fn relinking_keeps_the_absolute_position() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    let id = w.id;
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    let upper = p.storeys.as_slice()[1].id;
    let before = p.vertical_extent(p.element(id).unwrap());
    let moved = archicraft_model::command::relink_to_storey(&p, p.element(id).unwrap(), upper).unwrap();
    h.execute(&mut p, Command::UpdateElements(vec![moved])).unwrap();
    let after = p.element(id).unwrap();
    assert_eq!(after.storey, upper);
    assert_eq!(p.vertical_extent(after), before);
    assert!(matches!(after.as_wall().unwrap().top, TopLink::Storey { storey, .. } if storey == upper));
}

/// The plan grid defaults of the design of INP-013: a 5 cm snap grid with
/// snapping on, a hidden construction grid of 1 m with 25 cm auxiliary lines,
/// unrotated at the project origin.
fn assert_default_plan_grid(grid: &PlanGrid) {
    assert_eq!(grid.snap_step, DVec2::new(0.05, 0.05));
    assert!(grid.snap_enabled);
    assert_eq!(grid.main_spacing, DVec2::new(1.0, 1.0));
    assert_eq!(grid.auxiliary_spacing, DVec2::new(0.25, 0.25));
    assert_eq!(grid.rotation, 0.0);
    assert_eq!(grid.origin, DVec2::ZERO);
    assert!(!grid.visible);
}

/// INP-013, PRJ-007: one plan grid per project, with defaults in the
/// project settings and the default template.
#[test]
fn projects_start_with_the_default_plan_grid() {
    assert_default_plan_grid(&ProjectSettings::default().plan_grid);
    assert_default_plan_grid(&default_metric_project().settings.plan_grid);
    assert_eq!(PlanGrid::default(), ProjectSettings::default().plan_grid);
}

/// INP-013, UND-001: changing the plan grid is one undoable command.
#[test]
fn plan_grid_changes_undo_and_redo_to_the_exact_project() {
    let mut p = default_metric_project();
    let before = p.clone();
    let mut h = History::default();
    let settings = ProjectSettings {
        plan_grid: PlanGrid {
            snap_step: DVec2::new(0.1, 0.05),
            snap_enabled: false,
            main_spacing: DVec2::new(1.2, 0.9),
            auxiliary_spacing: DVec2::new(0.3, 0.3),
            rotation: 30f64.to_radians(),
            origin: DVec2::new(1.0, 1.0),
            visible: true,
        },
        ..p.settings.clone()
    };
    h.execute(&mut p, Command::SetSettings(settings.clone())).unwrap();
    assert_eq!(p.settings, settings);
    let after = p.clone();

    h.undo(&mut p).unwrap();
    let mut expected = before;
    expected.revision = p.revision;
    assert_eq!(p, expected);

    h.redo(&mut p).unwrap();
    let mut expected = after;
    expected.revision = p.revision;
    assert_eq!(p, expected);
}

/// INP-013: steps and spacings are finite, positive and at most 1 km, the
/// auxiliary spacing is not larger than the main spacing, and rotation and
/// origin are finite, the origin within the model's ±100 km.
#[test]
fn invalid_plan_grids_are_rejected() {
    let mut p = default_metric_project();
    let before = p.clone();
    let mut h = History::default();
    let with = |change: &dyn Fn(&mut PlanGrid)| {
        let mut settings = before.settings.clone();
        change(&mut settings.plan_grid);
        Command::SetSettings(settings)
    };
    let bad = [f64::NAN, f64::INFINITY, -0.5, 0.0, 1000.5];
    for value in bad {
        for command in [
            with(&|g| g.snap_step.x = value),
            with(&|g| g.snap_step.y = value),
            with(&|g| g.main_spacing.x = value),
            with(&|g| g.main_spacing.y = value),
            with(&|g| g.auxiliary_spacing.x = value),
            with(&|g| g.auxiliary_spacing.y = value),
        ] {
            assert!(
                matches!(h.execute(&mut p, command), Err(CommandError::Invalid(_))),
                "{value} is rejected"
            );
        }
    }
    for command in [
        with(&|g| g.auxiliary_spacing.x = 1.5),
        with(&|g| g.auxiliary_spacing.y = 1.5),
        with(&|g| g.rotation = f64::NAN),
        with(&|g| g.origin.x = f64::INFINITY),
        with(&|g| g.origin.y = -100_001.0),
    ] {
        assert!(matches!(h.execute(&mut p, command), Err(CommandError::Invalid(_))));
    }
    assert_eq!(p, before);
    assert!(!h.can_undo());
    // The bounds themselves are allowed.
    h.execute(
        &mut p,
        with(&|g| {
            g.main_spacing = DVec2::splat(1000.0);
            g.auxiliary_spacing = DVec2::splat(1000.0);
        }),
    )
    .unwrap();
}

/// The project without its revision, which counts every change including
/// undo and redo.
#[cfg(feature = "test-kinds")]
fn content(p: &archicraft_model::Project) -> archicraft_model::Project {
    archicraft_model::Project {
        revision: 0,
        ..p.clone()
    }
}

/// SRS 3.2, ELM-019, UND-001, UND-002: the test kind goes through the
/// generic command paths like any kind. AddElements names it X-001 and bumps
/// only its counter; UpdateElements, MoveElements and DeleteElements apply,
/// and undo and redo restore the exact project after every step.
#[cfg(feature = "test-kinds")]
#[test]
fn the_test_kind_goes_through_the_command_paths() {
    use archicraft_model::kinds::Dummy;

    let mut p = default_metric_project();
    let mut h = History::default();
    let start = content(&p);
    let d = Dummy::new(DVec2::new(1.0, 2.0), DVec2::new(0.5, 0.4), p.materials[0].id).into_element(&p, ground(&p), 0);
    let id = d.id;
    assert_eq!(d.name, "X-001");
    h.execute(&mut p, Command::AddElements(vec![d])).unwrap();
    assert_eq!(p.counters.get(KindTag::Dummy), 1);
    for &kind in KindTag::ALL.iter().filter(|k| **k != KindTag::Dummy) {
        assert_eq!(p.counters.get(kind), 0, "{kind:?}");
    }
    assert_eq!(p.next_element_name(KindTag::Dummy, 0), "X-002");

    let mut states = vec![start, content(&p)];
    let mut edited = p.element(id).unwrap().clone();
    edited.as_dummy_mut().unwrap().size = DVec2::new(2.0, 3.0);
    let delta = DVec2::new(0.25, -1.5);
    for command in [
        Command::UpdateElements(vec![edited]),
        Command::MoveElements { ids: vec![id], delta },
        Command::DeleteElements(vec![id]),
    ] {
        h.execute(&mut p, command).unwrap();
        states.push(content(&p));
    }
    assert_eq!(
        states[2].element(id).unwrap().as_dummy().unwrap().size,
        DVec2::new(2.0, 3.0)
    );
    let moved = states[3].element(id).unwrap().as_dummy().unwrap();
    assert_eq!(moved.origin, DVec2::new(1.0, 2.0) + delta);
    assert!(p.element(id).is_none());

    for expected in states.iter().rev().skip(1) {
        h.undo(&mut p).unwrap();
        assert_eq!(&content(&p), expected);
    }
    for expected in states.iter().skip(1) {
        h.redo(&mut p).unwrap();
        assert_eq!(&content(&p), expected);
    }
}

/// ELM-011: an IFC class outside the test kind's descriptor set is rejected
/// with `CommandError::Invalid`, and the project stays unchanged.
#[cfg(feature = "test-kinds")]
#[test]
fn a_test_kind_element_with_a_foreign_class_is_rejected() {
    use archicraft_model::IfcClass;
    use archicraft_model::kinds::Dummy;

    let mut p = default_metric_project();
    let mut h = History::default();
    let before = p.clone();
    let mut d = Dummy::new(DVec2::ZERO, DVec2::ONE, p.materials[0].id).into_element(&p, ground(&p), 0);
    d.ifc_class = IfcClass::Wall;
    assert!(!KindTag::Dummy.ifc_classes().contains(&IfcClass::Wall));
    match h.execute(&mut p, Command::AddElements(vec![d])) {
        Err(CommandError::Invalid(message)) => {
            assert_eq!(message.key(), "model-error-ifc-class-not-allowed");
            assert_eq!(message.arg("kind"), Some(&Arg::Kind(KindTag::Dummy)));
        }
        other => panic!("expected CommandError::Invalid, got {other:?}"),
    }
    assert_eq!(p, before);
}
