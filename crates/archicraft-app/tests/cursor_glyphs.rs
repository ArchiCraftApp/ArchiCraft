//! The cursor glyphs of point input (INP-008, INP-024): a glyph for every
//! kind of snap, an overlay for every command state and the label beside
//! the cursor that names the kind, drawn without a GPU.

#![allow(clippy::unwrap_used)]

mod common;

use archicraft_app::input::{
    ArcTarget, CursorState, DivisionMode, SnapIndex, SnapKind, SnapSource, SourceClass, TargetOwner, TargetSink,
    candidates,
};
use archicraft_app::views::plan;
use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::WriteOptions;
use archicraft_model::glam::DVec2;
use archicraft_model::samples;
use common::{data_dir, harness};
use egui::{Align2, Color32, FontId, Pos2, Shape};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

/// A circle around (5, 0) with radius 3 and an edge from (-3, -4) to
/// (7, -4): the tangent points from the origin are (3.2, ±2.4), the foot of
/// the perpendicular from it is (0, -4), away from the edge's midpoint.
struct Synthetic;

impl SnapSource for Synthetic {
    fn owner(&self) -> TargetOwner {
        TargetOwner::Reference(1)
    }

    fn targets(&self, sink: &mut TargetSink) {
        sink.arc(SourceClass::Reference, ArcTarget::circle(DVec2::new(5.0, 0.0), 3.0));
        sink.edge(SourceClass::Reference, [DVec2::new(-3.0, -4.0), DVec2::new(7.0, -4.0)]);
    }
}

/// The best snap near `cursor` from the origin, among [`Synthetic`]'s
/// targets, without the grid.
fn synthetic_snap(cursor: DVec2) -> SnapKind {
    let mut index = SnapIndex::default();
    index.sync(1, &[&Synthetic]);
    candidates(&index, &[], cursor, 0.3, 0.0, Some(DVec2::ZERO), &[])[0].kind
}

/// INP-008, INP-024: every kind of snap has a glyph of its own and every
/// command state an overlay of its own, the crosshair stands for a free
/// point, and all of them are drawn in one frame with the labels of a
/// tangent and a perpendicular snap, which name their kind.
#[test]
fn every_snap_kind_and_cursor_state_has_its_own_glyph() {
    let color = Color32::WHITE;
    let at = Pos2::new(40.0, 40.0);
    let glyphs: Vec<Vec<Shape>> = SnapKind::ALL.iter().map(|&k| plan::snap_glyph(at, k, color)).collect();
    for (i, glyph) in glyphs.iter().enumerate() {
        assert!(!glyph.is_empty(), "{:?} has a glyph", SnapKind::ALL[i]);
        for (j, other) in glyphs.iter().enumerate().skip(i + 1) {
            assert_ne!(
                glyph,
                other,
                "{:?} and {:?} look alike",
                SnapKind::ALL[i],
                SnapKind::ALL[j]
            );
        }
    }
    assert_eq!(plan::snap_glyph(at, SnapKind::Free, color), plan::crosshair(at, color));

    let overlays: Vec<Vec<Shape>> = CursorState::ALL
        .iter()
        .map(|&s| plan::cursor_state_glyph(at, s, color))
        .collect();
    for (i, overlay) in overlays.iter().enumerate() {
        let state = CursorState::ALL[i];
        assert_eq!(overlay.is_empty(), state == CursorState::Normal, "{state:?}");
        for (j, other) in overlays.iter().enumerate().skip(i + 1) {
            assert_ne!(overlay, other, "{state:?} and {:?} look alike", CursorState::ALL[j]);
        }
    }

    let tangent = synthetic_snap(DVec2::new(3.25, 2.35));
    let perpendicular = synthetic_snap(DVec2::new(0.1, -3.95));
    assert_eq!((tangent, perpendicular), (SnapKind::Tangent, SnapKind::Perpendicular));
    let labels = [
        plan::cursor_label(Some("ΔX 3".into()), tangent),
        plan::cursor_label(None, perpendicular),
        plan::cursor_label(None, SnapKind::Free),
    ];
    assert_eq!(labels[0], "ΔX 3\ntangent");
    assert_eq!(labels[1], "perpendicular");
    assert_eq!(labels[2], "");

    let mut harness = Harness::new_ui(move |ui| {
        let painter = ui.painter();
        for (i, glyph) in glyphs.iter().enumerate() {
            let offset = egui::vec2(40.0 * i as f32, 0.0);
            painter.extend(glyph.iter().cloned().map(|mut s| {
                s.translate(offset);
                s
            }));
        }
        for (i, overlay) in overlays.iter().enumerate() {
            let offset = egui::vec2(40.0 * i as f32, 60.0);
            painter.extend(overlay.iter().cloned().map(|mut s| {
                s.translate(offset);
                s
            }));
        }
        for (i, label) in labels.iter().enumerate() {
            let pos = Pos2::new(20.0, 140.0 + 40.0 * i as f32);
            painter.text(pos, Align2::LEFT_TOP, label, FontId::monospace(12.0), color);
        }
    });
    harness.run();
}

/// Opens two_storey_shell, written to an IFCX file under `name`, and
/// chooses the wall tool.
fn wall_tool_on_two_storey_shell(name: &str) -> Harness<'static, archicraft_app::ArchiCraftApp> {
    let project = samples::two_storey_shell();
    let geometry = ModelGeometry::evaluate(&project);
    let options = WriteOptions {
        author: "glyph test".into(),
        timestamp: "2026-10-22T00:00:00Z".into(),
        data_version: 1,
    };
    let (bytes, _) = archicraft_ifcx::save(&project, &geometry, &options, false);
    let dir = data_dir(name);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("two-storey-shell.ifcx");
    std::fs::write(&path, bytes).unwrap();
    let mut harness = harness(&format!("{name}-app"));
    harness.state_mut().open_path(&path);
    harness.run_steps(3);
    harness.get_by_label("Wall").click();
    harness.run_steps(2);
    harness
}

/// INP-024, INP-008: in the plan view of two_storey_shell the wall tool's
/// cursor snaps to the centroid of the ground slab, draws the centroid
/// glyph without a command state and labels it 'centroid'.
#[test]
fn the_plan_view_labels_a_centroid_snap() {
    let mut harness = wall_tool_on_two_storey_shell("centroid-glyph");
    let (storey, pos) = {
        let ws = harness.state_mut().workspace();
        let storey = ws.active_storey;
        let state = &ws.plan_views[&storey];
        (storey, state.camera.to_screen(state.last_rect, DVec2::new(6.0, 4.5)))
    };
    harness.hover_at(pos);
    harness.run_steps(2);
    let ws = harness.state_mut().workspace();
    let state = &ws.plan_views[&storey];
    assert_eq!(state.glyph, Some((SnapKind::Centroid, CursorState::Normal)));
    let label = state.label.clone().unwrap_or_default();
    assert!(label.ends_with("\ncentroid"), "{label}");
}

/// INP-010, INP-008: with four equal parts chosen in the preferences, the
/// plan view of two_storey_shell hovers the south wall, draws ticks at its
/// three division points and snaps to the one near the cursor with the
/// division glyph, labelled 'division point'.
#[test]
fn the_plan_view_draws_division_ticks_and_snaps_to_them() {
    let mut harness = wall_tool_on_two_storey_shell("division-ticks");
    harness.state_mut().workspace().prefs.input.division = DivisionMode::Equal(4);
    let (storey, pos) = {
        let ws = harness.state_mut().workspace();
        let storey = ws.active_storey;
        let state = &ws.plan_views[&storey];
        (storey, state.camera.to_screen(state.last_rect, DVec2::new(3.0, -0.03)))
    };
    harness.hover_at(pos);
    harness.run_steps(2);
    let ws = harness.state_mut().workspace();
    let state = &ws.plan_views[&storey];
    let expected = [3.0, 6.0, 9.0].map(|x| DVec2::new(x, 0.0));
    assert_eq!(state.ticks.len(), expected.len(), "{:?}", state.ticks);
    for (tick, x) in state.ticks.iter().zip(expected) {
        assert!(tick.distance(x) < 1e-9, "{:?} is not {expected:?}", state.ticks);
    }
    assert_eq!(state.glyph, Some((SnapKind::Division, CursorState::Normal)));
    let label = state.label.clone().unwrap_or_default();
    assert!(label.ends_with("\ndivision point"), "{label}");
}
