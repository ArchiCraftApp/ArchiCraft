//! The performance instrumentation of the `perf` feature (NFR-PERF-001): the
//! recorder samples frames and edit latency in the running application, the
//! View menu opens its window, a scripted run drives the camera and quits, and
//! the results file has the shape documented in docs/performance/README.md.
//!
//! Run with `cargo test -p archicraft-app --features perf`; without the
//! feature this file is empty.

#![cfg(feature = "perf")]

mod common;

use std::path::PathBuf;

use archicraft_app::ArchiCraftApp;
use archicraft_app::perf::{Metric, PerfRecorder, Script, ScriptRun, WARM_UP_FRAMES};
use archicraft_app::tools::ToolKind;
use archicraft_app::workspace::Pane;
use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::WriteOptions;
use archicraft_model::glam::DVec2;
use archicraft_model::{Command, KindTag, samples};
use common::{assert_present, harness};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

type App = Harness<'static, ArchiCraftApp>;

/// A fresh scratch folder for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("archicraft-perf-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the scratch folder can be created");
    dir
}

/// The application with `two_storey_shell` opened from an IFCX file, as a
/// scripted measurement opens it.
fn two_storey_shell(name: &str) -> App {
    let project = samples::two_storey_shell();
    let geometry = ModelGeometry::evaluate(&project);
    let options = WriteOptions {
        author: "perf test".into(),
        timestamp: "2026-10-20T00:00:00Z".into(),
        data_version: 1,
    };
    let (bytes, _) = archicraft_ifcx::save(&project, &geometry, &options, false);
    let path = scratch(name).join("two-storey-shell.ifcx");
    std::fs::write(&path, bytes).expect("the project file can be written");
    let mut h = harness(name);
    h.state_mut().open_path(&path);
    h.run_steps(3);
    assert_eq!(h.state_mut().workspace().doc.project.elements.len(), 12);
    h
}

/// NFR-PERF-001 (measuring NFR-PERF-006): moving a wall of two_storey_shell
/// through `Workspace::execute` is one edit-latency sample once the geometry
/// and the active plan show the new revision, and every frame is a
/// frame-time sample.
#[test]
fn nfr_perf_001_records_edit_latency_and_frames_while_editing() {
    let mut h = two_storey_shell("edit");
    let frames_before = h.state_mut().perf().histogram(Metric::FrameTime).count();
    assert!(frames_before >= 3, "every frame is sampled, got {frames_before}");
    assert_eq!(h.state_mut().perf().histogram(Metric::EditLatency).count(), 0);

    let ws = h.state_mut().workspace();
    let storey = ws.active_storey;
    let wall = ws
        .doc
        .project
        .elements
        .values()
        .find(|e| e.tag() == KindTag::Wall && e.storey == storey)
        .map(|e| e.id)
        .unwrap();
    assert!(ws.execute(Command::MoveElements {
        ids: vec![wall],
        delta: DVec2::new(0.5, 0.0),
    }));
    // The edit is pending until the plan view has drawn the new revision.
    assert_eq!(h.state_mut().perf().histogram(Metric::EditLatency).count(), 0);
    h.run_steps(2);

    let perf = h.state_mut().perf();
    let edit = perf.histogram(Metric::EditLatency);
    assert_eq!(edit.count(), 1, "one edit, one sample");
    assert!(edit.max() > std::time::Duration::ZERO);
    assert!(perf.histogram(Metric::FrameTime).count() >= frames_before + 2);
    assert!(perf.longest_frame() >= perf.histogram(Metric::FrameTime).max());
}

/// NFR-PERF-001: the View menu opens the Performance window with a row per
/// metric.
#[test]
fn nfr_perf_001_view_menu_opens_the_performance_window() {
    let mut h = two_storey_shell("window");
    // The menu bar is above the browser, which has a "View" header too.
    h.query_all_by_label("View")
        .min_by(|a, b| a.rect().min.y.total_cmp(&b.rect().min.y))
        .unwrap()
        .click();
    h.run_steps(1);
    h.get_by_label("Performance").click();
    h.run_steps(2);
    for metric in Metric::ALL {
        assert_present(&h, metric.title());
    }
}

/// NFR-PERF-001: the results file parses and has the documented shape: the
/// format version, the run, and p50, p95, p99 and max in microseconds for
/// every metric.
#[test]
fn nfr_perf_001_results_file_has_the_documented_shape() {
    let mut recorder = PerfRecorder::with_script(ScriptRun::new(Script::PlanPan, 600));
    for ms in 1..=100 {
        recorder.record(Metric::FrameTime, std::time::Duration::from_millis(ms));
    }
    let dir = scratch("json");
    let path = recorder.write_results(&dir).unwrap();
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        name.starts_with("perf-") && name.ends_with(".json") && !name.contains(':'),
        "perf-<timestamp>.json without colons, got {name}"
    );

    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(json["format"], 1);
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
    assert!(json["started"].as_str().unwrap().ends_with('Z'));
    assert!(json["duration_s"].as_f64().unwrap() >= 0.0);
    assert_eq!(json["script"], "plan-pan");
    assert_eq!(json["frames_over_1s"], 0);
    assert_eq!(json["longest_frame_us"], 100_000);
    let metrics = json["metrics"].as_object().unwrap();
    let names: Vec<&str> = metrics.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        [
            "frame_time",
            "frame_interval",
            "edit_latency",
            "pointer_feedback",
            "autosave_blocking"
        ]
    );
    for (name, metric) in metrics {
        let keys: Vec<&str> = metric.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["count", "p50_us", "p95_us", "p99_us", "max_us"], "{name}");
    }
    let frame = &metrics["frame_time"];
    assert_eq!(frame["count"], 100);
    assert_eq!(frame["max_us"], 100_000);
    let p50 = frame["p50_us"].as_u64().unwrap();
    assert!((47_000..=54_000).contains(&p50), "p50 of 1..=100 ms, got {p50} us");
    // An empty metric reports zero samples and no percentiles.
    assert_eq!(metrics["edit_latency"]["count"], 0);
    assert!(metrics["edit_latency"]["p50_us"].is_null());
}

/// NFR-PERF-001 (measuring NFR-PERF-005): moving the pointer over the plan
/// with the wall tool is a pointer-feedback sample per frame with a snap
/// result and preview.
#[test]
fn nfr_perf_001_records_pointer_feedback_in_the_plan() {
    let mut h = two_storey_shell("pointer");
    let ws = h.state_mut().workspace();
    ws.tool.select(ToolKind::Wall);
    let rect = ws.plan_views[&ws.active_storey].last_rect;
    assert_eq!(h.state_mut().perf().histogram(Metric::PointerFeedback).count(), 0);
    for step in 0..3 {
        h.event(egui::Event::PointerMoved(
            rect.center() + egui::vec2(10.0 * step as f32, 5.0),
        ));
        h.run_steps(1);
    }
    assert!(h.state_mut().perf().histogram(Metric::PointerFeedback).count() >= 2);
}

/// Whether the last frame asked the root viewport to close.
fn close_requested(h: &App) -> bool {
    h.output()
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .is_some_and(|v| v.commands.contains(&egui::ViewportCommand::Close))
}

/// NFR-PERF-001 (measuring NFR-PERF-004): a scripted plan-pan run opens the
/// plan of the active storey, pans its camera every frame, drops the samples
/// of the warm-up and closes the application after its recorded frames.
#[test]
fn nfr_perf_001_scripted_run_pans_the_plan_and_quits() {
    let mut h = two_storey_shell("script");
    let storey = h.state_mut().workspace().active_storey;
    // Close the plan the file opened, so the run has to open it.
    let tree = h.state_mut().tree_mut();
    let tile = tree.tiles.find_pane(&Pane::Plan(storey)).unwrap();
    tree.remove_recursively(tile);
    h.state_mut().workspace().active_pane = None;
    h.state_mut().workspace().perf = PerfRecorder::with_script(ScriptRun::new(Script::PlanPan, 3));
    // The first frames open and fit the plan; the camera after the warm-up
    // is where the recorded frames start.
    for _ in 0..WARM_UP_FRAMES {
        h.step();
        assert!(!close_requested(&h), "no close during the warm-up");
    }
    let warm = h.state_mut().workspace().plan_views[&storey].camera;
    let warm_frames = h.state_mut().perf().histogram(Metric::FrameTime).count();
    assert_eq!(u64::from(WARM_UP_FRAMES), warm_frames, "every warm-up frame is sampled");

    for _ in 0..3 {
        h.step();
        assert!(!close_requested(&h), "no close during the recorded frames");
    }
    h.step();
    assert!(close_requested(&h), "the run closes the application at its end");

    assert!(
        h.state_mut().tree_mut().tiles.find_pane(&Pane::Plan(storey)).is_some(),
        "the run opened the plan"
    );
    let ws = h.state_mut().workspace();
    assert_eq!(ws.active_pane, Some(Pane::Plan(storey)));
    assert_ne!(
        ws.plan_views[&storey].camera.center, warm.center,
        "the plan camera moved"
    );
    let frames = h.state_mut().perf().histogram(Metric::FrameTime).count();
    assert!(
        (3..=4).contains(&frames),
        "the warm-up samples are dropped, {frames} recorded frames"
    );
}
