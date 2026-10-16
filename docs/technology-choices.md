# Technology Choices

**Status:** Proposed · **Date:** 2026-10-16 · **Applies to:** first prototype

This document lists the technologies we intend to use for Archicraft: a native BIM/CAD application for architects and civil engineers that creates and edits IFCX models, with a user interface inspired by Archicad.

It is not a software architecture. It says what we build with, not how the pieces fit together.

## Scope of the prototype

- Native desktop application for Windows, macOS and Linux, written in Rust. No browser or PWA build.
- User interface inspired by Archicad's layout: menu bar, Toolbox, Info Box, Navigator, tabbed views.
- Reads and writes IFCX (IFC5) only.
- Out of scope: collaboration, importing older IFC versions (IFC2x3/IFC4 STEP files), other output formats (DXF, DWG, PDF), installers and distribution, crash reporting.

## Selection criteria

1. **Maturity and community:** many users, more than one maintainer, real production use. Download numbers are checked, not assumed (see [Health check](#health-check-2026-10-16)).
2. **Active maintenance:** releases within the last year.
3. **Permissive licenses:** MIT, Apache-2.0 or similar. No GPL, LGPL or AGPL. Weak copyleft (MPL-2.0) only after review.
4. **Pure Rust where possible:** every C/C++ dependency means a second toolchain to set up on three platforms.
5. **Only what the prototype needs:** things we will probably need later are listed as [deferred](#deferred-candidates-for-later), not added now.

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
| GPU rendering | wgpu, WGSL shaders, bytemuck |
| 2D tessellation for plan views | lyon |
| Math | glam |
| 2D geometry | geo (brings i_overlay, spade and rstar) |
| 3D geometry | Our own mesh generation, no geometry kernel |
| IFCX reading and writing | serde, serde_json, uuid |
| Project file | The `.ifcx` file itself |
| Errors and logging | thiserror, anyhow, tracing |
| Testing | cargo test, insta, egui_kittest, jsonschema |

All of these are licensed under MIT, Apache-2.0, or a choice of both.

## Choices in detail

### Language and toolchain: Rust

- Stable toolchain, edition 2024, pinned in the repository with `rust-toolchain.toml`.
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

How the Archicad layout maps onto egui:

| Archicad | egui |
|---|---|
| Menu bar, toolbars, Info Box | Top panels |
| Toolbox | Left side panel |
| Navigator, Organizer | Right side panel |
| Floor plan, 3D, section and elevation windows as tabs | Central area with egui_tiles (tabs and splits) |
| Floating palettes | egui windows, or separate native windows |

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

### GPU rendering: wgpu

- The standard GPU API in Rust: Metal on macOS, DX12 or Vulkan on Windows, Vulkan on Linux. About 4.8 million downloads per month.
- egui itself renders through wgpu, so the UI and our views share one GPU device.
- `bytemuck` to pass vertex data to the GPU.
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
- Used for wall outlines and joins in plan, slab and zone polygons, opening cut-outs, triangulating polygon faces for meshes, and the snapping index.
- Planar algorithms work for building geometry. We simply don't use the geographic ones (geodesic distances and the like).
- About 1.6 million downloads per month, MIT/Apache.
- Questioned:
  - Using i_overlay directly: it is the engine underneath, but it has had nine major versions. geo pins one (4.5.x) and gives us a calmer API.
  - If we use rstar directly, use the version geo uses (0.12), so we don't compile two copies.
  - kurbo (curve library) dropped: straight lines and arcs are all we need.

### 3D geometry: our own mesh generation, no kernel

- IFCX stores geometry as triangle meshes (`usd::usdgeom::mesh`, with `points` and `faceVertexIndices`), placed with 4×4 transforms (`usd::xformop`). The prototype therefore needs triangle meshes, not boundary representations (B-rep) or NURBS.
- Walls, slabs, columns, beams and openings are generated by extruding 2D outlines built with geo. Openings are cut as 2D polygons, in plan or in the wall's own plane, before extruding.
- Arcs are tessellated while generating the meshes.
- Questioned and dropped:
  - truck and Fornjot: young B-rep kernels with small communities, and we don't need B-rep.
  - OpenCASCADE: C++, LGPL, and was only needed for STEP import, which is out of scope.
- Manifold (general 3D booleans) is [deferred](#deferred-candidates-for-later).

### IFCX reading and writing: our own code on serde

- IFCX is JSON with four parts:
  - `header`
  - `imports`: URIs of schemas on ifcx.dev
  - `schemas`: definitions of custom attributes
  - `data`: nodes with `path`, `children`, `inherits` and `attributes`
- The container format is defined in `schema/ifcx.tsp` in buildingSMART's IFC5-development repository. It is small enough to write the serde types by hand. We don't need code generation (typify).
- Enable serde_json's `float_roundtrip` feature, so coordinates survive load and save cycles bit for bit.
- `uuid` for node paths, as in the official examples.
- Questioned:
  - ifc-lite (Rust, reads and writes IFCX): MPL-2.0, one main maintainer, and a fast-moving API (ifc-lite-core is already at 5.0). Useful as a reference and to cross-check our output, but not as a dependency.
  - IfcOpenShell and web-ifc were dropped together with legacy IFC import.

### Project file: the `.ifcx` file itself

- No separate project format in the prototype.
- Our parametric data (wall reference line, thickness, layer structure and so on) is stored as attributes in our own namespace (`archicraft::…`) and declared in the file's `schemas` section, the way IFCX intends custom data to be added.
- Standard IFC classes, properties and meshes are written alongside, so other IFCX tools can read the file.
- Questioned: a SQLite container (rusqlite). It gives incremental saves, crash recovery and undo history on disk, which matter for large real projects but not for a prototype. Deferred.

### Errors and logging

- `thiserror` for library errors, `anyhow` for application-level errors, `tracing` with `tracing-subscriber` for logging. These are the standard Rust choices, with no alternatives worth weighing.

### Testing

- `cargo test` for unit tests.
- **insta:** snapshot tests of generated IFCX, reviewed as JSON diffs.
- **egui_kittest:** UI tests that find widgets by label via AccessKit, plus screenshot comparisons rendered with wgpu. Made by the egui maintainers.
- **jsonschema:** validates our output against a JSON Schema of the IFCX container, either generated from `ifcx.tsp` with the TypeSpec compiler or taken from ifcx.dev once buildingSMART publishes it there.
- Manual check: open the output in buildingSMART's IFC5 viewer.
- Write our own test IFCX files. The IFC5-development repository asks users not to create derivatives of its examples.

## Changed since the first recommendation

| Earlier choice | Now | Why |
|---|---|---|
| Slint | egui | Team decision: larger community, production use, simpler viewport embedding |
| muda (native menu bars) | Menu bar drawn by egui | muda needs a GTK window on Linux. Deferred for macOS |
| vello | lyon + wgpu | One rendering pipeline. vello is pre-1.0 |
| i_overlay, spade, rstar, kurbo as separate picks | geo | One maintained ecosystem with a calmer API. kurbo isn't needed |
| Manifold | Deferred | Prototype elements don't need general 3D booleans. Its Rust bindings have a small community |
| truck, OpenCASCADE | Dropped | IFCX stores meshes, and there's no STEP import |
| IfcOpenShell (legacy IFC) | Dropped | Backward compatibility isn't needed |
| bevy_ecs | Plain Rust data structures | An entity-component system doesn't pay off for a document editor, and Bevy breaks its API every release |
| SQLite project file | `.ifcx` | Simpler for a prototype |
| typify (code generation) | Hand-written serde types | The container model is small |
| parry3d (collision queries) | Dropped | Not needed for selection and snapping in the prototype |
| cosmic-text, parley | egui's text rendering | Enough for UI and labels |
| wasmtime, Rhai (parametric objects) | Deferred | Element types are hard-coded in Rust for now |
| rayon, memmap2 | rayon deferred, memmap2 dropped | Not needed yet. JSON is read in full anyway |
| Loro/Automerge, krilla, dxf, ODA SDK, cargo-dist/Velopack, sentry | Dropped | Out of scope by decision |

## Deferred: candidates for later

| Need | Candidate | Add when |
|---|---|---|
| General 3D booleans (like Archicad's Solid Element Operations) | Manifold, through the `manifold3d` crate or our own bindings to its C API. Pure-Rust alternative: `boolmesh` (MPL-2.0, 0.1.x) | The first element that can't be built from 2D cuts |
| Parallel mesh generation | rayon | Mesh generation is measurably slow |
| Native macOS menu bar | muda | UI polish |
| Scriptable library objects (like GDL) | Rhai for simple scripting, or wasmtime for sandboxed WASM plugins | We need user-defined objects |
| Faster or crash-safe project storage | SQLite (rusqlite) | `.ifcx` save/load is too slow, or we need crash recovery |
| Constraint solving, general linear algebra | nalgebra | Sketch constraints |
| 3D proximity queries, clash detection | parry3d-f64 | Clash detection |

The Manifold library itself is well established; Blender 4.5 ships it as a boolean solver. Its Rust bindings are the weak point: about 2,200 downloads per month and one owner.

## Risks and open questions

1. **IFCX isn't finished.** Files still declare `"ifcxVersion": "ifcx_alpha"`, and the official standard is still IFC 4.3 (ISO 16739-1:2024). In August 2026 the final project plan for "IFCX Core & Modularisation" passed the next phase of buildingSMART's approval process, and voices in the OSArch community expect years before IFCX is ready for production work. Expect breaking format changes, and keep all IFCX code in one place. Most BIM tools can't exchange IFCX yet, so interoperability will be limited.
2. **Breaking upgrades every quarter** for egui and wgpu, and the docking crate lags behind egui releases. Plan a deliberate upgrade each quarter.
3. **Plan-view drawing quality** (line weights, hatching, cut versus projected lines) is our own work on wgpu and lyon. No library provides it.
4. **Linux runtime requirements:** a Vulkan driver, and an XDG desktop portal for file dialogs.
5. **Icons:** open icon sets (Lucide, Phosphor) cover generic actions but not BIM tools (wall, slab, stair, …). We need our own, and we must not copy Archicad's.
6. **Starting egui version:** whichever version egui_tiles supports when we start.

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
| manifold3d (deferred) | 0.4.1 | 2026-09-09 | 2.2 k | Apache/MIT | One owner |

serde, serde_json, uuid, thiserror, anyhow, tracing, bytemuck and insta are among the most widely used crates in the Rust ecosystem and were not checked individually.

## References

- [buildingSMART IFC5-development](https://github.com/buildingSMART/IFC5-development): IFCX container schema (`schema/ifcx.tsp`) and examples
- [IFC5 viewer](https://ifc5.technical.buildingsmart.org/viewer/)
- [IFC schema specifications](https://technical.buildingsmart.org/standards/ifc/ifc-schema-specifications/): current official IFC versions
- [OSArch: IFCX Core project update](https://community.osarch.org/discussion/3587/ifcx-core-project-update-by-buildingsmart)
- [Rerun](https://github.com/rerun-io/rerun): reference for an eframe + egui_tiles + wgpu application
- [Blender 4.5 Boolean modifier](https://docs.blender.org/manual/en/4.5/modeling/modifiers/generate/booleans.html): Manifold solver
