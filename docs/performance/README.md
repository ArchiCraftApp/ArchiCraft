# Performance benches

Performance targets are measured, not assumed (SRS NFR-PERF-001). The benches here measure the
headless quantities behind NFR-PERF-002 (open and save) and NFR-PERF-006 (update after an edit)
with [criterion](https://crates.io/crates/criterion). They run on every bench model. CI runs them
on every pull request and fails on a regression of more than 20 %
([CI regression gate](#ci-regression-gate)); measurements against the targets follow the
[measurement protocol](#measurement-protocol) and are kept as [results files](#results).

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
CI, and the bench job runs them (next section).

## CI regression gate

The job "Bench regression gate" (`.github/workflows/ci.yml`) runs on every pull request and every
push to `main`. It runs `scripts/bench-ci.sh <merge base>`, which

1. checks out the merge base in a temporary git worktree and builds the benches of both
   commits into one target directory, copying the merge base's bench executables aside before
   the pull request's build replaces them (cargo names them the same in both checkouts);
2. runs every bench of the pull request in smoke mode, interleaved bench by bench: first the
   merge base's build of that bench (saved as criterion baseline `base`), then the pull
   request's build with `--baseline-lenient base`, so criterion computes the bench's change
   against the merge base;
3. runs `scripts/bench-gate.py`, which reads `target/criterion/**/change/estimates.json` and
   flags a bench when the lower bound of its mean-change 95 % confidence interval is above
   +20 %;
4. measures each flagged bench once more, base then head, and fails the job only on the benches
   flagged again.

Smoke mode is `--warm-up-time 1 --measurement-time 3 --sample-size 20` with
`ARCHICRAFT_BENCH_CI=1`, which runs only the bench models flagged `ci`
(`samples::bench_models_to_run`). Base and head run in one job on one machine because shared
runners differ by more than 20 % from run to run: the gate never compares with numbers stored
from other runs. Interleaving keeps the two measurements of a bench seconds apart, so a change
in the machine's load during the job shifts both alike. Gating on the interval's lower bound
means a bench fails only when it is more than 20 % slower even at the favourable end of the
interval, and the second measurement keeps a single disturbed pair from failing the job.

Published per run: the job summary has a table of every bench (base, head, change with its
interval), and the artefact `criterion-reports` holds `target/criterion` with the raw estimates
and samples. A bench new on the branch has no baseline; the table lists it as new and the gate
does not fail on it.

A justified regression (a slower but correct algorithm, a bench that now does more work) is
accepted by a reviewer: add the label `perf-regression-accepted` to the pull request and re-run
the job. It then passes and names the accepted benches in its log and summary, which is the
record of the decision. The job reads the label live, because a re-run keeps the event of the
original push.

The same gate runs locally against any revision, uncommitted changes included. It empties
`target/criterion` first:

```bash
scripts/bench-ci.sh origin/main
```

`scripts/tests/test_bench_gate.py` tests the gate on canned criterion results; the bench job
runs it before the benches.

## Measurement protocol

Measurements against the targets of SRS NFR-PERF-002 to NFR-PERF-009 follow this protocol, so
results from different machines and releases can be compared (the acceptance run is SRS AS-07).

- **Hardware.** Every target is measured on the reference class of NFR-PERF-001 unless the
  target names the minimum class (NFR-PERF-003's RM-S on minimum hardware). Linux is measured on
  each class's Windows laptop with the current Ubuntu LTS (26.04). The classes and the machines
  are in [technology-choices](../technology-choices.md#hardware-classes).
- **Machine state.** Mains power, the default power plan, no other application running, the
  display at its native resolution and default scaling. Note anything that differs in the
  results file.
- **Build.** A release build of the commit under test with the pinned toolchain
  (`rust-toolchain.toml`) and `--locked`.
- **Benches, full mode.** `cargo bench --locked -p archicraft-geometry -p archicraft-ifcx`
  without options and without `ARCHICRAFT_BENCH_CI`: criterion's defaults (3 s warm-up, 5 s
  measurement, 100 samples) on every bench model, `ci` or not. Record the mean with its 95 %
  interval from each bench's `new/estimates.json` (the console line `time: [low mean high]`).
- **Scripted runs.** A release build with `--features perf`, each script of
  [Scripted runs](#scripted-runs) on each model the target names, with the default 600 recorded
  frames, the window visible and uncovered at its default size. Record `p50`, `p95`, `p99` and
  `max` of each metric from the `perf-<timestamp>.json` it writes.
- **Models.** The bench models of [Bench models](#bench-models), and the SRS 7.3 reference
  models RM-S, RM-M and RM-L once they exist (`perf-tuning-rm-m-l`). A target is met only on the
  model it names.
- **Repetitions.** Three runs of each bench command and scripted run, the machine idle between
  them. Report the median of the three runs per number; a run that differs from the median by
  more than 20 % is noted and repeated.

## Results

Each measurement session is one file `docs/performance/<YYYY-MM-DD>-<machine id>.md`:

| Machine id | Machine |
|---|---|
| `min-mac-m1` | the minimum class's Mac |
| `min-win-i5` | the minimum class's Windows laptop |
| `ref-mac-m2` | the reference class's Mac |
| `ref-win-u7` | the reference class's Windows laptop |
| `<id>-ubuntu` | Ubuntu on that Windows laptop, such as `ref-win-u7-ubuntu` |
| `dev-<name>` | a developer machine outside the classes: its numbers show the shape and the order of magnitude, never whether a target is met |

A results file starts with a header table: class, CPU, GPU, RAM, display, OS, commit, toolchain
and the date. Then a table per kind of measurement: the benches (bench id, mean, 95 % interval)
and the scripted runs (script, model, frames, and p50, p95, p99 and max per metric), each with
the targets it measures. Deviations from the protocol go into a "Notes" section. Results files
are never edited after they are committed; a new measurement is a new file.

| Date | Machine | File |
|---|---|---|
| 2026-10-20 | `dev-mbp-m3max` | [2026-10-20-dev-mbp-m3max.md](2026-10-20-dev-mbp-m3max.md) |

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
off by default, and without it none of this code is compiled. CI builds and tests
`archicraft-app` with the feature on Linux and runs clippy on it (`.github/workflows/ci.yml`),
so the feature-gated code does not rot.

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
noisy), and results from the minimum and reference machines of NFR-PERF-001, which are still to
be bought.
