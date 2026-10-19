//! Command and undo behaviour (NFR-MNT-001, UND-001, UND-002, STR-004,
//! STR-005, ELM-013, LAY-002, DSP-003, INP-013).

#![allow(clippy::unwrap_used)]

use archicraft_model::command::{StoreyElements, replay};
use archicraft_model::glam::DVec2;
use archicraft_model::template::default_metric_project;
use archicraft_model::{
    Command, CommandError, Element, ElementKind, History, Layer, PlanGrid, Project, ProjectSettings, Storey, TopLink,
};

fn ground(p: &Project) -> archicraft_model::StoreyId {
    p.storeys.ground()
}

fn wall(p: &Project, start: (f64, f64), end: (f64, f64)) -> Element {
    p.new_wall(ground(p), DVec2::new(start.0, start.1), DVec2::new(end.0, end.1), 0)
}

#[test]
fn add_undo_redo_keeps_identity() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    let id = w.id;
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    assert_eq!(p.elements.len(), 1);
    assert_eq!(p.element(id).unwrap().name, "W-001");
    assert_eq!(p.counters.wall, 1);
    assert_ne!(h.state_id(), 0);

    assert_eq!(h.undo(&mut p).as_deref(), Some("Create wall"));
    assert!(p.elements.is_empty());
    assert_eq!(p.counters.wall, 0);
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
