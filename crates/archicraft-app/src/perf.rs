//! Frame-time and latency instrumentation, compiled only with the `perf`
//! feature (NFR-PERF-001). It measures the running application for the
//! targets of NFR-PERF-003…007 and NFR-PERF-009; docs/performance/README.md
//! documents the metrics, the results file and the scripted runs.
//!
//! Samples go into fixed histograms with logarithmic buckets from 10 µs to
//! 10 s, so recording never allocates and a long session costs no memory.
//!
//! The View menu's Performance entry and the Performance window are literal
//! English, not `tr!` (NFR-L10N-001): the window is a developer diagnostic
//! that exists only in builds with the `perf` feature, which releases do not
//! enable, so no user ever sees it; its metric names match the keys of the
//! results file and docs/performance/README.md, which are English too.

use std::f32::consts::TAU;
use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use egui::{Id, Ui};
use serde::Serialize;

use crate::persistence::utc_timestamp;
use crate::workspace::{Pane, Workspace};

/// What the recorder measures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Metric {
    /// CPU time of one frame: from the start to the end of
    /// `ArchiCraftApp::ui` (NFR-PERF-003, -004, -007).
    FrameTime,
    /// egui's time between the starts of two frames; its inverse is the frame
    /// rate (NFR-PERF-003, -004).
    FrameInterval,
    /// From `Workspace::execute` until the geometry and the plan of the
    /// active view show the new revision at the end of a frame
    /// (NFR-PERF-006).
    EditLatency,
    /// From the start of a frame with a pointer move in a plan view to the
    /// snap result and the preview of the drawing tool (NFR-PERF-005).
    PointerFeedback,
    /// UI-thread time of an autosave: taking the geometry and handing a copy
    /// of the project to the autosave thread (NFR-PERF-009).
    AutosaveBlocking,
}

impl Metric {
    pub const ALL: [Metric; 5] = [
        Metric::FrameTime,
        Metric::FrameInterval,
        Metric::EditLatency,
        Metric::PointerFeedback,
        Metric::AutosaveBlocking,
    ];

    /// The key in the results file.
    pub fn key(self) -> &'static str {
        match self {
            Metric::FrameTime => "frame_time",
            Metric::FrameInterval => "frame_interval",
            Metric::EditLatency => "edit_latency",
            Metric::PointerFeedback => "pointer_feedback",
            Metric::AutosaveBlocking => "autosave_blocking",
        }
    }

    /// The row title in the Performance window.
    pub fn title(self) -> &'static str {
        match self {
            Metric::FrameTime => "Frame time",
            Metric::FrameInterval => "Frame interval",
            Metric::EditLatency => "Edit latency",
            Metric::PointerFeedback => "Pointer feedback",
            Metric::AutosaveBlocking => "Autosave blocking",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// Buckets per factor of ten: neighbouring bucket bounds differ by 6 %,
/// the precision of the percentiles.
const PER_DECADE: usize = 40;
/// Decades from 10 µs to 10 s.
const DECADES: usize = 6;
/// The lowest bucket bound in microseconds.
const LOWEST_US: f64 = 10.0;
/// One bucket below 10 µs, the logarithmic ones, one from 10 s up.
const BUCKETS: usize = PER_DECADE * DECADES + 2;

/// A histogram of durations with logarithmic buckets from 10 µs to 10 s.
#[derive(Clone, Debug)]
pub struct Histogram {
    buckets: [u64; BUCKETS],
    count: u64,
    max_us: u64,
}

impl Default for Histogram {
    fn default() -> Self {
        Self {
            buckets: [0; BUCKETS],
            count: 0,
            max_us: 0,
        }
    }
}

impl Histogram {
    fn bucket(us: u64) -> usize {
        if (us as f64) < LOWEST_US {
            return 0;
        }
        let step = ((us as f64 / LOWEST_US).log10() * PER_DECADE as f64).floor() as usize;
        (step + 1).min(BUCKETS - 1)
    }

    /// The upper bound of bucket `index` in microseconds; the last bucket has
    /// none.
    fn upper_us(index: usize) -> Option<f64> {
        (index < BUCKETS - 1).then(|| LOWEST_US * 10f64.powf(index as f64 / PER_DECADE as f64))
    }

    pub fn record(&mut self, duration: Duration) {
        let us = u64::try_from(duration.as_micros()).unwrap_or(u64::MAX);
        self.buckets[Self::bucket(us)] += 1;
        self.count += 1;
        self.max_us = self.max_us.max(us);
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    /// The longest sample.
    pub fn max(&self) -> Duration {
        Duration::from_micros(self.max_us)
    }

    /// The `q` quantile (0 < q ≤ 1): the upper bound of the bucket that holds
    /// it, at most the longest sample. None without samples.
    pub fn percentile(&self, q: f64) -> Option<Duration> {
        if self.count == 0 {
            return None;
        }
        let rank = ((q * self.count as f64).ceil() as u64).clamp(1, self.count);
        let mut seen = 0;
        for (index, n) in self.buckets.iter().enumerate() {
            seen += n;
            if seen >= rank {
                let us = Self::upper_us(index).map_or(self.max_us, |u| (u.round() as u64).min(self.max_us));
                return Some(Duration::from_micros(us));
            }
        }
        Some(self.max())
    }
}

/// A scripted measurement (`ARCHICRAFT_PERF_SCRIPT`): it drives the camera
/// of one view for a fixed number of frames and then quits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Script {
    /// Orbits the first 3D view (NFR-PERF-003).
    Orbit,
    /// Pans the first 3D view in a circle (NFR-PERF-003).
    Pan,
    /// Zooms the first 3D view in and out (NFR-PERF-003).
    Zoom,
    /// Pans the plan of the active storey in a circle (NFR-PERF-004).
    PlanPan,
}

/// Frames per cycle of the pan and zoom motions.
const CYCLE: f32 = 120.0;

impl Script {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "orbit" => Some(Script::Orbit),
            "pan" => Some(Script::Pan),
            "zoom" => Some(Script::Zoom),
            "plan-pan" => Some(Script::PlanPan),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Script::Orbit => "orbit",
            Script::Pan => "pan",
            Script::Zoom => "zoom",
            Script::PlanPan => "plan-pan",
        }
    }

    /// The view the script drives.
    pub fn pane(self, ws: &Workspace) -> Option<Pane> {
        match self {
            Script::PlanPan => Some(Pane::Plan(ws.active_storey)),
            Script::Orbit | Script::Pan | Script::Zoom => ws.doc.project.views.first().map(|v| Pane::View3d(v.id)),
        }
    }

    /// Moves the camera of `pane` by the step of `frame`. Pans and zooms
    /// return to the start after every cycle.
    pub fn step(self, ws: &mut Workspace, pane: Pane, frame: u32) {
        let t = frame as f32 * TAU / CYCLE;
        let circle = egui::vec2(t.cos(), t.sin()) * 8.0;
        match (self, pane) {
            (Script::PlanPan, Pane::Plan(storey)) => {
                if let Some(state) = ws.plan_views.get_mut(&storey) {
                    state.camera.pan(circle);
                }
            }
            (_, Pane::View3d(view)) => {
                if let Some(state) = ws.views_3d.get_mut(&view) {
                    let rect = state.last_rect();
                    match self {
                        Script::Orbit => state.camera.orbit(egui::vec2(4.0, 0.0)),
                        Script::Pan => state.camera.pan(circle, rect.height()),
                        Script::Zoom => state
                            .camera
                            .zoom_at(rect, rect.center(), f64::from(0.02 * t.cos()).exp()),
                        Script::PlanPan => {}
                    }
                }
            }
            (_, Pane::Plan(_)) => {}
        }
    }
}

/// A running scripted measurement.
#[derive(Clone, Debug)]
pub struct ScriptRun {
    pub script: Script,
    /// Frames to record after the warm-up.
    pub frames: u32,
    frame: u32,
}

/// Frames at the start of a scripted run that are not recorded: opening the
/// view, the first layout and the font atlas are not navigation.
pub const WARM_UP_FRAMES: u32 = 30;
/// Recorded frames of a scripted run unless `ARCHICRAFT_PERF_FRAMES` says
/// otherwise.
pub const DEFAULT_FRAMES: u32 = 600;

impl ScriptRun {
    pub fn new(script: Script, frames: u32) -> Self {
        Self {
            script,
            frames,
            frame: 0,
        }
    }

    /// The run asked for by `ARCHICRAFT_PERF_SCRIPT` and
    /// `ARCHICRAFT_PERF_FRAMES`.
    pub fn from_env() -> Option<Self> {
        let name = std::env::var("ARCHICRAFT_PERF_SCRIPT").ok()?;
        let Some(script) = Script::parse(&name) else {
            tracing::warn!("unknown ARCHICRAFT_PERF_SCRIPT “{name}”: use orbit, pan, zoom or plan-pan");
            return None;
        };
        let frames = std::env::var("ARCHICRAFT_PERF_FRAMES")
            .ok()
            .and_then(|f| f.parse().ok())
            .unwrap_or(DEFAULT_FRAMES);
        Some(Self::new(script, frames))
    }

    /// The index of the next frame, counting the warm-up; None once the run
    /// is over.
    pub fn advance(&mut self) -> Option<u32> {
        let frame = self.frame;
        if frame >= WARM_UP_FRAMES + self.frames {
            return None;
        }
        self.frame += 1;
        Some(frame)
    }
}

/// Collects the samples of one session.
#[derive(Debug)]
pub struct PerfRecorder {
    started: SystemTime,
    since: Instant,
    histograms: [Histogram; Metric::ALL.len()],
    frame_start: Option<Instant>,
    /// The revision of an edit whose views are not updated yet, and when it
    /// was executed.
    pending_edit: Option<(u64, Instant)>,
    frames_over_1s: u64,
    script: Option<ScriptRun>,
}

impl Default for PerfRecorder {
    fn default() -> Self {
        Self {
            started: SystemTime::now(),
            since: Instant::now(),
            histograms: Default::default(),
            frame_start: None,
            pending_edit: None,
            frames_over_1s: 0,
            script: None,
        }
    }
}

/// The results of one metric in the results file, in microseconds.
#[derive(Debug, Serialize)]
pub struct Summary {
    pub count: u64,
    pub p50_us: Option<u64>,
    pub p95_us: Option<u64>,
    pub p99_us: Option<u64>,
    pub max_us: Option<u64>,
}

/// The results file `perf-<timestamp>.json` (docs/performance/README.md).
#[derive(Debug, Serialize)]
pub struct Results {
    /// The version of this file's layout.
    pub format: u32,
    /// The ArchiCraft version.
    pub version: &'static str,
    pub started: String,
    pub duration_s: f64,
    pub script: Option<&'static str>,
    pub frames_over_1s: u64,
    pub longest_frame_us: u64,
    /// One [`Summary`] per [`Metric::key`], in the order of [`Metric::ALL`].
    pub metrics: serde_json::Map<String, serde_json::Value>,
}

fn micros(d: Duration) -> u64 {
    u64::try_from(d.as_micros()).unwrap_or(u64::MAX)
}

fn millis(d: Option<Duration>) -> String {
    d.map_or_else(|| "–".to_owned(), |d| format!("{:.1}", d.as_secs_f64() * 1000.0))
}

impl PerfRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// A recorder for a scripted measurement.
    pub fn with_script(run: ScriptRun) -> Self {
        Self {
            script: Some(run),
            ..Self::default()
        }
    }

    /// A recorder with the scripted run the environment asks for, if any.
    pub fn from_env() -> Self {
        ScriptRun::from_env().map_or_else(Self::new, Self::with_script)
    }

    pub fn script_mut(&mut self) -> Option<&mut ScriptRun> {
        self.script.as_mut()
    }

    pub fn histogram(&self, metric: Metric) -> &Histogram {
        &self.histograms[metric.index()]
    }

    /// The longest frame time; frames over 1 s are logged as warnings
    /// (NFR-PERF-007).
    pub fn longest_frame(&self) -> Duration {
        self.histogram(Metric::FrameTime).max()
    }

    pub fn record(&mut self, metric: Metric, duration: Duration) {
        if metric == Metric::FrameTime && duration > Duration::from_secs(1) {
            self.frames_over_1s += 1;
            tracing::warn!(ms = duration.as_millis(), "a frame took longer than 1 s (NFR-PERF-007)");
        }
        self.histograms[metric.index()].record(duration);
    }

    /// Drops the samples so far, for example after the warm-up of a scripted
    /// run.
    pub fn reset_samples(&mut self) {
        self.histograms = Default::default();
        self.frames_over_1s = 0;
        self.started = SystemTime::now();
        self.since = Instant::now();
    }

    pub fn begin_frame(&mut self) {
        self.frame_start = Some(Instant::now());
    }

    /// Ends the frame begun last; `interval` is egui's time since the
    /// previous frame.
    pub fn end_frame(&mut self, interval: Duration) {
        if let Some(start) = self.frame_start.take() {
            self.record(Metric::FrameTime, start.elapsed());
        }
        if interval > Duration::ZERO {
            self.record(Metric::FrameInterval, interval);
        }
    }

    /// A pointer move in a plan view has its snap result and preview.
    pub fn pointer_feedback(&mut self) {
        if let Some(start) = self.frame_start {
            self.record(Metric::PointerFeedback, start.elapsed());
        }
    }

    /// An edit that started at `started` produced `revision`. A second edit
    /// before the views caught up keeps the first start.
    pub fn edit_executed(&mut self, revision: u64, started: Instant) {
        let started = self.pending_edit.map_or(started, |(_, s)| s);
        self.pending_edit = Some((revision, started));
    }

    /// Closes the pending edit once the views show `revision` (`current`).
    pub fn edit_settled(&mut self, revision: u64, current: bool) {
        if let Some((pending, started)) = self.pending_edit
            && current
            && revision >= pending
        {
            self.pending_edit = None;
            self.record(Metric::EditLatency, started.elapsed());
        }
    }

    pub fn results(&self) -> Results {
        let metrics = Metric::ALL
            .iter()
            .map(|m| {
                let h = self.histogram(*m);
                let p = |q| h.percentile(q).map(micros);
                let summary = Summary {
                    count: h.count(),
                    p50_us: p(0.50),
                    p95_us: p(0.95),
                    p99_us: p(0.99),
                    max_us: (h.count() > 0).then(|| micros(h.max())),
                };
                let value = serde_json::to_value(summary).unwrap_or_default();
                (m.key().to_owned(), value)
            })
            .collect();
        Results {
            format: 1,
            version: env!("CARGO_PKG_VERSION"),
            started: utc_timestamp(self.started),
            duration_s: self.since.elapsed().as_secs_f64(),
            script: self.script.as_ref().map(|r| r.script.name()),
            frames_over_1s: self.frames_over_1s,
            longest_frame_us: micros(self.longest_frame()),
            metrics,
        }
    }

    /// Writes the results to `perf-<timestamp>.json` in `dir` and returns its
    /// path. The timestamp has no colons, which Windows forbids in names.
    pub fn write_results(&self, dir: &Path) -> io::Result<PathBuf> {
        std::fs::create_dir_all(dir)?;
        let stamp = utc_timestamp(SystemTime::now()).replace(':', "");
        let mut path = dir.join(format!("perf-{stamp}.json"));
        let mut n = 2;
        while path.exists() {
            path = dir.join(format!("perf-{stamp}-{n}.json"));
            n += 1;
        }
        let json = serde_json::to_vec_pretty(&self.results()).map_err(io::Error::other)?;
        std::fs::write(&path, json)?;
        Ok(path)
    }

    /// One line with p50, p95, p99 and max per metric in milliseconds.
    pub fn summary(&self) -> String {
        let mut line = String::from("performance (ms, p50/p95/p99/max):");
        for m in Metric::ALL {
            let h = self.histogram(m);
            let _ = write!(
                line,
                " {} {}/{}/{}/{} (n={});",
                m.key(),
                millis(h.percentile(0.50)),
                millis(h.percentile(0.95)),
                millis(h.percentile(0.99)),
                millis((h.count() > 0).then(|| h.max())),
                h.count()
            );
        }
        line
    }
}

fn window_id() -> Id {
    Id::new("perf-window")
}

/// The View menu's entry that shows the Performance window.
pub fn menu_item(ui: &mut Ui) {
    ui.separator();
    let mut open = ui.ctx().data(|d| d.get_temp::<bool>(window_id()).unwrap_or(false));
    if ui.checkbox(&mut open, "Performance").clicked() {
        ui.ctx().data_mut(|d| d.insert_temp(window_id(), open));
        ui.close();
    }
}

/// The Performance window: the percentiles of every metric so far.
pub fn window(ctx: &egui::Context, recorder: &PerfRecorder) {
    let mut open = ctx.data(|d| d.get_temp::<bool>(window_id()).unwrap_or(false));
    if !open {
        return;
    }
    egui::Window::new("Performance").open(&mut open).show(ctx, |ui| {
        egui::Grid::new("perf-grid").striped(true).show(ui, |ui| {
            for title in ["", "Samples", "p50 ms", "p95 ms", "p99 ms", "Max ms"] {
                ui.strong(title);
            }
            ui.end_row();
            for m in Metric::ALL {
                let h = recorder.histogram(m);
                ui.label(m.title());
                ui.label(h.count().to_string());
                for q in [0.50, 0.95, 0.99] {
                    ui.label(millis(h.percentile(q)));
                }
                ui.label(millis((h.count() > 0).then(|| h.max())));
                ui.end_row();
            }
        });
        ui.label(format!("Frames over 1 s: {}", recorder.frames_over_1s));
    });
    ctx.data_mut(|d| d.insert_temp(window_id(), open));
    ctx.request_repaint_after(Duration::from_secs(1));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// Within the 6 % precision of a bucket.
    fn close(actual: Duration, expected: Duration) -> bool {
        let (a, e) = (actual.as_secs_f64(), expected.as_secs_f64());
        (a - e).abs() <= e * 0.06
    }

    #[test]
    fn percentiles_of_a_uniform_distribution() {
        let mut h = Histogram::default();
        for n in 1..=1000 {
            h.record(ms(n));
        }
        assert_eq!(h.count(), 1000);
        assert_eq!(h.max(), ms(1000));
        for (q, expected) in [(0.50, 500), (0.95, 950), (0.99, 990)] {
            let p = h.percentile(q).unwrap();
            assert!(close(p, ms(expected)), "p{q}: {p:?}");
            assert!(p >= ms(expected), "the bucket's upper bound: {p:?}");
        }
        assert_eq!(h.percentile(1.0), Some(ms(1000)));
    }

    #[test]
    fn percentiles_of_equal_samples_are_the_sample() {
        let mut h = Histogram::default();
        for _ in 0..50 {
            h.record(Duration::from_micros(16_667));
        }
        for q in [0.5, 0.95, 0.99] {
            assert_eq!(h.percentile(q), Some(Duration::from_micros(16_667)));
        }
    }

    #[test]
    fn percentiles_of_a_skewed_distribution() {
        // 98 fast frames and two slow ones: p95 is fast, p99 slow.
        let mut h = Histogram::default();
        for _ in 0..98 {
            h.record(ms(5));
        }
        h.record(ms(400));
        h.record(ms(2000));
        assert!(close(h.percentile(0.95).unwrap(), ms(5)));
        assert!(close(h.percentile(0.99).unwrap(), ms(400)));
        assert_eq!(h.max(), ms(2000));
    }

    #[test]
    fn samples_outside_the_buckets() {
        let mut h = Histogram::default();
        h.record(Duration::from_micros(3));
        assert_eq!(h.percentile(0.5), Some(Duration::from_micros(3)));
        h.record(Duration::from_secs(30));
        assert_eq!(h.percentile(1.0), Some(Duration::from_secs(30)));
        assert_eq!(h.percentile(0.5), Some(Duration::from_micros(10)));
        assert_eq!(Histogram::default().percentile(0.5), None);
    }

    #[test]
    fn buckets_cover_10_us_to_10_s() {
        assert_eq!(Histogram::bucket(9), 0);
        assert_eq!(Histogram::bucket(10), 1);
        assert_eq!(Histogram::bucket(9_999_999), BUCKETS - 2);
        assert_eq!(Histogram::bucket(10_000_000), BUCKETS - 1);
        assert_eq!(Histogram::bucket(u64::MAX), BUCKETS - 1);
    }

    #[test]
    fn an_edit_is_sampled_once_the_views_are_current() {
        let mut r = PerfRecorder::new();
        r.edit_settled(1, true);
        assert_eq!(r.histogram(Metric::EditLatency).count(), 0, "nothing pending");
        r.edit_executed(5, Instant::now());
        r.edit_settled(5, false);
        assert_eq!(r.histogram(Metric::EditLatency).count(), 0, "plan not updated yet");
        r.edit_executed(6, Instant::now());
        r.edit_settled(6, true);
        r.edit_settled(6, true);
        assert_eq!(r.histogram(Metric::EditLatency).count(), 1);
    }

    #[test]
    fn frames_over_one_second_are_counted() {
        let mut r = PerfRecorder::new();
        r.record(Metric::FrameTime, ms(16));
        r.record(Metric::FrameTime, ms(1500));
        assert_eq!(r.frames_over_1s, 1);
        assert_eq!(r.longest_frame(), ms(1500));
        r.reset_samples();
        assert_eq!(r.frames_over_1s, 0);
        assert_eq!(r.histogram(Metric::FrameTime).count(), 0);
    }

    #[test]
    fn frames_are_timed_from_begin_to_end() {
        let mut r = PerfRecorder::new();
        r.begin_frame();
        r.pointer_feedback();
        r.end_frame(ms(16));
        r.end_frame(Duration::ZERO);
        assert_eq!(r.histogram(Metric::FrameTime).count(), 1, "one begun frame");
        assert_eq!(r.histogram(Metric::FrameInterval).count(), 1, "no zero intervals");
        assert_eq!(r.histogram(Metric::PointerFeedback).count(), 1);
    }

    #[test]
    fn scripts_by_name_and_length() {
        for s in [Script::Orbit, Script::Pan, Script::Zoom, Script::PlanPan] {
            assert_eq!(Script::parse(s.name()), Some(s));
        }
        assert_eq!(Script::parse("spin"), None);
        let mut run = ScriptRun::new(Script::PlanPan, 3);
        let frames: Vec<u32> = std::iter::from_fn(|| run.advance()).collect();
        assert_eq!(frames.len() as u32, WARM_UP_FRAMES + 3);
        assert_eq!(run.advance(), None);
    }

    #[test]
    fn summary_line_names_every_metric() {
        let mut r = PerfRecorder::new();
        r.record(Metric::EditLatency, ms(12));
        let line = r.summary();
        for m in Metric::ALL {
            assert!(line.contains(m.key()), "{line}");
        }
        assert!(line.contains("edit_latency 12.0/12.0/12.0/12.0 (n=1)"), "{line}");
    }
}
