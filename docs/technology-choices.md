# Technology Choices

**Status:** Proposed, aligned with SRS v0.2 · **Date:** 2026-10-16 · **Applies to:** ArchiCraft v1, starting with the first prototype

This document lists the technologies we intend to use for ArchiCraft: a native BIM/CAD application for architects and civil engineers that creates and edits IFCX models. What ArchiCraft must do is specified in the [SRS](SRS.md), which takes proven workflows from Archicad as a reference but uses its own terms (SRS Appendix B).

It is not a software architecture. It says what we build with, not how the pieces fit together.

## Scope

These limits apply to all of v1, not only to the first prototype. They were aligned with the SRS on 2026-10-16 (see [Alignment with the SRS](#alignment-with-the-srs-2026-10-16)).

- Native desktop application for Windows, macOS and Linux, all three required (SRS NFR-PORT-001…003), written in Rust. No browser or PWA build.
- User interface with the main-window parts of SRS UI-001: menu bar, tool palette, properties panel, project browser, tabbed views.
- Reads and writes IFCX (IFC5), both our own project files and IFCX files from other tools. No other file format is imported or exported (SRS D-11).
- Registers itself for `.ifcx` files at first start, with the user's consent (SRS SW-001). There is no installer (SRS D-12).
- Out of scope: collaboration; older IFC versions (IFC2x3/IFC4 STEP files); all other file formats, such as DXF, DWG, PDF, images, survey point files, glTF and STL; installers and automatic updates; and any online service, including crash reporting. Crash reports stay on the user's disk (SRS NFR-SEC-002).

## Selection criteria

1. **Maturity and community:** many users, more than one maintainer, real production use. Download numbers are checked, not assumed (see [Health check](#health-check-2026-10-16)).
2. **Active maintenance:** releases within the last year.
3. **Permissive licenses:** MIT, Apache-2.0 or similar. No GPL, LGPL or AGPL. Weak copyleft (MPL-2.0) only after review. This keeps every distribution model open; the SRS hasn't chosen one yet (SRS O-5).
4. **Pure Rust where possible:** every C/C++ dependency means a second toolchain to set up on three platforms. Manifold (planned) is the one exception we accept so far.
5. **Only what the current increment needs:** things v1 needs later are listed as [planned](#planned-for-v1), things we may need someday as [deferred](#deferred-candidates-for-later). Neither is added early.

## Stack at a glance

| Area | Choice |
|---|---|
| Language | Rust (stable toolchain, edition 2024) |
| App framework and windowing | eframe (winit underneath) |
| UI toolkit | egui |
| Docking and tabbed views | egui_tiles |
| Tables and SVG icons | egui_extras |
| Menu bar and keyboard shortcuts | egui (drawn inside the window) |
| File dialogs | rfd |
| User-interface text | Fluent message catalogues (fluent-bundle, unic-langid) |
| GPU rendering | wgpu, WGSL shaders, bytemuck |
| 2D tessellation for plan views | lyon |
| Math | glam |
| 2D geometry | geo (brings i_overlay, spade and rstar) |
| 3D geometry | Our own mesh generation; 3D Booleans with Manifold (planned, at the latest in SRS increment 2) |
| IFCX reading and writing | serde, serde_json, uuid; our own attribute validator and number formatting |
| Project file | The `.ifcx` file itself; autosave writes a recovery copy |
| Errors and logging | thiserror, anyhow, tracing; crash reports from a panic hook |
| Testing | cargo test, insta, egui_kittest, jsonschema, cargo-fuzz, syn (dev-only, the literal-string lint); buildingSMART's reference code and viewer for conformance (test-only, under Node.js) |
| Licence checks | cargo-deny |

All of these are licensed under MIT, Apache-2.0, or a choice of both; the planned Manifold library is Apache-2.0. The Node.js test tools are not shipped.

## Choices in detail

### Language and toolchain: Rust

- Stable toolchain, edition 2024, pinned in the repository with `rust-toolchain.toml`. Only fuzzing needs nightly (see [Testing](#testing)).
- `rustfmt` and `clippy` as standard.
- Shaders are written in WGSL, wgpu's native shader language.

### UI toolkit: egui with eframe

Why:

- The largest Rust GUI community: about 2.4 million downloads per month and more than 2,200 dependent crates.
- Proven in a large production app with 3D views. The Rerun viewer is built on eframe, egui_tiles and wgpu, which makes it the best open-source reference for our setup.
- Embedding our own wgpu rendering is built in (`egui_wgpu::Callback`), so 3D and plan views live inside egui panels.
- Pure Rust, MIT/Apache, accessibility through AccessKit (which egui_kittest also uses for UI tests).
- eframe uses wgpu by default and runs on Windows, macOS and Linux (X11 and Wayland). It supports several native windows, so palettes can be torn off into their own windows.

Questioned:

- **egui's API is not stable.** It is pre-1.0, and every release (about four per year) has breaking changes. Slint, the earlier recommendation, is semver-stable at 1.x. We chose egui for its community, production use and viewport integration. To limit the cost: pin exact versions, upgrade deliberately once per release, and keep UI regression tests (egui_kittest).
- egui redraws the whole interface every frame. That's fine for panels. The 3D and plan views must keep their GPU buffers and re-render only when something changed.
- egui doesn't look native. That's acceptable for a professional tool: Blender doesn't look native either.

How the main window (SRS UI-001) maps onto egui. The Archicad names are only for orientation; the UI uses the SRS terms (SRS NFR-LEG-002, Appendix B).

| Main-window part (SRS) | Archicad equivalent | egui |
|---|---|---|
| Menu bar, toolbars, properties panel (UI-002) | Info Box | Top panels |
| Tool palette | Toolbox | Left side panel |
| Project browser (VIEW-003) | Navigator | Right side panel |
| Plan, 3D, section and elevation views as tabs (UI-006, UI-007) | Floor plan, 3D, section and elevation windows | Central area with egui_tiles (tabs and splits) |
| View-control bar, status line with the input prompt (UI-001) | — | Bottom panel |
| Floating panels (UI-004) | Floating palettes | egui windows, or separate native windows |

### Docking: egui_tiles

- Maintained by egui's author at Rerun, released in step with egui, and used in the Rerun viewer.
- About 190,000 downloads per month, against 79,000 for the alternative egui_dock, which has a single community maintainer.
- Limitation: tabs can't float. Floating palettes are plain egui windows instead.
- On 2026-10-16, egui_tiles 0.17.1 still targets egui 0.36, while egui 0.37.0 came out the day before. Start on whichever egui version egui_tiles supports.

### Tables and icons: egui_extras

- egui's official companion crate: tables (element lists, property tables) and image loaders, including SVG for tool icons.

### Menu bar and keyboard shortcuts: drawn by egui

- One implementation on all three platforms, like Archicad on Windows, where the menu bar sits inside the window.
- Questioned the native menu bar (muda): on Linux, muda menus have to be attached to a GTK window, and eframe/winit windows aren't GTK windows. A native macOS menu bar can be added later with muda ([deferred](#deferred-candidates-for-later)).

### File dialogs: rfd

- Native open and save dialogs on all platforms. About 4.7 million downloads per month, MIT.
- On Linux it uses XDG desktop portals, which need D-Bus and a portal backend. Standard desktops have both.
- Considered and rejected: file dialogs drawn in egui. Users expect the native dialog.

### User-interface text: Fluent

- Every string the user reads comes from a [Fluent](https://projectfluent.org) message catalogue embedded in the application (`crates/archicraft-app/locales/`), looked up by key (SRS NFR-L10N-001). English is the default; a German catalogue (NFR-L10N-002) is added beside it without code changes.
- **fluent-bundle** (with fluent-syntax for reading the catalogue files) instead of a hand-written key table, because the German translation needs CLDR plural rules and arguments in a different order. **unic-langid** names the languages. All three are Mozilla's Project Fluent crates; Firefox uses them.
- Numbers are formatted with the user's number format before they are passed to a message (NFR-L10N-004), not by Fluent.

### GPU rendering: wgpu

- The standard GPU API in Rust: Metal on macOS, DX12 or Vulkan on Windows, Vulkan on Linux. About 4.8 million downloads per month.
- egui itself renders through wgpu, so the UI and our views share one GPU device.
- `bytemuck` to pass vertex data to the GPU.
- The hardware classes of SRS NFR-PERF-001 (Apple M1 and M2; Intel Iris Xe and Arc graphics) are all covered by these backends. Whether the performance targets are met is measured (SRS AS-07), not assumed.
- Considered and rejected:
  - OpenGL: deprecated on macOS.
  - Vulkan directly (ash): far more code, and needs MoltenVK on macOS.
  - The Bevy engine: game-engine scope, and breaking releases.
- Watch out: wgpu releases a breaking major version every three months, and egui-wgpu is tied to one wgpu major. Always use the re-export `egui_wgpu::wgpu` so versions can't drift apart, and upgrade both together.

### Plan views: lyon

- Turns 2D fills and outlines (cut walls, hatches, symbols) into triangles that we draw with wgpu.
- Stable 1.0 API since 2022, still maintained (1.0.19 in March 2026), about 1.3 million downloads per month.
- Considered and rejected:
  - vello (GPU 2D renderer): pre-1.0, and it would add a second rendering pipeline next to ours.
  - egui's own painter: it re-tessellates every frame and only fills convex shapes, so it's too slow for large plans. It is still the right tool for small overlays such as snap markers and dimensions being drawn.

### Math: glam

- f64 types (`DVec2`, `DVec3`, `DMat4`) for the model, f32 types for the GPU.
- About 19 million downloads per month.
- Model coordinates are f64 in meters. GPU data is f32, relative to a local origin near the camera, so large site coordinates don't jitter.
- Questioned: nalgebra is more general (decompositions, solvers) but heavier and slower to compile. Add it only if we need a constraint solver.

### 2D geometry: geo

- One maintained ecosystem (georust) covering most of what plans need:
  - polygon booleans (union, difference, intersection) through i_overlay
  - buffering and offsetting
  - constrained Delaunay triangulation through spade (enabled by default)
  - a spatial index through rstar
- Used for wall outlines and joins in plan, slab and zone polygons, opening cut-outs, triangulating polygon faces for meshes, the terrain surface with its breaklines (SRS TER-001), and the snapping index.
- Planar algorithms work for building geometry. We simply don't use the geographic ones (geodesic distances and the like).
- About 1.6 million downloads per month, MIT/Apache.
- Questioned:
  - Using i_overlay directly: it is the engine underneath, but it has had nine major versions. geo pins one (4.5.x) and gives us a calmer API.
  - If we use rstar directly, use the version geo uses (0.12), so we don't compile two copies.
  - kurbo (curve library) dropped: straight lines and arcs are all we need.

### 3D geometry: our own mesh generation, Booleans with Manifold

- IFCX stores geometry as triangle meshes (`usd::usdgeom::mesh`, with `points` and `faceVertexIndices`), placed with 4×4 transforms (`usd::xformop`). We therefore need triangle meshes, not boundary representations (B-rep) with curved surfaces or NURBS.
- Walls, slabs, columns, beams and openings are generated by extruding 2D outlines built with geo. Openings are cut as 2D polygons, in plan or in the wall's own plane, before extruding.
- Arcs are tessellated while generating the meshes, with the tolerances of SRS IFX-034.
- The SRS needs more than 2D cuts can give:
  - **General 3D Booleans** (Must): associative solid operations, trims to roofs and slabs, terrain cuts, and openings that cut several elements (SRS SOL-020…031, TER-006, OPN-002). We use **Manifold** for these ([planned](#planned-for-v1)). It comes in when the first case can't be built from 2D cuts, at the latest in SRS increment 2.
  - **Free-form solids** (SRS SOL-001…008) need an editable polygon mesh with planar faces, per-face materials and per-edge flags: our own half-edge structure, not a B-rep kernel.
  - **Multi-plane roofs** (SRS ROF-003) get their planes, hips and valleys from a straight skeleton weighted by each edge's pitch. That is our own code; no library in the stack provides it.
  - **Plan and section cuts** are sliced from the 3D bodies, so that plan, section, 3D view and IFCX agree (SRS WAL-027). Also our own code.
- Questioned and dropped:
  - truck and Fornjot: young B-rep kernels with small communities, and we don't need curved-surface B-rep.
  - OpenCASCADE: C++, LGPL, and was only needed for STEP import, which is out of scope.

### IFCX reading and writing: our own code on serde

- IFCX is JSON with four parts:
  - `header`
  - `imports`: URIs of schemas on ifcx.dev
  - `schemas`: definitions of custom attributes
  - `data`: nodes with `path`, `children`, `inherits` and `attributes`
- The container format is defined in `schema/ifcx.tsp` in buildingSMART's IFC5-development repository. The SRS pins commit `1a63082` and the schema versions (SRS Table 5-2). The format is small enough to write the serde types by hand. We don't need code generation (typify).
- What we write is specified in SRS Section 5.2. The standard schema files of SRS Table 5-2 are bundled with the application, so opening, validating and saving never need the network (SRS IFX-004).
- Numbers:
  - Enable serde_json's `float_roundtrip` feature, so the parameters in our namespace survive load and save cycles bit for bit (SRS NFR-ACC-002).
  - Mesh and curve coordinates are rounded to 10⁻⁶ m and written in plain decimal notation (SRS IFX-007). serde_json would write 0.000002 as `2e-6`, so these arrays get their own number formatting.
- serde_json refuses JSON nested 128 levels deep or more. We keep that limit; SRS Appendix D.9 uses it.
- `uuid` for node paths, as in the official examples: random UUIDs (v4) for new objects and name-based UUIDs (v5) for regenerated sub-nodes (SRS IFX-010).
- Attribute values are checked against their IFCX schemas by our own validator, which follows buildingSMART's reference code (SRS IFX-005, IFX-083). JSON Schema (see [Testing](#testing)) only covers the container.
- Questioned:
  - ifc-lite (Rust, reads and writes IFCX): MPL-2.0, one main maintainer, and a fast-moving API (ifc-lite-core is already at 5.0). Useful as a reference and to cross-check our output, but not as a dependency.
  - IfcOpenShell and web-ifc were dropped together with legacy IFC import.

### Project file: the `.ifcx` file itself

- No separate project format.
- Our parametric data (wall reference line, thickness, layer structure and so on) is stored as attributes in our own namespace (`archicraft::…`) and declared in the file's `schemas` section, the way IFCX intends custom data to be added.
- Standard IFC classes, properties and meshes are written alongside, so other IFCX tools can read the file.
- Autosave (SRS PRJ-004 and NFR-REL-002, both Must, needed from SRS increment 0) writes the model as an `.ifcx` file to a recovery folder. It runs in a background thread, so editing isn't blocked (SRS NFR-PERF-009). Saving writes a temporary file and then replaces the project file (SRS NFR-REL-001).
- Questioned: a SQLite container (rusqlite). It gives incremental saves and undo history on disk, which matter for large real projects. Crash recovery doesn't need it, because autosave covers that. Deferred.

### Errors and logging

- `thiserror` for library errors, `anyhow` for application-level errors, `tracing` with `tracing-subscriber` for logging. These are the standard Rust choices, with no alternatives worth weighing.
- Crash reports (SRS NFR-SEC-002, Must): a panic hook from Rust's standard library writes the panic message, backtrace, version, operating system and the most recent log lines to a local file, and the application shows where that file is at the next start. Nothing is sent; the user decides whether to pass the file on. Native crashes, for example in a GPU driver or in Manifold's C++ code, are not caught (see [Deferred](#deferred-candidates-for-later)).

### Testing

- `cargo test` for unit tests.
- **insta:** snapshot tests of generated IFCX, reviewed as JSON diffs.
- **egui_kittest:** UI tests that find widgets by label via AccessKit, plus screenshot comparisons rendered with wgpu. Made by the egui maintainers.
- **syn** (with proc-macro2 for line numbers), dev-only: the literal-string lint (`crates/archicraft-app/tests/string_lint.rs`) parses the sources of archicraft-app and archicraft-model and fails on user-visible string literals outside the message catalogue (SRS NFR-L10N-001). A real Rust parser instead of a text search, so attributes, test modules and log macros are skipped reliably. Not shipped.
- **jsonschema:** validates our output against a JSON Schema of the IFCX container, either generated from `ifcx.tsp` with the TypeSpec compiler or taken from ifcx.dev once buildingSMART publishes it there.
- **Conformance (SRS IFX-006 and NFR-MNT-003, both Must):** on every change, CI loads our test files with schema validation in buildingSMART's reference code (`src/ifcx-core` at the commit the SRS pins). It also checks the spatial tree in their viewer (`src/viewer`), in a headless browser driven by Playwright. Both run under Node.js; they are test tools and aren't shipped. The visual check in the hosted IFC5 viewer stays manual, once per release.
- **cargo-fuzz:** fuzzes the IFCX reader for at least an hour per release (SRS NFR-REL-007, Appendix D.9). It needs a nightly toolchain, so it runs in its own CI job.
- **cargo-deny:** checks every dependency's licence against the selection criteria and lists them all, which gives the licence inventory of SRS NFR-LEG-001.
- CI builds and tests on Linux on every change, on Windows on `main` and nightly, and on macOS (Apple silicon) for releases and by hand (docs/loop.md).
- Write our own test IFCX files. The IFC5-development repository asks users not to create derivatives of its examples, so tests that read those examples (SRS AS-06) fetch them at test time instead of storing them in the repository (SRS NFR-LEG-003).

## Changed since the first recommendation

| Earlier choice | Now | Why |
|---|---|---|
| Slint | egui | Team decision: larger community, production use, simpler viewport embedding |
| muda (native menu bars) | Menu bar drawn by egui | muda needs a GTK window on Linux. Deferred for macOS |
| vello | lyon + wgpu | One rendering pipeline. vello is pre-1.0 |
| i_overlay, spade, rstar, kurbo as separate picks | geo | One maintained ecosystem with a calmer API. kurbo isn't needed |
| Manifold | Planned for v1, at the latest in SRS increment 2 | The first increments don't need general 3D booleans, but the SRS solid operations, trims and terrain cuts do (all Must). Its Rust bindings have a small community |
| truck, OpenCASCADE | Dropped | IFCX stores meshes, and there's no STEP import |
| IfcOpenShell (legacy IFC) | Dropped | Backward compatibility isn't needed |
| bevy_ecs | Plain Rust data structures | An entity-component system doesn't pay off for a document editor, and Bevy breaks its API every release |
| SQLite project file | `.ifcx` | Simpler, and autosave covers crash recovery |
| typify (code generation) | Hand-written serde types | The container model is small |
| parry3d (collision queries) | Dropped | Not needed for selection and snapping in the prototype |
| cosmic-text, parley | egui's text rendering | Enough for UI and labels |
| wasmtime, Rhai (parametric objects) | Deferred | Element types are hard-coded in Rust for now |
| rayon, memmap2 | rayon deferred, memmap2 dropped | Not needed yet. JSON is read in full anyway |
| Loro/Automerge, krilla, dxf, ODA SDK, cargo-dist/Velopack, sentry | Dropped | Out of scope by decision: collaboration, file formats other than IFCX (SRS D-11), installers, updates and online services (SRS D-12) |

## Alignment with the SRS (2026-10-16)

The SRS and this document were compared on 2026-10-16, and the product owner decided the conflicts. Both documents now reflect the decisions below; the SRS records its side as version 0.2.

| Topic | Conflict | Decision | Where |
|---|---|---|---|
| File formats | SRS v0.1 had DXF and image underlays, survey-point import, glTF/STL/PNG export and textures; this document read and wrote IFCX only | IFCX is the only file format | SRS D-11 (22 requirements withdrawn); [Scope](#scope) |
| Installer and updates | SRS SW-001 registered `.ifcx` files during installation, and SET-005 checked for updates; this document had no installer | No installer, updater or online service; `.ifcx` registration at first start | SRS D-12, SW-001; [Scope](#scope) |
| 3D Booleans | SRS solid operations, trims and terrain cuts are Must; this document had no geometry kernel and deferred Manifold | The requirements stay Must; Manifold is planned for v1 | [3D geometry](#3d-geometry-our-own-mesh-generation-booleans-with-manifold), [Planned for v1](#planned-for-v1) |
| Linux | Should in the SRS; treated like Windows and macOS here | Linux is required | SRS NFR-PORT-003 (now Must), D-2 |
| Crash reports | SRS NFR-SEC-002 (Must) kept them locally; this document had crash reporting out of scope | Local crash file from a panic hook; nothing is sent | SRS NFR-SEC-002, NFR-SEC-004; [Errors and logging](#errors-and-logging) |
| Product name | "Archicraft" here, "ArchiCraft" in the SRS and README; the name also resembles "ArchiCAD" | ArchiCraft; trademark check before any public release | This document; SRS O-17 |
| UI terms | Archicad names (Info Box, Navigator) here; neutral terms in the SRS (NFR-LEG-002) | SRS terms; Archicad names only for orientation | [UI toolkit](#ui-toolkit-egui-with-eframe) |
| IFCX numbers | "Coordinates survive bit for bit" here; SRS IFX-007 rounds mesh coordinates to 10⁻⁶ m in plain notation | Parameters round-trip exactly; mesh and curve coordinates get their own formatting | [IFCX](#ifcx-reading-and-writing-our-own-code-on-serde) |
| IFCX validation | JSON Schema and a manual viewer check here; SRS IFX-006 requires automated runs of buildingSMART's reference code and viewer | Our own attribute validator; reference code and viewer in CI under Node.js (test-only) | [IFCX](#ifcx-reading-and-writing-our-own-code-on-serde), [Testing](#testing) |
| Autosave | Crash recovery tied to the deferred SQLite store here; SRS PRJ-004 is Must from increment 0 | Autosave writes a recovery `.ifcx`; SQLite stays deferred | [Project file](#project-file-the-ifcx-file-itself) |
| Parser limits | SRS D.9 proposed a JSON nesting depth of 256; serde_json stops at 128 | Fewer than 128 levels; the IFCX reader is fuzzed with cargo-fuzz | SRS Appendix D.9; [Testing](#testing) |
| Licences | SRS NFR-LEG-001 expected the distribution model to be chosen here; it is still open (SRS O-5) | Permissive licences only; cargo-deny keeps the inventory | SRS NFR-LEG-001; [Testing](#testing) |
| Hardware | SRS NFR-PERF-001 expected the test machines to be confirmed here | wgpu supports both hardware classes; the machines are still to be chosen (SRS O-15) | [GPU rendering](#gpu-rendering-wgpu) |

## Planned and deferred

### Planned for v1

The SRS requires these, but the first increments don't. Each comes in at the point given, after the same health check as everything else. Increments are those of SRS Appendix F.

| Need | Choice | Add when |
|---|---|---|
| General 3D Booleans: solid operations, trims, terrain cuts (SRS SOL-020…031, TER-006; like Archicad's Solid Element Operations) | Manifold, through our own bindings to its C API or the `manifold3d` crate. Pure-Rust alternative: `boolmesh` (MPL-2.0, 0.1.x) | The first case that can't be built from 2D cuts, at the latest SRS increment 2 |
| Fetching remote schema imports of third-party IFCX files, after consent (SRS IFX-081) | A small HTTP client, for example ureq | Opening third-party IFCX files (SRS increment 3) |

The Manifold library itself is well established; Blender 4.5 ships it as a boolean solver. Its Rust bindings are the weak point: about 2,200 downloads per month and one owner. Manifold is C++, so it brings a C++ compiler and CMake to all three platforms (selection criterion 4).

### Deferred: candidates for later

| Need | Candidate | Add when |
|---|---|---|
| Parallel mesh generation | rayon | Mesh generation is measurably slow |
| Native macOS menu bar | muda | UI polish |
| Scriptable library objects (like GDL) | Rhai for simple scripting, or wasmtime for sandboxed WASM plugins | We need user-defined objects |
| Faster project storage, undo history on disk | SQLite (rusqlite) | `.ifcx` save or load misses the SRS targets (NFR-PERF-002) |
| Constraint solving, general linear algebra | nalgebra | Sketch constraints |
| 3D proximity queries, clash detection | parry3d-f64 | Clash detection |
| Crash reports for native crashes, not only Rust panics | A minidump handler | Native crashes show up in testing or use |

## Risks and open questions

1. **IFCX isn't finished.** Files still declare `"ifcxVersion": "ifcx_alpha"`, and the official standard is still IFC 4.3 (ISO 16739-1:2024). In August 2026 the final project plan for "IFCX Core & Modularisation" passed the next phase of buildingSMART's approval process, and voices in the OSArch community expect years before IFCX is ready for production work. Expect breaking format changes, and keep all IFCX code in one place. Most BIM tools can't exchange IFCX yet, and IFCX is our only format (SRS D-11), so interoperability will be limited.
2. **Breaking upgrades every quarter** for egui and wgpu, and the docking crate lags behind egui releases. Plan a deliberate upgrade each quarter.
3. **Plan-view drawing quality** (line weights, hatching, cut versus projected lines) is our own work on wgpu and lyon. No library provides it.
4. **Linux runtime requirements:** a Vulkan driver, and an XDG desktop portal for file dialogs. Linux is required (SRS NFR-PORT-003), so CI covers it from the start.
5. **Icons:** open icon sets (Lucide, Phosphor) cover generic actions but not BIM tools (wall, slab, stair, …). We need our own, and we must not copy Archicad's.
6. **Starting egui version:** whichever version egui_tiles supports when we start.
7. **Manifold is C++:** a second toolchain on three platforms, and Rust bindings with one owner. Writing our own thin bindings to its C API limits the second risk.
8. **Geometry no library provides:** the roof skeleton, free-form solid editing and mesh slicing are our own code. The SRS fixtures and geometry regression tests (SRS Appendix D.7, NFR-MNT-005) are the safety net.

## Health check (2026-10-16)

Figures from lib.rs on 2026-10-16.

| Crate | Latest | Released | Downloads/month | License | Notes |
|---|---|---|---|---|---|
| egui | 0.37.0 | 2026-10-15 | 2.37 M | MIT/Apache | Pre-1.0; breaking changes in every release |
| egui_tiles | 0.17.1 | 2026-08-18 | 190 k | MIT/Apache | Rerun; targets egui 0.36 |
| egui_dock (rejected) | 0.21.1 | 2026-08-06 | 79 k | MIT | Single maintainer; targets egui 0.36 |
| egui_kittest | 0.37.0 | 2026-10-15 | 210 k | MIT/Apache | Rerun |
| wgpu | 30.0.1 | 2026-08-22 | 4.80 M | MIT/Apache | Breaking major every 3 months |
| glam | 0.34.1 | 2026-10-06 | 19.2 M | MIT/Apache | |
| geo | 0.33.1 | 2026-04-20 | 1.56 M | MIT/Apache | georust; pins i_overlay 4.5.x and rstar 0.12 |
| i_overlay | 9.0.1 | 2026-10-08 | 1.42 M | MIT/Apache | Used through geo |
| spade | 2.15.1 | 2026-03-24 | 1.81 M | MIT/Apache | Used through geo |
| rstar | 0.13.0 | 2026-05-24 | 4.87 M | MIT/Apache | georust |
| lyon | 1.0.19 | 2026-03-08 | 1.29 M | MIT/Apache | Stable 1.0 API |
| rfd | 0.17.2 | 2026-01-12 | 4.67 M | MIT | |
| jsonschema | 0.58.6 | 2026-10-06 | 11.8 M | MIT | |
| manifold3d (planned) | 0.4.1 | 2026-09-09 | 2.2 k | Apache/MIT | One owner |
| fluent-bundle | 0.16.0 | 2025-05-22 | 2.5 M | MIT/Apache | Project Fluent (Mozilla), six owners; mature, few releases |
| fluent-syntax | 0.12.0 | 2025-05-22 | 2.6 M | MIT/Apache | Project Fluent; released with fluent-bundle |
| unic-langid | 0.9.6 | 2025-05-09 | 3.2 M | MIT/Apache | Zibi Braniecki and Manish Goregaokar |
| criterion | 0.8.2 | 2026-02-04 | 22.6 M | MIT/Apache | Dev-dependency for the benches; default features (rayon, plotters) off |
| syn | 3.0.9 | 2026-10-12 | 246 M | MIT/Apache | David Tolnay; dev-dependency for the literal-string lint, with proc-macro2 (already in the tree) for line numbers |

The Fluent rows were added on 2026-10-19 from crates.io (downloads: the last 90 days divided by three). Their last releases are more than a year old, so they miss criterion 2 (releases within the last year); they are kept because the format and its plural rules are stable and Firefox depends on them. The criterion and syn rows were added on 2026-10-20 the same way.

serde, serde_json, uuid, thiserror, anyhow, tracing, bytemuck and insta are among the most widely used crates in the Rust ecosystem and were not checked individually. Not checked yet: the tools added when aligning with the SRS (cargo-fuzz, cargo-deny, Playwright) and the planned HTTP client. Playwright is pinned as `@playwright/test` 1.63.0 (2026-09-04, Apache-2.0) with the three.js release the pinned viewer loads (0.177.0, MIT), both in `conformance/package.json`; like Node.js they are CI test tools, and Playwright's browser download never happens in the product.

## References

- [ArchiCraft SRS](SRS.md): the requirements this stack has to meet
- [buildingSMART IFC5-development](https://github.com/buildingSMART/IFC5-development): IFCX container schema (`schema/ifcx.tsp`), reference code and examples
- [IFC5 viewer](https://ifc5.technical.buildingsmart.org/viewer/)
- [IFC schema specifications](https://technical.buildingsmart.org/standards/ifc/ifc-schema-specifications/): current official IFC versions
- [OSArch: IFCX Core project update](https://community.osarch.org/discussion/3587/ifcx-core-project-update-by-buildingsmart)
- [Rerun](https://github.com/rerun-io/rerun): reference for an eframe + egui_tiles + wgpu application
- [Manifold](https://github.com/elalish/manifold): the 3D Boolean library planned for v1
- [Blender 4.5 Boolean modifier](https://docs.blender.org/manual/en/4.5/modeling/modifiers/generate/booleans.html): Manifold solver
