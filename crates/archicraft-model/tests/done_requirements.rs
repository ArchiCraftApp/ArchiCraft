//! Regression net for the done model requirements (ELM-001, ELM-004, ELM-010, STR-001,
//! STR-004, STR-005, STR-008, STR-013, UND-001, UND-002, NFR-MNT-001,
//! NFR-ACC-002, NFR-ACC-005, INP-005).
//!
//! Every test is named after the requirement it pins, so the traceability
//! table in docs/testing.md and the status script can find it. Projects are
//! built from the default template and changed only through commands. Tests
//! that already exist elsewhere (tests/commands.rs and the unit tests in
//! storey.rs, project.rs and units.rs) are referenced in docs/testing.md
//! rather than repeated here.

mod common;

use archicraft_model::command::{DEFAULT_UNDO_LIMIT, JournalEntry, StoreyElements, replay};
use archicraft_model::glam::DVec2;
use archicraft_model::project::{Building, Location, ProjectInfo, Site, SlabDefaults, WallDefaults};
use archicraft_model::template::default_metric_project;
use archicraft_model::units::{LengthUnit, NumberFormat, format_angle, format_length, parse_length};
use archicraft_model::validate::validate_element;
use archicraft_model::{
    Command, CommandError, Element, ElementKind, History, IfcClass, KindTag, Layer, LayerId, MaterialId, Message,
    Position, Project, ReferenceLine, SlabReference, Storey, StoreyId, TopLink,
};
use common::{ground, wall};

/// Asserts that `p` equals `expected` in everything but the cache revision,
/// which every applied edit increments.
#[track_caller]
fn assert_same_model(p: &Project, expected: &Project) {
    let mut expected = expected.clone();
    expected.revision = p.revision;
    assert_eq!(p, &expected);
}

/// Executes `command`, then checks that undo restores the exact state before
/// and redo the exact state after it. Leaves the command applied.
#[track_caller]
fn assert_undo_redo_exact(p: &mut Project, h: &mut History, command: Command) {
    let label = command.label();
    let before = p.clone();
    let depth = h.undo_len();
    h.execute(p, command).unwrap_or_else(|e| panic!("{label}: {e}"));
    let after = p.clone();
    assert_ne!(after, before, "{label} changes the project");
    assert_eq!(h.undo_len(), depth + 1, "{label} is one undo step");

    assert_eq!(h.undo(p).as_ref(), Some(&label));
    assert_same_model(p, &before);
    assert_eq!(h.undo_len(), depth);

    assert_eq!(h.redo(p).as_ref(), Some(&label));
    assert_same_model(p, &after);
    assert_eq!(h.undo_len(), depth + 1);
}

/// Builds a command against the current state of the project.
type CommandFor = Box<dyn Fn(&Project) -> Command>;

fn storey_at(p: &Project, index: usize) -> StoreyId {
    p.storeys.as_slice()[index].id
}

fn top_storey(p: &Project) -> StoreyId {
    storey_at(p, p.storeys.len() - 1)
}

fn layer_named(p: &Project, name: &str) -> LayerId {
    p.layers
        .iter()
        .find(|l| l.name == name)
        .map(|l| l.id)
        .unwrap_or_else(|| panic!("the template has the layer {name}"))
}

fn rectangle(min: DVec2, max: DVec2) -> Vec<DVec2> {
    vec![min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)]
}

/// ELM-004: every element has an editable name; renaming is an undoable
/// UpdateElements, and an empty name is rejected (validate.rs).
#[test]
fn elm_004_renaming_an_element_is_an_undoable_edit() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    let id = w.id;
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    assert_eq!(p.element(id).unwrap().name, "W-001");

    let renamed = Element {
        name: "Party wall".into(),
        ..p.element(id).unwrap().clone()
    };
    assert_undo_redo_exact(&mut p, &mut h, Command::UpdateElements(vec![renamed]));
    assert_eq!(p.element(id).unwrap().name, "Party wall");
    h.undo(&mut p).unwrap();
    assert_eq!(p.element(id).unwrap().name, "W-001");

    let before = p.clone();
    for empty in ["", "   "] {
        let unnamed = Element {
            name: empty.into(),
            ..p.element(id).unwrap().clone()
        };
        assert!(matches!(
            h.execute(&mut p, Command::UpdateElements(vec![unnamed])),
            Err(CommandError::Invalid(_))
        ));
    }
    assert_eq!(p, before);
}

/// ELM-010: every element is on exactly one existing layer; new walls and
/// slabs take their tool's default layer; deleting a layer reassigns its
/// elements and the tool defaults to the replacement layer.
#[test]
fn elm_010_every_element_is_on_exactly_one_existing_layer() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let walls = layer_named(&p, "Exterior walls");
    let slabs = layer_named(&p, "Slabs");
    assert_eq!(p.tool_defaults.wall.layer, walls);
    assert_eq!(p.tool_defaults.slab.layer, slabs);

    let w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    let s = p.new_slab(ground(&p), rectangle(DVec2::ZERO, DVec2::splat(5.0)), Vec::new(), 0);
    let (wid, sid) = (w.id, s.id);
    h.execute(&mut p, Command::AddElements(vec![w, s])).unwrap();
    assert_eq!(p.element(wid).unwrap().layer, walls);
    assert_eq!(p.element(sid).unwrap().layer, slabs);

    let stray = Element {
        layer: LayerId::new(),
        ..wall(&p, (0.0, 1.0), (5.0, 1.0))
    };
    assert!(validate_element(&p, &stray).is_err());
    assert!(matches!(
        h.execute(&mut p, Command::AddElements(vec![stray])),
        Err(CommandError::Invalid(_))
    ));

    let structure = layer_named(&p, "Structure");
    h.execute(
        &mut p,
        Command::DeleteLayer {
            layer: walls,
            reassign_to: structure,
        },
    )
    .unwrap();
    assert!(p.layer(walls).is_none());
    assert_eq!(p.element(wid).unwrap().layer, structure);
    assert_eq!(p.element(sid).unwrap().layer, slabs);
    assert_eq!(p.tool_defaults.wall.layer, structure);
    assert_eq!(p.new_wall(ground(&p), DVec2::ZERO, DVec2::X, 0).layer, structure);
    for e in p.elements.values() {
        assert_eq!(p.layers.iter().filter(|l| l.id == e.layer).count(), 1, "{}", e.name);
    }
    assert!(p.integrity_problems().is_empty());
}

/// STR-001: ordered storeys with name, signed elevation and positive height,
/// numbered 0 at the ground, positive above and negative below; the top
/// storey's nominal height is used by links beyond it.
#[test]
fn str_001_storeys_are_ordered_numbered_and_the_top_has_a_nominal_height() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let g = ground(&p);
    let basement = Storey::new("-01 Basement", 0.0, 2.6);
    let second = Storey::new("02 Second floor", 0.0, 2.8);
    let (basement_id, second_id) = (basement.id, second.id);
    h.execute(
        &mut p,
        Command::InsertStorey {
            reference: g,
            above: false,
            storey: basement,
        },
    )
    .unwrap();
    let top = top_storey(&p);
    h.execute(
        &mut p,
        Command::InsertStorey {
            reference: top,
            above: true,
            storey: second,
        },
    )
    .unwrap();

    let numbers: Vec<_> = p.storeys.iter().map(|s| p.storeys.number(s.id).unwrap()).collect();
    assert_eq!(numbers, [-1, 0, 1, 2]);
    assert!(p.storeys.as_slice().windows(2).all(|w| w[0].elevation < w[1].elevation));
    assert_eq!(p.storeys.get(basement_id).unwrap().elevation, -2.6);
    assert_eq!(p.storeys.get(second_id).unwrap().elevation, 6.0);
    assert!(p.storeys.iter().all(|s| s.height > 0.0 && !s.name.is_empty()));

    let before = p.clone();
    for height in [0.0, -1.0] {
        assert!(matches!(
            h.execute(
                &mut p,
                Command::InsertStorey {
                    reference: g,
                    above: true,
                    storey: Storey::new("Bad", 0.0, height),
                },
            ),
            Err(CommandError::Invalid(_))
        ));
        assert!(matches!(
            h.execute(
                &mut p,
                Command::SetStoreyHeight {
                    storey: g,
                    height,
                    shift_above: true,
                },
            ),
            Err(CommandError::Invalid(_))
        ));
    }
    assert_eq!(p, before);

    // A wall on the top storey linked to "home + 1" ends one nominal height
    // above it, and follows a change of that nominal height.
    let w = p.new_wall(second_id, DVec2::ZERO, DVec2::X * 4.0, 0);
    assert_eq!(
        w.as_wall().unwrap().top,
        TopLink::Relative {
            storeys: 1,
            offset: 0.0
        }
    );
    let id = w.id;
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    assert_eq!(p.vertical_extent(p.element(id).unwrap()), Some((6.0, 6.0 + 2.8)));
    h.execute(
        &mut p,
        Command::SetStoreyHeight {
            storey: second_id,
            height: 3.2,
            shift_above: true,
        },
    )
    .unwrap();
    assert_eq!(p.vertical_extent(p.element(id).unwrap()), Some((6.0, 6.0 + 3.2)));
}

/// STR-004 (model half): deleting a storey either moves its elements to
/// another storey keeping their absolute position, or deletes them. The
/// prompt shows `elements_on(storey).count()`.
#[test]
fn str_004_deleting_a_storey_moves_or_deletes_its_elements() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let g = ground(&p);
    let first = storey_at(&p, 1);
    let kept = wall(&p, (0.0, 5.0), (4.0, 5.0));
    let linked = p.new_wall(first, DVec2::ZERO, DVec2::X * 4.0, 1);
    let mut fixed = p.new_wall(first, DVec2::ZERO, DVec2::Y * 4.0, 2);
    if let ElementKind::Wall(w) = &mut fixed.kind {
        w.top = TopLink::Fixed { height: 2.5 };
    }
    let slab = p.new_slab(first, rectangle(DVec2::ZERO, DVec2::splat(4.0)), Vec::new(), 0);
    let kept_id = kept.id;
    let moved_ids = [linked.id, fixed.id, slab.id];
    h.execute(&mut p, Command::AddElements(vec![kept, linked, fixed, slab]))
        .unwrap();

    assert_eq!(p.elements_on(first).count(), 3, "the count the delete prompt shows");
    let before: Vec<Element> = moved_ids.iter().map(|id| p.element(*id).unwrap().clone()).collect();
    let extents: Vec<_> = before.iter().map(|e| p.vertical_extent(e).unwrap()).collect();
    h.execute(
        &mut p,
        Command::DeleteStorey {
            storey: first,
            elements: StoreyElements::MoveTo(g),
        },
    )
    .unwrap();
    assert!(p.storeys.get(first).is_none());
    assert_eq!(p.elements_on(g).count(), 4);
    for ((id, old), extent) in moved_ids.iter().zip(&before).zip(&extents) {
        let moved = p.element(*id).unwrap();
        assert_eq!(moved.storey, g, "{}", moved.name);
        assert_eq!(p.vertical_extent(moved).as_ref(), Some(extent), "{}", moved.name);
        let mut in_plan = moved.kind.clone();
        let mut old_in_plan = old.kind.clone();
        for kind in [&mut in_plan, &mut old_in_plan] {
            match kind {
                ElementKind::Wall(w) => {
                    w.base_offset = 0.0;
                    w.top = TopLink::Fixed { height: 1.0 };
                }
                ElementKind::Slab(s) => s.offset = 0.0,
                other => panic!("only walls and slabs here: {other:?}"),
            }
        }
        assert_eq!(in_plan, old_in_plan, "{} keeps its plan position", moved.name);
    }

    h.undo(&mut p).unwrap();
    assert_eq!(p.elements_on(first).count(), 3);
    h.execute(
        &mut p,
        Command::DeleteStorey {
            storey: first,
            elements: StoreyElements::Delete,
        },
    )
    .unwrap();
    assert!(moved_ids.iter().all(|id| p.element(*id).is_none()));
    assert!(p.element(kept_id).is_some());
    assert_eq!(p.elements.len(), 1);
}

/// STR-005: each storey command undoes to the exact start state and redoes
/// to the edited state, and the undo stack only grows (UND-002).
#[test]
fn str_005_every_storey_command_undoes_exactly() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();

    let second = Storey::new("02 Second floor", 0.0, 2.8);
    let second_id = second.id;
    let steps: Vec<CommandFor> = vec![
        Box::new(move |p| Command::InsertStorey {
            reference: top_storey(p),
            above: true,
            storey: second.clone(),
        }),
        Box::new(|p| Command::InsertStorey {
            reference: ground(p),
            above: false,
            storey: Storey::new("-01 Basement", 0.0, 2.6),
        }),
        Box::new(|p| Command::RenameStorey {
            storey: ground(p),
            name: "EG".into(),
        }),
        Box::new(|p| Command::SetStoreyElevation {
            storey: storey_at(p, 2),
            elevation: 3.2,
            shift_above: false,
        }),
        Box::new(|p| Command::SetStoreyHeight {
            storey: ground(p),
            height: 3.4,
            shift_above: true,
        }),
        Box::new(|p| Command::SetStoreyCutPlane {
            storey: storey_at(p, 2),
            height: Some(1.5),
        }),
        Box::new(move |_| Command::DeleteStorey {
            storey: second_id,
            elements: StoreyElements::Delete,
        }),
    ];
    let mut depth = h.undo_len();
    for step in &steps {
        let command = step(&p);
        assert_undo_redo_exact(&mut p, &mut h, command);
        assert_eq!(h.undo_len(), depth + 1);
        depth = h.undo_len();
    }
    assert_eq!(h.undo_len(), 1 + steps.len());
}

/// STR-008: the cut plane is 1.10 m above the storey elevation by default and
/// a per-storey override wins; SetStoreyCutPlane undoes.
#[test]
fn str_008_cut_plane_defaults_to_1_10_m_and_a_storey_override_wins() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let (g, first) = (ground(&p), storey_at(&p, 1));
    assert_eq!(p.settings.cut_plane_height, 1.10);
    for s in p.storeys.iter() {
        assert_eq!(s.cut_plane_height, None);
        assert_eq!(p.cut_plane_height(s.id), 1.10);
    }

    let set = |height| Command::SetStoreyCutPlane { storey: first, height };
    assert_undo_redo_exact(&mut p, &mut h, set(Some(1.5)));
    assert_eq!(p.cut_plane_height(first), 1.5);
    assert_eq!(p.cut_plane_height(g), 1.10);

    let mut settings = p.settings.clone();
    settings.cut_plane_height = 1.2;
    h.execute(&mut p, Command::SetSettings(settings)).unwrap();
    assert_eq!(p.cut_plane_height(g), 1.2);
    assert_eq!(p.cut_plane_height(first), 1.5, "the override wins over the default");

    // The model can clear the override back to the project default.
    assert_undo_redo_exact(&mut p, &mut h, set(None));
    assert_eq!(p.cut_plane_height(first), 1.2);
    h.undo(&mut p).unwrap();
    h.undo(&mut p).unwrap();
    assert_eq!(p.cut_plane_height(first), 1.5);
    h.undo(&mut p).unwrap();
    assert_eq!(p.cut_plane_height(first), 1.10);
    assert_eq!(p.storeys.get(first).unwrap().cut_plane_height, None);
}

/// STR-013: one site and one building with editable names.
#[test]
fn str_013_one_site_and_one_building_with_editable_names() {
    let mut p = default_metric_project();
    let mut h = History::default();
    // One of each by construction: single fields, not collections.
    let (site, building): (&Site, &Building) = (&p.site, &p.building);
    assert_eq!((site.name.as_str(), building.name.as_str()), ("Site", "Building"));

    assert_undo_redo_exact(
        &mut p,
        &mut h,
        Command::SetSpatialNames {
            site: "Parcel 12".into(),
            building: "House A".into(),
        },
    );
    assert_eq!(
        (p.site.name.as_str(), p.building.name.as_str()),
        ("Parcel 12", "House A")
    );
    h.undo(&mut p).unwrap();
    assert_eq!((p.site.name.as_str(), p.building.name.as_str()), ("Site", "Building"));

    for (site, building) in [("", "House A"), ("Parcel 12", " ")] {
        assert!(matches!(
            h.execute(
                &mut p,
                Command::SetSpatialNames {
                    site: site.into(),
                    building: building.into(),
                },
            ),
            Err(CommandError::Invalid(_))
        ));
    }
}

/// ELM-001: the wall and slab tool defaults are part of the project (the
/// template supplies them), SetToolDefaults changes them as one undoable
/// step, and new walls and slabs take every default while elements that
/// already exist keep their values. Writing them to the file is pinned by
/// IFX-067.
#[test]
fn elm_001_tool_defaults_live_in_the_project_and_apply_to_new_elements() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let old = wall(&p, (0.0, 0.0), (5.0, 0.0));
    let old_id = old.id;
    h.execute(&mut p, Command::AddElements(vec![old.clone()])).unwrap();

    // The template supplies the defaults (Appendix D.2 pins every value).
    assert_eq!(
        (p.tool_defaults.wall.thickness, p.tool_defaults.slab.thickness),
        (0.25, 0.20)
    );
    assert_eq!(p.tool_defaults.wall.layer, layer_named(&p, "Exterior walls"));
    assert_eq!(old.as_wall().unwrap().thickness, 0.25);

    let mut defaults = p.tool_defaults.clone();
    let structure = layer_named(&p, "Structure");
    let material = p
        .materials
        .iter()
        .map(|m| m.id)
        .find(|&m| m != defaults.wall.material && m != defaults.slab.material)
        .expect("the template has a third material");
    defaults.wall = WallDefaults {
        thickness: 0.365,
        reference_line: ReferenceLine::Inside,
        base_offset: -0.05,
        top: TopLink::Fixed { height: 2.6 },
        material,
        layer: structure,
        position: Position::Interior,
        ifc_class: IfcClass::Footing,
    };
    defaults.slab = SlabDefaults {
        thickness: 0.18,
        reference: SlabReference::Bottom,
        offset: 0.02,
        material,
        layer: structure,
        position: Position::Exterior,
        ifc_class: IfcClass::Covering,
    };
    assert_ne!(defaults, p.tool_defaults);
    let template = p.tool_defaults.clone();
    assert_undo_redo_exact(&mut p, &mut h, Command::SetToolDefaults(defaults.clone()));
    assert_eq!(p.tool_defaults, defaults);

    let w = p.new_wall(ground(&p), DVec2::ZERO, DVec2::Y, 0);
    let ww = w.as_wall().unwrap();
    assert_eq!(
        (ww.thickness, ww.reference_line, ww.base_offset, ww.top, ww.material),
        (
            0.365,
            ReferenceLine::Inside,
            -0.05,
            TopLink::Fixed { height: 2.6 },
            material
        )
    );
    assert_eq!(
        (w.layer, w.position, w.ifc_class),
        (structure, Position::Interior, IfcClass::Footing)
    );
    let s = p.new_slab(ground(&p), rectangle(DVec2::ZERO, DVec2::splat(4.0)), Vec::new(), 0);
    let ss = s.as_slab().unwrap();
    assert_eq!(
        (ss.thickness, ss.reference, ss.offset, ss.material),
        (0.18, SlabReference::Bottom, 0.02, material)
    );
    assert_eq!(
        (s.layer, s.position, s.ifc_class),
        (structure, Position::Exterior, IfcClass::Covering)
    );
    h.execute(&mut p, Command::AddElements(vec![w, s])).unwrap();
    assert_eq!(p.element(old_id).unwrap(), &old, "existing elements keep their values");

    // Invalid defaults are rejected and leave the project's defaults alone.
    let mut bad = defaults.clone();
    bad.wall.layer = LayerId::new();
    assert!(matches!(
        h.execute(&mut p, Command::SetToolDefaults(bad)),
        Err(CommandError::Invalid(_))
    ));
    assert_eq!(p.tool_defaults, defaults);

    while h.undo(&mut p).is_some() {}
    assert_eq!(p.tool_defaults, template);
}

/// UND-001: one command of each category (element edit, storey operation,
/// layer and material resource change, tool defaults, settings, project
/// information, location) undoes and redoes to the exact state.
#[test]
fn und_001_every_change_category_undoes_and_redoes_exactly() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let g = ground(&p);
    let w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    let id = w.id;
    assert_undo_redo_exact(&mut p, &mut h, Command::AddElements(vec![w]));
    assert_undo_redo_exact(
        &mut p,
        &mut h,
        Command::MoveElements {
            ids: vec![id],
            delta: DVec2::new(1.0, 2.0),
        },
    );
    assert_undo_redo_exact(
        &mut p,
        &mut h,
        Command::RenameStorey {
            storey: g,
            name: "EG".into(),
        },
    );
    assert_undo_redo_exact(&mut p, &mut h, Command::AddLayer(Layer::new("Survey")));
    let mut brick = p.material(p.tool_defaults.wall.material).unwrap().clone();
    brick.opacity = 0.5;
    assert_undo_redo_exact(&mut p, &mut h, Command::UpdateMaterial(brick));
    let mut defaults = p.tool_defaults.clone();
    defaults.wall.thickness = 0.365;
    assert_undo_redo_exact(&mut p, &mut h, Command::SetToolDefaults(defaults));
    let mut settings = p.settings.clone();
    settings.units.length = LengthUnit::Millimeter;
    assert_undo_redo_exact(&mut p, &mut h, Command::SetSettings(settings));
    let info = ProjectInfo {
        name: "House A".into(),
        number: "2026-017".into(),
        ..p.info.clone()
    };
    assert_undo_redo_exact(&mut p, &mut h, Command::SetInfo(info));
    let location = Location {
        altitude: 312.5,
        survey_point: [500_000.0, 5_300_000.0],
        true_north: 0.25,
    };
    assert_undo_redo_exact(&mut p, &mut h, Command::SetLocation(location));
    assert_eq!(h.undo_len(), 9);
}

/// UND-001: at least 100 steps: 150 commands followed by 100 undos all
/// succeed with a limit of 100 and with the default limit.
#[test]
fn und_001_history_keeps_at_least_100_steps() {
    const { assert!(DEFAULT_UNDO_LIMIT >= 100) };
    for (limit, mut h) in [(100, History::new(100)), (DEFAULT_UNDO_LIMIT, History::default())] {
        let mut p = default_metric_project();
        let mut states = vec![p.clone()];
        for i in 0..150 {
            let w = p.new_wall(ground(&p), DVec2::new(0.0, i as f64), DVec2::new(4.0, i as f64), 0);
            h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
            states.push(p.clone());
        }
        assert_eq!(h.undo_len(), limit.min(150));
        for back in 1..=100 {
            assert!(h.undo(&mut p).is_some(), "undo {back} with a limit of {limit}");
            assert_same_model(&p, &states[150 - back]);
        }
        assert_eq!(p.elements.len(), 50);
        if limit == 100 {
            assert!(!h.can_undo(), "a limit of 100 keeps exactly 100 steps");
        }
    }
}

/// One instance of every [`Command`] variant. The match has no wildcard arm,
/// so a new variant does not compile until it is added here and to
/// [`scripted_session`].
fn variant(command: &Command) -> usize {
    match command {
        Command::AddElements(_) => 0,
        Command::DeleteElements(_) => 1,
        Command::UpdateElements(_) => 2,
        Command::MoveElements { .. } => 3,
        Command::InsertStorey { .. } => 4,
        Command::DeleteStorey { .. } => 5,
        Command::RenameStorey { .. } => 6,
        Command::SetStoreyElevation { .. } => 7,
        Command::SetStoreyHeight { .. } => 8,
        Command::SetStoreyCutPlane { .. } => 9,
        Command::AddLayer(_) => 10,
        Command::UpdateLayer(_) => 11,
        Command::DeleteLayer { .. } => 12,
        Command::AddMaterial(_) => 13,
        Command::UpdateMaterial(_) => 14,
        Command::DeleteMaterial { .. } => 15,
        Command::SetToolDefaults(_) => 16,
        Command::SetSettings(_) => 17,
        Command::SetInfo(_) => 18,
        Command::SetSpatialNames { .. } => 19,
        Command::SetLocation(_) => 20,
    }
}

const VARIANTS: usize = 21;

/// A recorded session that executes every command variant at least once.
struct Session {
    start: Project,
    end: Project,
    history: History,
    commands: Vec<Command>,
}

/// Builds each command against the current project and executes it,
/// checking after every step that the undo stack only grew (UND-002).
fn scripted_session() -> Session {
    let mut p = default_metric_project();
    let start = p.clone();
    let mut h = History::default();
    let mut commands = Vec::new();
    let mut run = |p: &mut Project, command: Command| {
        let label = command.label();
        let depth = h.undo_len();
        if let Err(e) = h.execute(p, command.clone()) {
            panic!("{label}: {e}");
        }
        assert_eq!(h.undo_len(), depth + 1, "{label} keeps the history");
        commands.push(command);
    };

    let g = ground(&p);
    let w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    let doomed = wall(&p, (0.0, 3.0), (5.0, 3.0));
    let (wid, doomed_id) = (w.id, doomed.id);
    run(&mut p, Command::AddElements(vec![w, doomed]));
    run(&mut p, Command::DeleteElements(vec![doomed_id]));
    let renamed = Element {
        name: "Party wall".into(),
        ..p.element(wid).expect("the wall exists").clone()
    };
    run(&mut p, Command::UpdateElements(vec![renamed]));
    run(
        &mut p,
        Command::MoveElements {
            ids: vec![wid],
            delta: DVec2::new(0.5, 0.25),
        },
    );
    let basement = Storey::new("-01 Basement", 0.0, 2.6);
    let basement_id = basement.id;
    run(
        &mut p,
        Command::InsertStorey {
            reference: g,
            above: false,
            storey: basement,
        },
    );
    run(
        &mut p,
        Command::RenameStorey {
            storey: basement_id,
            name: "-01 Cellar".into(),
        },
    );
    run(
        &mut p,
        Command::SetStoreyElevation {
            storey: basement_id,
            elevation: -2.8,
            shift_above: false,
        },
    );
    run(
        &mut p,
        Command::SetStoreyHeight {
            storey: g,
            height: 3.2,
            shift_above: true,
        },
    );
    run(
        &mut p,
        Command::SetStoreyCutPlane {
            storey: g,
            height: Some(1.3),
        },
    );
    run(
        &mut p,
        Command::DeleteStorey {
            storey: basement_id,
            elements: StoreyElements::MoveTo(g),
        },
    );
    let survey = Layer::new("Survey");
    let survey_id = survey.id;
    run(&mut p, Command::AddLayer(survey.clone()));
    run(
        &mut p,
        Command::UpdateLayer(Layer {
            visible: false,
            ..survey
        }),
    );
    let structure = layer_named(&p, "Structure");
    run(
        &mut p,
        Command::DeleteLayer {
            layer: survey_id,
            reassign_to: structure,
        },
    );
    let concrete = p.materials[0].id;
    let mut earth = p.materials[0].clone();
    earth.id = MaterialId::new();
    earth.name = "Rammed earth".into();
    earth.code = "RE-E".into();
    let earth_id = earth.id;
    run(&mut p, Command::AddMaterial(earth.clone()));
    earth.opacity = 0.8;
    run(&mut p, Command::UpdateMaterial(earth));
    run(
        &mut p,
        Command::DeleteMaterial {
            material: earth_id,
            replacement: concrete,
        },
    );
    let mut defaults = p.tool_defaults.clone();
    defaults.slab.thickness = 0.25;
    run(&mut p, Command::SetToolDefaults(defaults));
    let mut settings = p.settings.clone();
    settings.units.length_decimals = 2;
    run(&mut p, Command::SetSettings(settings));
    let info = ProjectInfo {
        name: "House A".into(),
        ..p.info.clone()
    };
    run(&mut p, Command::SetInfo(info));
    run(
        &mut p,
        Command::SetSpatialNames {
            site: "Parcel 12".into(),
            building: "House A".into(),
        },
    );
    let location = Location {
        altitude: 312.5,
        ..p.site.location
    };
    run(&mut p, Command::SetLocation(location));

    Session {
        start,
        end: p,
        history: h,
        commands,
    }
}

/// UND-002: a session executing every command variant ends with an undo
/// stack that holds every successful command; none clears the history.
#[test]
fn und_002_no_command_clears_the_history() {
    let session = scripted_session();
    let mut covered = [false; VARIANTS];
    for command in &session.commands {
        covered[variant(command)] = true;
    }
    assert!(covered.iter().all(|c| *c), "every variant runs: {covered:?}");
    assert_eq!(session.history.undo_len(), session.commands.len());
    assert!(session.history.applied_commands().eq(session.commands.iter()));
}

/// NFR-MNT-001: every command variant round-trips through serde_json.
#[test]
fn nfr_mnt_001_every_command_variant_round_trips_through_serde_json() {
    let session = scripted_session();
    let mut covered = [false; VARIANTS];
    for command in &session.commands {
        let json = serde_json::to_string(command).unwrap();
        let back: Command = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, command, "{json}");
        covered[variant(command)] = true;
    }
    assert!(covered.iter().all(|c| *c), "every variant round-trips: {covered:?}");
}

/// NFR-MNT-001: a recorded session, logged as JSON and replayed with
/// `command::replay` onto the starting project, yields the same project.
#[test]
fn nfr_mnt_001_replaying_a_recorded_session_rebuilds_the_project() {
    let session = scripted_session();
    let log = serde_json::to_string(&session.history.applied_commands().collect::<Vec<_>>()).unwrap();
    let commands: Vec<Command> = serde_json::from_str(&log).unwrap();
    let mut replayed = session.start.clone();
    let history = replay(&mut replayed, &commands).unwrap();
    assert_same_model(&replayed, &session.end);
    assert_eq!(history.undo_len(), commands.len());
}

/// NFR-MNT-001: the journal logs executed, undone and redone commands with
/// their labels.
#[test]
fn nfr_mnt_001_the_journal_lists_executed_undone_and_redone_commands() {
    let mut p = default_metric_project();
    let mut h = History::default();
    let g = ground(&p);
    let w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    h.execute(
        &mut p,
        Command::RenameStorey {
            storey: g,
            name: "EG".into(),
        },
    )
    .unwrap();
    h.undo(&mut p).unwrap();
    h.redo(&mut p).unwrap();
    let journal: Vec<_> = h.journal().cloned().collect();
    assert_eq!(
        journal,
        [
            JournalEntry::Executed(Message::new("model-label-create-kind").kind("kind", KindTag::Wall)),
            JournalEntry::Executed(Message::new("model-label-rename-storey")),
            JournalEntry::Undone(Message::new("model-label-rename-storey")),
            JournalEntry::Redone(Message::new("model-label-rename-storey")),
        ]
    );
}

/// INP-005: typed lengths accept the suffixes mm, cm, m, ft, in, ′ and ″
/// (and the ASCII ' and ") and imperial fractions such as 3'-4 1/2" in every
/// project length unit, and give the same length whatever the unit setting.
#[test]
fn inp_005_typed_lengths_accept_unit_suffixes_and_imperial_fractions_in_any_unit() {
    let nf = NumberFormat::default();
    let cases = [
        ("365mm", 0.365),
        ("365 mm", 0.365),
        ("30cm", 0.3),
        ("2.5m", 2.5),
        ("2.5 m", 2.5),
        ("10ft", 3.048),
        ("6in", 0.1524),
        ("12'", 12.0 * 0.3048),
        ("12′", 12.0 * 0.3048),
        ("4\"", 4.0 * 0.0254),
        ("4″", 4.0 * 0.0254),
        ("3'-4 1/2\"", 3.0 * 0.3048 + 4.5 * 0.0254),
        ("3′-4 1/2″", 3.0 * 0.3048 + 4.5 * 0.0254),
        ("1/2\"", 0.0127),
    ];
    for unit in LengthUnit::ALL {
        for (text, metres) in cases {
            let v = parse_length(text, unit, &nf).unwrap_or_else(|e| panic!("{text} in {unit:?}: {e:?}"));
            assert!((v - metres).abs() < 1e-12, "{text} in {unit:?} parses to {v}");
        }
    }
}

/// NFR-ACC-002: input resolution of at least 0.1 mm and 1/64 inch, stored at
/// full double precision. Extends `display_rounding_does_not_lose_typed_precision`
/// in units.rs.
#[test]
fn nfr_acc_002_input_resolves_0_1_mm_and_1_64_inch_at_full_precision() {
    let nf = NumberFormat::default();
    let tenth_mm = 0.0001;
    for (text, unit) in [
        ("0.1 mm", LengthUnit::Meter),
        ("0.0001", LengthUnit::Meter),
        ("0.1", LengthUnit::Millimeter),
    ] {
        let v = parse_length(text, unit, &nf).unwrap();
        assert!((v - tenth_mm).abs() <= 1e-15, "{text} parses to {v}");
    }
    let sixty_fourth = 0.0254 / 64.0;
    for (text, unit) in [("1/64\"", LengthUnit::Meter), ("1/64\"", LengthUnit::FeetInches)] {
        let v = parse_length(text, unit, &nf).unwrap();
        assert!((v - sixty_fourth).abs() <= 1e-15, "{text} parses to {v}");
    }

    let typed = parse_length("1.23456789", LengthUnit::Meter, &nf).unwrap();
    assert_eq!(typed.to_bits(), 1.234_567_89_f64.to_bits());
    let mut p = default_metric_project();
    let mut h = History::default();
    let w = wall(&p, (0.0, 0.0), (typed, 0.0));
    let id = w.id;
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    assert_eq!(
        p.element(id).unwrap().as_wall().unwrap().end.x.to_bits(),
        typed.to_bits()
    );
}

/// NFR-ACC-005: values are rounded only for display, according to the unit
/// settings; changing the units changes the display, never the stored value.
#[test]
fn nfr_acc_005_values_are_rounded_only_for_display() {
    let nf = NumberFormat::default();
    let mut p = default_metric_project();
    let mut h = History::default();
    let length = 1.234_567_89;
    let w = wall(&p, (0.0, 0.0), (length, 0.0));
    let id = w.id;
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    let angle = 0.123_456_7;
    let location = Location {
        true_north: angle,
        ..p.site.location
    };
    h.execute(&mut p, Command::SetLocation(location)).unwrap();
    let stored = |p: &Project| p.element(id).unwrap().as_wall().unwrap().end.x;

    assert_eq!(p.settings.units.length_decimals, 3);
    assert_eq!(format_length(stored(&p), &p.settings.units, &nf, true), "1.235 m");
    assert_eq!(
        format_angle(p.site.location.true_north, &p.settings.units, &nf, true),
        "7.07°"
    );
    assert_eq!(stored(&p).to_bits(), length.to_bits());

    let elements = p.elements.clone();
    let mut settings = p.settings.clone();
    settings.units.length = LengthUnit::Millimeter;
    settings.units.length_decimals = 1;
    h.execute(&mut p, Command::SetSettings(settings)).unwrap();
    assert_eq!(format_length(stored(&p), &p.settings.units, &nf, true), "1234.6 mm");
    assert_eq!(p.elements, elements);
    assert_eq!(stored(&p).to_bits(), length.to_bits());
    assert_eq!(p.site.location.true_north.to_bits(), angle.to_bits());
}
