# 2026-10-20 · dev-mbp-m3max

A developer machine outside the hardware classes of NFR-PERF-001
([technology-choices](../technology-choices.md#hardware-classes)): the numbers show the shape of a
results file and the order of magnitude, never whether a target is met. Protocol:
[README](README.md#measurement-protocol).

| | |
|---|---|
| Class | none (developer machine) |
| Machine | Apple MacBook Pro (Mac15,9) |
| CPU | Apple M3 Max, 16 cores (12 performance, 4 efficiency) |
| GPU | Apple M3 Max, 40 cores (Metal) |
| RAM | 128 GB |
| Display | built-in Liquid Retina XDR, 3456 × 2234, 120 Hz |
| OS | macOS 27.0 (26A428) |
| Commit | `34b65fe` with the uncommitted changes of archicraft-0ca2 (benches select their models, no change to the measured code) |
| Toolchain | Rust 1.97.0 (`rust-toolchain.toml`), release build, `--locked` |
| Date | 2026-10-20 |

## Benches (full mode)

`cargo bench --locked -p archicraft-geometry -p archicraft-ifcx`, criterion's defaults, every
bench model. Mean time per iteration with its 95 % confidence interval.

| Bench id | Mean | 95 % interval |
|---|---|---|
| `geometry/edit_then_evaluate/tiled_shell` | 5.34 ms | 5.32 ms – 5.37 ms |
| `geometry/edit_then_evaluate/two_storey_shell` | 38.5 µs | 38.4 µs – 38.7 µs |
| `geometry/evaluate/tiled_shell` | 4.11 ms | 4.10 ms – 4.13 ms |
| `geometry/evaluate/two_storey_shell` | 22.7 µs | 22.6 µs – 22.8 µs |
| `geometry/plan/tiled_shell` | 2.75 ms | 2.74 ms – 2.77 ms |
| `geometry/plan/two_storey_shell` | 34.6 µs | 34.5 µs – 34.8 µs |
| `geometry/slice/tiled_shell` | 940.9 µs | 937.6 µs – 944.3 µs |
| `geometry/slice/two_storey_shell` | 11.7 µs | 11.6 µs – 11.7 µs |
| `ifcx/load/tiled_shell` | 20.0 ms | 19.9 ms – 20.1 ms |
| `ifcx/load/two_storey_shell` | 510.5 µs | 504.8 µs – 516.9 µs |
| `ifcx/save/tiled_shell` | 20.1 ms | 20.0 ms – 20.2 ms |
| `ifcx/save/two_storey_shell` | 630.8 µs | 628.1 µs – 633.5 µs |
| `ifcx/save_indented/tiled_shell` | 20.2 ms | 20.1 ms – 20.3 ms |
| `ifcx/save_indented/two_storey_shell` | 644.9 µs | 641.0 µs – 649.0 µs |
| `ifcx/validate/tiled_shell` | 6.13 ms | 6.10 ms – 6.17 ms |
| `ifcx/validate/two_storey_shell` | 120.0 µs | 119.6 µs – 120.5 µs |

The open and save targets of NFR-PERF-002 (RM-M ≤ 10 s open, ≤ 5 s save) and the edit target
of NFR-PERF-006 (≤ 200 ms) are for RM-M, which does not exist yet; `tiled_shell` has 972
elements, about a fifth of RM-M.

## Scripted run

A release build with `--features perf`, `ARCHICRAFT_PERF_SCRIPT=orbit`, the default 600
recorded frames after 30 warm-up frames, the window at its default size on the built-in display:

```bash
ARCHICRAFT_DATA_DIR=target/perf ARCHICRAFT_PERF_SCRIPT=orbit \
  target/release/archicraft target/reference-models/two-storey-shell.ifcx
```

| Script | Model | Samples | Metric | p50 | p95 | p99 | max | Target |
|---|---|---|---|---|---|---|---|---|
| `orbit` | `two-storey-shell` (12 elements) | 602 | `frame_time` | 0.20 ms | 0.25 ms | 0.28 ms | 0.34 ms | NFR-PERF-003, NFR-PERF-007 |
| | | | `frame_interval` | 8.41 ms | 8.91 ms | 8.91 ms | 24.97 ms | NFR-PERF-003 (≤ 33.3 ms) |

Samples is the `count` the results file reports for the metric. No frame took longer than 1 s. The frame interval is the display's refresh period (120 Hz,
8.3 ms): the frames wait for vertical sync, so the interval says nothing about headroom; the
frame time does. `edit_latency`, `pointer_feedback` and `autosave_blocking` have no samples in
a camera script.

## Notes

- One run of each command instead of three: this file shows the convention, not a
  measurement for acceptance.
- The machine was not idle: other builds ran at the same time (load average 11 to 18 on 16
  cores during the bench run). The benches' intervals are narrow, but the means may be a few
  per cent high.
- On mains power with macOS's default energy settings.
