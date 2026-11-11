//! Command and undo behaviour (NFR-MNT-001, UND-001, UND-002, STR-004,
//! STR-005, ELM-013, ELM-015, LAY-002, DSP-003, INP-013, PRF-004).

#![allow(clippy::unwrap_used)]

mod common;

use archicraft_model::command::{StoreyElements, replay};
use archicraft_model::glam::DVec2;
use archicraft_model::project::{DefaultsSlot, ToolDefaults};
use archicraft_model::storey::MIN_STOREY_HEIGHT;
use archicraft_model::template::default_metric_project;
use archicraft_model::transfer::SettingsSource;
use archicraft_model::validate::{MAX_GRID_SPACING, MAX_HEIGHT, MAX_OFFSET, MAX_THICKNESS, MIN_HEIGHT, MIN_THICKNESS};
use archicraft_model::{
    AnchorPos, Arg, Command, CommandError, DraftingView, Element, ElementId, ElementKind, Group, GroupId, History,
    IfcClass, KindTag, Layer, Material, MaterialId, Message, ParametricShape, PlanGrid, Profile, ProfileId,
    ProfileRegion, ProfileShape, Project, ProjectSettings, Slab, Space, Storey, StoreyId, Text, TextStyleId, TopLink,
    Wall,
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

/// The wall parameters of a tool defaults slot.
fn wall_of(slot: &mut DefaultsSlot) -> &mut Wall {
    match &mut slot.kind {
        ElementKind::Wall(w) => w,
        other => panic!("not a wall slot: {other:?}"),
    }
}

/// The slab parameters of a tool defaults slot.
fn slab_of(slot: &mut DefaultsSlot) -> &mut Slab {
    match &mut slot.kind {
        ElementKind::Slab(s) => s,
        other => panic!("not a slab slot: {other:?}"),
    }
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

/// A wall on `home` whose top is linked to `storey` with `offset`.
fn linked_wall(p: &archicraft_model::Project, home: StoreyId, storey: StoreyId, offset: f64) -> Element {
    let mut w = p.new_wall(home, DVec2::ZERO, DVec2::X * 4.0, 0);
    if let ElementKind::Wall(wall) = &mut w.kind {
        wall.base_offset = 0.0;
        wall.top = TopLink::Storey { storey, offset };
    }
    w
}

/// Asserts that `command` is refused because it would leave the element
/// named `name`, and `others` more, lower than MIN_HEIGHT, and that the
/// project is unchanged.
fn assert_too_low(p: &mut archicraft_model::Project, h: &mut History, command: Command, name: &str, others: usize) {
    let before = p.clone();
    let (variant, message) = rejection(h.execute(p, command));
    assert_eq!(
        (variant, message.key()),
        ("Invalid", "model-error-linked-element-too-low")
    );
    assert_eq!(message.arg("name"), Some(&Arg::Text(name.into())));
    assert_eq!(message.arg("count"), Some(&Arg::Count(others as u64)));
    assert_eq!(message.arg("min"), Some(&Arg::Length(MIN_HEIGHT)));
    assert_eq!(*p, before, "a refused storey edit leaves the project unchanged");
}

/// NFR-REL-005, ELM-017, STR-002: moving a storey or changing its height
/// does not shorten a wall linked to it below the 1 mm minimum height; the
/// command is refused with a message naming the wall, and a move that keeps
/// it at least 1 mm high is accepted.
#[test]
fn storey_moves_refuse_to_drop_linked_walls_below_the_minimum_height() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let [g, first] = [0, 1].map(|i| p.storeys.as_slice()[i].id);
    // 1 cm high: the first floor is at 3 m.
    let w = linked_wall(&p, g, first, -2.99);
    let id = w.id;
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    let height = |p: &archicraft_model::Project| {
        let (bottom, top) = p.vertical_extent(p.element(id).unwrap()).unwrap();
        top - bottom
    };

    for elevation in [2.990_5, 2.99, 2.5] {
        let command = Command::SetStoreyElevation {
            storey: first,
            elevation,
            shift_above: false,
        };
        assert_too_low(&mut p, &mut h, command, "W-001", 0);
    }
    let command = Command::SetStoreyHeight {
        storey: g,
        height: 2.990_5,
        shift_above: true,
    };
    assert_too_low(&mut p, &mut h, command, "W-001", 0);

    // 5 mm is still at least the minimum.
    h.execute(
        &mut p,
        Command::SetStoreyElevation {
            storey: first,
            elevation: 2.995,
            shift_above: false,
        },
    )
    .unwrap();
    assert!((height(&p) - 0.005).abs() < 1e-9, "{}", height(&p));

    // Every offending element counts; the first one is named.
    let second = linked_wall(&p, g, first, -2.99);
    h.execute(&mut p, Command::AddElements(vec![second])).unwrap();
    let command = Command::SetStoreyHeight {
        storey: g,
        height: 2.0,
        shift_above: false,
    };
    assert_too_low(&mut p, &mut h, command, "W-001", 1);
}

/// Asserts that `command` is refused because it would raise the element
/// named `name`, and `others` more, above MAX_HEIGHT, and that the project
/// is unchanged.
fn assert_too_high(p: &mut archicraft_model::Project, h: &mut History, command: Command, name: &str, others: usize) {
    let before = p.clone();
    let (variant, message) = rejection(h.execute(p, command));
    assert_eq!(
        (variant, message.key()),
        ("Invalid", "model-error-linked-element-too-high")
    );
    assert_eq!(message.arg("name"), Some(&Arg::Text(name.into())));
    assert_eq!(message.arg("count"), Some(&Arg::Count(others as u64)));
    assert_eq!(message.arg("max"), Some(&Arg::Length(MAX_HEIGHT)));
    assert_eq!(*p, before, "a refused storey edit leaves the project unchanged");
}

/// NFR-REL-005, ELM-017, STR-002: moving, inserting a storey or changing
/// its height does not raise a wall linked to it above the 1000 m maximum
/// height; the command is refused with a message naming the wall and
/// counting the others, and an edit that keeps it at most 1000 m high is
/// accepted.
#[test]
fn storey_edits_refuse_to_raise_linked_walls_above_the_maximum_height() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let [g, first] = [0, 1].map(|i| p.storeys.as_slice()[i].id);
    // 3 m high, two of them, linked to the first floor at 3 m.
    let walls = [linked_wall(&p, g, first, 0.0), linked_wall(&p, g, first, 0.0)];
    let id = walls[0].id;
    h.execute(&mut p, Command::AddElements(walls.to_vec())).unwrap();

    let command = Command::SetStoreyElevation {
        storey: first,
        elevation: 1000.5,
        shift_above: false,
    };
    assert_too_high(&mut p, &mut h, command, "W-001", 1);
    let command = Command::SetStoreyHeight {
        storey: g,
        height: 1000.5,
        shift_above: true,
    };
    assert_too_high(&mut p, &mut h, command, "W-001", 1);
    // A storey inserted above the ground lifts the first floor by its height.
    let command = Command::InsertStorey {
        reference: g,
        above: true,
        storey: Storey::new("00a Atrium", 0.0, 998.0),
    };
    assert_too_high(&mut p, &mut h, command, "W-001", 1);

    // 1000 m is still at most the maximum.
    h.execute(
        &mut p,
        Command::SetStoreyHeight {
            storey: g,
            height: 1000.0,
            shift_above: true,
        },
    )
    .unwrap();
    let (bottom, top) = p.vertical_extent(p.element(id).unwrap()).unwrap();
    assert!((top - bottom - 1000.0).abs() < 1e-9, "{}", top - bottom);
}

/// NFR-REL-005, STR-002: an element made higher than the maximum outside
/// the commands does not block storey edits that leave it as it is, or
/// lower it, beyond rounding; an edit that raises it further is refused.
#[test]
fn storey_edits_that_do_not_raise_a_too_high_element_are_accepted() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let [g, first] = [0, 1].map(|i| p.storeys.as_slice()[i].id);
    let w = linked_wall(&p, g, first, 0.0);
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    p.storeys = p.storeys.set_elevation(first, 1000.5, false).unwrap();
    h.execute(
        &mut p,
        Command::RenameStorey {
            storey: first,
            name: "01 Upper floor".into(),
        },
    )
    .unwrap();
    h.execute(
        &mut p,
        Command::SetStoreyElevation {
            storey: first,
            elevation: 1000.2,
            shift_above: false,
        },
    )
    .unwrap();
    let command = Command::SetStoreyElevation {
        storey: first,
        elevation: 1000.3,
        shift_above: false,
    };
    assert_too_high(&mut p, &mut h, command, "W-001", 0);
}

/// NFR-REL-005, STR-002: inserting or deleting a storey moves the storeys
/// above it, so it is refused when that would leave a linked wall lower than
/// the minimum height.
#[test]
fn inserting_or_deleting_a_storey_refuses_to_drop_linked_walls_below_the_minimum_height() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let [g, first] = [0, 1].map(|i| p.storeys.as_slice()[i].id);
    // On the first floor (3 m), top 1 cm above it, linked to the ground.
    let w = linked_wall(&p, first, g, 3.01);
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    // A storey inserted above the ground lifts the first floor by 3 m.
    let command = Command::InsertStorey {
        reference: g,
        above: true,
        storey: Storey::new("00a Mezzanine", 0.0, 3.0),
    };
    assert_too_low(&mut p, &mut h, command, "W-001", 0);

    // On the ground, top 1 cm high, linked to a storey two above.
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
    let w = linked_wall(&p, g, second_id, -5.99);
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    // Deleting the first floor (and W-001 on it) lowers the second floor by
    // 3 m.
    let command = Command::DeleteStorey {
        storey: first,
        elements: StoreyElements::Delete,
    };
    assert_too_low(&mut p, &mut h, command, "W-002", 0);
}

/// NFR-REL-005, STR-002: a top linked relative to the home storey counts
/// too. Inserting a storey re-targets "home + 2" to a lower storey, which is
/// refused when it leaves the wall lower than the minimum height.
#[test]
fn inserting_a_storey_refuses_to_drop_relatively_linked_walls_below_the_minimum_height() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let g = ground(&p);
    // "Home + 2" lies beyond the top storey: 6 m, from the first floor's
    // nominal height. The top is 1 cm above the base.
    let mut w = p.new_wall(g, DVec2::ZERO, DVec2::X * 4.0, 0);
    if let ElementKind::Wall(wall) = &mut w.kind {
        wall.base_offset = 0.0;
        wall.top = TopLink::Relative {
            storeys: 2,
            offset: -5.99,
        };
    }
    let id = w.id;
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    assert_eq!(p.vertical_extent(p.element(id).unwrap()).map(|(b, _)| b), Some(0.0));
    // A 2 m storey above the ground: "home + 2" becomes the first floor,
    // now at 5 m.
    let command = Command::InsertStorey {
        reference: g,
        above: true,
        storey: Storey::new("00a Mezzanine", 0.0, 2.0),
    };
    assert_too_low(&mut p, &mut h, command, "W-001", 0);
}

/// NFR-REL-005: a storey move does not change the height of an element with
/// a fixed height, so a 1 mm high wall on the moved storey does not block it,
/// even where rounding makes its computed height a little less than 1 mm.
#[test]
fn moving_a_storey_with_a_fixed_height_wall_of_the_minimum_height_is_accepted() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let first = p.storeys.as_slice()[1].id;
    h.execute(
        &mut p,
        Command::SetStoreyElevation {
            storey: first,
            elevation: 4.0,
            shift_above: false,
        },
    )
    .unwrap();
    let mut w = p.new_wall(first, DVec2::ZERO, DVec2::X * 4.0, 0);
    if let ElementKind::Wall(wall) = &mut w.kind {
        wall.base_offset = 0.0;
        wall.top = TopLink::Fixed { height: MIN_HEIGHT };
    }
    let id = w.id;
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    h.execute(
        &mut p,
        Command::SetStoreyElevation {
            storey: first,
            elevation: 3.5,
            shift_above: false,
        },
    )
    .unwrap();
    let (bottom, top) = p.vertical_extent(p.element(id).unwrap()).unwrap();
    assert_eq!(bottom, 3.5);
    assert!(top - bottom < MIN_HEIGHT, "rounding: {}", top - bottom);
}

/// NFR-REL-005: the check refuses only edits that lower an element below the
/// minimum. An element that is already lower (here put there by changing the
/// storey list directly, outside the commands) does not block unrelated
/// storey edits, nor edits that raise it again.
#[test]
fn storey_edits_that_do_not_lower_a_too_low_element_are_accepted() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let [g, first] = [0, 1].map(|i| p.storeys.as_slice()[i].id);
    let w = linked_wall(&p, g, first, -2.99);
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    p.storeys = p.storeys.set_elevation(first, 2.990_005, false).unwrap();
    h.execute(
        &mut p,
        Command::RenameStorey {
            storey: first,
            name: "01 Upper floor".into(),
        },
    )
    .unwrap();
    h.execute(
        &mut p,
        Command::SetStoreyElevation {
            storey: first,
            elevation: 2.990_5,
            shift_above: false,
        },
    )
    .unwrap();
    let command = Command::SetStoreyElevation {
        storey: first,
        elevation: 2.990_1,
        shift_above: false,
    };
    assert_too_low(&mut p, &mut h, command, "W-001", 0);
}

/// DSP-003, LAY-002, ELM-001, PRF-004: deleting a material or layer needs a
/// replacement, which the elements, the profile regions and exactly the tool
/// defaults slots that used the deleted one get; undo restores the slots and
/// the profiles.
#[test]
fn deleting_resources_requires_a_replacement() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let template = p.tool_defaults.clone();
    let w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    let id = w.id;
    let brick = w.kind.material().unwrap();
    let concrete = p.materials[0].id;
    let other = p
        .materials
        .iter()
        .find(|m| m.id != brick && m.id != concrete)
        .unwrap()
        .id;
    assert_eq!(template.get(KindTag::Wall).unwrap().kind.material().unwrap(), brick);
    assert_ne!(template.get(KindTag::Slab).unwrap().kind.material().unwrap(), brick);
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    // PRF-004: a parametric profile of brick and a custom profile with one
    // brick region and one region of another material.
    let parametric = Profile::parametric("Brick pier", ParametricShape::ALL[0], brick);
    let custom = Profile::custom(
        "Faced pier",
        vec![region(DVec2::ZERO, brick), region(DVec2::new(0.1, 0.0), other)],
    );
    h.execute(&mut p, Command::AddProfile(parametric.clone())).unwrap();
    h.execute(&mut p, Command::AddProfile(custom.clone())).unwrap();
    let profiles_before = p.profiles.clone();
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
    assert_eq!(p.element(id).unwrap().kind.material().unwrap(), concrete);
    assert_eq!(p.profile(parametric.id).unwrap().materials(), [concrete]);
    let ProfileShape::Custom { regions } = &p.profile(custom.id).unwrap().shape else {
        panic!("custom");
    };
    assert_eq!(
        regions.iter().map(|r| r.material).collect::<Vec<_>>(),
        [concrete, other],
        "only the brick region is reassigned"
    );
    let mut expected = template.clone();
    expected.get_mut(KindTag::Wall).unwrap().kind.set_material(concrete);
    assert_eq!(p.tool_defaults, expected, "only the wall slot used the brick");
    assert!(p.material(brick).is_none());

    let profiles_after = p.profiles.clone();

    let walls_layer = p.element(id).unwrap().layer;
    let other_layer = p.layers[0].id;
    assert_eq!(template.get(KindTag::Wall).unwrap().layer, walls_layer);
    assert_ne!(template.get(KindTag::Slab).unwrap().layer, walls_layer);
    h.execute(
        &mut p,
        Command::DeleteLayer {
            layer: walls_layer,
            reassign_to: other_layer,
        },
    )
    .unwrap();
    assert_eq!(p.element(id).unwrap().layer, other_layer);
    expected.get_mut(KindTag::Wall).unwrap().layer = other_layer;
    assert_eq!(p.tool_defaults, expected, "only the wall slot was on the layer");
    assert!(p.integrity_problems().is_empty());

    h.undo(&mut p).unwrap();
    assert_eq!(
        p.profiles, profiles_after,
        "undoing the layer deletion leaves the profiles"
    );
    h.undo(&mut p).unwrap();
    assert_eq!(p.tool_defaults, template);
    assert_eq!(
        p.profiles, profiles_before,
        "undoing the material deletion restores both profiles exactly"
    );
    assert!(p.material(brick).is_some());
}

/// A square region of `material`, 0.1 m wide, with its corner at `min`.
fn region(min: DVec2, material: MaterialId) -> ProfileRegion {
    ProfileRegion {
        outer: vec![
            min,
            min + DVec2::new(0.1, 0.0),
            min + DVec2::splat(0.1),
            min + DVec2::new(0.0, 0.1),
        ],
        holes: Vec::new(),
        material,
    }
}

/// PRF-004: AddProfile, UpdateProfile and DeleteProfile execute, undo to
/// the exact starting project and redo. A duplicate id is invalid, an
/// unchanged profile changes nothing, and an unused profile is deleted
/// without a replacement.
#[test]
fn profile_commands_execute_undo_and_redo() {
    let mut p = default_metric_project();
    let start = p.clone();
    let mut h = History::default();
    let material = p.materials[0].id;
    let ipe = Profile::parametric("IPE 200", ParametricShape::ALL[4], material);
    let id = ipe.id;

    let add = Command::AddProfile(ipe.clone());
    assert_eq!(add.label().key(), "model-label-new-profile");
    h.execute(&mut p, add.clone()).unwrap();
    assert_eq!(p.profiles, std::slice::from_ref(&ipe));
    assert_eq!(
        h.execute(&mut p, add),
        Err(CommandError::Invalid(Message::new("model-error-duplicate-profile-id")))
    );
    assert_eq!(
        h.execute(&mut p, Command::UpdateProfile(ipe.clone())),
        Err(CommandError::NoChange)
    );

    let renamed = Profile {
        name: "IPE 200 S355".into(),
        ..ipe.clone()
    };
    let update = Command::UpdateProfile(renamed.clone());
    assert_eq!(update.label().key(), "model-label-change-profile");
    h.execute(&mut p, update).unwrap();
    assert_eq!(p.profile(id), Some(&renamed));
    let updated = p.clone();

    let delete = Command::DeleteProfile {
        profile: id,
        replacement: None,
    };
    assert_eq!(delete.label().key(), "model-label-delete-profile");
    h.execute(&mut p, delete).unwrap();
    assert!(p.profiles.is_empty(), "an unused profile needs no replacement");
    assert_eq!(h.undo_len(), 3);

    h.undo(&mut p).unwrap();
    let mut expected = updated.clone();
    expected.revision = p.revision;
    assert_eq!(p, expected);
    while h.undo(&mut p).is_some() {}
    let mut expected = start;
    expected.revision = p.revision;
    assert_eq!(p, expected, "undo restores the exact starting project");

    h.redo(&mut p).unwrap();
    assert_eq!(p.profiles, std::slice::from_ref(&ipe), "redo adds the profile");
    h.redo(&mut p).unwrap();
    assert_eq!(p.profiles, updated.profiles, "redo renames it");
    h.redo(&mut p).unwrap();
    assert!(p.profiles.is_empty(), "redo deletes it");
    assert!(h.redo(&mut p).is_none());
    h.undo(&mut p).unwrap();
    assert_eq!(p.profiles, updated.profiles, "redo and undo replay the same edits");
}

/// PRF-004, NFR-REL-005: invalid profile commands are rejected and leave
/// the project unchanged: an invalid profile, a profile that does not
/// exist, and a replacement that is the profile itself or does not exist.
#[test]
fn invalid_profile_commands_are_rejected() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let material = p.materials[0].id;
    let ipe = Profile::parametric("IPE 200", ParametricShape::ALL[4], material);
    h.execute(&mut p, Command::AddProfile(ipe.clone())).unwrap();
    let before = p.clone();

    let blank = Profile {
        name: " ".into(),
        ..Profile::parametric("", ParametricShape::ALL[0], material)
    };
    assert_eq!(
        h.execute(&mut p, Command::AddProfile(blank)),
        Err(CommandError::Invalid(Message::new("model-error-profile-name")))
    );
    let missing_material = Profile::parametric("Ghost", ParametricShape::ALL[0], MaterialId::new());
    assert!(matches!(
        h.execute(&mut p, Command::AddProfile(missing_material)),
        Err(CommandError::Invalid(_))
    ));
    assert_eq!(
        h.execute(
            &mut p,
            Command::UpdateProfile(Profile {
                name: String::new(),
                ..ipe.clone()
            })
        ),
        Err(CommandError::Invalid(Message::new("model-error-profile-name")))
    );
    let unknown = Profile::parametric("Unknown", ParametricShape::ALL[0], material);
    assert_eq!(
        h.execute(&mut p, Command::UpdateProfile(unknown.clone())),
        Err(CommandError::NotFound(Message::new("model-error-profile-not-found")))
    );
    assert_eq!(
        h.execute(
            &mut p,
            Command::DeleteProfile {
                profile: unknown.id,
                replacement: None
            }
        ),
        Err(CommandError::NotFound(Message::new("model-error-profile-not-found")))
    );
    for replacement in [ipe.id, ProfileId::new()] {
        assert_eq!(
            h.execute(
                &mut p,
                Command::DeleteProfile {
                    profile: ipe.id,
                    replacement: Some(replacement)
                }
            ),
            Err(CommandError::Invalid(Message::new("model-error-replacement-profile")))
        );
    }
    assert_eq!(p, before);
    assert_eq!(h.undo_len(), 1);
}

/// PRF-004: a custom profile is stored as validate_profile normalises it,
/// its outer ring counter-clockwise.
#[test]
fn added_profiles_are_stored_normalised() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let material = p.materials[0].id;
    let mut clockwise = region(DVec2::ZERO, material);
    clockwise.outer.reverse();
    let custom = Profile::custom("Plate", vec![clockwise]);
    h.execute(&mut p, Command::AddProfile(custom.clone())).unwrap();
    let ProfileShape::Custom { regions } = &p.profile(custom.id).unwrap().shape else {
        panic!("custom");
    };
    let outer = &regions[0].outer;
    let twice_area: f64 = outer
        .iter()
        .zip(outer.iter().cycle().skip(1))
        .map(|(a, b)| a.perp_dot(*b))
        .sum();
    assert!(
        twice_area > 0.0,
        "the stored outer ring is counter-clockwise: {outer:?}"
    );
    assert_eq!(
        h.execute(&mut p, Command::UpdateProfile(custom)),
        Err(CommandError::NoChange),
        "the same profile in another orientation is no change"
    );
}

/// PRF-004, DSP-003: the profile commands find users only through the
/// Kind hooks, here the test kind standing in for a column. Deleting a
/// profile in use needs a replacement; the replacement re-points the user in
/// one undoable step and must allow the user's kind; an edit that removes a
/// usage flag a user needs is rejected; an element cannot use a profile that
/// does not exist or does not allow its kind.
#[cfg(feature = "test-kinds")]
#[test]
fn profile_users_are_re_pointed_and_re_validated() {
    use archicraft_model::ProfileUsage;
    use archicraft_model::kinds::Dummy;

    let mut p = default_metric_project();
    let mut h = History::default();
    let material = p.materials[0].id;
    let ipe = Profile::parametric("IPE 300", ParametricShape::ALL[4], material);
    let hea = Profile::parametric("HEA 300", ParametricShape::ALL[4], material);
    let beams_only = Profile {
        usage: ProfileUsage {
            columns: false,
            ..ProfileUsage::MEMBERS
        },
        ..Profile::parametric("Beam 300", ParametricShape::ALL[4], material)
    };
    for profile in [&ipe, &hea, &beams_only] {
        h.execute(&mut p, Command::AddProfile(profile.clone())).unwrap();
    }
    let ground = ground(&p);
    let column = |p: &Project, profile| {
        let mut dummy = Dummy::new(DVec2::ZERO, DVec2::ONE, material);
        dummy.profile = Some(profile);
        dummy.into_element(p, ground, 0)
    };

    let dangling = column(&p, ProfileId::new());
    assert_eq!(
        h.execute(&mut p, Command::AddElements(vec![dangling])),
        Err(CommandError::Invalid(Message::new(
            "model-error-element-profile-missing"
        )))
    );
    let usage = Message::new("model-error-profile-usage")
        .text("name", "Beam 300")
        .kind("kind", KindTag::Dummy);
    let usage_error = Err(CommandError::Invalid(
        Message::new("model-error-element-invalid")
            .text("name", "X-001")
            .nested("reason", usage.clone()),
    ));
    let not_a_column = column(&p, beams_only.id);
    assert_eq!(
        h.execute(&mut p, Command::AddElements(vec![not_a_column])),
        Err(CommandError::Invalid(usage.clone()))
    );

    let user = column(&p, ipe.id);
    let user_id = user.id;
    h.execute(&mut p, Command::AddElements(vec![user])).unwrap();
    assert_eq!(p.profile_users(ipe.id).map(|e| e.id).collect::<Vec<_>>(), [user_id]);
    assert_eq!(p.profile_users(hea.id).count(), 0);
    let before = p.clone();

    assert_eq!(
        h.execute(
            &mut p,
            Command::DeleteProfile {
                profile: ipe.id,
                replacement: None
            }
        ),
        Err(CommandError::Invalid(
            Message::new("model-error-profile-in-use")
                .text("name", "IPE 300")
                .count("count", 1)
        ))
    );
    assert_eq!(
        h.execute(
            &mut p,
            Command::DeleteProfile {
                profile: ipe.id,
                replacement: Some(beams_only.id)
            }
        ),
        usage_error,
        "the replacement must allow the user's kind"
    );
    let no_columns = Profile {
        usage: beams_only.usage,
        ..ipe.clone()
    };
    assert_eq!(
        h.execute(&mut p, Command::UpdateProfile(no_columns)),
        Err(CommandError::Invalid(
            Message::new("model-error-element-invalid")
                .text("name", "X-001")
                .nested(
                    "reason",
                    Message::new("model-error-profile-usage")
                        .text("name", "IPE 300")
                        .kind("kind", KindTag::Dummy)
                ),
        )),
        "an edit cannot remove a usage flag a user needs"
    );
    assert_eq!(p, before, "rejected commands change nothing");

    let depth = h.undo_len();
    h.execute(
        &mut p,
        Command::DeleteProfile {
            profile: ipe.id,
            replacement: Some(hea.id),
        },
    )
    .unwrap();
    assert_eq!(h.undo_len(), depth + 1, "one transaction");
    assert!(p.profile(ipe.id).is_none());
    assert_eq!(p.element(user_id).unwrap().kind.profiles(), [hea.id]);
    assert!(p.integrity_problems().is_empty());
    h.undo(&mut p).unwrap();
    let mut expected = before;
    expected.revision = p.revision;
    assert_eq!(p, expected, "undo restores the profile and its user exactly");
}

/// NFR-MNT-001, PRF-004: a command log with profile commands serialises and
/// replays onto the starting project to the same profiles.
#[test]
fn a_log_with_profile_commands_replays() {
    let mut p = default_metric_project();
    let start = p.clone();
    let mut h = History::default();
    let material = p.materials[0].id;
    let ipe = Profile::parametric("IPE 200", ParametricShape::ALL[4], material);
    let plate = Profile::custom("Plate", vec![region(DVec2::ZERO, material)]);
    h.execute(&mut p, Command::AddProfile(ipe.clone())).unwrap();
    h.execute(&mut p, Command::AddProfile(plate.clone())).unwrap();
    h.execute(
        &mut p,
        Command::UpdateProfile(Profile {
            catalogue: Some("IPE 200".into()),
            ..ipe
        }),
    )
    .unwrap();
    h.execute(
        &mut p,
        Command::DeleteProfile {
            profile: plate.id,
            replacement: None,
        },
    )
    .unwrap();
    let log = serde_json::to_string(&h.applied_commands().collect::<Vec<_>>()).unwrap();
    let commands: Vec<Command> = serde_json::from_str(&log).unwrap();
    let mut replayed = start;
    replay(&mut replayed, &commands).unwrap();
    assert_eq!(replayed.profiles, p.profiles);
}

/// ELM-011, ELM-001: SetToolDefaults checks every slot as the prototype
/// element it makes: a class outside the kind's descriptor set and a
/// payload that fails validation are rejected with an explanatory message,
/// a missing slot of a kind with a tool too, and an unchanged map changes
/// nothing.
#[test]
fn tool_defaults_slots_are_validated_per_kind() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let before = p.clone();

    let mut foreign_class = p.tool_defaults.clone();
    foreign_class.get_mut(KindTag::Wall).unwrap().ifc_class = IfcClass::Covering;
    assert!(!KindTag::Wall.ifc_classes().contains(&IfcClass::Covering));
    let (variant, message) = rejection(h.execute(&mut p, Command::SetToolDefaults(foreign_class)));
    assert_eq!((variant, message.key()), ("Invalid", "model-error-default-invalid"));
    assert_eq!(message.arg("kind"), Some(&Arg::Kind(KindTag::Wall)));
    match message.arg("reason") {
        Some(Arg::Nested(reason)) => {
            assert_eq!(reason.key(), "model-error-ifc-class-not-allowed");
            assert_eq!(reason.arg("class"), Some(&Arg::Text("IfcCovering".into())));
        }
        other => panic!("expected a nested reason, got {other:?}"),
    }

    let mut zero_thickness = p.tool_defaults.clone();
    slab_of(zero_thickness.get_mut(KindTag::Slab).unwrap()).thickness = 0.0;
    let (variant, message) = rejection(h.execute(&mut p, Command::SetToolDefaults(zero_thickness)));
    assert_eq!((variant, message.key()), ("Invalid", "model-error-default-invalid"));
    assert_eq!(message.arg("kind"), Some(&Arg::Kind(KindTag::Slab)));
    match message.arg("reason") {
        Some(Arg::Nested(reason)) => assert_eq!(reason.key(), "model-error-thickness-range"),
        other => panic!("expected a nested reason, got {other:?}"),
    }

    let without_slab: ToolDefaults = p
        .tool_defaults
        .iter()
        .filter(|(kind, _)| *kind != KindTag::Slab)
        .map(|(_, slot)| slot.clone())
        .collect();
    let (variant, message) = rejection(h.execute(&mut p, Command::SetToolDefaults(without_slab)));
    assert_eq!((variant, message.key()), ("Invalid", "model-error-default-missing"));
    assert_eq!(message.arg("kind"), Some(&Arg::Kind(KindTag::Slab)));

    let unchanged = p.tool_defaults.clone();
    assert_eq!(
        h.execute(&mut p, Command::SetToolDefaults(unchanged)),
        Err(CommandError::NoChange)
    );
    assert_eq!(p, before);
}

/// ELM-017, NFR-L10N-001: a slab default offset beyond ±[`MAX_OFFSET`] is
/// rejected with the slab offset message, its limit as a length argument, and
/// the project stays unchanged.
#[test]
fn tool_defaults_reject_a_slab_offset_beyond_the_limit() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let before = p.clone();

    let mut far_offset = p.tool_defaults.clone();
    slab_of(far_offset.get_mut(KindTag::Slab).unwrap()).offset = 1000.5;
    let (variant, message) = rejection(h.execute(&mut p, Command::SetToolDefaults(far_offset)));
    assert_eq!((variant, message.key()), ("Invalid", "model-error-default-invalid"));
    assert_eq!(message.arg("kind"), Some(&Arg::Kind(KindTag::Slab)));
    match message.arg("reason") {
        Some(Arg::Nested(reason)) => {
            assert_eq!(reason.key(), "model-error-slab-offset-range");
            assert_eq!(reason.arg("max"), Some(&Arg::Length(MAX_OFFSET)));
        }
        other => panic!("expected a nested reason, got {other:?}"),
    }
    assert_eq!(p, before);
    assert!(!h.can_undo());
}

/// ELM-006, STR-004: a default top linked to a deleted storey that would
/// become a height of zero is replaced by the height of the remaining ground
/// storey; the rest of the slot stays, and the defaults stay valid and
/// editable.
#[test]
fn deleting_a_storey_keeps_the_tool_defaults_valid() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let ground = p.storeys.ground();
    let mut defaults = p.tool_defaults.clone();
    let wall = defaults.get_mut(KindTag::Wall).unwrap();
    wall_of(wall).top = TopLink::Storey {
        storey: ground,
        offset: 0.0,
    };
    wall_of(wall).thickness = 0.3;
    h.execute(&mut p, Command::SetToolDefaults(defaults.clone())).unwrap();

    h.execute(
        &mut p,
        Command::DeleteStorey {
            storey: ground,
            elements: StoreyElements::Delete,
        },
    )
    .unwrap();
    // The first floor, 3 m high, is the ground storey now.
    wall_of(defaults.get_mut(KindTag::Wall).unwrap()).top = TopLink::Fixed { height: 3.0 };
    assert_eq!(p.tool_defaults, defaults);
    let mut thicker = p.tool_defaults.clone();
    wall_of(thicker.get_mut(KindTag::Wall).unwrap()).thickness = 0.365;
    h.execute(&mut p, Command::SetToolDefaults(thicker)).unwrap();
    let w = p.new_wall(p.storeys.ground(), DVec2::ZERO, DVec2::X, 0);
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
}

/// SPC-001, ANN-007 (preparation): a test element without a material
/// validates and adds no 'material missing' problem; deleting a material
/// with a replacement (DSP-003) changes the wall, the slab and the test
/// element that used it and leaves a material-less test element and a
/// material-less defaults slot untouched; undo restores the project.
#[cfg(feature = "test-kinds")]
#[test]
fn deleting_a_material_leaves_material_less_elements_and_slots_untouched() {
    use archicraft_model::kinds::Dummy;
    use archicraft_model::validate::validate_element;

    let mut p = default_metric_project();
    let mut h = History::default();
    let g = ground(&p);
    let w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    let brick = w.kind.material().unwrap();
    let concrete = p.materials[0].id;
    assert_ne!(brick, concrete);
    let mut s = p.new_slab(
        g,
        vec![
            DVec2::new(0.0, 1.0),
            DVec2::new(4.0, 1.0),
            DVec2::new(4.0, 4.0),
            DVec2::new(0.0, 4.0),
        ],
        Vec::new(),
        0,
    );
    s.kind.set_material(brick);
    let with = Dummy::new(DVec2::new(6.0, 0.0), DVec2::ONE, brick).into_element(&p, g, 0);
    let without = Dummy::without_material(DVec2::new(8.0, 0.0), DVec2::ONE).into_element(&p, g, 1);
    assert_eq!(without.kind.material(), None);
    assert!(validate_element(&p, &without).is_ok());
    let ids = [w.id, s.id, with.id];
    let without_id = without.id;
    h.execute(&mut p, Command::AddElements(vec![w, s, with, without.clone()]))
        .unwrap();
    assert!(p.integrity_problems().is_empty(), "{:?}", p.integrity_problems());

    let mut defaults = p.tool_defaults.clone();
    defaults.insert(DefaultsSlot {
        layer: p.layers[0].id,
        position: archicraft_model::Position::Undefined,
        structural: archicraft_model::StructuralFunction::Undefined,
        ifc_class: KindTag::Dummy.descriptor().default_class,
        predefined_type: None,
        kind: Dummy::without_material(DVec2::ZERO, DVec2::ONE).into(),
    });
    h.execute(&mut p, Command::SetToolDefaults(defaults)).unwrap();
    let before = p.clone();

    h.execute(
        &mut p,
        Command::DeleteMaterial {
            material: brick,
            replacement: concrete,
        },
    )
    .unwrap();
    for id in ids {
        assert_eq!(p.element(id).unwrap().kind.material(), Some(concrete));
    }
    assert_eq!(p.element(without_id), Some(&without));
    assert_eq!(
        p.tool_defaults.get(KindTag::Dummy),
        before.tool_defaults.get(KindTag::Dummy)
    );
    assert_eq!(
        p.tool_defaults.get(KindTag::Wall).unwrap().kind.material(),
        Some(concrete)
    );
    assert!(p.integrity_problems().is_empty(), "{:?}", p.integrity_problems());

    h.undo(&mut p).unwrap();
    let mut expected = before;
    expected.revision = p.revision;
    assert_eq!(p, expected);
}

/// ELM-006, STR-004: deleting a storey rewrites every tool defaults slot
/// whose top is linked to it, of every kind, to a fixed height; undo
/// restores the links.
#[cfg(feature = "test-kinds")]
#[test]
fn deleting_a_storey_unlinks_every_defaults_slot() {
    use archicraft_model::kinds::{Dummy, VerticalLink};

    let mut p = default_metric_project();
    let mut h = History::default();
    let upper = p.storeys.as_slice()[1].id;
    let linked = TopLink::Storey {
        storey: upper,
        offset: 0.5,
    };
    let mut defaults = p.tool_defaults.clone();
    wall_of(defaults.get_mut(KindTag::Wall).unwrap()).top = linked;
    let mut dummy = Dummy::new(DVec2::ZERO, DVec2::ONE, p.materials[0].id);
    dummy.link = VerticalLink {
        base_offset: 0.0,
        top: linked,
    };
    defaults.insert(DefaultsSlot {
        layer: p.layers[0].id,
        position: archicraft_model::Position::Undefined,
        structural: archicraft_model::StructuralFunction::Undefined,
        ifc_class: KindTag::Dummy.descriptor().default_class,
        predefined_type: None,
        kind: dummy.into(),
    });
    h.execute(&mut p, Command::SetToolDefaults(defaults.clone())).unwrap();
    assert_eq!(p.tool_defaults, defaults);

    h.execute(
        &mut p,
        Command::DeleteStorey {
            storey: upper,
            elements: StoreyElements::Delete,
        },
    )
    .unwrap();
    // The upper storey was 3 m above the ground storey: the tops stay 3.5 m
    // above the base (3.0 + 0.5 is exact in binary, so `==` is safe).
    let fixed = TopLink::Fixed { height: 3.5 };
    let mut after = p.tool_defaults.clone();
    assert_eq!(wall_of(after.get_mut(KindTag::Wall).unwrap()).top, fixed);
    match &after.get(KindTag::Dummy).unwrap().kind {
        ElementKind::Dummy(d) => assert_eq!(d.link.top, fixed),
        other => panic!("not a dummy slot: {other:?}"),
    }
    assert_eq!(
        p.tool_defaults.get(KindTag::Slab),
        defaults.get(KindTag::Slab),
        "the slab slot has no storey link"
    );

    h.undo(&mut p).unwrap();
    assert_eq!(p.tool_defaults, defaults);
}

/// ELM-006, ELM-001: when deleting a storey would leave a defaults slot
/// with an invalid fixed height (here a top linked to the basement, below
/// the base on the ground storey), only the top is replaced, by a fixed
/// height of one ground storey; the slot's other fields stay, and a kind
/// without template defaults (the dummy) keeps its slot.
#[cfg(feature = "test-kinds")]
#[test]
fn deleting_a_storey_resets_only_an_invalid_defaults_top() {
    use archicraft_model::kinds::{Dummy, VerticalLink};

    let mut p = default_metric_project();
    let mut h = History::default();
    let basement = Storey::new("-01 Basement", 0.0, 2.6);
    let basement_id = basement.id;
    let g = ground(&p);
    h.execute(
        &mut p,
        Command::InsertStorey {
            reference: g,
            above: false,
            storey: basement,
        },
    )
    .unwrap();
    let linked = TopLink::Storey {
        storey: basement_id,
        offset: 0.5,
    };
    let mut defaults = p.tool_defaults.clone();
    let wall_slot = defaults.get_mut(KindTag::Wall).unwrap();
    let other_layer = p.layers.iter().find(|l| l.id != wall_slot.layer).unwrap().id;
    wall_slot.layer = other_layer;
    let w = wall_of(wall_slot);
    w.thickness = 0.37;
    w.top = linked;
    let mut dummy = Dummy::new(DVec2::ZERO, DVec2::ONE, p.materials[1].id);
    dummy.link = VerticalLink {
        base_offset: 0.2,
        top: linked,
    };
    dummy.parts = true;
    defaults.insert(DefaultsSlot {
        layer: p.layers[1].id,
        position: archicraft_model::Position::Undefined,
        structural: archicraft_model::StructuralFunction::Undefined,
        ifc_class: KindTag::Dummy.descriptor().default_class,
        predefined_type: None,
        kind: dummy.into(),
    });
    h.execute(&mut p, Command::SetToolDefaults(defaults.clone())).unwrap();

    h.execute(
        &mut p,
        Command::DeleteStorey {
            storey: basement_id,
            elements: StoreyElements::Delete,
        },
    )
    .unwrap();
    // The ground storey is 3 m high.
    let reset = TopLink::Fixed { height: 3.0 };
    let mut expected = defaults.clone();
    wall_of(expected.get_mut(KindTag::Wall).unwrap()).top = reset;
    match &mut expected.get_mut(KindTag::Dummy).unwrap().kind {
        ElementKind::Dummy(d) => d.link.top = reset,
        other => panic!("not a dummy slot: {other:?}"),
    }
    assert_eq!(p.tool_defaults, expected, "only the tops change");
    // Setting the defaults checks every slot.
    let mut edited = p.tool_defaults.clone();
    wall_of(edited.get_mut(KindTag::Wall).unwrap()).thickness = 0.4;
    h.execute(&mut p, Command::SetToolDefaults(edited))
        .expect("the reset slots are valid");
    h.undo(&mut p).unwrap();

    h.undo(&mut p).unwrap();
    assert_eq!(p.tool_defaults, defaults);
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

/// Removes every member named `key` from `value`, at any depth, and returns
/// how many it removed.
fn remove_members(value: &mut serde_json::Value, key: &str) -> usize {
    match value {
        serde_json::Value::Object(map) => {
            let own = usize::from(map.remove(key).is_some());
            own + map.values_mut().map(|v| remove_members(v, key)).sum::<usize>()
        }
        serde_json::Value::Array(items) => items.iter_mut().map(|v| remove_members(v, key)).sum(),
        _ => 0,
    }
}

/// ELM-020: a command log written before elements and tool defaults had a
/// structural function, so without any `structural` field, still loads and
/// replays (journals, autosave recovery), and its elements and slots read
/// as undefined.
#[test]
fn a_log_without_structural_functions_replays_as_undefined() {
    let mut p = default_metric_project();
    let start = p.clone();
    let mut h = History::default();
    let w = p.new_wall(p.storeys.ground(), DVec2::ZERO, DVec2::new(4.0, 0.0), 0);
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    let mut defaults = p.tool_defaults.clone();
    defaults.get_mut(KindTag::Wall).unwrap().position = archicraft_model::Position::Interior;
    h.execute(&mut p, Command::SetToolDefaults(defaults)).unwrap();

    let mut log = serde_json::to_value(h.applied_commands().collect::<Vec<_>>()).unwrap();
    assert!(remove_members(&mut log, "structural") >= 2, "{log}");
    let commands: Vec<Command> = serde_json::from_value(log).unwrap();
    let mut replayed = start;
    replay(&mut replayed, &commands).unwrap();
    assert_eq!(replayed.elements, p.elements);
    assert_eq!(replayed.tool_defaults, p.tool_defaults);
    assert!(
        replayed
            .elements
            .values()
            .all(|e| e.structural == archicraft_model::StructuralFunction::Undefined)
    );
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
    d.ifc_class = IfcClass::OpeningElement;
    assert!(!KindTag::Dummy.ifc_classes().contains(&IfcClass::OpeningElement));
    match h.execute(&mut p, Command::AddElements(vec![d])) {
        Err(CommandError::Invalid(message)) => {
            assert_eq!(message.key(), "model-error-ifc-class-not-allowed");
            assert_eq!(message.arg("kind"), Some(&Arg::Kind(KindTag::Dummy)));
        }
        other => panic!("expected CommandError::Invalid, got {other:?}"),
    }
    assert_eq!(p, before);
}

/// INP-017, UND-001: excluding an element and a layer from producing snap
/// targets is one undoable command each, through UpdateElements and
/// UpdateLayer.
#[test]
fn snap_exclusion_undoes_and_redoes_to_the_exact_project() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    let id = w.id;
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    let layer = p.layer(p.element(id).unwrap().layer).unwrap().clone();

    let changes = [
        Command::UpdateElements(vec![Element {
            snap_excluded: true,
            ..p.element(id).unwrap().clone()
        }]),
        Command::UpdateLayer(Layer {
            snap_excluded: true,
            ..layer
        }),
    ];
    for command in changes {
        let before = p.clone();
        assert!(p.provides_snaps(p.element(id).unwrap()));
        h.execute(&mut p, command).unwrap();
        assert!(!p.provides_snaps(p.element(id).unwrap()));
        let after = p.clone();

        h.undo(&mut p).unwrap();
        let mut expected = before;
        expected.revision = p.revision;
        assert_eq!(p, expected);

        h.redo(&mut p).unwrap();
        let mut expected = after;
        expected.revision = p.revision;
        assert_eq!(p, expected);
        h.undo(&mut p).unwrap();
    }
}

/// INP-017, LAY-001: locked elements and elements on locked layers provide
/// snap targets; hidden ones do not; and the snap flag is independent of
/// the visibility used for drawing and selection.
#[test]
fn locked_elements_provide_snaps_and_hidden_ones_do_not() {
    let mut p = default_metric_project();
    let mut w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    w.locked = true;
    assert!(p.provides_snaps(&w));

    let at = p.layers.iter().position(|l| l.id == w.layer).unwrap();
    let layer = p.layers[at].clone();
    p.layers[at] = Layer {
        locked: true,
        ..layer.clone()
    };
    assert!(p.provides_snaps(&w));
    p.layers[at] = Layer {
        visible: false,
        ..layer.clone()
    };
    assert!(!p.provides_snaps(&w));
    assert!(!p.is_visible(&w));

    p.layers[at] = Layer {
        snap_excluded: true,
        ..layer
    };
    assert!(!p.provides_snaps(&w));
    assert!(p.is_visible(&w), "an excluded layer stays visible");
    w.snap_excluded = true;
    p.layers[at].snap_excluded = false;
    assert!(!p.provides_snaps(&w));
    assert!(p.is_visible(&w), "an excluded element stays visible");
}

/// INP-017, ELM-013: an element-locked wall accepts the snap flag like its
/// lock flag; a wall on a locked layer rejects it with the lock reason.
#[test]
fn locked_walls_accept_the_snap_flag_unless_their_layer_is_locked() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let mut w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    w.locked = true;
    let id = w.id;
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    let excluded = Element {
        snap_excluded: true,
        ..p.element(id).unwrap().clone()
    };
    h.execute(&mut p, Command::UpdateElements(vec![excluded.clone()]))
        .unwrap();
    assert!(p.element(id).unwrap().snap_excluded);
    let moved = Element {
        snap_excluded: false,
        name: "Renamed".into(),
        ..p.element(id).unwrap().clone()
    };
    let (variant, message) = rejection(h.execute(&mut p, Command::UpdateElements(vec![moved])));
    assert_eq!((variant, message.key()), ("Locked", "model-error-element-locked"));
    h.undo(&mut p).unwrap();

    let unlocked = Element {
        locked: false,
        ..p.element(id).unwrap().clone()
    };
    h.execute(&mut p, Command::UpdateElements(vec![unlocked])).unwrap();
    let layer = p.layer(p.element(id).unwrap().layer).unwrap().clone();
    h.execute(&mut p, Command::UpdateLayer(Layer { locked: true, ..layer }))
        .unwrap();
    let excluded = Element {
        snap_excluded: true,
        ..p.element(id).unwrap().clone()
    };
    let (variant, message) = rejection(h.execute(&mut p, Command::UpdateElements(vec![excluded])));
    assert_eq!((variant, message.key()), ("Locked", "model-error-layer-locked"));
    assert!(!p.element(id).unwrap().snap_excluded);
}

/// Executes `command` and checks that undo gives back the project before it
/// and redo the project after it, apart from the cache revision, with the
/// elements and groups in the same order (UND-001).
#[track_caller]
fn assert_undo_redo_exact(p: &mut Project, h: &mut History, command: Command) {
    let order = |p: &Project| {
        let elements: Vec<ElementId> = p.elements.keys().copied().collect();
        let groups: Vec<GroupId> = p.groups.keys().copied().collect();
        (elements, groups)
    };
    let same = |p: &Project, expected: &Project| {
        assert_eq!(
            *p,
            Project {
                revision: p.revision,
                ..expected.clone()
            }
        );
        assert_eq!(order(p), order(expected));
    };
    let before = p.clone();
    h.execute(p, command).unwrap();
    let after = p.clone();
    assert_ne!(after, before);
    h.undo(p).unwrap();
    same(p, &before);
    h.redo(p).unwrap();
    same(p, &after);
}

/// Three walls A, B and C on the ground storey, added by command.
fn three_walls() -> (Project, History, [ElementId; 3]) {
    let mut p = default_metric_project();
    let mut h = History::default();
    let walls = [0.0, 1.0, 2.0].map(|y| wall(&p, (0.0, y), (5.0, y)));
    let ids = walls.each_ref().map(|w| w.id);
    h.execute(&mut p, Command::AddElements(walls.to_vec())).unwrap();
    (p, h, ids)
}

fn group_of(p: &Project, element: ElementId) -> Option<GroupId> {
    p.element(element).unwrap().group
}

/// ELM-015, UND-001: grouping, renaming and ungrouping are one undo step
/// each and undo and redo to the exact project. A new group is named from
/// the project's group counter.
#[test]
fn group_ungroup_and_rename_undo_and_redo_exactly() {
    let (mut p, mut h, [a, b, c]) = three_walls();
    let group = GroupId::new();
    let command = Command::Group {
        group,
        elements: vec![a, b],
    };
    assert_eq!(command.label(), Message::new("model-label-group"));
    assert_undo_redo_exact(&mut p, &mut h, command);
    assert_eq!(
        p.group(group),
        Some(&Group {
            id: group,
            name: "Group 1".into(),
            parent: None,
        })
    );
    assert_eq!(
        (group_of(&p, a), group_of(&p, b), group_of(&p, c)),
        (Some(group), Some(group), None)
    );
    assert_eq!(p.group_counter, 1);

    let rename = Command::RenameGroup {
        group,
        name: "  Stair core ".into(),
    };
    assert_eq!(rename.label(), Message::new("model-label-rename-group"));
    assert_undo_redo_exact(&mut p, &mut h, rename);
    assert_eq!(p.group(group).unwrap().name, "Stair core");

    let ungroup = Command::Ungroup { group };
    assert_eq!(ungroup.label(), Message::new("model-label-ungroup"));
    assert_undo_redo_exact(&mut p, &mut h, ungroup);
    assert!(p.groups.is_empty());
    assert!(p.elements.values().all(|e| e.group.is_none()));

    // The counter does not go back, so the next group gets a new name.
    let next = GroupId::new();
    h.execute(
        &mut p,
        Command::Group {
            group: next,
            elements: vec![b, c],
        },
    )
    .unwrap();
    assert_eq!(p.group(next).unwrap().name, "Group 2");
    h.undo(&mut p).unwrap();
    assert_eq!(p.group_counter, 1);
}

/// ELM-015: selecting a member of a group groups the group, which becomes
/// a child of the new one; ungrouping the outer group gives back the inner
/// group (A, B) unchanged.
#[test]
fn grouping_a_group_with_an_element_nests_it_and_ungrouping_restores_it() {
    let (mut p, mut h, [a, b, c]) = three_walls();
    let (inner, outer) = (GroupId::new(), GroupId::new());
    h.execute(
        &mut p,
        Command::Group {
            group: inner,
            elements: vec![a, b],
        },
    )
    .unwrap();
    let inner_before = p.group(inner).unwrap().clone();
    assert_undo_redo_exact(
        &mut p,
        &mut h,
        Command::Group {
            group: outer,
            elements: vec![a, c],
        },
    );
    assert_eq!(p.group(inner).unwrap().parent, Some(outer));
    assert_eq!(p.group(outer).unwrap().parent, None);
    assert_eq!(p.group(outer).unwrap().name, "Group 2");
    assert_eq!(
        (group_of(&p, a), group_of(&p, b), group_of(&p, c)),
        (Some(inner), Some(inner), Some(outer))
    );
    assert_eq!(p.group_members(outer), [a, b, c]);
    assert_eq!(p.group_path(b), [outer, inner]);

    h.execute(&mut p, Command::Ungroup { group: outer }).unwrap();
    assert_eq!(p.group(inner), Some(&inner_before));
    assert_eq!(p.groups.len(), 1);
    assert_eq!(p.group_members(inner), [a, b]);
    assert_eq!(group_of(&p, c), None);
}

/// ELM-015: ungrouping a nested group removes only that level: its
/// members and child groups move to its parent.
#[test]
fn ungrouping_a_nested_group_moves_its_contents_to_the_parent() {
    let (mut p, mut h, [a, b, c]) = three_walls();
    let d = wall(&p, (0.0, 3.0), (5.0, 3.0));
    let d_id = d.id;
    h.execute(&mut p, Command::AddElements(vec![d])).unwrap();
    let [innermost, middle, outer] = [GroupId::new(), GroupId::new(), GroupId::new()];
    for (group, elements) in [(innermost, vec![a, b]), (middle, vec![a, c]), (outer, vec![c, d_id])] {
        h.execute(&mut p, Command::Group { group, elements }).unwrap();
    }
    // outer { middle { innermost { A, B }, C }, D }
    assert_undo_redo_exact(&mut p, &mut h, Command::Ungroup { group: middle });
    assert_eq!(p.group(innermost).unwrap().parent, Some(outer));
    assert_eq!(group_of(&p, c), Some(outer));
    assert_eq!(group_of(&p, a), Some(innermost));
    assert!(p.group(middle).is_none());
    assert_eq!(p.group_members(outer), [a, b, c, d_id]);
}

/// ELM-015: fewer than two items after resolving each element to its
/// outermost group change nothing: one element, one element twice, nothing,
/// or only members of one group.
#[test]
fn grouping_a_single_item_changes_nothing() {
    let (mut p, mut h, [a, b, c]) = three_walls();
    let group = GroupId::new();
    h.execute(
        &mut p,
        Command::Group {
            group,
            elements: vec![a, b],
        },
    )
    .unwrap();
    let before = p.clone();
    let depth = h.undo_len();
    for elements in [vec![c], vec![c, c], vec![], vec![a], vec![a, b], vec![b, a, b]] {
        let result = h.execute(
            &mut p,
            Command::Group {
                group: GroupId::new(),
                elements: elements.clone(),
            },
        );
        assert_eq!(result, Err(CommandError::NoChange), "{elements:?}");
    }
    assert_eq!(p, before);
    assert_eq!(h.undo_len(), depth);
}

/// ELM-015: the group commands name existing groups and elements, a new
/// group's id is new, and a group name is not blank.
#[test]
fn group_commands_reject_unknown_ids_and_blank_names() {
    let (mut p, mut h, [a, b, c]) = three_walls();
    let group = GroupId::new();
    h.execute(
        &mut p,
        Command::Group {
            group,
            elements: vec![a, b],
        },
    )
    .unwrap();
    let before = p.clone();
    let unknown = GroupId::new();
    let cases = [
        (
            Command::Group {
                group: GroupId::new(),
                elements: vec![c, ElementId::new()],
            },
            ("NotFound", "model-error-element-not-found"),
        ),
        (
            Command::Group {
                group,
                elements: vec![a, c],
            },
            ("Invalid", "model-error-duplicate-group-id"),
        ),
        (
            Command::Ungroup { group: unknown },
            ("NotFound", "model-error-group-not-found"),
        ),
        (
            Command::RenameGroup {
                group: unknown,
                name: "Core".into(),
            },
            ("NotFound", "model-error-group-not-found"),
        ),
        (
            Command::RenameGroup {
                group,
                name: "  ".into(),
            },
            ("Invalid", "model-error-group-name"),
        ),
        (
            Command::RenameGroup {
                group,
                name: "Group 1".into(),
            },
            ("NoChange", "model-error-no-change"),
        ),
    ];
    for (command, expected) in cases {
        let label = command.label();
        let (variant, message) = rejection(h.execute(&mut p, command));
        assert_eq!((variant, message.key()), expected, "{label}");
    }
    assert_eq!(p, before);
}

/// ELM-015, UND-001: deleting every member of a group removes the group, and
/// undo restores both the elements and the group; deleting one of three
/// members keeps it.
#[test]
fn deleting_all_members_removes_the_group_and_undo_restores_both() {
    let (mut p, mut h, [a, b, c]) = three_walls();
    let group = GroupId::new();
    h.execute(
        &mut p,
        Command::Group {
            group,
            elements: vec![a, b, c],
        },
    )
    .unwrap();
    h.execute(&mut p, Command::DeleteElements(vec![b])).unwrap();
    assert_eq!(p.group_members(group), [a, c]);
    h.undo(&mut p).unwrap();

    assert_undo_redo_exact(&mut p, &mut h, Command::DeleteElements(vec![c, a, b]));
    assert!(p.elements.is_empty());
    assert!(p.groups.is_empty());
    h.undo(&mut p).unwrap();
    assert_eq!(p.group_members(group), [a, b, c]);
}

/// ELM-015: deleting the members of a nested group removes that group and
/// keeps its parent, which still has members.
#[test]
fn deleting_the_members_of_a_nested_group_keeps_its_parent() {
    let (mut p, mut h, [a, b, c]) = three_walls();
    let (inner, outer) = (GroupId::new(), GroupId::new());
    h.execute(
        &mut p,
        Command::Group {
            group: inner,
            elements: vec![a, b],
        },
    )
    .unwrap();
    h.execute(
        &mut p,
        Command::Group {
            group: outer,
            elements: vec![b, c],
        },
    )
    .unwrap();
    assert_undo_redo_exact(&mut p, &mut h, Command::DeleteElements(vec![a, b]));
    assert!(p.group(inner).is_none());
    assert_eq!(p.group_members(outer), [c]);
}

/// ELM-015: a storey deleted with its elements removes the groups it
/// leaves empty, undoably, and keeps a group with members elsewhere.
#[test]
fn deleting_a_storey_with_its_elements_removes_the_groups_left_empty() {
    let (mut p, mut h, [a, ..]) = three_walls();
    let first = p.storeys.as_slice()[1].id;
    let upper: Vec<Element> = [0.0, 1.0]
        .map(|y| {
            let mut w = p.new_wall(first, DVec2::new(0.0, y), DVec2::new(4.0, y), 0);
            if let ElementKind::Wall(wall) = &mut w.kind {
                wall.top = TopLink::Fixed { height: 2.5 };
            }
            w.name = format!("Upper {y}");
            w
        })
        .to_vec();
    let [u1, u2] = [upper[0].id, upper[1].id];
    h.execute(&mut p, Command::AddElements(upper)).unwrap();
    let (upstairs, across) = (GroupId::new(), GroupId::new());
    h.execute(
        &mut p,
        Command::Group {
            group: upstairs,
            elements: vec![u1, u2],
        },
    )
    .unwrap();
    h.execute(
        &mut p,
        Command::Group {
            group: across,
            elements: vec![a, u1],
        },
    )
    .unwrap();
    // across { upstairs { U1, U2 }, A }
    assert_undo_redo_exact(
        &mut p,
        &mut h,
        Command::DeleteStorey {
            storey: first,
            elements: StoreyElements::Delete,
        },
    );
    assert!(p.group(upstairs).is_none());
    assert_eq!(p.group_members(across), [a]);
}

/// ELM-015, ELM-013: grouping edits neither geometry nor parameters, so
/// locked elements and elements on locked layers may be grouped and
/// ungrouped.
#[test]
fn locked_elements_can_be_grouped_and_ungrouped() {
    let (mut p, mut h, [a, b, c]) = three_walls();
    let locked = Element {
        locked: true,
        ..p.element(a).unwrap().clone()
    };
    h.execute(&mut p, Command::UpdateElements(vec![locked])).unwrap();
    let layer = p.layer(p.element(b).unwrap().layer).unwrap().clone();
    h.execute(&mut p, Command::UpdateLayer(Layer { locked: true, ..layer }))
        .unwrap();
    let group = GroupId::new();
    h.execute(
        &mut p,
        Command::Group {
            group,
            elements: vec![a, b, c],
        },
    )
    .unwrap();
    assert_eq!(p.group_members(group), [a, b, c]);
    h.execute(&mut p, Command::Ungroup { group }).unwrap();
    assert!(p.groups.is_empty());
}

/// ELM-015: membership changes only through the group commands. Updating
/// an element to another group is refused, edits and moves keep the group,
/// and an added element names an existing group or none.
#[test]
fn membership_changes_only_through_the_group_commands() {
    let (mut p, mut h, [a, b, c]) = three_walls();
    let group = GroupId::new();
    h.execute(
        &mut p,
        Command::Group {
            group,
            elements: vec![a, b],
        },
    )
    .unwrap();
    for target in [None, Some(GroupId::new())] {
        let moved = Element {
            group: target,
            ..p.element(a).unwrap().clone()
        };
        let (variant, message) = rejection(h.execute(&mut p, Command::UpdateElements(vec![moved])));
        assert_eq!((variant, message.key()), ("Invalid", "model-error-group-change"));
    }
    let joined = Element {
        group: Some(group),
        ..p.element(c).unwrap().clone()
    };
    let (variant, message) = rejection(h.execute(&mut p, Command::UpdateElements(vec![joined])));
    assert_eq!((variant, message.key()), ("Invalid", "model-error-group-change"));

    h.execute(
        &mut p,
        Command::MoveElements {
            ids: vec![a, b],
            delta: DVec2::new(1.0, 0.0),
        },
    )
    .unwrap();
    assert_eq!((group_of(&p, a), group_of(&p, b)), (Some(group), Some(group)));

    let mut stray = wall(&p, (0.0, 9.0), (5.0, 9.0));
    stray.group = Some(GroupId::new());
    let (variant, message) = rejection(h.execute(&mut p, Command::AddElements(vec![stray.clone()])));
    assert_eq!((variant, message.key()), ("NotFound", "model-error-group-not-found"));
    stray.group = Some(group);
    h.execute(&mut p, Command::AddElements(vec![stray.clone()])).unwrap();
    assert_eq!(p.group_members(group), [a, b, stray.id]);
}

/// The L-shaped boundary of the space tests (SPC-001): a 6 m x 4 m
/// rectangle from the origin without its 2 m x 2 m north-east corner,
/// 20 m².
fn l_shape() -> Vec<DVec2> {
    [(0.0, 0.0), (6.0, 0.0), (6.0, 2.0), (4.0, 2.0), (4.0, 4.0), (0.0, 4.0)]
        .map(|(x, y)| DVec2::new(x, y))
        .to_vec()
}

/// A 1 m x 1 m hole inside [`l_shape`].
fn unit_hole() -> Vec<DVec2> {
    [(1.0, 1.0), (2.0, 1.0), (2.0, 2.0), (1.0, 2.0)]
        .map(|(x, y)| DVec2::new(x, y))
        .to_vec()
}

/// The space with the id `id` in `p`.
fn space_of(p: &Project, id: ElementId) -> &Space {
    p.element(id).and_then(Element::as_space).unwrap()
}

/// SPC-001, ELM-019, UND-001, UND-002: adding a space names it SP-001 and
/// bumps only its counter; undo restores the exact project, redo the added
/// space.
#[test]
fn adding_a_space_and_undo_redo_restore_the_project() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let start = content(&p);
    let s = p.new_space(ground(&p), l_shape(), Vec::new(), 0);
    let id = s.id;
    assert_eq!(s.name, "SP-001");
    assert_eq!(s.ifc_class, IfcClass::Space);
    h.execute(&mut p, Command::AddElements(vec![s])).unwrap();
    let added = content(&p);
    assert_eq!(p.counters.get(KindTag::Space), 1);
    assert_eq!(p.counters.get(KindTag::Wall), 0);
    assert_eq!(space_of(&p, id).boundary.outer().len(), 6);

    let label = h.undo(&mut p).unwrap();
    assert_eq!(label.arg("kind"), Some(&Arg::Kind(KindTag::Space)));
    assert_eq!(content(&p), start);
    h.redo(&mut p).unwrap();
    assert_eq!(content(&p), added);
}

/// SPC-001, EDT-018: a self-intersecting and a zero-area boundary are
/// rejected with `CommandError::Invalid`, and so is a space without a
/// positive height (its top at its base); the project stays unchanged.
#[test]
fn a_space_with_an_invalid_boundary_or_height_is_rejected() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let before = p.clone();
    let bow_tie = [(0.0, 0.0), (4.0, 3.0), (4.0, 0.0), (0.0, 4.0)].map(|(x, y)| DVec2::new(x, y));
    let flat = [(0.0, 0.0), (2.0, 0.0), (4.0, 0.0)].map(|(x, y)| DVec2::new(x, y));
    let mut flat_top = p.new_space(ground(&p), l_shape(), Vec::new(), 0);
    flat_top.as_space_mut().unwrap().link.top = TopLink::Relative {
        storeys: 0,
        offset: 0.0,
    };
    let cases = [
        (
            p.new_space(ground(&p), bow_tie.to_vec(), Vec::new(), 0),
            "model-error-polygon-self-intersection",
        ),
        (
            p.new_space(ground(&p), flat.to_vec(), Vec::new(), 0),
            "model-error-polygon-area",
        ),
        (flat_top, "model-error-top-below-base"),
    ];
    for (space, key) in cases {
        match h.execute(&mut p, Command::AddElements(vec![space])) {
            Err(CommandError::Invalid(message)) => assert_eq!(message.key(), key),
            other => panic!("expected CommandError::Invalid({key}), got {other:?}"),
        }
        assert_eq!(p, before);
    }
}

/// SPC-001: the net area of the L-shaped 6 m x 4 m boundary without its
/// 2 m x 2 m notch is 20 m², and 19 m² with a 1 m x 1 m hole; validation
/// orients the rings and keeps the area.
#[test]
fn a_space_net_area_deducts_the_notch_and_its_holes() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let mut clockwise = l_shape();
    clockwise.reverse();
    let plain = p.new_space(ground(&p), clockwise, Vec::new(), 0);
    let holed = p.new_space(ground(&p), l_shape(), vec![unit_hole()], 1);
    let (plain_id, holed_id) = (plain.id, holed.id);
    h.execute(&mut p, Command::AddElements(vec![plain, holed])).unwrap();
    let plain = space_of(&p, plain_id);
    assert!(plain.boundary.outer().is_counter_clockwise());
    assert!((plain.net_area() - 20.0).abs() < 1e-9, "{}", plain.net_area());
    let holed = space_of(&p, holed_id);
    assert!(!holed.boundary.holes()[0].is_counter_clockwise());
    assert!((holed.net_area() - 19.0).abs() < 1e-9, "{}", holed.net_area());
}

/// SPC-001, STR-006: a space's top is linked to the next storey by
/// default, so changing the storey height changes its resolved top; undo
/// restores it.
#[test]
fn a_space_top_follows_the_storey_height() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let s = p.new_space(ground(&p), l_shape(), Vec::new(), 0);
    let id = s.id;
    h.execute(&mut p, Command::AddElements(vec![s])).unwrap();
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
    h.undo(&mut p).unwrap();
    assert_eq!(p.vertical_extent(p.element(id).unwrap()), Some((0.0, 3.0)));
}

/// SPC-001: a space and the space tool's slot have no material, so
/// deleting a material with a replacement (DSP-003) leaves both unchanged
/// while the wall that used it changes.
#[test]
fn deleting_a_material_leaves_spaces_unchanged() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    let brick = w.kind.material().unwrap();
    let concrete = p.materials[0].id;
    let s = p.new_space(ground(&p), l_shape(), Vec::new(), 0);
    assert_eq!(s.kind.material(), None);
    let (wall_id, space_id) = (w.id, s.id);
    h.execute(&mut p, Command::AddElements(vec![w, s])).unwrap();
    let before = p.clone();
    assert_eq!(p.tool_defaults.get(KindTag::Space).unwrap().kind.material(), None);

    h.execute(
        &mut p,
        Command::DeleteMaterial {
            material: brick,
            replacement: concrete,
        },
    )
    .unwrap();
    assert_eq!(p.element(wall_id).unwrap().kind.material(), Some(concrete));
    assert_eq!(p.element(space_id), before.element(space_id));
    assert_eq!(
        p.tool_defaults.get(KindTag::Space),
        before.tool_defaults.get(KindTag::Space)
    );
    assert!(p.integrity_problems().is_empty(), "{:?}", p.integrity_problems());
}

/// SPC-001: the suggested number of a new space is the storey number and
/// a two-digit count of the storey's spaces plus one: 0.01 for the first on
/// the ground storey, 0.02 after it, 1.01 on the storey above.
#[test]
fn space_numbers_are_suggested_per_storey() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let g = ground(&p);
    let upper = p.storeys.relative(g, 1).unwrap().id;
    assert_eq!(p.suggested_space_number(g).as_deref(), Some("0.01"));
    let s = p.new_space(g, l_shape(), Vec::new(), 0);
    h.execute(&mut p, Command::AddElements(vec![s])).unwrap();
    assert_eq!(p.suggested_space_number(g).as_deref(), Some("0.02"));
    assert_eq!(p.suggested_space_number(upper).as_deref(), Some("1.01"));
    assert_eq!(p.suggested_space_number(StoreyId::new()), None);
}

/// SPC-001, ELM-017, NFR-SEC-003: a fixed space height outside 1 mm to
/// 1000 m and a boundary with more points than a slab may have are
/// rejected with the space's own messages, and the project stays
/// unchanged.
#[test]
fn a_space_beyond_its_limits_is_rejected_with_its_own_messages() {
    use archicraft_model::validate::MAX_SLAB_POINTS;

    let mut p = default_metric_project();
    let mut h = History::default();
    let before = p.clone();
    let mut flat = p.new_space(ground(&p), l_shape(), Vec::new(), 0);
    flat.as_space_mut().unwrap().link.top = TopLink::Fixed { height: 0.0 };
    let n = MAX_SLAB_POINTS + 1;
    let circle: Vec<DVec2> = (0..n)
        .map(|i| DVec2::from_angle(std::f64::consts::TAU * i as f64 / n as f64) * 100.0)
        .collect();
    let crowded = p.new_space(ground(&p), circle, Vec::new(), 0);
    for (space, key) in [
        (flat, "model-error-space-height-range"),
        (crowded, "model-error-space-points"),
    ] {
        match h.execute(&mut p, Command::AddElements(vec![space])) {
            Err(CommandError::Invalid(message)) => assert_eq!(message.key(), key),
            other => panic!("expected CommandError::Invalid({key}), got {other:?}"),
        }
        assert_eq!(p, before);
    }
}

/// The text with the id `id` in `p`.
fn text_of(p: &Project, id: ElementId) -> &Text {
    p.element(id).and_then(Element::as_text).unwrap()
}

/// The explanation of a rejected element: the error itself when it was
/// added, the reason nested in `model-error-element-invalid` when it was
/// edited.
fn explanation(error: CommandError) -> Message {
    let CommandError::Invalid(message) = error else {
        panic!("expected CommandError::Invalid, got {error:?}");
    };
    if message.key() != "model-error-element-invalid" {
        return message;
    }
    match message.arg("reason") {
        Some(Arg::Nested(reason)) => (**reason).clone(),
        other => panic!("expected a nested reason, got {other:?}"),
    }
}

/// ANN-007, ELM-019, UND-001, UND-002: adding a text names it TX-001,
/// binds it to the plan view of its storey with the text tool's defaults
/// (the 'Standard 2.5 mm' style, bottom left, angle 0, the '2D construction'
/// layer) and bumps only its counter; undo restores the exact project, redo
/// the added text.
#[test]
fn adding_a_text_and_undo_redo_restore_the_project() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let start = content(&p);
    let t = p.new_text(ground(&p), DVec2::new(2.0, 1.5), "Living room\n24 m²", 0);
    let id = t.id;
    assert_eq!(t.name, "TX-001");
    assert_eq!(t.ifc_class, IfcClass::Annotation);
    assert_eq!(p.layer(t.layer).unwrap().name, "2D construction");
    let text = t.as_text().unwrap();
    assert_eq!(text.view, DraftingView::plan(ground(&p)));
    assert_eq!(text.anchor, DVec2::new(2.0, 1.5));
    assert_eq!(
        (text.anchor_pos, text.angle, text.size_mm),
        (AnchorPos::BottomLeft, 0.0, None)
    );
    let style = p.text_style(text.style).unwrap();
    assert_eq!(style.name, "Standard 2.5 mm");
    assert_eq!(text.effective_size_mm(style), 2.5);
    let larger = Text {
        size_mm: Some(5.0),
        ..text.clone()
    };
    assert_eq!(larger.effective_size_mm(style), 5.0, "the override wins");

    h.execute(&mut p, Command::AddElements(vec![t])).unwrap();
    let added = content(&p);
    assert_eq!(p.counters.get(KindTag::Text), 1);
    assert_eq!(p.counters.get(KindTag::Wall), 0);
    assert_eq!(text_of(&p, id).content, "Living room\n24 m²");

    let label = h.undo(&mut p).unwrap();
    assert_eq!(label.arg("kind"), Some(&Arg::Kind(KindTag::Text)));
    assert_eq!(content(&p), start);
    h.redo(&mut p).unwrap();
    assert_eq!(content(&p), added);
}

/// ANN-007, ANN-002, ELM-017: a text with empty or blank content, a style
/// the project does not have, a size override that is not a positive finite
/// number of millimetres, an angle that is not finite or a view of another
/// storey than its home storey is rejected with `CommandError::Invalid` and
/// an explanation; the project stays unchanged. An edit that breaks a rule
/// is rejected the same way.
#[test]
fn a_text_with_invalid_content_style_size_or_view_is_rejected() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let before = p.clone();
    let upper = p.storeys.as_slice()[1].id;
    let with = |change: &dyn Fn(&mut Text)| {
        let mut element = before.new_text(ground(&before), DVec2::new(1.0, 1.0), "Hall", 0);
        change(element.as_text_mut().unwrap());
        element
    };
    let cases = [
        (with(&|t| t.content.clear()), "model-error-text-content-empty"),
        (with(&|t| t.content = " \n\t".into()), "model-error-text-content-empty"),
        (
            with(&|t| t.style = TextStyleId::new()),
            "model-error-text-style-missing",
        ),
        (with(&|t| t.size_mm = Some(0.0)), "model-error-text-size"),
        (with(&|t| t.size_mm = Some(-1.0)), "model-error-text-size"),
        (with(&|t| t.size_mm = Some(f64::NAN)), "model-error-text-size"),
        (with(&|t| t.size_mm = Some(f64::INFINITY)), "model-error-text-size"),
        (with(&|t| t.angle = f64::NAN), "model-error-text-angle"),
        (
            with(&|t| t.anchor = DVec2::new(f64::INFINITY, 0.0)),
            "model-error-coordinates-range",
        ),
        (
            with(&|t| t.view = DraftingView::plan(upper)),
            "model-error-view-not-home-storey",
        ),
    ];
    for (text, key) in cases {
        let error = h.execute(&mut p, Command::AddElements(vec![text])).unwrap_err();
        let explained = explanation(error);
        assert_eq!(explained.key(), key);
        if key == "model-error-view-not-home-storey" {
            assert_eq!(explained.arg("view"), Some(&Arg::Text("01 First floor".into())));
            assert_eq!(explained.arg("home"), Some(&Arg::Text("00 Ground floor".into())));
        }
        assert_eq!(p, before);
    }

    // A valid override is kept; an edit to an invalid one is rejected.
    let element = with(&|t| t.size_mm = Some(5.0));
    let id = element.id;
    h.execute(&mut p, Command::AddElements(vec![element])).unwrap();
    let added = p.clone();
    assert_eq!(text_of(&p, id).size_mm, Some(5.0));
    let mut edited = p.element(id).unwrap().clone();
    edited.as_text_mut().unwrap().content = String::new();
    let error = h.execute(&mut p, Command::UpdateElements(vec![edited])).unwrap_err();
    assert_eq!(explanation(error).key(), "model-error-text-content-empty");
    assert_eq!(p, added);
}

/// ANN-002, ELM-005, STR-004: moving a text to another storey rebinds its
/// view to that storey's plan and keeps its anchor; deleting its storey
/// with the elements moved to another one does the same; undo restores the
/// view.
#[test]
fn moving_a_text_to_another_storey_rebinds_its_view() {
    use archicraft_model::command::relink_to_storey;

    let mut p = default_metric_project();
    let mut h = History::default();
    let (ground, upper) = (ground(&p), p.storeys.as_slice()[1].id);
    let element = p.new_text(ground, DVec2::new(1.5, -2.25), "Hall", 0);
    let id = element.id;
    h.execute(&mut p, Command::AddElements(vec![element])).unwrap();

    let moved = relink_to_storey(&p, p.element(id).unwrap(), upper).unwrap();
    h.execute(&mut p, Command::UpdateElements(vec![moved])).unwrap();
    assert_eq!(p.element(id).unwrap().storey, upper);
    assert_eq!(text_of(&p, id).view, DraftingView::plan(upper));
    assert_eq!(text_of(&p, id).anchor, DVec2::new(1.5, -2.25));
    assert_eq!(p.vertical_extent(p.element(id).unwrap()), Some((3.0, 3.0)));

    h.execute(
        &mut p,
        Command::DeleteStorey {
            storey: upper,
            elements: StoreyElements::MoveTo(ground),
        },
    )
    .unwrap();
    assert_eq!(text_of(&p, id).view, DraftingView::plan(ground));
    h.undo(&mut p).unwrap();
    assert_eq!(text_of(&p, id).view, DraftingView::plan(upper));
}

/// The corners of a text block of `size` (width along the reading
/// direction, height across it) placed as the plan views place it: the
/// anchor position on the anchor, the reading direction at the angle turned
/// upright (ANN-007).
fn block_corners(text: &Text, size: DVec2) -> [DVec2; 4] {
    let along = DVec2::from_angle(archicraft_model::text::upright(text.angle));
    let up = along.perp();
    let bottom_left = text.anchor
        - along * size.x * text.anchor_pos.x_fraction()
        - up * size.y * (1.0 - text.anchor_pos.y_fraction());
    [
        bottom_left,
        bottom_left + along * size.x,
        bottom_left + along * size.x + up * size.y,
        bottom_left + up * size.y,
    ]
}

/// `points` rounded to micrometres and sorted, to compare blocks as point
/// sets.
fn point_set(points: impl IntoIterator<Item = DVec2>) -> Vec<(i64, i64)> {
    let mut set: Vec<(i64, i64)> = points
        .into_iter()
        .map(|c| ((c.x * 1e6).round() as i64, (c.y * 1e6).round() as i64))
        .collect();
    set.sort_unstable();
    set
}

/// EDT-002, ANN-007: rotating a text by 90 degrees about a point moves its
/// anchor about that point and adds 90 degrees to its angle; a move only
/// moves the anchor. Both are one undo step.
#[test]
fn rotating_a_text_turns_its_anchor_and_angle() {
    use std::f64::consts::FRAC_PI_2;

    use archicraft_model::transform::PlanTransform;

    let mut p = default_metric_project();
    let mut h = History::default();
    let mut element = p.new_text(ground(&p), DVec2::new(3.0, 1.0), "Hall", 0);
    element.as_text_mut().unwrap().angle = 0.25;
    let id = element.id;
    h.execute(&mut p, Command::AddElements(vec![element])).unwrap();
    let added = content(&p);

    let mut turned = p.element(id).unwrap().clone();
    turned
        .kind
        .transform(&PlanTransform::rotation(DVec2::new(1.0, 1.0), FRAC_PI_2))
        .unwrap();
    h.execute(&mut p, Command::UpdateElements(vec![turned])).unwrap();
    let text = text_of(&p, id);
    assert!(text.anchor.distance(DVec2::new(1.0, 3.0)) < 1e-12, "{}", text.anchor);
    assert!((text.angle - (0.25 + FRAC_PI_2)).abs() < 1e-12, "{}", text.angle);
    assert_eq!(text.anchor_pos, AnchorPos::BottomLeft);
    h.undo(&mut p).unwrap();
    assert_eq!(content(&p), added);

    h.execute(
        &mut p,
        Command::MoveElements {
            ids: vec![id],
            delta: DVec2::new(0.5, -1.0),
        },
    )
    .unwrap();
    let text = text_of(&p, id);
    assert_eq!((text.anchor, text.angle), (DVec2::new(3.5, 0.0), 0.25));
}

/// EDT-003, ANN-007: mirroring a text never mirrors its glyphs: it moves the
/// anchor to its mirror image and keeps the text readable, its reading
/// direction upright, with the block covering the mirror image of the block
/// before, whichever way the mirror axis runs; mirroring twice draws the
/// same block again (with the same angle, or the opposite one where the
/// angle before did not read upright).
#[test]
fn mirroring_a_text_keeps_it_readable() {
    use archicraft_model::kinds::Kind;
    use archicraft_model::transform::PlanTransform;

    let p = default_metric_project();
    let size = DVec2::new(2.0, 0.5);
    let axes = [
        (DVec2::ZERO, DVec2::Y),
        (DVec2::ZERO, DVec2::X),
        (DVec2::new(1.0, 0.0), DVec2::new(2.0, 1.0)),
        (DVec2::new(0.0, 4.0), DVec2::new(-1.0, 2.5)),
    ];
    for angle in [0.0, 0.5, -1.2, std::f64::consts::FRAC_PI_2, 2.8] {
        for anchor_pos in AnchorPos::ALL {
            for (a, b) in axes {
                let mirror = PlanTransform::mirror(a, b).unwrap();
                let mut element = p.new_text(ground(&p), DVec2::new(3.0, 1.0), "Hall", 0);
                let text = element.as_text_mut().unwrap();
                text.angle = angle;
                text.anchor_pos = anchor_pos;
                let before = text.clone();
                text.transform(&mirror).unwrap();
                let case = format!("angle {angle}, {anchor_pos:?}, axis {a} {b}");
                assert!(text.anchor.distance(mirror.apply(before.anchor)) < 1e-12, "{case}");
                let reading = archicraft_model::text::upright(text.angle);
                assert!(
                    (reading - text.angle).abs() < 1e-9,
                    "{case}: reads upright, {}",
                    text.angle
                );
                let mirrored = point_set(block_corners(&before, size).map(|c| mirror.apply(c)));
                assert_eq!(
                    point_set(block_corners(text, size)),
                    mirrored,
                    "{case}: covers the mirror image"
                );
                text.transform(&mirror).unwrap();
                assert!(text.anchor.distance(before.anchor) < 1e-9, "{case}");
                let half_turns = (text.angle - before.angle) / std::f64::consts::PI;
                assert!(
                    (half_turns - half_turns.round()).abs() < 1e-9,
                    "{case}: the angle modulo pi"
                );
                assert_eq!(
                    point_set(block_corners(text, size)),
                    point_set(block_corners(&before, size)),
                    "{case}: twice"
                );
            }
        }
    }
}

/// A project with a 0.30 m footing wall on the interior walls layer, of
/// the second material, centre-lined, 0.10 m up and with a fixed 2.4 m top
/// (the source), a default wall and a default slab, all added by command.
fn transfer_project() -> (Project, History, [ElementId; 3]) {
    use archicraft_model::{Position, PredefinedType, ReferenceLine};
    let mut p = default_metric_project();
    let mut h = History::default();
    let mut source = wall(&p, (0.0, 0.0), (4.0, 0.0));
    let w = source.as_wall_mut().unwrap();
    w.thickness = 0.30;
    w.reference_line = ReferenceLine::Center;
    w.base_offset = 0.10;
    w.top = TopLink::Fixed { height: 2.4 };
    w.material = p.materials[1].id;
    source.layer = p.layers.iter().find(|l| l.name == "Interior walls").unwrap().id;
    source.position = Position::Interior;
    source.ifc_class = IfcClass::Footing;
    source.predefined_type = Some(PredefinedType::StripFooting);
    let target = wall(&p, (0.0, 3.0), (2.0, 5.0));
    let slab = p.new_slab(
        ground(&p),
        vec![DVec2::ZERO, DVec2::X, DVec2::ONE, DVec2::Y],
        Vec::new(),
        0,
    );
    let ids = [source.id, target.id, slab.id];
    h.execute(&mut p, Command::AddElements(vec![source, target, slab]))
        .unwrap();
    (p, h, ids)
}

/// ELM-003, UND-001: injecting a wall's settings into a wall and a slab is
/// one undo step, labelled with the number of targets, that undoes and
/// redoes to the exact project; the wall gets every setting, the slab the
/// compatible ones, and both keep their geometry and names.
#[test]
fn inject_settings_undoes_and_redoes_to_the_exact_project() {
    let (mut p, mut h, [source, target, slab]) = transfer_project();
    let before = p.clone();
    let command = Command::InjectSettings {
        source: SettingsSource::Element(source),
        targets: vec![target, slab, target],
    };
    let label = command.label();
    assert_eq!(
        (label.key(), label.arg("count")),
        ("model-label-inject-settings", Some(&Arg::Count(2)))
    );
    assert_undo_redo_exact(&mut p, &mut h, command);
    let (s, w, sl) = (
        p.element(source).unwrap(),
        p.element(target).unwrap(),
        p.element(slab).unwrap(),
    );
    let mut expected = Element {
        id: w.id,
        name: w.name.clone(),
        ..s.clone()
    };
    if let (Some(e), Some(old)) = (expected.as_wall_mut(), before.element(target).unwrap().as_wall()) {
        (e.start, e.end) = (old.start, old.end);
    }
    assert_eq!(*w, expected);
    assert_eq!(sl.as_slab().unwrap().thickness, 0.30);
    assert_eq!(sl.as_slab().unwrap().material, s.as_wall().unwrap().material);
    assert_eq!((sl.layer, sl.position), (s.layer, s.position));
    assert_eq!(
        sl.as_slab().unwrap().outline,
        before.element(slab).unwrap().as_slab().unwrap().outline
    );
    assert_eq!(sl.name, before.element(slab).unwrap().name);
    assert_eq!(h.undo_label().unwrap().key(), "model-label-inject-settings");
}

/// ELM-003, ELM-013: injecting into a locked element is refused with the
/// lock reason, and a target that already has the source's settings changes
/// nothing.
#[test]
fn inject_settings_refuses_locked_targets_and_reports_no_change() {
    let (mut p, mut h, [source, target, _]) = transfer_project();
    let locked = Element {
        locked: true,
        ..p.element(target).unwrap().clone()
    };
    h.execute(&mut p, Command::UpdateElements(vec![locked])).unwrap();
    let inject = |targets: Vec<ElementId>| Command::InjectSettings {
        source: SettingsSource::Element(source),
        targets,
    };
    let (variant, message) = rejection(h.execute(&mut p, inject(vec![target])));
    assert_eq!((variant, message.key()), ("Locked", "model-error-element-locked"));
    let name = p.element(target).unwrap().name.clone();
    assert_eq!(message.arg("name"), Some(&Arg::Text(name)));

    let unlocked = Element {
        locked: false,
        ..p.element(target).unwrap().clone()
    };
    h.execute(&mut p, Command::UpdateElements(vec![unlocked])).unwrap();
    h.execute(&mut p, inject(vec![target])).unwrap();
    let unchanged = p.clone();
    assert_eq!(h.execute(&mut p, inject(vec![target])), Err(CommandError::NoChange));
    assert_eq!(h.execute(&mut p, inject(vec![source])), Err(CommandError::NoChange));
    assert_eq!(p, unchanged);
}

/// ELM-003, ELM-001: injecting a tool's defaults gives an element the
/// slot's settings.
#[test]
fn inject_settings_from_the_tool_defaults() {
    let (mut p, mut h, [source, _, _]) = transfer_project();
    h.execute(
        &mut p,
        Command::InjectSettings {
            source: SettingsSource::Defaults(KindTag::Wall),
            targets: vec![source],
        },
    )
    .unwrap();
    let slot = p.tool_defaults.get(KindTag::Wall).unwrap();
    let w = p.element(source).unwrap();
    assert_eq!(
        (w.layer, w.ifc_class, w.position),
        (slot.layer, slot.ifc_class, slot.position)
    );
    let (ElementKind::Wall(defaults), Some(wall)) = (&slot.kind, w.as_wall()) else {
        panic!("a wall slot")
    };
    assert_eq!(
        (wall.thickness, wall.top, wall.material),
        (defaults.thickness, defaults.top, defaults.material)
    );
}

/// ELM-003, UND-001: picking up a wall into the wall slot makes the slot's
/// prototype carry the wall's settings and layer; into the slab slot only
/// the compatible fields change. Both undo and redo to the exact project.
#[test]
fn pick_up_settings_into_the_same_and_another_kind() {
    let (mut p, mut h, [source, _, _]) = transfer_project();
    let before = p.clone();
    let command = Command::PickUpSettings {
        element: source,
        into: KindTag::Wall,
    };
    assert_eq!(command.label().key(), "model-label-pick-up-settings");
    assert_undo_redo_exact(&mut p, &mut h, command);
    let s = p.element(source).unwrap().clone();
    let slot = p.tool_defaults.get(KindTag::Wall).unwrap();
    assert_eq!(
        (slot.layer, slot.position, slot.ifc_class, slot.predefined_type),
        (s.layer, s.position, s.ifc_class, s.predefined_type)
    );
    let (ElementKind::Wall(proto), Some(wall)) = (&slot.kind, s.as_wall()) else {
        panic!("a wall slot")
    };
    assert_eq!(
        (
            proto.thickness,
            proto.reference_line,
            proto.base_offset,
            proto.top,
            proto.material
        ),
        (
            wall.thickness,
            wall.reference_line,
            wall.base_offset,
            wall.top,
            wall.material
        )
    );
    let ElementKind::Wall(old) = &before.tool_defaults.get(KindTag::Wall).unwrap().kind else {
        panic!("a wall slot")
    };
    assert_eq!((proto.start, proto.end), (old.start, old.end));
    assert_eq!(
        h.execute(
            &mut p,
            Command::PickUpSettings {
                element: source,
                into: KindTag::Wall,
            }
        ),
        Err(CommandError::NoChange)
    );

    let old_slab = p.tool_defaults.get(KindTag::Slab).unwrap().clone();
    assert_undo_redo_exact(
        &mut p,
        &mut h,
        Command::PickUpSettings {
            element: source,
            into: KindTag::Slab,
        },
    );
    let slot = p.tool_defaults.get(KindTag::Slab).unwrap();
    let mut expected = old_slab.clone();
    expected.layer = s.layer;
    expected.position = s.position;
    // IfcFooting is a slab class too, with its predefined type (ELM-011).
    expected.ifc_class = s.ifc_class;
    expected.predefined_type = s.predefined_type;
    if let ElementKind::Slab(slab) = &mut expected.kind {
        slab.thickness = 0.30;
        slab.material = wall.material;
    }
    assert_eq!(*slot, expected);
}

/// ELM-003: when no target takes any field of the source, the command is
/// refused. Here the only target's top is linked to a storey that is gone,
/// so the target fails validation whatever a slab gives it.
#[test]
fn inject_settings_is_refused_when_no_target_takes_any_setting() {
    let (mut p, _, [_, target, slab]) = transfer_project();
    let first = p.storeys.relative(ground(&p), 1).unwrap().id;
    if let Some(w) = p.elements.get_mut(&target).and_then(Element::as_wall_mut) {
        w.top = TopLink::Storey {
            storey: first,
            offset: 0.0,
        };
    }
    p.storeys = p.storeys.remove(first).unwrap();
    let mut h = History::default();
    let (variant, message) = rejection(h.execute(
        &mut p,
        Command::InjectSettings {
            source: SettingsSource::Element(slab),
            targets: vec![target],
        },
    ));
    assert_eq!(
        (variant, message.key()),
        ("Invalid", "model-error-transfer-unsupported")
    );
}
