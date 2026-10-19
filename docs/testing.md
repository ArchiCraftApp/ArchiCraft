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

_To be completed: the table-driven test of the SRS Appendix D defaults._

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

_To be completed: the security review._

## Performance

_To be completed: the performance harness and its budgets._
