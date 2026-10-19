# Testing

How ArchiCraft is tested and which check keeps each done requirement done. This file is the
skeleton of the regression net (epic `regression-and-status`, docs/roadmap.md); the tasks of
that epic fill in their sections. The commands to run are in the README and in CLAUDE.md.

## Golden files and ARCHICRAFT_BLESS

The IFCX writer's output is pinned by golden files in `crates/archicraft-ifcx/tests/golden/`,
compared byte for byte by `cargo test -p archicraft-ifcx --test roundtrip`. They change only
for an intended format change: on a bead labelled `format-change`, regenerate them with

```sh
ARCHICRAFT_BLESS=1 cargo test -p archicraft-ifcx --test roundtrip
```

and review the text diff. `.gitattributes` marks `*.ifcx` as `-text`, so no checkout converts
their line endings (NFR-PORT-004); they still diff as text.

_To be completed: the blessing workflow in detail._

## Snapshots

`.gitattributes` keeps `*.snap` files at LF on every checkout, so snapshots compare equal on
Windows.

_To be completed: readable insta snapshots of IFCX output and egui_kittest UI snapshots._

## Conformance

`conformance/run.sh` loads IFCX files with buildingSMART's reference implementation and viewer
composition at the pinned commit (IFX-006); CI runs it on the reference models and the golden
files. Its build output (`.ifc5/`, `node_modules/`, `.build/`, and the Playwright
`test-results/` and `playwright-report/`) is ignored by `conformance/.gitignore`, so a local
run leaves `git status` clean.

_To be completed: the headless Playwright run of the reference viewer._

## Fixture registry

_To be completed: the D.7 fixture registry (`crates/archicraft-model/tests/fixtures.rs`, f1..f9)._

## Appendix D defaults

`crates/archicraft-model/tests/defaults.rs` pins the defaults of SRS Appendix D for a new
project from `default_metric_project()`, so a refactor cannot change one silently. `ROWS` has
one entry per value. Each entry's reference starts with `D.<n> <setting>:`, using the setting
name from the SRS table (`D.2 Wall: 0.25 m thick`), and every failure message starts with that
reference. Each entry is one of three kinds:

- `Checked`: a getter or a constant compared with the Appendix D value. Floats are compared
  with 1e-12; lower bounds such as "at least 100 undo steps" are compared with `>=`.
- `Elsewhere`: the value lives in another crate, and that crate's `tests/defaults.rs` pins it
  with the same reference in its assertion message. The model test checks that the file
  contains the reference. These are the D.1 view range in `crates/archicraft-geometry` and the
  preference defaults (autosave, backups, undo clamp, snap radius, angle increment) in
  `crates/archicraft-app`, which loads a temporary preferences file to run the private clamp.
- `Pending { owner }`: the value does not exist yet. `owner` names the roadmap epic that adds
  it. The test prints the pending list (`cargo test -p archicraft-model --test defaults --
  --nocapture`).

`every_appendix_d_row_is_in_the_table` reads the D.1 to D.3 tables of `docs/SRS.md` and fails
when a row has no entry. `d4_every_template_material_has_the_priority_of_its_group` maps every
template material by name to its D.4 group and checks the group priority. A material without a
group fails, and so does a mapped name that is not in the template. `d4_groups_match_the_srs_table`
keeps the group table equal to the SRS one.

To convert a pending row when the owner epic adds the value: replace
`pending OWNER` with a check closure (`|p| close(p.tool_defaults.column.width, 0.30)`), or with
`elsewhere "<crate>/tests/defaults.rs"` when the value lives outside the model crate. In the
second case, add the assertion there and repeat the reference in its message. A setting with
several values becomes one row per value, each starting with the same `D.<n> <setting>:`
prefix. Changing an Appendix D value is an SRS change: update `docs/SRS.md` and the row
together.

## Done-requirement traceability

Every requirement marked done in docs/implementation-status.md names the check that fails if it
regresses. Repository guards are scripts and policies that CI runs on every push and pull
request; the CI step is the regression check.

| Requirement | What is guarded | Regression check (CI job: step) |
|---|---|---|
| NFR-PORT-004 | Golden files keep their exact bytes on every platform: `.gitattributes` sets `*.ifcx -text` and `*.snap text eol=lf` | Test (windows-latest): "Enable end-of-line conversion (core.autocrlf=true)", then "Test" (`golden_files_are_reproduced`) |
| NFR-LEG-002 | No trademark of the reference product or its maker outside reviewed nominative lines (`scripts/legal-allowlist.txt`); no tracked image, library-part or template file without origin and licence (`scripts/asset-allowlist.txt`) | Repository guards: "No unreviewed trademarks or third-party assets (NFR-LEG-002)" (`scripts/check-legal.sh`) and "Negative self-test of the guards" |
| NFR-SEC-001 | No HTTP client crate in the dependency graph: `deny.toml` [bans] | Licences and sources: `cargo deny check advisories licenses sources bans` |
| NFR-SEC-004 | As NFR-SEC-001, until ifcx-interop adds the consent-gated import fetch (IFX-081) and lifts the ban | Licences and sources: `cargo deny check advisories licenses sources bans` |
| NFR-MNT-002 | The model, geometry and IFCX crates have no UI or windowing dependency (normal and build dependencies) | Repository guards: "Layering of the headless crates (NFR-MNT-002, NFR-MNT-004)" (`scripts/check-layering.sh`) and "Negative self-test of the guards"; also the bead gate |
| NFR-MNT-004 | The model and geometry crates do not depend on the IFCX crate, so a new IFCX version needs no change to them | Repository guards: "Layering of the headless crates (NFR-MNT-002, NFR-MNT-004)" and "Negative self-test of the guards" |
| ELM-004 | Renaming through UpdateElements changes `Element.name`, undo restores it, an empty name is rejected (model half; the Name field is pinned with the app requirements) | Test: `elm_004_renaming_an_element_is_an_undoable_edit` |
| ELM-010 | New walls and slabs take their tool's default layer, an unknown layer is rejected, DeleteLayer reassigns the elements and the tool defaults, so every element stays on exactly one existing layer | Test: `elm_010_every_element_is_on_exactly_one_existing_layer`; also `deleting_resources_requires_a_replacement` (commands.rs) |
| STR-001 | Storeys ordered by signed elevation and numbered -1/0/+1/+2 from the ground, heights positive (zero and negative rejected), links beyond the top storey use its nominal height | Test: `str_001_storeys_are_ordered_numbered_and_the_top_has_a_nominal_height`; also `sorted_and_numbered` and `relative_elevation_extrapolates_beyond_top` (storey.rs) |
| STR-004 | DeleteStorey moves the elements keeping their absolute extent and plan position, or deletes them; `elements_on(storey).count()` is the count the prompt shows (model half; the prompt is pinned with the app requirements) | Test: `str_004_deleting_a_storey_moves_or_deletes_its_elements`; also `deleting_a_storey_can_keep_its_elements` (commands.rs) |
| STR-005 | Each of the storey commands (insert above and below, delete, rename, elevation, height, cut plane) undoes to the exact start state, redoes to the edited state and only grows the undo stack | Test: `str_005_every_storey_command_undoes_exactly`; also `insert_and_remove_are_inverse` (storey.rs) |
| STR-008 | Cut plane 1.10 m by default, a per-storey override wins over the project default, SetStoreyCutPlane undoes and can clear the override | Test: `str_008_cut_plane_defaults_to_1_10_m_and_a_storey_override_wins` |
| STR-013 | One site and one building (single fields), SetSpatialNames renames both, undo restores them, empty names are rejected | Test: `str_013_one_site_and_one_building_with_editable_names` |
| UND-001 | One command per category (element edit, storey, layer, material, tool defaults, settings, project information, location) undoes and redoes exactly; 150 commands then 100 undos succeed with a limit of 100 and with the default; the preference path never yields a limit below 100 | Test: `und_001_every_change_category_undoes_and_redoes_exactly`, `und_001_history_keeps_at_least_100_steps`; `invalid_values_are_clamped` (`crates/archicraft-app/src/persistence/prefs.rs`); also `undo_restores_the_exact_project` and `history_is_bounded_and_replayable` (commands.rs) |
| UND-002 | A scripted session executing every Command variant ends with every command on the undo stack; the variant list has a compile-time exhaustiveness guard (`variant`, a match without a wildcard arm) | Test: `und_002_no_command_clears_the_history` |
| NFR-MNT-001 | Every Command variant round-trips through serde_json, a recorded session replayed with `command::replay` rebuilds the same project, the journal lists Executed, Undone and Redone entries with labels | Test: `nfr_mnt_001_every_command_variant_round_trips_through_serde_json`, `nfr_mnt_001_replaying_a_recorded_session_rebuilds_the_project`, `nfr_mnt_001_the_journal_lists_executed_undone_and_redone_commands` |
| NFR-ACC-002 | `parse_length` resolves 0.1 mm and 1/64 inch within 1e-15 m, and a typed 1.23456789 m is stored bit-identical | Test: `nfr_acc_002_input_resolves_0_1_mm_and_1_64_inch_at_full_precision`; also `display_rounding_does_not_lose_typed_precision` and `parses_imperial_fractions` (units.rs) |
| NFR-ACC-005 | `format_length` and `format_angle` round to the unit settings while the stored values stay bit-identical; switching to mm with 1 decimal changes only the display | Test: `nfr_acc_005_values_are_rounded_only_for_display`; also `formats_lengths` (units.rs) |

Rows with "Test:" name `cargo test` functions, which the "Test" step of the "Test" job runs on
Linux and Windows for every push and pull request, and `.github/workflows/macos.yml` on macOS
for releases. Unless another file is named, they live in
`crates/archicraft-model/tests/done_requirements.rs`, which builds every project from the
default template and changes it only through commands; tests named with a file in
parentheses already existed and are not repeated there.

The negative self-test (`scripts/guards-selftest.sh`) runs each guard on a canned violation
(`cargo tree` output in `scripts/testdata/`, and a scratch repository with a trademark line and
tracked `*.gsm` and `*.png` files) and fails if a guard lets it pass.

_To be completed: rows for the other done requirements._

## Fuzzing

_To be completed: cargo-fuzz targets, seed corpus and the nightly fuzz job. Their output
(`fuzz/target/`, `fuzz/corpus/`, `fuzz/artifacts/`, `fuzz/coverage/`) is git-ignored._

## Security review

`cargo deny check advisories` checks the dependency graph against the RustSec advisory
database in CI; `deny.toml` lists each ignored advisory with its reason.

**Untrusted input (NFR-SEC-003).** Every file ArchiCraft opens is untrusted input. The IFCX
reader enforces the limits of SRS Appendix D.9, each tested at its boundary in
`crates/archicraft-ifcx/tests/limits.rs`:

- file size at most 2 GB, checked against the file's length before reading
  (`archicraft_ifcx::load_path`), and the read stops one byte past the limit, so a file that
  grows while it is read is rejected too;
- JSON nesting fewer than 128 levels in the whole document (the recursion limit of
  `serde_json`);
- `children`/`inherits` chains at most 64 deep, reported with the node path;
- at most 32 imports.

**No execution.** File content only ever becomes data: the reader has no `eval`, loads no
plugins or libraries and spawns no processes, and nothing in a file selects code to run.
`deny.toml` `[bans]` guards this mechanically: it denies script engines and WebAssembly
runtimes (rhai, mlua, rlua, boa_engine, deno_core, v8, wasmtime, wasmer), and allows
`libloading` only under the graphics and windowing crates that load the system's GPU and
display libraries (ash, dlib, khronos-egl, wgpu-hal, x11rb). Adding any of them to an
ArchiCraft crate fails `cargo deny check bans` in the CI job "Licences and sources".

This property is reviewed before every release: check that the reader still holds no code
path from file content to execution, and that the bans and their reasons still apply.

## Performance

_To be completed: the performance harness and its budgets._
