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
