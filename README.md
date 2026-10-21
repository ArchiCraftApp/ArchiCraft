# ArchiCraft

ArchiCraft is a desktop application for drawing buildings as precise 3D
models and saving them as IFCX (IFC5), the open buildingSMART format. It runs
on Windows, macOS and Linux, works offline, and sends no data anywhere.

What it must do is specified in the [SRS](docs/SRS.md); what it is built with
is in the [technology choices](docs/technology-choices.md).

## Status

This is the first version: increment 0 of the SRS, the "walking skeleton"
(SRS Appendix F). It covers:

- **Projects** from a default metric template (storeys, layers, 25 materials,
  tool defaults), with storeys, layers and materials editable in dialogs.
- **Straight walls** (single, chain, rectangle, rotated rectangle) and
  **slabs** (polygon, rectangle, rotated rectangle), with order-independent
  L and T wall junctions.
- **Plan views** generated from the 3D bodies (cut, projected and overhead
  lines, cut fills from materials), with snapping, typed distances and angles
  in any unit, and click or area selection.
- **3D views** rendered with wgpu: perspective and parallel projection,
  presets, orbit, pan, zoom, picking, and four display modes.
- **Properties panel** and settings dialog for tool defaults and selections,
  with mixed values for multiple selections.
- **Undo and redo** for every change, through command objects.
- **IFCX** as the native format: spatial tree, IFC classes, names, meshes and
  materials that any IFCX viewer shows, plus the full parametric model in the
  declared `archicraft::` namespace, so a saved project opens fully editable.
- **Safe saving**: atomic saves with backups, autosave to a separate recovery
  folder with recovery after a crash, and local crash reports.

Doors, windows, roofs, layered walls, columns, beams, stairs and the other
SRS increments come next; the work is tracked with beads (`bd ready`).

## Building and running

You need the Rust toolchain; `rust-toolchain.toml` selects the right version.

```sh
cargo run --release -p archicraft-app            # start ArchiCraft
cargo run --release -p archicraft-app -- my.ifcx  # open a project
```

On Linux, the application needs a Vulkan or OpenGL driver and, for file
dialogs, an XDG desktop portal; standard desktops have both.

On macOS, `scripts/package-macos.sh` builds a release and packs it into
`target/dmg/ArchiCraft-<version>-macos-arm64.dmg`, with `ArchiCraft.app` and a
link to Applications to drag it to. The app is signed ad hoc; set
`CODESIGN_IDENTITY` to sign it with a Developer ID instead. On another Mac, an
ad hoc signed app opens only after the user allows it under System Settings →
Privacy & Security.

`scripts/package-linux.sh` (a tarball with the executable, icon and desktop entry) and
`scripts/package-windows.sh` (a zip with the executable, icon embedded; Git Bash, Windows SDK
and Python with Pillow) do the same on Linux and Windows. All three take the app icon from
`ArchiCraft_Logo.png`. An in-between release for testers, with all three packages of a pushed
commit as a GitHub pre-release, comes from the workflow
[Preview release](.github/workflows/preview-release.yml): `/preview-release` in Claude Code, or
`gh workflow run preview-release.yml --ref main -f notes="What to try"`.

## System requirements

ArchiCraft runs on Windows, macOS and Linux and draws with the GPU: Metal on macOS, DirectX 12
or Vulkan on Windows, Vulkan on Linux. The performance targets refer to two hardware classes
(SRS NFR-PERF-001; [the classes in detail](docs/technology-choices.md#hardware-classes)):

| Class | macOS | Windows | Linux |
|---|---|---|---|
| Minimum | MacBook Air M1, 8 GB RAM | Intel Core i5-1135G7 (Iris Xe graphics), 8 GB RAM, 1920 × 1080 display | Ubuntu 26.04 LTS on the minimum Windows laptop |
| Reference | MacBook Air M2, 16 GB RAM | Intel Core Ultra 7 155H (Arc graphics), 16 GB RAM | Ubuntu 26.04 LTS on the reference Windows laptop |

ArchiCraft works on the minimum class; the performance targets are met on the reference class
unless a target names the minimum class. Measurements are in
[docs/performance](docs/performance/README.md). Every release publishes this section
(SRS NFR-PORT-006, [release checklist](docs/test-plan/release-checklist.md) item 4).

## Testing

```sh
cargo test --workspace                      # unit, integration, golden-file and UI tests
cargo clippy --workspace --all-targets      # lints
conformance/run.sh crates/archicraft-ifcx/tests/golden/*.ifcx
```

The conformance check loads IFCX files with buildingSMART's reference
implementation and viewer composition (SRS IFX-006); it needs Node.js and
network access to fetch the pinned reference code. CI runs all of this on
Linux, plus cargo-deny (licences, advisories, sources and bans); the tests run
on Windows for every change to `main` and nightly.
The tests run on macOS (Apple silicon) when a version tag `v*` is pushed, or
by hand with `gh workflow run macos.yml`. The conformance check runs on Linux,
Windows and macOS every night and by hand, and on Linux for pull requests that
change the model, geometry, IFCX or conformance code
(`.github/workflows/conformance-matrix.yml`). The IFCX reader and the
typed-input parser are fuzzed every night, and for an hour per target before a
release (`gh workflow run fuzz.yml -f minutes=60`; docs/testing.md, Fuzzing).

Each release also goes through the manual checks of
[docs/test-plan/release-checklist.md](docs/test-plan/release-checklist.md),
among them the hosted IFC5 viewer check of IFX-006.

Repository guards keep done requirements done: `scripts/check-layering.sh`
keeps UI crates out of the headless crates, `scripts/check-legal.sh` refuses
trademarks and unlisted images, library parts and templates, and `deny.toml`
refuses HTTP client crates, which is why adding such a dependency fails CI
(see [docs/testing.md](docs/testing.md)).

## Implementing with Claude Code

Implementation runs through the bead loop: one task (bead) per Claude Code session, tests
first, an automatic gate, an independent review, and a pull request that merges on green CI.
Start with the tutorial in [docs/loop-tutorial.md](docs/loop-tutorial.md); the design and its
rules are in [docs/loop.md](docs/loop.md).

## Repository layout

| Path | Content |
|---|---|
| `crates/archicraft-model` | Domain model, command/undo core, units, default template |
| `crates/archicraft-geometry` | Meshes, wall junctions, plan cuts |
| `crates/archicraft-ifcx` | IFCX reader and writer (`ifcx_alpha` adapter), bundled schemas |
| `crates/archicraft-app` | The desktop application (eframe, egui, wgpu) |
| `conformance/` | IFX-006 check with buildingSMART's reference code (test-only) |
| `scripts/` | Packaging (macOS DMG), repository guards, the bead loop (`scripts/loop/`) |
| `docs/` | SRS, technology choices, roadmap, implementation status, testing, the bead loop |

## License

This work is licensed under [Creative Commons Attribution 4.0 International (CC BY 4.0)](https://creativecommons.org/licenses/by/4.0/). See [LICENSE](LICENSE).
