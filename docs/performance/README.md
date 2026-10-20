# Performance benches

Performance targets are measured, not assumed (SRS NFR-PERF-001). The benches here measure the
headless quantities behind NFR-PERF-002 (open and save) and NFR-PERF-006 (update after an edit)
with [criterion](https://crates.io/crates/criterion). They run on every bench model.

## Running them locally

```bash
cargo bench -p archicraft-geometry -p archicraft-ifcx
```

A quick run, for example to check that every bench still works:

```bash
cargo bench -p archicraft-geometry -p archicraft-ifcx -- --warm-up-time 1 --measurement-time 2 --sample-size 10
```

A filter after `--` runs a subset, for example `-- ifcx/load` or `-- tiled_shell`. Reports go to
`target/criterion/<group>/<model>/`, where `<group>` is the bench id's first two parts joined by
`_` (`ifcx_save`); `new/estimates.json` holds the latest estimates and `base/` the previous run
that criterion compares against. Plots are off (criterion's `plotters` and `rayon` features are
disabled), so there is no HTML report.

The benches compile in CI (`cargo build --workspace --all-targets`), so a broken bench fails
CI. They are not yet run there; the regression gate and the published results follow in a later
task.

## Benches

Bench ids are stable strings `<crate>/<bench>/<model>`, because the regression gate compares
results by id. Each input (model, geometry, file bytes) is built and checked outside the timed
part: a model with geometry problems, save warnings or validation problems fails the bench
instead of making it look fast.

| Bench id | What is timed | Source |
|---|---|---|
| `geometry/evaluate/<model>` | `ModelGeometry::evaluate`: wall junctions and every element body | `crates/archicraft-geometry/benches/geometry.rs` |
| `geometry/plan/<model>` | `plan_drawing` for every storey: slicing at the cut plane and classification | same |
| `geometry/slice/<model>` | `slice_loops` on every wall mesh at its storey's cut plane | same |
| `geometry/edit_then_evaluate/<model>` | `MoveElements` on the first interior wall (0.5 m along its T junction), then evaluation and the plan of its storey: the headless part of NFR-PERF-006 | same |
| `ifcx/save/<model>` | `archicraft_ifcx::save`: write and compact serialisation | `crates/archicraft-ifcx/benches/ifcx.rs` |
| `ifcx/save_indented/<model>` | the same with indented output | same |
| `ifcx/load/<model>` | `archicraft_ifcx::load`: parse and read the compact file | same |
| `ifcx/validate/<model>` | `validate::validate` on the parsed file | same |

## Bench models

`archicraft_model::samples::bench_models()` is the single registry; every bench iterates it.

| Model id | Elements | CI | Built by |
|---|---|---|---|
| `two_storey_shell` | 12 | yes | `samples::two_storey_shell` |
| `tiled_shell` | 972 | yes | `samples::tiled_shell(9)`: the two-storey shell on a 9 x 9 grid, 20 m apart |

`tiled_shell` is provisional. It stands in for the seeded 1,000, 5,000 and 20,000-element models
of `element-kind-framework` and for RM-M and RM-L (SRS 7.3, `perf-tuning-rm-m-l`), which are
appended to `bench_models()`; models too slow for CI set `ci: false`, and the CI gate runs only
`ci` models. `tiled_shell` is also registered as the extra fixture `tiled-shell`
(`crates/archicraft-model/src/fixtures.rs`), so it passes the generic model, geometry and IFCX
checks of docs/testing.md.

## Measuring the running application

The benches above cannot see frame rates or the latency a user feels. Builds with the
`perf` feature of `archicraft-app` record them in the running application. The feature is
off by default, and without it none of this code is compiled.

```bash
cargo run --release -p archicraft-app --features perf -- <project.ifcx>
```

### Metrics

Samples go into histograms with logarithmic buckets from 10 µs to 10 s, 40 per decade. A
percentile is the upper bound of its bucket, so it is at most 6 % above the true value, and
never above the longest sample.

| Metric | What is timed | Target it measures |
|---|---|---|
| `frame_time` | CPU time of one frame: from the start to the end of `ArchiCraftApp::ui` (layout, plan drawing, scene upload). Tessellation and GPU rendering come after it. | NFR-PERF-003, NFR-PERF-004; its maximum is the longest frame, and frames over 1 s are logged as warnings (NFR-PERF-007) |
| `frame_interval` | egui's time between the starts of two frames. Its inverse is the frame rate. egui repaints only on input, so outside a scripted run it also measures idle time. | NFR-PERF-003 (3D ≥ 30 frames/s, interval ≤ 33.3 ms), NFR-PERF-004 (plan ≥ 30 frames/s) |
| `edit_latency` | From `Workspace::execute` until, at the end of a frame, the geometry and the plan of the active view show the new revision. A second edit before the views catch up keeps the first start. | NFR-PERF-006 (≤ 200 ms) |
| `pointer_feedback` | In a plan view with a drawing tool, from the start of a frame in which the pointer moved to the snap result, the preview and the tracker. The wait for the frame itself is in `frame_interval`. | NFR-PERF-005 (≤ 50 ms) |
| `autosave_blocking` | UI-thread time of an autosave: taking the geometry and handing a copy of the project to the autosave thread. | NFR-PERF-009 (≤ 200 ms) |

**View → Performance** shows the percentiles so far. On exit the application writes
`perf-<timestamp>.json` to its data folder (`ARCHICRAFT_DATA_DIR` if set) and logs a summary
line at info level. The line is visible with `RUST_LOG=archicraft_app=info`.

### Results file

```json
{
  "format": 1,
  "version": "0.1.0",
  "started": "2026-10-20T08:55:02Z",
  "duration_s": 5.05,
  "script": "plan-pan",
  "frames_over_1s": 0,
  "longest_frame_us": 138,
  "metrics": {
    "frame_time": { "count": 602, "p50_us": 94, "p95_us": 119, "p99_us": 133, "max_us": 138 },
    "frame_interval": { "...": "..." },
    "edit_latency": { "count": 0, "p50_us": null, "p95_us": null, "p99_us": null, "max_us": null },
    "pointer_feedback": { "...": "..." },
    "autosave_blocking": { "...": "..." }
  }
}
```

- `format` is the version of this layout and `version` is the ArchiCraft version.
- `started` is the time recording started, in UTC.
- `script` is the scripted run, or `null`.
- `metrics` has one entry per metric, always all five and in this order. Times are whole
  microseconds. A metric without samples has `count` 0 and `null` percentiles.

The file name's timestamp has no colons, because Windows does not allow them in file names.
The test `crates/archicraft-app/tests/perf.rs` pins this shape.

### Scripted runs

`ARCHICRAFT_PERF_SCRIPT` makes the application drive one view's camera for a fixed number
of frames, then quit and write the results. It requests a repaint every frame, so the frame
rate is not limited by input.

| Script | View | Motion | Target |
|---|---|---|---|
| `orbit` | the first 3D view | turns about 1.8° per frame, like 4 points of pointer motion | NFR-PERF-003 |
| `pan` | the first 3D view | pans in a circle of 8 points per frame, one circle per 120 frames | NFR-PERF-003 |
| `zoom` | the first 3D view | zooms in and out around the centre, one cycle per 120 frames | NFR-PERF-003 |
| `plan-pan` | the plan of the active storey | pans like `pan` | NFR-PERF-004 |

The first 30 frames are a warm-up and are not recorded: opening the view and the first
layout are not navigation. Then `ARCHICRAFT_PERF_FRAMES` frames are recorded (600 by
default). A run on `two_storey_shell`:

```bash
cargo run --release -p archicraft-ifcx --example reference_models -- target/reference-models
cargo build --release -p archicraft-app --features perf
ARCHICRAFT_DATA_DIR=target/perf RUST_LOG=archicraft_app=info ARCHICRAFT_PERF_SCRIPT=plan-pan \
  target/release/archicraft target/reference-models/two-storey-shell.ifcx
```

The results land in `target/perf/perf-<timestamp>.json`. Leave the window visible and
uncovered while the run is going: some systems throttle hidden windows. Measure release
builds, because a debug build is several times slower. The example file above comes from
such a run on a developer machine with a 120 Hz display (frame interval 8.4 ms). It shows the
shape of the file and is not a reference measurement.

Not covered yet: GPU timestamp queries, thresholds on frame times in CI (shared runners are too
noisy), and results from the reference hardware of NFR-PERF-001.
