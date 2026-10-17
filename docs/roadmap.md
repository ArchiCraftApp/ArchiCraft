# ArchiCraft roadmap

_Epic-level plan for the entire remaining SRS (567 live requirement IDs, each in exactly one epic), generated 2026-10-17 from docs/SRS.md v0.2, docs/technology-choices.md, docs/implementation-status.md and gap-analysis.json. Starting point: roadmap B (judges' winner, foundations first). Grafted: the early verification and performance assets of C, the stair split, the arc-wall and junction fixes of A, and every problem the three judges and the coverage reports listed. 40 epics, 550 engineer-days._

> The work is tracked in beads: every epic below is a beads epic (id in brackets) whose tasks carry the full description, acceptance criteria and design notes. Use `bd ready` for the next tasks and `bd show <id>` for details.

## Ordering and rationale

### The angle: B's foundations, C's verification discipline, A's dependency hygiene

The implementation status shows an increment-0 skeleton that is sound (command/undo core, storeys, straight walls, slabs, plan and 3D views, IFCX round trip, autosave, conformance in CI) but closed at exactly the points the next 505 open requirements need to open: the element kind is a two-variant enum matched in about fifteen places across all four crates, there is no relation store although increments 1 to 3 are all about relations (host, junction, trim, solid operation, annotation binding), geometry is re-evaluated wholesale on every edit, the point-input pipeline is inlined in `views/plan.rs`, the plan generator only looks at a storey's own elements, there is no attribute system under the plan drawing, and every string is a literal. Roadmap B opened all of these first and was the judges' winner; this roadmap keeps that spine and repairs what the judges found.

Increment 1 therefore opens with four independent foundation epics that run in parallel: `element-kind-framework` (kind registry, relation store, compound-part codec, tolerance module, incremental evaluation, seeded fixture builder), `point-input-snapping` (the shared `PointInput` service with the complete snap and guide model), `commands-and-l10n` (command registry, palette, shortcuts, string externalisation including `Command::label` in the headless crate) and `regression-and-status`, which now carries the verification infrastructure that B deferred to increment 2 or 4: the D.7 fixture registry, the headless Playwright run of the reference viewer (IFX-006 b), cargo-fuzz with the D.9 limits, byte determinism, the Appendix D defaults test, the AS-05 end-to-end crash-recovery run and the performance harness with the NFR-PERF-001 hardware decision. Nothing of that needs a prerequisite beyond the existing reader and writer, so it runs from day one and every later slice is measured and fuzzed as it lands.

Features then follow in strict dependency order. The wall chain is `layer-structures` (F2, F3) -> `junction-engine` (F1, F6, F7, join/unjoin, order independence, IFX-033) and, unlike B, it is built on 2D per-skin footprints with geo/i_overlay only: the increment-1 exit criterion no longer waits for a C++ toolchain, which follows technology-choices ("Manifold when the first case cannot be built from 2D cuts, at the latest in increment 2"). For the same reason `host-openings` cuts single-host openings as 2D polygons in the wall plane before extrusion; because OPN-007 demands the live host preview in 3D views as well, it depends on `3d-input-navigation` for the Camera3d `ViewSurface`, a prerequisite B left implicit. `3d-view-restrictions` implements cut planes and the section box as GPU clip planes with a stencil cap pass, which lets the Must view restrictions (VIEW-040/042/043) land in increment 1 as soon as `plan-views-browser` exists. `profiles-columns-beams`, `components-dw-catalogue` -> `doors-windows-core` supply the remaining increment-1 kinds; `plan-attributes` -> `plan-renderer` (which also owns the skin separator and layered-display options, so they come after layer structures and pens exist) and the merged `plan-views-browser` (view range and display ranges plus saved views, browser, layers and layout) give every later kind correct plans for free; `selection-transforms` introduces the `PartRef` edit-mode infrastructure of SEL-008 before the first compound element exists and depends on the kind registry whose transform hook it uses; `edit-reshape-palette` owns contour detection (INP-018) because slab-hole tracing, 2D cutters and roof pivot polygons need it in increments 1 and 2, not in increment 4, and `drafting-trim-split` depends on it because ANN-002 makes 2D elements the tracing input of that detection.

Increment 2 opens with the risk slices in parallel: `solid-kernel-ops` (Manifold, the associative solid operations with the fixed pipeline order that lets explicit operations prevail over automatic junctions, OPN-002, the auxiliary flag), `roofs` (own straight skeleton; no kernel dependency), `stair-solver-tool` (pure solver functions, no dependency on roofs), `third-party` composition in `ifcx-interop` (pulled from increment 3, because the R2 composition semantics are a reader risk best proven right after the reader generalisation), `grids` (AS-02 as a committed asset) and `curved-geometry` (arc walls on the general-polygon junction engine, together with the two requirements that need an arc edge or an arc wall: fillet EDT-014 and units in arc walls DW-019, which B had scheduled before arcs existed). The second wave consumes them: `cleanup-and-trims` (the cross-kind priority cleanup COL-006/BEA-004/SLB-007 as one generic contact-and-priority stage on the kernel, and the associative trims SOL-030..032 and ROF-010 that deliver the AS-01 gable walls), `free-form-solids` (which also owns the non-associative crop SOL-033, since cropping rewrites the element as a free-form solid), `terrain-and-site` (terrain plus georeferencing, depending on the 2D polylines it builds contours from), `stair-body-plan` (flights, landings, plan symbol, compound parts, display range), and `reference-models` closes the increment with RM-S, AS-01/AS-03 automation, F3/F9 and the NFR-USE-006 demonstration — depending only on increment-2 epics, so the exit criterion "AS-01 without railings" has its asset in the increment it belongs to.

Increment 3 carries the Should scope, each epic depending only on real prerequisites: `sections-elevations-dims`, `3d-display-styles`, `railings-stairs-adv`, `curtain-walls` (which lists `cleanup-and-trims` because CW-008 applies the Trim relation to frames and panels), `templates-presets` (incl. the stair rule presets), `library-dw-exchange` (object library, advanced units, multi-window and cross-project paste) and `members-profiles-adv` (F4, F5). Increment 4 holds `perf-tuning-rm-m-l` (RM-M/RM-L generated in-process, AS-07 against the harness built in increment 1), `release-candidate` (three-OS acceptance, usability test with a five-day findings buffer, minidump handler tied to the C++ kernel, German translation, licence gate) and `could-scope-polish` (the Could items without a feature home, each a bead taken up by feedback and cut first).

### Schedule shape

Increment 1 holds 19 epics and 254 engineer-days because Appendix F's increment 1 names most Must areas; the four foundation epics run in parallel, so with unlimited parallelism the increment's critical path is 56 engineer-days (point input -> selection and transforms -> reshape palette -> drafting), while the exit-criterion paths are shorter: framework -> layer structures -> junction engine (F1/F2/F6/F7) closes at 42 and framework -> 3D input -> host openings -> doors (the precise ground floor with openings) at 54. Increment 2 (11 epics, 153 days) closes at 83 with `reference-models`; increment 3 (7 epics, 101 days) at 86 because its epics only need increment-2 kinds; increment 4 (3 epics, 42 days) at 101. The 550 engineer-days of epic estimates are caps of 15 days each; where an epic's risks name a beads split, the gap-analysis effort sizes say a second wave is likely.

### Moves against Appendix F, with justification

- **Manifold in increment 2, first epic** (`solid-kernel-ops`): B pulled it into increment 1 and made the F2/F6/F7 exit depend on it; two judges objected and technology-choices says "at the latest increment 2". Everything increment 1 needs is built from 2D cuts.
- **OPN-002 (Must, openings cutting several elements) to increment 2**: technology-choices names it as a Manifold case; it rides with the kernel and depends on `host-openings`.
- **WAL-002 arc walls (Must) in increment 2** (`curved-geometry`): Appendix F's increment-1 list does not name it, AS-01 and F2/F6/F7 need no arcs, and the junction engine must be general-polygon first; EDT-014 and DW-019 move with it so no increment-1 acceptance needs an arc.
- **Third-party IFCX composition (Should) from increment 3 to 2** (`ifcx-interop`): a reader risk best proven on a small model right after the reader generalisation; the AS-06 run is repeated at the increment-3 exit.
- **PRJ-001 templates-as-artefact (Must) in increment 3** (`templates-presets`): its remaining gap (a selectable template file) only matters once layer structures and presets exist to be carried.
- **OBJ-001 (Must) in increment 3** (`library-dw-exchange`): Appendix F puts component library features in increment 3; doors and windows are usable from increment 1 through `components-dw-catalogue`.
- **VIEW-040..047/061 (Must and Should) in increment 1** (`3d-view-restrictions`): named by no Appendix F increment; with GPU clip planes they have no prerequisite beyond saved views, so they land as early as their dependency allows.
- **Should-level input features INP-012/INP-021 in increment 1** (B's move, kept): cheap once the snap-target model exists and expensive to retrofit.

### Priority mapping for beads

1 = increment-1 Must work and enabling infrastructure (every increment-1 epic); 2 = later Must work, including Must NFR epics; 3 = Should; 4 = Could. An epic takes the priority of its highest-priority open requirement; the build script derives the value from the requirement table and the increment, so no epic can violate the rule. Labels are `increment-N`, then the crates the epic touches (`archicraft-model`, `archicraft-geometry`, `archicraft-ifcx`, `archicraft-app`), then the area.

### Verification assets and where they live

| Asset | Owning epic |
|---|---|
| Seeded fixture builder (v5 UUIDs from fixture names), synthetic 1,000/5,000/20,000-element stand-ins | `element-kind-framework` |
| D.7 fixture registry (`crates/archicraft-model/tests/fixtures.rs`, f1..f9) | `regression-and-status` (entries filled by the owners below) |
| F1 (trapezoidal wall), F6, F7 | `junction-engine` |
| F2 (680 mm six-skin wall), F3 (floating floor) | `layer-structures` (F3 re-asserted with RM-S in `reference-models`) |
| F4 (segmented beam), F5 (corbel column) | `members-profiles-adv` |
| F8 (slab traced from an arc wall) | `curved-geometry` |
| F9 (sloping site, storeys -3..+1) | `terrain-and-site` (registered in `reference-models`) |
| AS-01 (RM-S house, without railing), AS-03 | `reference-models`; railing variant in `railings-stairs-adv` |
| AS-02 (`samples::structural_frame`) | `grids` |
| AS-04 (typed-input precision) | `point-input-snapping` (3D step in `3d-input-navigation`) |
| AS-05 (end-to-end crash recovery, injectable autosave clock, three OSes) | `regression-and-status` |
| AS-06 (third-party composition, fetched at test time) | `ifcx-interop` |
| AS-07 (performance protocol), RM-M, RM-L | `perf-tuning-rm-m-l` |
| AS-08 (usability test) | `release-candidate` |
| RM-S golden file (NFR-MNT-003) | `reference-models` |
| Performance harness (criterion benches, frame/edit-latency instrumentation, CI smoke-mode regression gate, `docs/performance/<date>-<machine>.md`), NFR-PERF-001 hardware decision | `regression-and-status` |
| IFX-006 (b) headless viewer under Playwright on three CI runners; byte determinism; fuzz corpus and nightly cargo-fuzz job with D.9 limit tests | `regression-and-status` |
| Appendix D defaults: code-level table test | `regression-and-status`; shipped-template `template_contents` test in `templates-presets` |
| NFR-MNT-005 junction regression suite | `junction-engine` (cases added by `host-openings`, `cleanup-and-trims`, `curved-geometry`, `members-profiles-adv`) |
| Plan golden images (egui_kittest + wgpu on a software adapter, tolerance in docs/testing.md) | `plan-renderer`; 3D styles in `3d-display-styles` |
| Incremental-versus-full evaluation equivalence property test | `element-kind-framework` |
| Scripted manual procedures (SRS 7.1) for AS-01..AS-05 under docs/test-plan/ | `doors-windows-core` (ground-floor scenario), `reference-models`, `release-candidate` |
| Status enforcement: CI compares each merged epic's srs_ids with the changed rows of docs/implementation-status.md | `regression-and-status` |
| NFR-USE-006 demonstration (RM-S built from template attributes only) | `reference-models` |

### Rules respected

Decisions D-1..D-12 and Section 8 are respected: IFCX is the native and only file format (templates, libraries and reference models are `.ifcx`); no installer or online service (the only network access is the consent-gated import fetch of `ifcx-interop`); one home storey per element with site-level containment only for terrain and grids (D-3); one material concept with per-face appearance overrides (D-5); junctions from reference-line contact with tested order independence (D-6); explicit commands for every gesture (D-7); sections, railings, curtain walls and presets as Should in increment 3 (D-8, D-9); one closed circular wall (D-10); `.ifcx` registration with consent and local crash reports (D-12). Nothing from Section 8 is planned (no scripting SDK, no DXF, no rendering). The headless crates (`archicraft-model`, `archicraft-geometry`, `archicraft-ifcx`) gain no UI dependency in any epic; every epic places UI work in `archicraft-app` only. Dependencies list only technical prerequisites (a type, a service or a kind that must exist); increment membership expresses the soft ordering, no dependency points to a later increment, and the build script checks coverage (567 IDs, each exactly once), key format, sizes, cycles and increment order mechanically.


## Epic table

| # | Key | Title | Inc | Prio | Days | M/S/C/done | Depends on |
|---|---|---|---|---|---|---|---|
| 1 | `element-kind-framework` | Extensible element kinds, relations, compound parts and incremental evaluation | 1 | 1 | 15 | 13/0/0/1 | — |
| 2 | `point-input-snapping` | Shared point-input service, snapping, guides and construction grid | 1 | 1 | 15 | 13/8/0/1 | — |
| 3 | `selection-transforms` | Selection model, part selection (edit mode) and transform commands | 1 | 1 | 15 | 13/3/1/0 | `point-input-snapping`, `element-kind-framework` |
| 4 | `edit-reshape-palette` | Node/edge edit palette, offsets, polygon editing, contour detection and validation | 1 | 1 | 13 | 8/4/0/0 | `point-input-snapping`, `selection-transforms` |
| 5 | `plan-attributes` | Graphic attribute resources: pens, line types, fills and material plan display | 1 | 1 | 10 | 9/4/1/0 | `element-kind-framework` |
| 6 | `drafting-trim-split` | 2D construction geometry with the shared arc type, trim, extend, split and array | 1 | 1 | 13 | 8/3/2/0 | `element-kind-framework`, `point-input-snapping`, `plan-attributes`, `selection-transforms`, `edit-reshape-palette` |
| 7 | `plan-renderer` | GPU plan renderer with drawing scale, line weights, hatches, skin separators and golden images | 1 | 1 | 12 | 2/5/1/0 | `plan-attributes`, `layer-structures` |
| 8 | `plan-views-browser` | Plan view range and display ranges, saved views, project browser, layers and layout persistence | 1 | 1 | 15 | 10/6/0/0 | `element-kind-framework` |
| 9 | `layer-structures` | Layer structures, skins, core faces and multi-skin bodies | 1 | 1 | 12 | 9/2/0/0 | `element-kind-framework` |
| 10 | `profiles-columns-beams` | Profiles, columns and beams | 1 | 1 | 14 | 9/3/0/0 | `element-kind-framework`, `point-input-snapping` |
| 11 | `junction-engine` | Priority-based wall junction engine on per-skin footprints (F1, F6, F7) | 1 | 1 | 15 | 8/4/0/0 | `layer-structures` |
| 12 | `host-openings` | Host relation and the opening element (2D cuts in the host plane) | 1 | 1 | 12 | 9/2/0/0 | `element-kind-framework`, `layer-structures`, `point-input-snapping`, `3d-input-navigation` |
| 13 | `components-dw-catalogue` | Component definitions and the door/window catalogue | 1 | 1 | 15 | 7/8/0/0 | `element-kind-framework` |
| 14 | `doors-windows-core` | Doors and windows in walls | 1 | 1 | 13 | 11/1/0/0 | `host-openings`, `components-dw-catalogue`, `layer-structures` |
| 15 | `properties-and-dialogs` | Properties panel, settings-dialog framework, reference levels and datum display | 1 | 1 | 12 | 7/6/1/1 | `element-kind-framework` |
| 16 | `commands-and-l10n` | Command registry, palette, shortcuts, string externalization and units UI | 1 | 1 | 12 | 13/6/0/0 | — |
| 17 | `3d-input-navigation` | Working plane, 3D input, measure tool, camera and navigation | 1 | 1 | 14 | 9/5/0/2 | `point-input-snapping` |
| 18 | `3d-view-restrictions` | 3D view filters, hidden elements, cut planes and section box as GPU clip planes | 1 | 1 | 12 | 5/2/1/0 | `plan-views-browser` |
| 19 | `regression-and-status` | Regression net, fixture registry, fuzzing, headless viewer, performance harness and status upkeep | 1 | 1 | 15 | 4/3/2/56 | — |
| 20 | `solid-kernel-ops` | Manifold Boolean kernel and associative solid operations | 2 | 2 | 15 | 10/4/0/0 | `element-kind-framework`, `host-openings` |
| 21 | `grids` | Grid systems | 2 | 2 | 9 | 4/3/1/0 | `element-kind-framework`, `point-input-snapping`, `profiles-columns-beams` |
| 22 | `curved-geometry` | Arc walls, curved slab edges, fillet, units in arc walls, polygonal and inclined walls | 2 | 2 | 15 | 5/3/2/0 | `junction-engine`, `point-input-snapping`, `drafting-trim-split`, `edit-reshape-palette`, `doors-windows-core` |
| 23 | `roofs` | Single- and multi-plane roofs with a pitch-weighted straight skeleton | 2 | 2 | 15 | 8/4/0/0 | `layer-structures`, `host-openings`, `point-input-snapping`, `junction-engine`, `edit-reshape-palette` |
| 24 | `cleanup-and-trims` | Cross-kind priority cleanup and associative trims to roofs and slabs | 2 | 2 | 15 | 5/2/0/0 | `solid-kernel-ops`, `junction-engine`, `profiles-columns-beams`, `roofs` |
| 25 | `free-form-solids` | Free-form solids with sub-element editing | 2 | 2 | 15 | 6/5/1/0 | `solid-kernel-ops`, `3d-input-navigation`, `selection-transforms` |
| 26 | `terrain-and-site` | Terrain, site-level elements and georeferencing | 2 | 2 | 15 | 9/5/1/0 | `element-kind-framework`, `solid-kernel-ops`, `point-input-snapping`, `3d-input-navigation`, `drafting-trim-split` |
| 27 | `stair-solver-tool` | Stair element, rule-based solver and placement tool | 2 | 2 | 15 | 12/0/0/0 | `element-kind-framework`, `point-input-snapping` |
| 28 | `stair-body-plan` | Stair flights, landings, finishes, plan symbol and compound parts | 2 | 2 | 15 | 7/5/0/0 | `stair-solver-tool`, `selection-transforms`, `plan-views-browser` |
| 29 | `ifcx-interop` | Third-party IFCX composition, imported elements, foreign-data preservation and material categories | 2 | 2 | 14 | 4/8/2/1 | `element-kind-framework`, `selection-transforms`, `plan-attributes` |
| 30 | `reference-models` | Reference model RM-S, AS-01/AS-03 automation and the defaults demonstration | 2 | 2 | 10 | 1/1/0/0 | `stair-body-plan`, `roofs`, `cleanup-and-trims`, `terrain-and-site`, `doors-windows-core`, `junction-engine`, `grids` |
| 31 | `sections-elevations-dims` | Section and elevation views, dimensions, labels and quantities | 3 | 3 | 15 | 0/12/7/0 | `plan-views-browser`, `plan-renderer`, `grids`, `host-openings`, `drafting-trim-split` |
| 32 | `3d-display-styles` | Display styles, sun, lighting, backgrounds, face materials and themes | 3 | 2 | 14 | 1/9/2/0 | `plan-views-browser`, `host-openings`, `terrain-and-site` |
| 33 | `railings-stairs-adv` | Railings, winders, spiral stairs and stair extras | 3 | 3 | 15 | 0/14/5/0 | `stair-body-plan`, `profiles-columns-beams`, `3d-input-navigation`, `curved-geometry`, `free-form-solids`, `solid-kernel-ops` |
| 34 | `curtain-walls` | Curtain walls | 3 | 3 | 13 | 0/8/4/0 | `element-kind-framework`, `components-dw-catalogue`, `selection-transforms`, `profiles-columns-beams`, `cleanup-and-trims` |
| 35 | `templates-presets` | Templates, start screen, presets, parameter transfer, groups, selection criteria and resource folders | 3 | 2 | 14 | 1/12/2/0 | `layer-structures`, `properties-and-dialogs`, `plan-views-browser`, `selection-transforms`, `stair-solver-tool` |
| 36 | `library-dw-exchange` | Object library, advanced doors and windows, multi-window and cross-project exchange | 3 | 2 | 15 | 2/11/3/0 | `components-dw-catalogue`, `doors-windows-core`, `selection-transforms`, `profiles-columns-beams`, `curved-geometry` |
| 37 | `members-profiles-adv` | Segmented and inclined members, advanced profiles and more opening hosts | 3 | 3 | 15 | 0/11/3/0 | `profiles-columns-beams`, `host-openings`, `curved-geometry`, `terrain-and-site`, `free-form-solids`, `solid-kernel-ops` |
| 38 | `perf-tuning-rm-m-l` | Reference models RM-M and RM-L, AS-07 and performance tuning | 4 | 2 | 15 | 5/2/0/0 | `reference-models`, `curtain-walls` |
| 39 | `release-candidate` | Three-OS acceptance, usability test, German translation, legal and security checks | 4 | 2 | 12 | 9/1/0/0 | `commands-and-l10n`, `reference-models`, `templates-presets`, `solid-kernel-ops` |
| 40 | `could-scope-polish` | Could scope by user feedback: spaces, buildings, shells, gravity, walk mode, devices, toolbars, history, scripting | 4 | 4 | 15 | 0/0/12/0 | `edit-reshape-palette`, `free-form-solids`, `roofs`, `terrain-and-site`, `commands-and-l10n`, `3d-input-navigation`, `plan-views-browser` |

Total: 40 epics, 550 engineer-days (15-day epics are caps; the risks name the beads split where one is expected).

### Increment 1 — 19 epics, 254 days

Exit criterion: Fixtures F1, F2, F6, F7 pass; a ground floor with openings can be drawn precisely (Appendix F); AS-04 and AS-05 are automated; the harness, fuzzing and headless viewer run in CI.

`element-kind-framework`, `point-input-snapping`, `selection-transforms`, `edit-reshape-palette`, `plan-attributes`, `drafting-trim-split`, `plan-renderer`, `plan-views-browser`, `layer-structures`, `profiles-columns-beams`, `junction-engine`, `host-openings`, `components-dw-catalogue`, `doors-windows-core`, `properties-and-dialogs`, `commands-and-l10n`, `3d-input-navigation`, `3d-view-restrictions`, `regression-and-status`

### Increment 2 — 11 epics, 153 days

Exit criterion: AS-01 without railings and AS-02 pass; RM-S exists as a golden file; AS-03 and AS-06 are automated.

`solid-kernel-ops`, `grids`, `curved-geometry`, `roofs`, `cleanup-and-trims`, `free-form-solids`, `terrain-and-site`, `stair-solver-tool`, `stair-body-plan`, `ifcx-interop`, `reference-models`

### Increment 3 — 7 epics, 101 days

Exit criterion: AS-01 to AS-06 pass in full (RM-S with railing); RM-M can be built.

`sections-elevations-dims`, `3d-display-styles`, `railings-stairs-adv`, `curtain-walls`, `templates-presets`, `library-dw-exchange`, `members-profiles-adv`

### Increment 4 — 3 epics, 42 days

Exit criterion: AS-07 and AS-08 pass; release candidate.

`perf-tuning-rm-m-l`, `release-candidate`, `could-scope-polish`

## Topological order (dependencies first)

`element-kind-framework`, `point-input-snapping`, `commands-and-l10n`, `regression-and-status`, `plan-attributes`, `plan-views-browser`, `layer-structures`, `components-dw-catalogue`, `properties-and-dialogs`, `selection-transforms`, `profiles-columns-beams`, `3d-input-navigation`, `stair-solver-tool`, `3d-view-restrictions`, `plan-renderer`, `junction-engine`, `edit-reshape-palette`, `ifcx-interop`, `grids`, `host-openings`, `stair-body-plan`, `templates-presets`, `drafting-trim-split`, `doors-windows-core`, `solid-kernel-ops`, `roofs`, `sections-elevations-dims`, `curved-geometry`, `free-form-solids`, `terrain-and-site`, `cleanup-and-trims`, `library-dw-exchange`, `railings-stairs-adv`, `3d-display-styles`, `members-profiles-adv`, `could-scope-polish`, `reference-models`, `curtain-walls`, `release-candidate`, `perf-tuning-rm-m-l`

## Epics

### 1. `element-kind-framework` (archicraft-0n41) — Extensible element kinds, relations, compound parts and incremental evaluation

- **Increment:** 1 · **Priority:** 1 · **Size:** 15 days · **Requirements:** 14 (13 M / 0 S / 0 C / 1 done)
- **Labels:** increment-1, archicraft-model, archicraft-geometry, archicraft-ifcx, archicraft-app, architecture
- **Depends on:** nothing
- **SRS IDs:** ELM-011 ELM-006 STR-006 IFX-012 IFX-060 IFX-062 IFX-024 IFX-016 NFR-ACC-003 NFR-PERF-006 NFR-REL-005 NFR-REL-006 VIEW-001 UI-012

**Goal.** Replace the closed ElementKind/KindTag/IfcClass/NameCounters/ToolDefaults enums matched in about fifteen places across all four crates with a per-kind registry (KindDescriptor: label, code, name prefix, IFC class set and predefined types, defaults slot, validator, geometry generator, transform hook, part list, IFCX attribute codec), add the relation store of SRS 3.3 (host, junction, trim, solid operation, annotation binding, storey link) to Project with dependency queries and integrity checks, and make geometry evaluation incremental (dirty set = edited elements plus relation and contact neighbours). The same epic closes the increment-0 residuals that live in this layer: the full D.5 class list with predefined types in the archicraft namespace (IFX-024), the generic compound-part codec that writes child nodes with their own IFC class (IFX-016, first consumed by roofs and stairs), one coincidence tolerance module replacing the five ad-hoc constants (NFR-ACC-003), storey commands that validate the elements linked to them (NFR-REL-005), the fallback representation for failed elements (NFR-REL-006), the clickable problems panel (UI-012) and format-version migration (IFX-062). It also ships the seeded fixture builder (v5 UUIDs from fixture names) and synthetic 1,000/5,000/20,000-element stand-ins that the regression epic benchmarks. Built first because columns, beams, openings, roofs, stairs, terrain, grids and 2D elements all register through it.

**Acceptance.** A module-layout test adds a test-only dummy kind and asserts that only its own module plus one registry entry changed; a test-only compound kind writes Part_n children with their own IfcClass and v5 ids, round-trips and passes the conformance job (IFX-016 mechanism; roofs and stairs re-assert it with real elements); IfcClass covers the full D.5 list and slab/footing predefined types round-trip in archicraft:: (IFX-024); after one edit in the 5,000-element stand-in the dirty set contains only the edited element and its relation/contact neighbours and a property test compares incremental against full evaluation on random edit sequences (bit-identical); MODEL_TOLERANCE (0.01 mm) is the only tolerance constant in model, geometry and snapping (grep test); moving a storey below a linked wall's base is rejected with CommandError::Invalid; a deliberately panicking test kind renders as a fallback box, is pickable and appears in the problems panel whose click selects and zooms (kittest); a FORMAT_VERSION n-1 file migrates with a report; all existing golden files, round-trip tests and the conformance job stay green.

**Design notes.** archicraft-model: element.rs split into kinds/{wall,slab}.rs behind a KindDescriptor table (kinds/mod.rs); relations.rs with typed edges keyed by ElementId and integrity_problems(); tolerance.rs exporting MODEL_TOLERANCE with JOIN_TOLERANCE derived from it; NameCounters becomes a map keyed by kind code (schema.rs counters object opened); samples.rs grows a public builder with deterministic v5 ids and stand_in(n). archicraft-geometry: evaluate.rs gains evaluate_incremental(dirty) with per-element Arc<ElementGeometry> keyed by element revision; wall.rs/mesh.rs/slice.rs/plan.rs read the shared tolerance. archicraft-ifcx: per-kind attribute codec registered in alpha/schema.rs, alpha/writer.rs and alpha/reader.rs; parts() of a kind written as child nodes with class and v5 ids (generalises the Skin_n mechanism of IFX-017); migration table in alpha/mod.rs; predefined types declared in registry.rs. archicraft-app: document.rs caches per element and per storey instead of dropping all caches; ui/dialogs.rs problems panel becomes clickable. Headless crates stay free of UI dependencies (NFR-MNT-002/004); the registry is a plain table, not a plugin system (Section 8).

**Risks.** Largest refactor of the codebase; reduced by doing it before any new kind exists and by using the round-trip, golden, geometry and kittest suites as the net. Missed invalidations in incremental evaluation are caught by the incremental-versus-full property test. Fourteen requirements in 15 days only works because most are S-effort residuals; the beads split is registry+relations+codec first, evaluation+tolerance+residuals second.

### 2. `point-input-snapping` (archicraft-nimm) — Shared point-input service, snapping, guides and construction grid

- **Increment:** 1 · **Priority:** 1 · **Size:** 15 days · **Requirements:** 22 (13 M / 8 S / 0 C / 1 done)
- **Labels:** increment-1, archicraft-app, input
- **Depends on:** nothing
- **SRS IDs:** INP-001 INP-002 INP-003 INP-004 INP-005 INP-006 INP-007 INP-008 INP-009 INP-010 INP-011 INP-012 INP-013 INP-014 INP-017 INP-020 INP-021 INP-022 INP-023 INP-024 UND-003 NFR-PERF-005

**Goal.** Extract the pipeline inlined in views/plan.rs into one headless PointInput service (typed value → lock → constraint → snap) used by every creation, transform, reshape, measuring and dimensioning step, and complete snapping (full target set, precedence, cycling, division points, inference guides with dwell, centroids, exclusions), locks, Cartesian and absolute entry with user origin, arithmetic, persistent guide lines, live context distances, click-click/drag preference and Backspace.

**Acceptance.** AS-04 automated: a closed wall polygon drawn purely by typed polar and relative Cartesian values measures back exactly (0.1 mm, 0.01°); unit tests for the precedence node > intersection > division > guide intersection > edge > guide > grid, Tab cycling, X/Y/distance/angle locks drawn as guides, dwell-marked references producing parallel/perpendicular/bisector guides, all division modes and centroid snaps; Esc cancels one step and Backspace one point; input feedback under 50 ms on two_storey_shell (frame-time assertion); kittest covers typing during input. The step/preview API is exercised by two consumers in this epic's tests (the wall tool and a transform stub) so that selection-transforms and 3d-input-navigation plug in without changing it.

**Design notes.** archicraft-app input/ module: a state machine without egui (tested like tools.rs), SnapTargets built from an rstar index (geo) updated incrementally instead of per frame, snap glyph set in plan.rs, guides and user origin in per-view state (guide lines excluded from IFCX), live distances from a generic target provider (walls now, grids/columns/beams when they exist). A ViewSurface trait implemented by the plan view now and by the 3D working plane later. Preferences gain snap radius, dwell, angle increment and input mode. Contour detection (INP-018) is not here: it lives in edit-reshape-palette on the polygon machinery.

**Risks.** The service must serve transforms and 3D without a second rewrite; the step/preview API is defined against EDT-001 and INP-015 use cases up front. Removes the per-frame snap-target rebuild debt.

### 3. `selection-transforms` (archicraft-r3rc) — Selection model, part selection (edit mode) and transform commands

- **Increment:** 1 · **Priority:** 1 · **Size:** 15 days · **Requirements:** 17 (13 M / 3 S / 1 C / 0 done)
- **Labels:** increment-1, archicraft-model, archicraft-app, editing, selection
- **Depends on:** `point-input-snapping`, `element-kind-framework`
- **SRS IDs:** SEL-001 SEL-002 SEL-003 SEL-004 SEL-007 SEL-008 VIEW-045 ELM-013 ELM-007 EDT-001 EDT-002 EDT-003 EDT-006 EDT-007 EDT-016 EDT-021 IFX-011

**Goal.** Complete picking (edges and nodes, area shapes and containment modes, view filters, locked ghosting, selection kept across views), introduce the generic part-selection infrastructure of SEL-008 (Selection holds (ElementId, Option<PartRef>), PartRef is an extensible enum registered per kind, an EditMode state enters and leaves with part highlighting) proven on slab outline nodes and wall skins before the first compound element exists, and give EDT its first user-facing commands: move, rotate, mirror and vertical move with copy and multi-copy variants, rotate plus/minus 90 degrees, resize, cut/copy/paste, re-association after vertical moves, applicability messages for transforms a kind cannot represent, align/distribute and new identities for copies. Transforms are planned through the per-kind transform hook of the kind registry, hence the dependency on element-kind-framework.

**Acceptance.** commands.rs tests for Transform{Move,Rotate,Mirror,VerticalMove} with copy variants and undo for every registered kind (a test iterates the registry); kittest: marquee in the three containment modes, Tab cycles candidates, Shift-click toggles, a locked element shows ghost handles and the hover reason; edit mode on a slab selects a single outline node and on a layered wall a single skin, leaves with Esc, and later compound kinds register their PartRef variants without touching selection code (module-layout test); copies get new UUIDs (round-trip test); rotating a wall about a horizontal axis is refused with a message naming the kind (EDT-007 applicability clause: the registry transform hook returns Unsupported; free-form solids and imported elements assert any-axis rotation and objects vertical-axis rotation in their own epics through the same hook); paste onto another storey keeps the relative elevation and offers re-association; zoom to selection works plan to 3D and back.

**Design notes.** archicraft-model: Command::Transform family planned into ReplaceElement edits through per-kind transform(&Isometry) from the kind registry; Project::reidentify for copies; clipboard as serialised elements plus referenced resources (prepares SW-003); PartRef enum and EditMode in a selection module shared with the app. archicraft-app: workspace.rs Selection gains part references; tools/transform.rs steps on the point-input service; cursor variant glyphs; candidates from plan segments and 3D picking through an id buffer in views/render3d.rs (wires the dead HOVER_TINT path); actions for the D.8 transform shortcuts.

**Risks.** Per-frame 3D hover picking is costly: pick on pointer move only, through the id buffer, never by ray casting per frame. Mirroring parametric elements must mirror parameters, not meshes, covered by per-kind transform tests. The PartRef design must serve stairs, segments, grid axes, curtain walls and free-form solids: it is a kind-registered enum with stable addressing (segment index plus role), never mesh indices.

### 4. `edit-reshape-palette` (archicraft-xhsl) — Node/edge edit palette, offsets, polygon editing, contour detection and validation

- **Increment:** 1 · **Priority:** 1 · **Size:** 13 days · **Requirements:** 12 (8 M / 4 S / 0 C / 0 done)
- **Labels:** increment-1, archicraft-model, archicraft-app, editing, input
- **Depends on:** `point-input-snapping`, `selection-transforms`
- **SRS IDs:** EDT-008 EDT-009 EDT-010 EDT-023 EDT-015 EDT-018 EDT-019 WAL-033 SLB-003 SLB-005 SLB-008 INP-018

**Goal.** Direct manipulation of element geometry: a contextual palette on nodes and edges, stretching with numeric input, offset edge/all edges and offset copy, chamfer, polygon union and difference, wall face and endpoint dragging, slab holes, edge angles and elevation dragging, with polygon validation and tolerance repair on every edit; plus contour detection (INP-018): with a creation tool active a key switches to detection, and clicking previews and creates an element of the active kind from the clicked edge, a chain of connected edges or the smallest closed region of the plan segments (walls, slabs and, once they exist, 2D lines). Fillet (EDT-014) waits for the arc edge type in curved-geometry.

**Acceptance.** kittest: clicking a selected slab node opens the palette with move/insert/delete, an edge with offset/offset-all/curve/straighten (curve is a stub until curved-geometry); Command::Reshape tests for every palette command with undo; dragging a wall face changes the thickness while the reference line stays (WAL-004 invariant); union and difference with a drawn polygon; a hole traced by contour detection from four interior walls of two_storey_shell subtracts exactly their enclosed region and a slab is created from the smallest closed region inside the exterior ring; validation tests: duplicate vertices removed, self-intersections rejected with a message, edges under 0.1 mm repaired or listed in the problems panel; offset copy of a joined wall chain stays joined.

**Design notes.** archicraft-model: one Outline type shared by slabs, roofs and 2D geometry with operations in validate.rs on geo (union/difference via i_overlay, buffer for offsets); Command::Reshape{target: PartRef, op} addressing node and edge indices through the PartRef of selection-transforms. archicraft-geometry: contour.rs builds the planar graph of the storey's plan segments with geo and finds minimal cycles and edge chains (reused by spaces and roof pivot polygons). archicraft-app: tools/reshape.rs on the point-input service, the palette as an egui popup anchored at the part, the detection key in the tool state machine. Arc edges are prepared in Outline and implemented in curved-geometry.

**Risks.** Handle hit-testing and palette ergonomics; the NFR-USE-002 rule (same commands for every polygon-based kind) is met by building everything on the one Outline type. Contour detection on near-coincident segments is numerically fragile: it uses MODEL_TOLERANCE and is covered by F-style fixtures.

### 5. `plan-attributes` (archicraft-wobl) — Graphic attribute resources: pens, line types, fills and material plan display

- **Increment:** 1 · **Priority:** 1 · **Size:** 10 days · **Requirements:** 14 (9 M / 4 S / 1 C / 0 done)
- **Labels:** increment-1, archicraft-model, archicraft-ifcx, archicraft-app, display
- **Depends on:** `element-kind-framework`
- **SRS IDs:** DSP-001 DSP-002 DSP-003 DSP-005 DSP-006 DSP-007 DSP-008 DSP-009 DSP-010 DSP-011 DSP-020 MAT-001 MAT-002 MAT-003

**Goal.** Introduce project-level graphic attributes (pens by role, line types, solid and hatch fills with roles) with stable IDs, references from elements and materials, a complete default set, in-use protection, editors, and material cut fills as real fill references, so that the plan renderer and every later element kind draw from one attribute system.

**Acceptance.** The template contains the DSP-005 role pen table with the six weights, at least six line types and eight hatch fills (template test); each of the 25 materials references a cut fill and the D.4 priority; renaming an attribute keeps references (ID test); deleting an in-use attribute requires a replacement (command test); editing a material's cut fill changes every element's plan output (plan regression); pen/line-type/fill dialogs with kittest labels; attributes stored in archicraft::project round-trip.

**Design notes.** archicraft-model attributes.rs (Pen, LineType, Fill{Solid, Hatch{angle, spacing, pen}} with roles) and ids via id_type!, Material.cut_fill becomes a fill reference plus pens; archicraft-ifcx project attribute schema extension with a format-version bump through the framework migration; archicraft-app dialogs following the Materials dialog pattern.

**Risks.** A large template change: golden files are re-blessed once with reviewed diffs.

### 6. `drafting-trim-split` (archicraft-sre3) — 2D construction geometry with the shared arc type, trim, extend, split and array

- **Increment:** 1 · **Priority:** 1 · **Size:** 13 days · **Requirements:** 13 (8 M / 3 S / 2 C / 0 done)
- **Labels:** increment-1, archicraft-model, archicraft-geometry, archicraft-app, drafting
- **Depends on:** `element-kind-framework`, `point-input-snapping`, `plan-attributes`, `selection-transforms`, `edit-reshape-palette`
- **SRS IDs:** ANN-001 ANN-002 ANN-003 ANN-010 ANN-011 EDT-004 EDT-005 EDT-011 EDT-012 EDT-013 EDT-020 WAL-030 WAL-031

**Goal.** Add the 2D reference-element kinds (lines, chains, polylines, arcs, circles, then rectangles, splines, tangent arcs and drafting fills) bound to a view and a layer, usable as tracing input and as cutters, together with the exact Arc/PolyCurve type and the single tessellate(curve, settings) function that arc walls, slab rings, plan cuts and the IFCX writer reuse later (so curved-geometry and fixture F8 are not a retrofit), and the linear editing commands: trim, extend, split, explicit-command variants of every gesture (D-7), arrays and path distribution, plus wall split and merge. Join/unjoin (WAL-022) lives with the junction engine.

**Acceptance.** 2D elements round-trip in the archicraft:: namespace and optionally as IfcAnnotation curves; a three-point arc stores centre, radius and sweep exactly and tessellates per the IFX-034 settings, and two producers tessellating the same curve id yield bit-identical vertex lists; 2D elements register as a segment source of the contour detection built in edit-reshape-palette, so a slab is traced from a closed chain of 2D lines in one pick (ANN-002 tracing clause); trimming a wall back to the nearest intersecting wall or 2D line, extending to a picked wall face and splitting a slab or wall by a two-point line keep the chosen part selected (fixtures); array tests for linear, radial, vertical and rectangular arrays with all four methods; splitting a wall yields two joined walls that keep their hosted elements (hook re-run by doors-windows-core); every gesture has a menu command with a preview (inspection checklist); the 2D tool group appears in the toolbox with D.8 shortcuts (kittest).

**Design notes.** archicraft-model: curve.rs (Arc, PolyCurve with straight and arc segments, exact storage) and kinds/drafting.rs (Line, Polyline, Arc, Circle, Fill) with view binding and pen/line-type references, registered as a PlanSegmentSource for contour.rs (hence the dependency on edit-reshape-palette, whose Outline operations the cutters also reuse); archicraft-geometry: tessellate(curve, settings) reading ProjectSettings.tessellation (so far unused), intersection utilities in primitives, reshape.rs with segment intersection search on geo; archicraft-app: tools/drafting.rs and tools/linear_edit.rs with the cutter pick step through the point-input service; decision D-7: explicit commands first, modifier gestures as accelerators.

**Risks.** Splines are a rabbit hole: v1 tessellates Catmull-Rom/Bezier through points with node editing only. WAL-030's hosted-element clause can only be verified once doors exist; a test hook is prepared and re-run by doors-windows-core through the relation store. Arc parameters are written with float_roundtrip so round trips stay exact (NFR-ACC-002).

### 7. `plan-renderer` (archicraft-dbc9) — GPU plan renderer with drawing scale, line weights, hatches, skin separators and golden images

- **Increment:** 1 · **Priority:** 1 · **Size:** 12 days · **Requirements:** 8 (2 M / 5 S / 1 C / 0 done)
- **Labels:** increment-1, archicraft-app, display, rendering
- **Depends on:** `plan-attributes`, `layer-structures`
- **SRS IDs:** DSP-004 DSP-012 DSP-013 DSP-014 DSP-017 DSP-018 DSP-019 VIEW-013

**Goal.** Replace the egui-painter plan drawing with a lyon + wgpu renderer: a drawing scale per 2D view, paper-millimetre weights resolved at scale with hairline and uniform-thin option, anti-aliased lines with caps and joins, hatches without moire with the 3-pixel flat fallback, the layered-element drawing rules (same-material regions merged without seams, skin separator lines with their own pen switchable per view, and the Could option to show layered elements full, core only or without finishes) and cross-platform golden-image tests.

**Acceptance.** egui_kittest wgpu snapshot tests of fixture plans pass on all three CI platforms within the per-pixel tolerance documented in docs/testing.md, rendered on a software adapter (lavapipe on Linux, WARP on Windows) in CI; cut lines are heavier than projected lines at every zoom and no line is thinner than one device pixel (pixel assertions); hatch spacing below 3 px renders as a tone; the hard-coded 'Scale 1:100' label becomes a per-view drawing scale in ViewState that zooming never changes (VIEW-013; the ViewDefinition of plan-views-browser persists it); the cores of joined same-material walls draw without a seam while the F2 wall shows six skin regions with separator lines in the separator pen that a per-view switch hides; the core-only view option hides finish skins.

**Design notes.** archicraft-app views/plan_gpu.rs tessellates PlanDrawing into vertex buffers with lyon (stroke widths = paper mm x scale x dpi), cached per revision and storey; hatches as instanced line batches clipped to regions; the egui painter in views/plan.rs stays for overlays (snap markers, rubber bands); PlanOptions per view carry the separator and layered-display switches consumed by archicraft-geometry plan.rs when it emits skin regions; wgpu through the egui_wgpu re-export.

**Risks.** Platform rendering differences in snapshot tests: tolerance plus a software adapter in CI; lyon/wgpu version coupling handled by the pinned egui family.

### 8. `plan-views-browser` (archicraft-j3me) — Plan view range and display ranges, saved views, project browser, layers and layout persistence

- **Increment:** 1 · **Priority:** 1 · **Size:** 15 days · **Requirements:** 16 (10 M / 6 S / 0 C / 0 done)
- **Labels:** increment-1, archicraft-model, archicraft-geometry, archicraft-ifcx, archicraft-app, views
- **Depends on:** `element-kind-framework`
- **SRS IDs:** VIEW-010 VIEW-011 VIEW-012 ELM-008 ELM-009 ELM-018 VIEW-003 VIEW-004 VIEW-060 VIEW-015 LAY-001 LAY-002 LAY-003 UI-004 PRJ-007 SET-002

**Goal.** Make the plan generator cut and project every element intersecting a storey's view range regardless of home storey, add the per-element storey display range with the remote-storey style, per-view cut-plane and range overrides, per-kind plan display options (projected, with overhead, cut only) and the overhead class switch, so that every later kind (columns, beams, roofs, stairs) gets correct plan output for free; and add the ViewDefinition resource (camera or zoom and rotation, scale, layer states or combination, kind filters, storey range, cut planes, hidden elements, style, sun), folders, redefine and open semantics, the full project browser with context menus, layer combinations, hidden/locked-layer warnings, view rotation, the trace underlay and per-user persistence of the tile layout. The optional sections of ViewDefinition are filled by 3d-view-restrictions and 3d-display-styles.

**Acceptance.** Geometry test in crates/archicraft-geometry/tests/storey.rs: a wall on storey n-1 reaching into storey n's range is cut on n and drawn remote-style (regression for the known plan.rs debt); each display-range mode produces the expected drawing on every storey (fixture); the overhead class can be switched off per view; a per-view cut-plane override round-trips; ELM-018 options change the classification per kind in tests; saving, redefining and opening a view re-applies its state to the live model (test); views and layer combinations persist in archicraft::project and round-trip; the browser lists storeys highest first, 3D views and saved views in folders, double-click opens a tab, the context menu offers rename, delete, settings and save-as-view (kittest); a rotated plan view keeps snapping and constraints aligned to the rotated axes; the trace underlay with opacity and offset toggles with one command; placing on a hidden layer warns; the egui_tiles tree is serialised in preferences and restored.

**Design notes.** archicraft-geometry plan.rs iterates a z-extent index of all elements with PlanOptions per view (range, cut height, overhead on/off) instead of elements_on(storey); archicraft-model DisplayRange on Element, the remote style in ProjectSettings, views.rs (ViewDefinition as a superset of View3d with optional sections for restrictions and styles) and LayerCombination; archicraft-app per-pane ViewState mapped to ViewDefinition, Tree<Pane> serde through egui_tiles persistence in persistence/prefs.rs, ui/browser.rs rewritten on an egui_extras tree with Behavior::tab_ui context menus; the HashMap-order tab insertion heuristic is removed.

**Risks.** Cross-storey evaluation cost relies on the incremental per-element cache of the framework epic. VIEW-004 is a union of many later features; the data model is defined now with optional sections to avoid repeated schema churn. Saved views referencing deleted layers or storeys re-apply tolerantly with a report.

### 9. `layer-structures` (archicraft-djxf) — Layer structures, skins, core faces and multi-skin bodies

- **Increment:** 1 · **Priority:** 1 · **Size:** 12 days · **Requirements:** 11 (9 M / 2 S / 0 C / 0 done)
- **Labels:** increment-1, archicraft-model, archicraft-geometry, archicraft-ifcx, archicraft-app, walls
- **Depends on:** `element-kind-framework`
- **SRS IDs:** PRF-001 PRF-002 PRF-003 WAL-003 WAL-004 WAL-005 WAL-006 SLB-002 SLB-009 SLB-006 IFX-017

**Goal.** Introduce the LayerStructure resource (ordered skins with material, thickness and role, at least 48), use it in walls and slabs with core-face reference positions, structure flips and swaps, slab core reference planes and sloped or tapered slabs, and write multi-material bodies as Skin_n parts; the prerequisite of the junction engine, doors (reveals) and roofs.

**Acceptance.** Fixture F2 (brick 120 / air 60 / insulation 180 / plaster 10 / concrete 300 / plaster 10 = 680 mm) builds, cuts in plan with one region per skin and exports six Skin parts each naming its material (conformance plus viewer check); fixture F3 floating floor; editing a structure updates all users (command test); core-face reference-line locations, move-reference-line and flip commands keep the body in place (geometry tests); sloped constant-thickness and tapered slab bodies are closed. F2 and F3 are registered under their ids in the fixture registry of regression-and-status.

**Design notes.** archicraft-model LayerStructure resource and Structure::{Thickness, Layered(id)} on Wall and Slab; archicraft-geometry wall footprint per skin as offset bands along the reference line, extrusion per skin, slab skins stacked; archicraft-ifcx Skin_i child parts with v5 ids and an archicraft::layerStructure schema; the template gains the D.2 structures. Skin ends stay square at junctions in this epic; junction-engine resolves them and asserts F2 again.

**Risks.** Per-skin geometry multiplies mesh count; kept per element in the incremental cache and merged for display where materials match.

### 10. `profiles-columns-beams` (archicraft-xvmz) — Profiles, columns and beams

- **Increment:** 1 · **Priority:** 1 · **Size:** 14 days · **Requirements:** 12 (9 M / 3 S / 0 C / 0 done)
- **Labels:** increment-1, archicraft-model, archicraft-geometry, archicraft-ifcx, archicraft-app, structure
- **Depends on:** `element-kind-framework`, `point-input-snapping`
- **SRS IDs:** PRF-004 PRF-005 PRF-011 COL-001 COL-002 COL-003 BEA-001 BEA-002 BEA-008 BEA-010 WAL-007 IFX-035

**Goal.** Add the Profile resource (parametric standard shapes and a steel catalogue) and the column and beam kinds with anchors, end cuts, inclination by end elevations, chain input, plan representation and axis export, as the first new element kinds built through the registry and the first consumers of profiles.

**Acceptance.** D.2 column and beam defaults in the template; a column is placed with one click plus an optional rotation click, a beam between two points and as a chain; end-cut angles editable numerically and by handle; the profile editor produces closed cross-sections for rectangle, circle, hollow sections, I/H, T, L, U/C and Z with area regression tests and a catalogue of IPE/HEA/HEB/HEM/UPE/L/W/C/HSS sections; IfcColumn and IfcBeam with Axis basiscurves pass conformance; beams below and above the cut plane are classified projected and overhead with correct angled end cuts.

**Design notes.** archicraft-model kinds/column.rs, kinds/beam.rs and profile.rs (regions with holes and materials, insertion origin, kind flags, catalogue data file); archicraft-geometry sweep.rs sweeps a profile along a segment and cuts the ends with its own convex plane clipper (no Boolean kernel needed for planar end cuts), caps triangulated with triangulate.rs; archicraft-ifcx archicraft::column|beam|profile codecs and Axis basiscurves; archicraft-app tools per the tool-extension recipe. Cross-kind cleanup by priority (COL-006, BEA-004) follows in cleanup-and-trims.

**Risks.** Profile editor scope creep: user-drawn profiles use the polygon input of the point-input service; creation from geometry and stretch zones belong to members-profiles-adv.

### 11. `junction-engine` (archicraft-ehhx) — Priority-based wall junction engine on per-skin footprints (F1, F6, F7)

- **Increment:** 1 · **Priority:** 1 · **Size:** 15 days · **Requirements:** 12 (8 M / 4 S / 0 C / 0 done)
- **Labels:** increment-1, archicraft-geometry, archicraft-model, walls, junctions
- **Depends on:** `layer-structures`
- **SRS IDs:** WAL-020 WAL-021 WAL-022 WAL-023 WAL-024 WAL-025 WAL-026 WAL-027 WAL-036 WAL-008 IFX-033 NFR-MNT-005

**Goal.** Replace the L/T-only mitre in archicraft-geometry wall.rs with a node-based junction engine over 2D per-skin footprints: X crossings, three or more walls per node with the documented deterministic rule and per-wall junction order, skins trimmed by material priority with per-skin overrides, join groups with a reserved never-join value, auto-join on placement and move, explicit join and unjoin commands, trapezoidal walls (fixture F1), agreement of plan, 3D and IFCX by slicing the resolved bodies, and the guarantee that exported geometry is the evaluated result of the cleanup (IFX-033, re-asserted by openings, trims and solid operations as they arrive). Built on geo/i_overlay polygon Booleans only, so the increment-1 exit criterion does not wait for the C++ Boolean kernel; cross-kind 3D cleanup (columns, beams, slabs) is cleanup-and-trims. Owns the NFR-MNT-005 regression suite.

**Acceptance.** Fixtures F1 (300 to 500 mm trapezoidal wall with the reference-side face straight), F6 (junction orders 8/8/8 versus 8/8/7) and F7 (concrete 880 and insulation 400 priorities, then swapped) pass as geometry tests with golden skin outlines and closed bodies; a property test shuffles creation order over three to four walls at a node and gets identical meshes (WAL-026); X-junction and collinear continuation tests; a wall end moved within snap tolerance of another reference line auto-joins and moving it away unjoins; join on two selected walls extends or trims to the reference-line meeting point and unjoin separates them through the junction relation; a wall in a never-join group stays square; same-material same-priority skins merge into one plan region; the written mesh equals the evaluated resolved body (hash test, IFX-033); the suite enumerates every junction case of Section 4 (L, T, X, 3-way, equal priority, priority inversion, core-face reference lines, openings at ends) as a table-driven test in CI; the increment-1 exit 'F2, F6, F7 pass' is green.

**Design notes.** archicraft-geometry junction.rs: node graph from reference-line contact within MODEL_TOLERANCE (decision D-6), per-node resolution producing per-skin polygons with geo/i_overlay Booleans in priority order, then extrusion; footprints become general polygons (prerequisite for arc walls); plan regions are slices of the resolved bodies (WAL-027). archicraft-model: JoinGroup and junction_order on Wall, Wall.end_thickness for WAL-008, commands JoinWalls/UnjoinWalls; junction edges recorded in the relation store. Determinism rule documented in docs/ and asserted by tests; fixtures f1/f6/f7 registered in crates/archicraft-model/tests/fixtures.rs.

**Risks.** The hardest geometry in v1 and the increment-1 exit criterion; reduced by writing F1/F6/F7 first and by falling back to square ends plus a reported problem instead of corrupt geometry (NFR-REL-005). Combinatorial growth at nodes is bounded by local per-node resolution.

### 12. `host-openings` (archicraft-3mbu) — Host relation and the opening element (2D cuts in the host plane)

- **Increment:** 1 · **Priority:** 1 · **Size:** 12 days · **Requirements:** 11 (9 M / 2 S / 0 C / 0 done)
- **Labels:** increment-1, archicraft-model, archicraft-geometry, archicraft-ifcx, archicraft-app, openings
- **Depends on:** `element-kind-framework`, `layer-structures`, `point-input-snapping`, `3d-input-navigation`
- **SRS IDs:** OPN-001 OPN-003 OPN-004 OPN-005 OPN-006 OPN-007 OPN-009 ELM-016 IFX-015 IFX-036 NFR-REL-004

**Goal.** Implement the hosted-element framework (host relation with cascade delete and copy, host picking with live preview in plan and, through the ViewSurface trait, in 3D, Void child nodes) and the generic single-host opening with rectangular, circular and polygonal cross-sections, perpendicular or vertical orientation, through or limited depth, storey-spanning shafts through several slabs and plan symbols, cut as 2D polygons in the wall's own plane or as slab holes before extrusion (technology-choices), so no Boolean kernel is needed; openings that cut further elements (OPN-002) arrive with the kernel in solid-kernel-ops. The base that doors, windows, roof openings and beam notches reuse.

**Acceptance.** The niche acceptance (limited opening 0 to 0.40 m in a 0.50 m wall leaves 0.10 m) as a geometry test; a 1.0 x 1.0 m opening in the F2 wall removes the volume from every skin (signed-volume check); an opening moves and is deleted with its host and copied with it (command tests with undo); a shaft from storey -1 to +2 cuts all slabs in range and follows a storey elevation change; the tool creates nothing without a host under the pointer (kittest) and the preview follows the host in plan and in a 3D view through the Camera3d ViewSurface (OPN-007, hence the dependency on 3d-input-navigation); IFCX: the opening is a child of its host with an invisible closed Void mesh (conformance and viewer); a file with an orphaned hosted element is repaired deterministically and reported on load; an opening at a wall end adjacent to a junction is added to the NFR-MNT-005 suite.

**Design notes.** archicraft-model Host relation in the relation store and kinds/opening.rs with placement relative to the host face; archicraft-geometry cuts the void polygon in the wall plane per skin band before extrusion (extrude.rs gains holes in vertical faces), slab openings as outline holes, the Void prism kept as a separate part; incremental evaluation marks host and hosted elements dirty; archicraft-ifcx hosted children (IFX-015) and Void v5 sub-nodes (IFX-036); archicraft-app tools/opening.rs with host hover candidates and preview.

**Risks.** Niches and openings crossing skin boundaries need robust 2D subtraction: geo Booleans with MODEL_TOLERANCE and validation of every resulting ring. Roofs as hosts are verified again in the roofs epic.

### 13. `components-dw-catalogue` (archicraft-1sg) — Component definitions and the door/window catalogue

- **Increment:** 1 · **Priority:** 1 · **Size:** 15 days · **Requirements:** 15 (7 M / 8 S / 0 C / 0 done)
- **Labels:** increment-1, archicraft-model, archicraft-geometry, archicraft-ifcx, archicraft-app, components
- **Depends on:** `element-kind-framework`
- **SRS IDs:** OBJ-002 OBJ-004 OBJ-005 OBJ-006 OBJ-011 OBJ-014 IFX-037 IFX-050 IFX-051 DW-011 DW-012 DW-013 DW-017 DW-022 DSP-015

**Goal.** Build the component-definition resource (ID, version, category, parameter schema, parametric 3D geometry and plan symbol, detail levels, key points), its storage in the project file, placeholders for missing definitions and shared type geometry in IFCX, then the built-in door, window and empty-opening catalogue with the DW-012 parameter set, generated plan symbols and handedness conventions.

**Acceptance.** Catalogue tests instantiate every DW-011 type with default parameters into closed part meshes (frame, leaf or sash, glazing, sill) and generated plan symbols at two detail levels; a project with a missing definition opens with labelled placeholders and a relink list; identical instances share one type node and mesh in IFCX and mirrored ones a separate type (conformance); DIN 107 handedness label test; definitions embedded in archicraft::project so the project opens on any machine.

**Design notes.** archicraft-model components.rs (Definition, ParameterSchema, instance parameters) with the built-in catalogue as Rust generators (no scripting, O-12); archicraft-geometry part generators per door and window family sharing one frame/leaf kit; archicraft-ifcx type nodes plus occurrence inherits and archicraft::component; archicraft-app catalogue basics (the full browser is in library-objects-dw).

**Risks.** DW-011 breadth (multi-sash grids, sidelights, transoms); generators share one kit and keep part geometry to boxes in v1.

### 14. `doors-windows-core` (archicraft-5il4) — Doors and windows in walls

- **Increment:** 1 · **Priority:** 1 · **Size:** 13 days · **Requirements:** 12 (11 M / 1 S / 0 C / 0 done)
- **Labels:** increment-1, archicraft-model, archicraft-geometry, archicraft-ifcx, archicraft-app, openings
- **Depends on:** `host-openings`, `components-dw-catalogue`, `layer-structures`
- **SRS IDs:** DW-001 DW-002 DW-003 DW-004 DW-005 DW-006 DW-007 DW-008 DW-009 DW-010 DW-018 VIEW-055

**Goal.** Place doors and windows as hosted elements with host-candidate cycling, exterior side, opening configuration, anchors, reveal offsets from faces or core faces, pinned-edge vertical position, live width and height edits, empty openings, IfcDoor/IfcWindow export with Void and parts, and the view option to hide them or show schematic versus detailed. Units in arc walls (DW-019) follow in curved-geometry.

**Acceptance.** The tool creates nothing without a host and says so (kittest); placement next to a junction cycles hosts with one key; the void cuts all skins of an F2 wall and the host is restored on delete (geometry tests with undo); resizing keeps the pinned edge and the anchor; sill and head references to any storey follow storey changes; splitting a host wall keeps the door on the right part (WAL-030 hook); IfcDoor/IfcWindow occurrences with Void children and material parts pass conformance; the AS-01 'walls show openings' check passes in the viewer; a ground floor with openings is drawn precisely end to end (increment-1 exit scenario scripted under docs/).

**Design notes.** archicraft-model kinds/door.rs and kinds/window.rs sharing a WallInsert placement (host relation, anchor, reveal, vertical pin) bound to a catalogue definition; archicraft-geometry void from the insert rectangle cut in the wall plane per skin (the host-openings path), parts placed by the host transform; plan symbols generated from parameters as renderer-agnostic primitives; archicraft-ifcx hosted children plus type occurrences; archicraft-app tools/insert.rs on the point-input service.

**Risks.** Interaction with the junction engine at wall ends; fixtures with doors at L and T corners are added to the NFR-MNT-005 suite.

### 15. `properties-and-dialogs` (archicraft-31wm) — Properties panel, settings-dialog framework, reference levels and datum display

- **Increment:** 1 · **Priority:** 1 · **Size:** 12 days · **Requirements:** 15 (7 M / 6 S / 1 C / 1 done)
- **Labels:** increment-1, archicraft-app, archicraft-model, ui
- **Depends on:** `element-kind-framework`
- **SRS IDs:** UI-002 UI-003 EDT-017 ELM-001 ELM-005 ELM-012 ELM-017 ELM-019 ELM-020 MEA-003 STR-002 UI-014 UI-015 STR-007 STR-011

**Goal.** Generalise the properties strip and settings dialog into a declarative field framework driven by the kind registry: collapsible sections, live preview, one dialog for tool defaults and selections, mixed-state fields for multi-selection, range checks with live dependent values, configurable ID prefixes, position and structural flags, derived values, incremental search and resizable dialogs that remember their size. Also adds named reference levels (project zero, sea level, user levels) with the single vertical datum so every elevation field can display relative to a chosen level while storage stays relative to project zero (STR-007, STR-011).

**Acceptance.** kittest: a mixed selection shows only common fields with mixed markers and 'n selected, m editable'; committing applies to every editable element; every registered kind has a dialog with the four sections and a preview (a test iterates the registry); an invalid value shows its explanation and leaves the model untouched; W-001-style IDs with a configurable prefix; derived values for walls, slabs, columns and beams; lists longer than one screen get a search field; the storey-height default shifts the storeys above. A datum 'sea level +412.000' shows a storey at +3.000 as +415.000 and typing +415.000 stores 3.000 (unit tests on widgets::length); the datum is written once to IFCX and never asked for twice.

**Design notes.** archicraft-app ui/fields.rs: FieldSpec per kind from the registry (getter, setter, range, unit) rendered by both the strip and the dialog; preview through a small offscreen render3d scene; dialog sizes in preferences; archicraft-model position and structural flags on Element and ID counters per kind. archicraft-model: ReferenceLevel list in ProjectSettings and datum-aware formatting in units.rs used by ui/widgets.rs.

**Risks.** egui dialog ergonomics and size persistence; layout data kept in preferences, not in egui memory side effects. Datum conversion errors leaking into storage are prevented by converting only in widgets, asserted by round-trip tests.

### 16. `commands-and-l10n` (archicraft-g1b) — Command registry, palette, shortcuts, string externalization and units UI

- **Increment:** 1 · **Priority:** 1 · **Size:** 12 days · **Requirements:** 19 (13 M / 6 S / 0 C / 0 done)
- **Labels:** increment-1, archicraft-app, archicraft-model, ui, l10n
- **Depends on:** nothing
- **SRS IDs:** UI-001 UI-005 UI-008 UI-009 UI-011 UI-017 NFR-USE-003 NFR-USE-005 SET-001 SET-004 HW-003 PRJ-003 STR-009 STR-010 NFR-L10N-001 NFR-L10N-003 NFR-L10N-004 NFR-A11Y-001 NFR-A11Y-002

**Goal.** Make every command an Action with description, menu and palette entry and bindable shortcut (conflict detection, keymap export/import), complete the D.8 shortcut table as commands land, add context menus, tooltips, prompt lines with live keys and the main-window shell slots (tool groups, view-control bar), externalize all user-visible strings with a locale and number-format layer, detect the OS locale on all platforms, expose unit and slope settings, and give panels accessible names. Done in increment 1 so every later epic adds string keys instead of literals.

**Acceptance.** A test asserts every Action has label, description, menu path, palette hit and a shortcut where D.8 requires one; palette kittest; rebinding with conflict confirmation and export/import round-trip; a CI lint finds no user-visible literal outside the strings table; OS locale detection on Windows and macOS through OS APIs with tests; slope formats and area/volume units editable in the project dialog; AccessKit names on all panels (kittest query by role); the window title shows the project name and a dirty marker. The literal-string lint also covers Command::label and CommandError messages in archicraft-model (keys live in the headless crate, the catalogue in the app); a test checks every ToolState prompt string against the keys actually handled in that state (NFR-USE-003).

**Design notes.** archicraft-app ui/actions.rs becomes a CommandRegistry (id, label key, description key, category, default shortcut, enabled predicate); view-local keys (R, Tab, Backspace) become Actions; an i18n module with Fluent-style key tables (English default) and number formatting through units.rs; a preferences schema version for SET-004 import. fluent (MIT/Apache) message catalogues in archicraft-app and sys-locale for STR-010; the model crate exposes message keys only.

**Risks.** Externalizing strings late is expensive, hence increment 1; shortcut conflicts with OS-reserved keys are checked per platform in the uniqueness test.

### 17. `3d-input-navigation` (archicraft-sgq) — Working plane, 3D input, measure tool, camera and navigation

- **Increment:** 1 · **Priority:** 1 · **Size:** 14 days · **Requirements:** 16 (9 M / 5 S / 0 C / 2 done)
- **Labels:** increment-1, archicraft-app, input, views
- **Depends on:** `point-input-snapping`
- **SRS IDs:** INP-015 INP-016 VIEW-020 VIEW-021 VIEW-022 VIEW-023 VIEW-025 MEA-001 MEA-006 EDT-022 SET-003 HW-001 HW-002 HW-006 HW-007 UND-005

**Goal.** Make every tool and edit command work in 3D views on a working plane (horizontal, vertical, three-point or picked face), add face snapping and axis guides, align-view-to-surface, the measure tool in 2D and 3D, mirroring about a 3D plane, full camera editing with presets, orbit pivot, two-point perspective, navigation presets, trackpad gestures, high-DPI and multi-monitor behaviour. Walk mode, 3D mouse and pen tablets (Could) are in could-scope-polish.

**Acceptance.** Drawing a wall in the 3D view on a storey working plane yields the same element as in plan (test through the ViewSurface trait); face snapping and X/Y/Z guides in 3D; look-perpendicular-to-working-plane and align-to-face commands; the measure tool reports distance, angle, height and area in working units in plan and 3D without creating elements (AS-04 step); camera eye, target and field of view editable numerically; three navigation presets switchable without restart; the trackpad orbit gesture works on macOS and Windows precision touchpads; a DPI change when moving between monitors re-renders correctly; camera moves never create undo steps (existing test kept).

**Design notes.** archicraft-app input::ViewSurface for Camera3d (ray-plane intersection, face pick from a GpuScene BVH), WorkingPlane state per 3D pane, camera.rs presets and two-point correction, preferences for SET-003, winit gesture events for pinch and rotate.

**Risks.** Input precision in perspective views; numeric entry and typed working-plane elevations keep AS-04 precision.

### 18. `3d-view-restrictions` (archicraft-5lh) — 3D view filters, hidden elements, cut planes and section box as GPU clip planes

- **Increment:** 1 · **Priority:** 1 · **Size:** 12 days · **Requirements:** 8 (5 M / 2 S / 1 C / 0 done)
- **Labels:** increment-1, archicraft-app, views, rendering
- **Depends on:** `plan-views-browser`
- **SRS IDs:** VIEW-040 VIEW-041 VIEW-042 VIEW-043 VIEW-044 VIEW-046 VIEW-047 VIEW-061

**Goal.** Per-view hide, isolate and show-all, kind and storey-range filters with trimming, user cut planes (two points or numeric, bindable to a storey) and an interactive section box with capped cut solids, region filters drawn in plan, the persistent restriction indicator with a one-command reset, exploded storeys, and the guarantee that none of this changes the model or the standard IFCX content. Implemented as GPU clip planes with a stencil cap pass in shader.wgsl, display-only, so it needs no Boolean kernel and ships in increment 1 as soon as saved views exist (VIEW-040/042/043 are Must and are not named by any Appendix F increment).

**Acceptance.** kittest for hide, isolate and show-all honouring layer visibility; a storey range with trimming shows capped bodies (snapshot); two cut planes plus a section box with draggable handles render closed caps in uniform or material colour; a cut plane bound to storey plus offset follows a storey edit; a region filter drawn in plan keeps cut elements selectable; the indicator appears whenever any restriction is active and the reset clears all; writer test: the file is byte-identical with and without restrictions (VIEW-047); restrictions are stored in saved views and re-applied.

**Design notes.** archicraft-app views/view3d.rs Restrictions in ViewState; views/shader.wgsl gains up to eight clip planes plus a box in uniforms and a stencil-based cap pass per plane; filters evaluated on the cached GpuScene per element, caps coloured from the style; actions for the D.8 view shortcuts; the model is untouched (decisions D-7, IFX-068). Kernel clipping is not required; trimmed storey ranges use clip planes at storey elevations.

**Risks.** Capping with several planes and a box in the shader: stencil caps per plane, only when restrictions are active. Exploded storeys (Could) as a per-storey transform offset, time-boxed.

### 19. `regression-and-status` (archicraft-m2fm) — Regression net, fixture registry, fuzzing, headless viewer, performance harness and status upkeep

- **Increment:** 1 · **Priority:** 1 · **Size:** 15 days · **Requirements:** 65 (4 M / 3 S / 2 C / 56 done)
- **Labels:** increment-1, archicraft-model, archicraft-geometry, archicraft-ifcx, archicraft-app, testing, performance
- **Depends on:** nothing
- **SRS IDs:** ELM-004 ELM-010 IFX-001 IFX-002 IFX-005 IFX-010 IFX-013 IFX-014 IFX-020 IFX-021 IFX-022 IFX-025 IFX-030 IFX-031 IFX-032 IFX-043 IFX-061 IFX-063 IFX-064 IFX-067 IFX-080 IFX-090 LAY-004 LAY-005 NFR-ACC-001 NFR-ACC-002 NFR-ACC-005 NFR-LEG-002 NFR-MNT-001 NFR-MNT-002 NFR-MNT-004 NFR-PORT-004 NFR-PORT-005 NFR-REL-001 NFR-REL-002 NFR-REL-003 NFR-SEC-001 NFR-SEC-004 NFR-USE-004 PRJ-004 PRJ-005 PRJ-010 STR-001 STR-003 STR-004 STR-005 STR-008 STR-013 UI-006 UI-007 UND-001 UND-002 VIEW-002 VIEW-014 WAL-001 XCH-008 IFX-006 IFX-007 IFX-008 IFX-091 IFX-092 NFR-SEC-003 NFR-REL-007 PRJ-008 NFR-PERF-001

**Goal.** Keep the increment-0 behaviour verified from day one and build the verification infrastructure every later epic plugs into: the D.7 fixture registry (crates/archicraft-model/tests/fixtures.rs with f1..f9 entries that the owning epics fill), IFX-006 (b) through the pinned reference viewer run headless under Playwright on every golden file and all three CI runners, byte determinism with rounded storey elevations and plan coordinates and the optional indented output, cargo-fuzz for the IFCX reader (and the preference importer once it exists) in a nightly job seeded with ArchiCraft files and fetched R2 examples with the D.9 limits unit-tested, a reader that never drops a record silently, the format-evolution path (successor adapter skeleton, schema publication readiness), the AS-05 end-to-end crash-recovery run with an injectable autosave clock, an Appendix D table-driven defaults test, the performance harness (criterion benches for open/save/evaluate/slice, frame and edit-latency instrumentation behind a feature, CI smoke-mode bench run failing on >20 % regressions, docs/performance/<date>-<machine>.md results convention) with the NFR-PERF-001 hardware decision recorded, and docs/implementation-status.md enforced per merge. Holds the done requirements no other epic extends.

**Acceptance.** CI runs the reference viewer headless (Playwright) on every golden file on Windows, macOS and Linux; saving twice yields identical bytes except timestamp and dataVersion with storey elevations and plan coordinates rounded (IFX-007) and the indented option round-trips; the nightly fuzz job runs at least one hour per release without a crash and tests cover >32 imports, >64 composition depth and the size limit before the whole file is read; corrupted fixtures (non-object layer, malformed view, missing toolDefaults, bad counters) load with every skipped or repaired object in the report (PRJ-008, no silent drops); a FORMAT_VERSION 1 fixture migrates and a mock successor version routes to the adapter stub (IFX-091); AS-05 end to end: a spawned app with an injected autosave clock is killed six minutes after the last save and offers recovery on restart with the original intact, on all three OSes; the Appendix D table test pins wall 0.25 m, slab 0.20 m, cut plane 1.10 m, 2 mm/10 degrees, 5 min, >= 100 undo steps, 8 px and the D.4 priorities; benches run on two_storey_shell and, once the framework lands, on the 5,000/20,000-element stand-ins, publish numbers per CI run and fail on a 20 % regression; the minimum and reference hardware classes are documented in docs/technology-choices.md (closes O-15); a CI script compares each merged epic's srs_ids with the changed rows of docs/implementation-status.md; golden-file diffs are reviewed in pull requests and no done requirement regresses.

**Design notes.** conformance/run.sh and conformance/check.ts gain a Playwright step driving src/viewer; a fuzz/ crate with cargo-fuzz targets in a nightly job in .github/workflows/ci.yml; insta snapshots of IFCX; crates/archicraft-model/tests/fixtures.rs registry; a test clock injected into persistence/autosave.rs; benches/ with criterion in archicraft-geometry and archicraft-ifcx, frame timing behind a feature in archicraft-app; scripts/status.py regenerates the table from gap-analysis.json and the per-merge check; the ARCHICRAFT_BLESS workflow documented; conformance build artefacts git-ignored; cargo-deny advisories added to the CI command; a copy of alpha/ as the pattern for a successor adapter (IFX-091).

**Risks.** CI time: conformance on changed goldens per push, full matrix nightly. Fuzz findings may force reader fixes (budgeted). Performance numbers on synthetic stand-ins are provisional and are replaced by RM-M/RM-L in increment 4.

### 20. `solid-kernel-ops` (archicraft-0kwv) — Manifold Boolean kernel and associative solid operations

- **Increment:** 2 · **Priority:** 2 · **Size:** 15 days · **Requirements:** 14 (10 M / 4 S / 0 C / 0 done)
- **Labels:** increment-2, archicraft-geometry, archicraft-model, archicraft-ifcx, archicraft-app, kernel, solids
- **Depends on:** `element-kind-framework`, `host-openings`
- **SRS IDs:** IFX-038 OPN-002 SOL-020 SOL-021 SOL-022 SOL-023 SOL-024 SOL-025 SOL-026 SOL-027 SOL-028 ELM-014 IFX-068 EDT-024

**Goal.** Bring in the 3D Boolean engine (Manifold) as the first epic of increment 2 (technology-choices: 'when the first case cannot be built from 2D cuts, at the latest in increment 2') and build on it the associative solid operations: subtract, intersect and add with plus/minus Z extrusion variants, several targets and operators, mutual relations acting on unmodified bodies (order independence), persistence in the relation store, related-element queries and relation removal restoring the target, operators keeping their effect while hidden or ghosted, face material choice, identical results in all views and IFCX, explicit operations prevailing over the automatic junction cleanup because the pipeline places the Boolean stage after the junction stage (SOL-027), splitting by a 3D plane, openings that cut further elements with host reassignment (OPN-002), the auxiliary flag with its IFCX exclusion and export-visible-only variant, and the IFX-038 mesh validity rules enforced before every Boolean and before writing.

**Acceptance.** CI builds the kernel on ubuntu, windows and macos runners with cached cmake builds and a feature flag disables it; archicraft-geometry exposes boolean(op, a, b) behind a SolidKernel trait and property tests show every result is closed, 2-manifold, outward-oriented and split at hard edges with at most 65,535 points per Body_n (IFX-038); a failed Boolean returns Err, the element keeps its last valid body and is listed (NFR-REL-005 clause); SOL-022: moving an operator by 0.10 m moves the cut by 0.10 m with no further action; random operator orderings give identical meshes; deleting an operator restores the target; an explicit subtract between two joined walls overrides their automatic junction where the two conflict (SOL-027 conflict test); a multi-element opening cuts two walls and both show the void in plan and IFCX, and moving the non-host cut element leaves the opening in place; a golden file with subtract/intersect/add round-trips its relations and passes IFX-006; auxiliary elements are absent from the standard IFCX building elements and the export-visible-only variant omits hidden elements while the default export does not; split by plane yields two closed solids; the plan slice, the 3D body and the exported mesh hash identically.

**Design notes.** Manifold through thin own bindings to its C API (or the manifold3d crate after a health check), isolated in archicraft-geometry kernel/ behind the SolidKernel trait so a pure-Rust fallback can be swapped; Relation::SolidOperation{targets, operators, kind, extrusion, material_source} in the relation store; evaluation pipeline order fixed as base bodies -> junction cleanup -> openings -> solid operations -> trims with check_closed before every Boolean and dirty propagation along relation edges; archicraft::solidOperation schema with a format-version bump through the framework migration; archicraft-model auxiliary flag on Element with pick exclusion; archicraft-app operation dialog, operator display states and relation inspection (decision D-7). cmake and a C++ toolchain added to .github/workflows/ci.yml and documented.

**Risks.** A C++ toolchain on three platforms and a one-owner binding crate: own bindings, CI from day one, kernel trait, vendored cmake build with caching. Boolean robustness on coplanar faces (walls on slabs) is covered by dedicated regression fixtures. Native crashes from the C++ kernel are the trigger for the minidump handler in release-candidate (NFR-SEC-002).

### 21. `grids` (archicraft-7qjj) — Grid systems

- **Increment:** 2 · **Priority:** 2 · **Size:** 9 days · **Requirements:** 8 (4 M / 3 S / 1 C / 0 done)
- **Labels:** increment-2, archicraft-model, archicraft-geometry, archicraft-ifcx, archicraft-app, grids
- **Depends on:** `element-kind-framework`, `point-input-snapping`, `profiles-columns-beams`
- **SRS IDs:** GRD-001 GRD-002 GRD-003 GRD-004 GRD-005 GRD-006 GRD-007 GRD-008

**Goal.** Orthogonal (optionally radial) grid systems as reference elements with axes, labels, bubbles, extensions, rotation and origin; axis editing; snapping in all views; storey-range visibility; the spacing dimension chain; columns at intersections and beams along axes in one command; IfcGrid export. Kept in increment 2 per Appendix F but buildable as soon as the framework exists.

**Acceptance.** AS-02 as a committed asset samples::structural_frame: 4 x 3 bays of 6.00 m, columns at all 20 intersections on three storeys via GRD-005, beams along the axes, slabs and pad footings as slabs with the IfcFooting class; a test asserts every member sits on its grid line and storey level within MODEL_TOLERANCE (NFR-ACC-003 alignment) and the conformance job verifies IfcColumn, IfcBeam, IfcSlab, IfcFooting and IfcGrid with one basiscurve per axis; axes snap in plan and 3D; the grid is visible on the chosen storeys; the spacing dimension chain updates when an axis moves; radial grids (Could) last.

**Design notes.** archicraft-model kinds/grid.rs (reference element with storey range) and a label-series generator; archicraft-geometry axis segments into the plan drawing and snap targets; archicraft-ifcx IfcGrid node with Axis children; archicraft-app tools/grid.rs and an axis edit palette reusing PartRef. samples::structural_frame built through commands with seeded ids; its golden file joins the conformance run.

**Risks.** Radial grids (Could) are time-boxed.

### 22. `curved-geometry` (archicraft-zft) — Arc walls, curved slab edges, fillet, units in arc walls, polygonal and inclined walls

- **Increment:** 2 · **Priority:** 2 · **Size:** 15 days · **Requirements:** 10 (5 M / 3 S / 2 C / 0 done)
- **Labels:** increment-2, archicraft-model, archicraft-geometry, archicraft-ifcx, archicraft-app, walls
- **Depends on:** `junction-engine`, `point-input-snapping`, `drafting-trim-split`, `edit-reshape-palette`, `doors-windows-core`
- **SRS IDs:** WAL-002 WAL-009 WAL-010 WAL-032 WAL-035 SLB-001 SLB-004 IFX-034 DW-019 EDT-014

**Goal.** Apply the shared arc type of drafting-trim-split to walls and slabs: arc walls (centre and radius, three points, tangent, a full circle as one element per D-10), slab outlines with arc edges coinciding exactly with curved walls (fixture F8), the circle mode and contour tracing with arcs preserved, fillet and chamfer on polygon outlines (EDT-014, which needs the arc edge), doors and windows as chord-aligned units in arc walls whose void cuts the full curved thickness (DW-019), polygonal and inclined walls and combined parallel walls. WAL-002 (Must) sits in increment 2 because Appendix F's increment 1 does not name it, AS-01 and F2/F6/F7 need no arcs, and the junction engine must be general-polygon first.

**Acceptance.** Fixture F8: a slab traced from an arc wall's reference line and then from its core face shows zero gaps and identical tessellation in plan and export (vertex lists compared bit for bit); a full circle entered in one operation is one IfcWall node with a closed mesh and no seam in plan, 3D and the viewer (D-10); tessellation obeys the 2 mm / 10 degree project settings and both are editable in the project dialog; arc-to-straight L and T junctions pass the junction suite; fillet of a slab corner with r = 0.5 m stores an arc edge exactly and 'fillet all corners' works on every polygon-based kind; a window in an arc wall is placed on the chord and its void cuts all skins through the full curved thickness (radial void prism, geometry test with conformance); openings in inclined walls follow the inclination; a polygonal wall joins straight walls in plan.

**Design notes.** archicraft-model Edge::{Line, Arc} in Outline and in the wall reference curve (exact storage from curve.rs); archicraft-geometry: footprints as general polygons from junction.rs with arc contact, the single tessellate() for wall footprints, slab rings, plan cuts and the writer, radial void prisms for DW-019 cut per skin band in the curved wall plane, extrude.rs with per-edge angles and inclined faces; archicraft-app arc input modes in the point-input service, radius handle in the reshape palette, fillet/chamfer palette commands; archicraft-ifcx exact arc parameters in archicraft::wall and tessellated Axis basiscurves.

**Risks.** WAL-002 is extra large; coincident slab and wall tessellation needs the one shared tessellator, enforced by F8. Arc junction maths with skins fall back to square ends plus a reported problem rather than invalid geometry. Inclined walls interact with openings and junctions: fixtures first.

### 23. `roofs` (archicraft-v7d6) — Single- and multi-plane roofs with a pitch-weighted straight skeleton

- **Increment:** 2 · **Priority:** 2 · **Size:** 15 days · **Requirements:** 12 (8 M / 4 S / 0 C / 0 done)
- **Labels:** increment-2, archicraft-model, archicraft-geometry, archicraft-ifcx, archicraft-app, roofs
- **Depends on:** `layer-structures`, `host-openings`, `point-input-snapping`, `junction-engine`, `edit-reshape-palette`
- **SRS IDs:** ROF-001 ROF-002 ROF-003 ROF-004 ROF-005 ROF-006 ROF-007 ROF-008 ROF-009 ROF-011 ROF-012 DW-021

**Goal.** Single-plane roofs (outline, pivot line, slope side, pitch formats, pivot elevation, thickness or structure measured perpendicular, eaves cuts, direct manipulation) and multi-plane roofs from a pivot polygon with per-edge pitch, overhang and gable type, break levels, straight-skeleton plane generation for non-convex footprints with mixed pitches, the three pivot-polygon modes (draw, pick a closed polygon through contour detection, outline of selected walls from the junction engine's outer boundary with the pivot at the wall top), splitting into single planes, computed values, hosted openings and roof windows, and IfcRoof with one child per plane through the compound codec. Plane bodies are prisms in the tilted plane clipped by their neighbours with own code; no Boolean kernel is needed. Trims of walls to roofs and roof joins are cleanup-and-trims.

**Acceptance.** A fixture corpus (rectangle, L, T, U, notched footprints; equal and mixed pitches; gable edges) produces planes whose ridge heights and hip/valley lines match hand-computed values and every roof body is closed and manifold; a randomised footprint test asserts closed output or a reported problem; the four eaves-cut variants; a mansard with one break level; the AS-01 gable roof is generated from the selected exterior walls with the pivot at the wall top; dragging the pivot line regenerates within 200 ms on the stand-in; IfcRoof with one part per plane passes conformance (IFX-016 re-asserted); a roof window is hosted in a plane and cuts it with a Void; split into single-plane roofs shows the one-time notice.

**Design notes.** archicraft-geometry roof/skeleton.rs with a pitch-weighted straight skeleton (own code per technology-choices) and roof/body.rs building each plane as a prism in its plane clipped by the neighbouring planes with layer thickness perpendicular to the plane; archicraft-model kinds/roof.rs with per-edge parameters and the structure reference surface, slope parsing in units.rs; archicraft-ifcx compound children per plane through the framework's parts codec; archicraft-app tools/roof.rs (pivot line, slope side, outline, wall-outline mode) on the point-input service with per-edge handles in the reshape palette.

**Risks.** Skeleton robustness on degenerate events: exact event ordering with MODEL_TOLERANCE, the fixture corpus and the randomised test; on failure keep the last valid roof and report (NFR-REL-005). Break levels (Should) are time-boxed.

### 24. `cleanup-and-trims` (archicraft-nz9) — Cross-kind priority cleanup and associative trims to roofs and slabs

- **Increment:** 2 · **Priority:** 2 · **Size:** 15 days · **Requirements:** 7 (5 M / 2 S / 0 C / 0 done)
- **Labels:** increment-2, archicraft-geometry, archicraft-model, archicraft-app, junctions, solids
- **Depends on:** `solid-kernel-ops`, `junction-engine`, `profiles-columns-beams`, `roofs`
- **SRS IDs:** COL-006 BEA-004 SLB-007 SOL-030 SOL-031 SOL-032 ROF-010

**Goal.** Generalise the junction engine from 2D wall footprints to 3D bodies with the Boolean kernel: columns and beams join walls, slabs and each other by material priority with axis-contact detection (L/T/X), slabs clean up against walls, columns and beams in 3D, sections and plan cuts; and add the associative Trim relation: trim walls, columns, beams and solids to roofs, shells and slabs with keep-side preview, the inclined-element rule, per-roof trimming volumes and roof-to-roof joins. Delivers the AS-01 gable walls trimmed to the roof. The pipeline-order rule (SOL-027) is solid-kernel-ops and the non-associative crop (SOL-033) is free-form-solids, so this epic holds only the relation-driven cleanup and trim stages.

**Acceptance.** Cleanup tests: a concrete column inside a masonry wall cuts the wall skins, a steel beam meeting a concrete column is trimmed to its face, a slab of priority 900 cuts a wall of 800 at the slab band, beams at L/T/X axis contact with walls and columns clean up, results are order-independent and identical in the plan slice, in a vertical slice and in the exported mesh (IFX-033 extended); the AS-01 gable walls are trimmed to the roof and follow a pitch change in one revision; a wall inclined 15 degrees outward under a hip roof keeps its full base (SOL-031); trimming volumes (downward from outline or pivot line, upward, finite) are selectable and displayable on demand; a dormer roof is joined to the main roof along the valley and follows a pitch change; every case joins the NFR-MNT-005 suite.

**Design notes.** archicraft-geometry cleanup.rs: contact detection on an rstar index of body bounds (axis/face overlap), one generic pairwise priority-cutting stage through the SolidKernel shared by the column, beam and slab cases, ordered before explicit operations in the evaluation pipeline fixed by solid-kernel-ops; Relation::Trim{element, by, keep_side, volume} evaluated through the kernel with downward/upward/finite trimming volumes from the roof outline or pivot line; archicraft-app selection-based commands with keep-side preview (decision D-7).

**Risks.** The gap analysis sizes SLB-007, COL-006 and BEA-004 as extra large because each estimate includes a Boolean engine; with the kernel and the junction node graph in place they share one contact-detection and priority stage, which is why they fit one epic, and the scope is bounded by the AS-02 and D.7 cases (everything else falls back to no cleanup plus a reported problem). The beads split is cleanup first, trims second; if the cleanup wave overruns, the trims become the first bead of reference-models' lead time rather than a new epic. Boolean failures on coplanar or tangent cases keep the last valid body and list the element (NFR-REL-005).

### 25. `free-form-solids` (archicraft-n209) — Free-form solids with sub-element editing

- **Increment:** 2 · **Priority:** 2 · **Size:** 15 days · **Requirements:** 12 (6 M / 5 S / 1 C / 0 done)
- **Labels:** increment-2, archicraft-model, archicraft-geometry, archicraft-app, solids
- **Depends on:** `solid-kernel-ops`, `3d-input-navigation`, `selection-transforms`
- **SRS IDs:** SOL-001 SOL-002 SOL-003 SOL-004 SOL-005 SOL-006 SOL-007 SOL-008 SOL-009 SOL-010 SOL-011 SOL-033

**Goal.** Free-form solids as box, extrusion or conversion, sub-element (vertex, edge, face) selection and 3D transforms with push/pull, per-face materials and edge flags, segmentation control, the closed-manifold guarantee, a selectable IFC class, refinement commands, revolve, sweep and merge, open auxiliary sheets, and the non-associative crop by a plane (a single-plane roof's plane, a picked face or three points) that permanently rewrites the cropped element as a free-form solid (SOL-033); uses the PartRef edit mode of selection-transforms and the kernel's validity checks.

**Acceptance.** Raising the top face of a 1 m cube by 1.000 m gives a 1×1×2 m body (SOL-002 acceptance); converting a layered wall keeps the evaluated geometry and face materials after one confirmation; operations that would open the volume are rejected; a profile swept along a non-coplanar 3D path shows no twist (frame-continuity test); the edit mode selects faces, edges and vertices in 3D with handles (kittest); the IFC class is selectable from the D.5 list. Rotating a solid about an arbitrary 3D axis and mirroring it about a 3D plane succeed where walls refuse (EDT-007/EDT-022 positive cases). Cropping a wall below a picked face converts it into a closed free-form solid cut at that plane, and moving or deleting the reference afterwards changes nothing (SOL-033).

**Design notes.** archicraft-geometry halfedge.rs PolyMesh structure with face materials and edge flags, tessellated to TriMesh with IFX-038 splitting, rotation-minimising frames for sweeps (SOL-007); archicraft-model kinds/solid.rs storing the PolyMesh and class; PartRef::{Vertex, Edge, Face} registered through the selection model; archicraft-app tools/solid.rs on 3D input with handles projected into 3D.

**Risks.** Manifold editing operations are intricate; v1 keeps planar faces only (technology-choices) and validates with kernel checks after every edit.

### 26. `terrain-and-site` (archicraft-9x87) — Terrain, site-level elements and georeferencing

- **Increment:** 2 · **Priority:** 2 · **Size:** 15 days · **Requirements:** 15 (9 M / 5 S / 1 C / 0 done)
- **Labels:** increment-2, archicraft-model, archicraft-geometry, archicraft-ifcx, archicraft-app, site
- **Depends on:** `element-kind-framework`, `solid-kernel-ops`, `point-input-snapping`, `3d-input-navigation`, `drafting-trim-split`
- **SRS IDs:** TER-001 TER-002 TER-004 TER-005 TER-006 TER-007 TER-008 TER-010 TER-012 TER-013 TER-014 STR-012 STR-015 IFX-070 NFR-ACC-004

**Goal.** Terrain as a closed solid with a TIN top through typed points and breaklines inside a boundary, created from points or from 2D contour polylines, edited in plan and 3D, cut associatively by building elements, drawn with labelled contours, smooth or faceted in 3D, thinned on demand, written as IfcGeographicElement under the site with a TopSurface part and meeting the 50,000-point budget; plus the site's georeferencing: survey point with EPSG code, latitude and longitude, true north written as the SurveyPoint node per IFX-070, with large survey coordinates never used as model coordinates.

**Acceptance.** AS-01 terrain from typed spot elevations is a closed manifold body; picking 2D contour polylines into a selected terrain assigns one elevation per contour and picking without a selection creates nothing; a basement slab subtracted with upward extrusion removes the soil above it and follows when the slab moves (SOL-021 relation); contour lines regenerate on edit with every n-th labelled and can be frozen to 2D lines; 50,000 points with 200 breaklines build in under 5 s in a benchmark and tile into closed Body_n parts of at most 65,535 points; IfcGeographicElement as a child of IfcSite with Body and TopSurface passes conformance; a project with 7-digit eastings keeps model coordinates near the origin and writes a SurveyPoint child with epsg<code> eastings/northings, the height attribute, epsg4326 latitude/longitude and the second +Y reference point, schemas declared in file; F9's sloping site with storeys -3..+1 is built as the RM-S base.

**Design notes.** archicraft-model site-level containment (Element.storey becomes Container::{Storey, Site} through the framework) and kinds/terrain.rs; Location gains latitude, longitude and EPSG code; archicraft-geometry terrain.rs with spade CDT and constraint edges (via geo), skirt and flat bottom, contour extraction by slicing the TIN, tiling in mesh.rs; archicraft-ifcx site children, SurveyPoint with usd::xformop and in-file epsg schema declarations through registry.rs; archicraft-app terrain tool with point elevation entry in 3D and the contour-pick step.

**Risks.** TIN and Boolean cost on large terrains: tiles are cut only where touched; terrain edges excluded from snap targets by default (INP-017). Georeferencing is offset, rotation and EPSG code only, no reprojection (O-2, O-8); the EPSG height attribute follows R2 when it appears.

### 27. `stair-solver-tool` (archicraft-169a) — Stair element, rule-based solver and placement tool

- **Increment:** 2 · **Priority:** 2 · **Size:** 15 days · **Requirements:** 12 (12 M / 0 S / 0 C / 0 done)
- **Labels:** increment-2, archicraft-model, archicraft-geometry, archicraft-ifcx, archicraft-app, stairs
- **Depends on:** `element-kind-framework`, `point-input-snapping`
- **SRS IDs:** STA-001 STA-002 STA-003 STA-004 STA-005 STA-006 STA-007 STA-008 STA-009 STA-010 STA-011 STA-024

**Goal.** Add the Stair kind with baseline, width, side and offset, storey-linked total rise and the Appendix D.3 rule set with independent enable flags and bounds, the placement tool with upward/downward input and live riser-count preview, and the solver as pure functions: riser count or target riser within the rules, admissible riser-count set, going range from the enabled constraints, fixed-going mode with a violation list, live R/G/2R+G/N/pitch read-out, walking line with the inner-edge rule, conflict detection naming the rules with alternatives and keep-last-valid, start and end with riser or tread. Runs in parallel with roofs; the flight and landing geometry follows in stair-body-plan.

**Acceptance.** Every numeric test of STA-003 to STA-008 as a solver unit test (3.000 m / 19 gives 157.89 mm; N in {15..20}; 300 <= G <= 350 at R = 150; 2R+G = 550 reported against 600); changing the storey height from 3.000 to 3.300 re-solves to 22 risers through the storey link; a rule conflict (min G 350, max 2R+G 600 at R 170) is reported with both rule names and alternatives and the last valid stair is kept (conflict dialog kittest); the live '19 R x 157.9 mm, 2R+G = 616 mm' read-out during input; the walking line distance is checked against the rule; the rule set and all parameters round-trip in archicraft::stair and archicraft::stairRules.

**Design notes.** archicraft-model stair/solver.rs (pure, property-tested functions over baseline and rules) and kinds/stair.rs with RuleSet and the D.3 values in template.rs; archicraft-geometry a provisional stepped extrusion along the baseline so the element is visible before stair-body-plan; archicraft-ifcx archicraft::stair codec; archicraft-app tools/stair.rs with the baseline polyline through the point-input service and the read-out in the tracker and in the properties fields (ELM-017 live values).

**Risks.** STA-001 is extra large: the solver is testable in isolation and ships first. National rule semantics (O-4) are open: the international default ships here, DIN and OENORM presets are templates-presets. No admissible N must never produce invalid geometry: keep-last-valid plus a reported problem.

### 28. `stair-body-plan` (archicraft-wfgq) — Stair flights, landings, finishes, plan symbol and compound parts

- **Increment:** 2 · **Priority:** 2 · **Size:** 15 days · **Requirements:** 12 (7 M / 5 S / 0 C / 0 done)
- **Labels:** increment-2, archicraft-geometry, archicraft-model, archicraft-ifcx, archicraft-app, stairs
- **Depends on:** `stair-solver-tool`, `selection-transforms`, `plan-views-browser`
- **SRS IDs:** STA-020 STA-021 STA-022 STA-023 STA-029 STA-040 STA-041 STA-043 STA-044 STA-045 STA-046 STA-051

**Goal.** Build the stair geometry: straight, L, U and polyline stairs with flight and landing segments and corner turning types, landing-corner riser offsets, node insertion with automatic landings of at least 1.00 m and surplus placement, symmetric riser distribution, the locked-baseline option, finishes and structure (none, monolithic smooth or stepped), seam suppression between same-material parts, part-level failure reporting with fallback, the plan symbol with step lines, walking line, break line and up/down label, display on the home storey and the storey above through the display range, and IfcStair with IfcStairFlight and IfcSlab children through the compound codec, with parts addressable in the edit mode.

**Acceptance.** A U-stair with 22 risers has 11 + 11 with a landing of at least 1.00 m and the surplus placed per option; inserting a node creates an automatic landing; locking the baseline keeps node positions and adapts the step distribution only; all parts are closed bodies and feature edges between same-material treads are suppressed; an unbuildable part is reported and the stair falls back to the last valid body; the plan symbol shows step lines, walking line, arrow and the break line at the configured height, and the storey above shows only the part above the break line (ELM-008 range); the AS-01 U-stair exports IfcStair with IfcStairFlight and IfcSlab landing children and passes conformance (IFX-016 re-asserted); entering edit mode selects a single flight and its properties (kittest on PartRef::Flight).

**Design notes.** archicraft-geometry stair.rs with flight and landing generators producing parts (treads, risers, structure) from a general polyhedron builder beyond extrude.rs, seam suppression via per-part edge flags, the plan symbol as renderer-agnostic primitives with the break line computed from the cut plane; archicraft-model segment list on Stair and PartRef::{Flight, Landing, Step} with ids derived from segment index and role; archicraft-ifcx compound children Flight_n and Landing_n through the framework's parts codec.

**Risks.** Stepped bodies with finishes have many coplanar faces; closedness and tolerance merging are asserted on every generated stair. Winders, spirals, stringers and conversions are railings-stairs-adv.

### 29. `ifcx-interop` (archicraft-vgqz) — Third-party IFCX composition, imported elements, foreign-data preservation and material categories

- **Increment:** 2 · **Priority:** 2 · **Size:** 14 days · **Requirements:** 15 (4 M / 8 S / 2 C / 1 done)
- **Labels:** increment-2, archicraft-ifcx, archicraft-model, archicraft-app, interoperability
- **Depends on:** `element-kind-framework`, `selection-transforms`, `plan-attributes`
- **SRS IDs:** IFX-003 IFX-004 IFX-023 IFX-040 IFX-041 IFX-065 IFX-066 IFX-081 IFX-082 IFX-083 IFX-084 MAT-006 MAT-007 SW-002 NFR-LEG-003

**Goal.** Open any IFCX file: composition per R2 (later entries win attribute by attribute, null removes, inherits order) with local schema resolution and consented remote fetch, imported elements with mesh geometry that can be selected, hidden, moved, copied, deleted and re-layered, divergence detection between stored and regenerated geometry, preservation of unknown attributes, nodes and schemas with a local import cache (so the imports actually written are derived from the attributes present), material import from other files, the material category mapping table with bSDD codes and URIs written on material nodes, optional base quantities, drag-and-drop opening and read-only reference models. Pulled from increment 3 to 2 as a risk slice: proving the composition semantics against compose3 is cheapest right after the reader generalisation of element-kind-framework; the AS-06 end-to-end run is re-run at the increment-3 exit.

**Acceptance.** AS-06 automated: Hello Wall, PCERT Building-Architecture and Tekla House are fetched at test time at the pinned commit (never committed, NFR-LEG-003), compose with the same tree and mesh counts as the reference compose3 in the conformance job, some elements are hidden, moved and deleted, and the saved file preserves unknown attributes and in-file schemas, keeps non-Table-5-2 imports through the local cache and passes IFX-006; a remote import URI opens a consent dialog and no network call happens before consent (test with a blocked resolver); an imported element is selected, hidden, moved, copied, rotated about an arbitrary 3D axis (EDT-007 clause through the registry transform hook), deleted and re-layered, never edited parametrically; a file whose stored geometry deviates from the regenerated one lists the elements with a regenerate-or-keep choice; material nodes carry bsi::ifc::material code and uri from the template's mapping table and pass conformance; NetVolume/NetArea appear as prop@v5a quantities when the option is on; a material library .ifcx imports into the project with category grouping; dropping a file opens it.

**Design notes.** archicraft-ifcx composition engine in validate.rs (flatten, inherits expansion, roots) producing ImportedElement entries and a Foreign side table per node written back verbatim; schema and import cache under DataDirs; ureq for consented fetches (planned in technology-choices); IFX-041 mapping table in template.rs; archicraft-model kinds/imported.rs (pass-through mesh parts, class, name, material, placement); archicraft-app load-report dialog upgrade, consent dialog (D-12) and drag-and-drop via eframe file-hover events; conformance/run.sh gains the R2 sparse checkout.

**Risks.** R2 examples may drift; the commit stays pinned and the test skips with a warning offline, never a redistributed copy. Preservation ordering must stay deterministic (IFX-007). The O-7 dictionary choice is pending; the mapping table is data, not code.

### 30. `reference-models` (archicraft-h8wc) — Reference model RM-S, AS-01/AS-03 automation and the defaults demonstration

- **Increment:** 2 · **Priority:** 2 · **Size:** 10 days · **Requirements:** 2 (1 M / 1 S / 0 C / 0 done)
- **Labels:** increment-2, archicraft-model, archicraft-ifcx, archicraft-app, testing
- **Depends on:** `stair-body-plan`, `roofs`, `cleanup-and-trims`, `terrain-and-site`, `doors-windows-core`, `junction-engine`, `grids`
- **SRS IDs:** NFR-MNT-003 NFR-USE-006

**Goal.** Turn the increment-2 exit into repository assets: RM-S (the AS-01 house on the F9 sloping site: storeys with attic, layered exterior walls, interior walls, slabs with stair opening, U-stair, doors and windows, gable roof with trimmed gable walls, stepped footings, terrain, about 300 elements) built through commands in the seeded fixture builder and kept as the golden IFCX file of NFR-MNT-003, automated AS-01 pass criteria and AS-03 (reopen, change the exterior structure thickness and a storey height, re-validate with unchanged UUIDs), the F3 and F9 fixtures registered, the NFR-USE-006 demonstration that the RM-S builder creates no material, layer structure or other attribute beyond the template, and the status document regenerated for the increment-2 exit. The railing variant of RM-S and RM-M follow in increment 3 and 4.

**Acceptance.** cargo test -p archicraft-ifcx reproduces RM-S byte for byte from the builder (seeded ids) and the conformance job runs on RM-S, the AS-02 frame and every registered fixture; conformance/check.ts asserts the AS-01 criteria (tree project/site/building/3 storeys, openings in walls, clean junctions, modelled colours against the material table); the AS-03 test reopens RM-S, changes the exterior structure thickness and the upper storey height, re-saves and passes IFX-006 with unchanged UUIDs while junctions, openings, slabs and roof follow; F3 (floating floor +6.10 / +6.00 / +5.75, base offset -0.10, resilient layer) and F9 pass; a test asserts the RM-S builder uses only template attributes (NFR-USE-006 demonstration script under docs/); docs/implementation-status.md reflects the increment-2 state.

**Design notes.** crates/archicraft-model/src/samples.rs rm_s() built through commands with v5 ids; crates/archicraft-ifcx/examples/reference_models.rs writes RM-S; fixtures f3/f9 in the registry; conformance/check.ts extended with the AS-01 assertions; golden regeneration via ARCHICRAFT_BLESS; scripted manual procedures for AS-01 and AS-03 under docs/test-plan/.

**Risks.** RM-S needs most increment-2 kinds, so this epic runs last in increment 2 while the builder grows with every epic. Golden-file churn when writers change: bless only with a reviewed diff.

### 31. `sections-elevations-dims` (archicraft-u4vm) — Section and elevation views, dimensions, labels and quantities

- **Increment:** 3 · **Priority:** 3 · **Size:** 15 days · **Requirements:** 19 (0 M / 12 S / 7 C / 0 done)
- **Labels:** increment-3, archicraft-model, archicraft-geometry, archicraft-app, views, annotation
- **Depends on:** `plan-views-browser`, `plan-renderer`, `grids`, `host-openings`, `drafting-trim-split`
- **SRS IDs:** VIEW-030 VIEW-031 VIEW-032 VIEW-058 VIEW-059 GRD-009 OPN-013 WAL-011 ANN-004 ANN-005 ANN-006 ANN-007 ANN-008 ANN-009 ANN-012 MEA-002 MEA-004 MEA-005 OPN-011

**Goal.** Section and elevation markers as reference elements driving live views from arbitrary-plane slices with the plan rules, storey level lines and grid axes, selection and stretching in sections, drawing openings, elevation outlines and 2D geometry there, four default elevations; linear, level and angle dimensions with associative bindings and copy semantics, text and labels, selection quantities, union area, net void volume and the geometry clean-up check. Arbitrary-plane slicing is a generalisation of slice.rs, not a Boolean, so no kernel dependency.

**Acceptance.** Moving a marker updates its section (slice hash test on RM-S); the section shows labelled storey lines and grid axes; a wall height is stretched in a section with numeric input; a new project has four elevations in the browser; a dimension picked on a wall node follows the wall and is flagged when its reference disappears; level dimensions relative to project zero, storey, sea level and user origin; selection quantities summed per kind; the clean-up check finds two overlapping walls and drops the entry once fixed. Dimension bindings reference element id plus a semantic feature key (face role, node index), never mesh indices, so a regenerated wall keeps its dimensions (test).

**Design notes.** archicraft-geometry slice.rs generalised to arbitrary planes with projection and depth clipping, reusing plan.rs classification; archicraft-model kinds/marker.rs and kinds/dimension.rs with the annotation-binding relation keyed by (ElementId, feature key); archicraft-app Pane::Section in egui_tiles drawn by the plan renderer; dimensions and text in paper units at the view scale (decision D-8: viewing and basic editing); text drawn by egui as an overlay layer at the resolved paper size.

**Risks.** VIEW-030 is extra large; projection depth and hidden-line removal stay simple (depth-ordered painting) in v1.

### 32. `3d-display-styles` (archicraft-490) — Display styles, sun, lighting, backgrounds, face materials and themes

- **Increment:** 3 · **Priority:** 2 · **Size:** 14 days · **Requirements:** 12 (1 M / 9 S / 2 C / 0 done)
- **Labels:** increment-3, archicraft-app, archicraft-model, archicraft-geometry, rendering
- **Depends on:** `plan-views-browser`, `host-openings`, `terrain-and-site`
- **SRS IDs:** VIEW-050 VIEW-051 VIEW-052 VIEW-053 VIEW-056 DSP-016 MAT-004 MAT-008 OPN-008 IFX-042 UI-010 NFR-A11Y-003

**Goal.** Finish the 3D display modes (hidden line, wireframe, edge thickness, transparency switch), named display styles as project resources with the shipped presets, a directional sun with shadows and solar position from location, date and time, higher-quality lighting reduced during navigation, backgrounds, display override rules, per-face material overrides including opening reveals, roughness with PBR export, canvas background and contrast rules, and WCAG-conformant light and dark themes.

**Acceptance.** Snapshot tests for the four display modes and every shipped style on RM-S; the shadow pass toggles per style and the sun computed for a given location, date and time matches an astronomical reference within 0.5°; sorted transparent faces render overlapping glass correctly; a face override on a wall end changes appearance only and exports per-part materials; the 'colour by storey' and 'poché' override presets; a contrast check passes on both themes; the canvas background is a preference.

**Design notes.** archicraft-app views/render3d.rs gains a shadow-map pass, an edge-extraction pass, style uniforms, a sorted transparent batch and culling; archicraft-model DisplayStyle resource and DisplayOverrideRule list referenced by saved views; sun vector from Location (latitude, longitude, true north) and date/time by an own solar-position routine; archicraft-geometry per-face parts for overrides (reveal materials as separate parts, OPN-008); archicraft-ifcx gltf::material declared in file (Could).

**Risks.** Shader work across Metal, DX12 and Vulkan; per-platform snapshot tests with tolerance.

### 33. `railings-stairs-adv` (archicraft-i39p) — Railings, winders, spiral stairs and stair extras

- **Increment:** 3 · **Priority:** 3 · **Size:** 15 days · **Requirements:** 19 (0 M / 14 S / 5 C / 0 done)
- **Labels:** increment-3, archicraft-model, archicraft-geometry, archicraft-ifcx, archicraft-app, stairs
- **Depends on:** `stair-body-plan`, `profiles-columns-beams`, `3d-input-navigation`, `curved-geometry`, `free-form-solids`, `solid-kernel-ops`
- **SRS IDs:** RAI-001 RAI-002 RAI-003 RAI-004 RAI-005 RAI-006 RAI-007 RAI-008 RAI-009 RAI-010 STA-025 STA-026 STA-027 STA-028 STA-042 STA-047 STA-048 STA-050 STA-049

**Goal.** Railings with 3D polyline reference lines, attachment to stair and slab edges, the component set (rails, posts, balusters, panels) with profiles and distribution modes, stair patterns, rail ends, presets, terrain draping, plan representation and IfcRailing export; plus winders, curved and spiral flights with chord or arc measures, stringers, step numbering, conversion to solids and ramps. Also stairs as solid-operation targets and operators (STA-049).

**Acceptance.** RAI-004: 4.300 m with a 1.200 m maximum gives 4 bays of 1.075 m; RAI-005: one tread with 3 divisions puts balusters at 1/3 and 2/3; a railing follows its stair when the riser count changes; the AS-01 railing on the U-stair exports IfcRailing with parts (conformance); winder count rules and the arc versus chord measures (r·θ versus 2r·sin(θ/2)) as unit tests; a ramp at 5° with 0.500 m rise is about 5.715 m long; a stair converts to one free-form solid per part after one notice. A stair cut into terrain through a solid operation follows when the stair re-solves (STA-049).

**Design notes.** archicraft-model kinds/railing.rs with a host relation to stair and slab edges and a pattern engine as pure functions; archicraft-geometry profile sweeps along 3D polylines (shared with beams) and posts as prisms; stair solver extensions for winders and arc baselines; archicraft-ifcx compound parts.

**Risks.** RAI-003 is extra large; the component kit is limited to the SRS list with no pattern editor (Section 8).

### 34. `curtain-walls` (archicraft-1lz) — Curtain walls

- **Increment:** 3 · **Priority:** 3 · **Size:** 13 days · **Requirements:** 12 (0 M / 8 S / 4 C / 0 done)
- **Labels:** increment-3, archicraft-model, archicraft-geometry, archicraft-ifcx, archicraft-app, envelope
- **Depends on:** `element-kind-framework`, `components-dw-catalogue`, `selection-transforms`, `profiles-columns-beams`, `cleanup-and-trims`
- **SRS IDs:** CW-001 CW-002 CW-003 CW-004 CW-005 CW-006 CW-007 CW-008 CW-009 CW-010 CW-011 CW-012

**Goal.** Curtain walls from a base path with grid patterns and distribution modes, frame and panel classes as project resources, an edit mode for per-cell overrides, crossing rules and priorities, symbolic or projected plan display, roof trims, insert and layered panels, and IfcCurtainWall export with IfcMember and IfcPlate children.

**Acceptance.** Best-fit distribution ends exactly at each segment end and locked columns keep their size (tests); deleting a division frame merges panels and the override survives a grid edit where the cell still exists; mullions run through and transoms are trimmed by default; the RM-M entrance curtain wall exports IfcCurtainWall with IfcMember and IfcPlate parts (conformance and viewer); a curtain wall trimmed by a roof keeps frames and panels on the trimmed outline.

**Design notes.** archicraft-model kinds/curtain_wall.rs with FrameClass and PanelClass resources and a cell override map keyed by stable cell ids; archicraft-geometry frames as profile sweeps along grid lines, panels as thin slabs; PartRef::{Frame, Panel} through the edit mode of selection-transforms; archicraft-ifcx compound children through the parts codec; CW-008 (roof trims, Could) is implemented last through the Trim relation of cleanup-and-trims, applied to frames and panels individually (hence that dependency).

**Risks.** Override stability across grid edits; cell ids derive from column and row indices with a remap on insert and delete.

### 35. `templates-presets` (archicraft-u5sx) — Templates, start screen, presets, parameter transfer, groups, selection criteria and resource folders

- **Increment:** 3 · **Priority:** 2 · **Size:** 14 days · **Requirements:** 15 (1 M / 12 S / 2 C / 0 done)
- **Labels:** increment-3, archicraft-model, archicraft-ifcx, archicraft-app, project
- **Depends on:** `layer-structures`, `properties-and-dialogs`, `plan-views-browser`, `selection-transforms`, `stair-solver-tool`
- **SRS IDs:** PRJ-001 PRJ-002 PRJ-006 PRJ-009 PRJ-012 PRJ-013 ELM-002 ELM-003 ELM-015 SEL-005 SEL-006 SEL-009 SEL-010 UI-016 STA-052

**Goal.** Templates as data (default metric and imperial, save-as-template, choice at New with fresh UUIDs), a start screen with recent projects, thumbnails and the sample project, in-use detection with read-only open, presets per tool with management, stair rule presets DIN 18065 and OENORM B 5371 as data, parameter pick-up and inject, select-similar, criteria selection, persistent selection regions, named selection sets, groups and nested resource folders by drag and drop. PRJ-001 stays in increment 3 because its remaining gap (template as a selectable artefact) only matters once layer structures and presets exist to be carried; multi-window and cross-project paste are library-dw-exchange.

**Acceptance.** New from the default template gives layer structures, presets and views with fresh UUIDs (identity test) and a template_contents test loads the shipped .ifcx template and asserts every D.1/D.2 value; the imperial template opens in feet-inches; start-screen kittest with a searchable recent list and the sample project; a second instance on the same file warns and opens read-only (lock-file test); a preset is saved from a tool and applied to a selection; the DIN and OENORM stair presets load with values documented against the standards (O-4); pick-up and inject between walls transfer only compatible settings; a criteria set survives save/load; a group moves as one and ungroups; moving a material into a folder keeps all references (DSP-003).

**Design notes.** Templates as .ifcx files under DataDirs with an archicraft::template marker and load_as_template id remapping (IFX-011); a Preset resource per kind in archicraft::project; per-project lock file separate from the autosave session lock; start screen as the 'no project open' app state; resource folders as path metadata on resources; selection sets and groups as resources and relations.

**Risks.** The 'no project open' state touches app.rs flow: covered by kittest flows for New/Open/Recover. Template files as data can drift from code defaults: the template_contents test pins them.

### 36. `library-dw-exchange` (archicraft-d6yl) — Object library, advanced doors and windows, multi-window and cross-project exchange

- **Increment:** 3 · **Priority:** 2 · **Size:** 15 days · **Requirements:** 16 (2 M / 11 S / 3 C / 0 done)
- **Labels:** increment-3, archicraft-model, archicraft-geometry, archicraft-app, components, library
- **Depends on:** `components-dw-catalogue`, `doors-windows-core`, `selection-transforms`, `profiles-columns-beams`, `curved-geometry`
- **SRS IDs:** OBJ-001 OBJ-003 OBJ-007 OBJ-008 OBJ-009 OBJ-012 OBJ-013 EDT-025 DW-014 DW-015 DW-016 DW-020 DW-023 PRJ-011 SW-003 PRF-009

**Goal.** Generic objects (box, cylinder) and a furniture and fixture set of at least 30 definitions, placement by insertion point with rotation snapping, storey-relative elevation and mirroring (objects rotate about the vertical axis per EDT-007), linked library folders, definition upgrades, save-selection-as-component, on-canvas handles; the catalogue browser with categories, thumbnails, search and two-sided previews, derived values and key points, corner windows, curved units in arc walls and arched openings; and two projects open in separate native windows with copy and paste between them carrying materials, layer structures and profiles with conflict resolution.

**Acceptance.** The built-in library lists at least 30 furniture and fixture definitions placed as IfcFurniture with shared type geometry; an object is placed by insertion point and snapped rotation, rotates about the vertical axis and mirrors on request while a request to rotate it about a horizontal axis is refused with the EDT-007 message; relinking a moved library folder restores instances; a selection saved as a component appears in the catalogue immediately; catalogue search and preview from both sides (kittest); clear-opening values and jamb key points are snappable; a corner window spans an L-junction with a post or a frameless connection; a curved unit follows an arc wall; two windows each hold a project with their own wgpu state and pasting a layered wall from one to the other brings its structure, materials and profiles with the use-existing / keep-both / overwrite choice (tests on the headless payload).

**Design notes.** archicraft-model components extended with library sources and versions; clipboard payload of selection-transforms extended with resource bundles (PRF-009); archicraft-geometry object placement transforms (objects do not cut); archicraft-app catalogue browser with egui_extras thumbnails rendered offscreen, a second eframe viewport with per-viewport render state (fallback: a second process exchanging the serialised payload through the OS clipboard); library files are .ifcx (D-11).

**Risks.** Multi-viewport wgpu lifetimes: the texture side channel is replaced by per-viewport render state first. Content authoring effort; definitions are primitive-based and contain no Graphisoft content (NFR-LEG-002).

### 37. `members-profiles-adv` (archicraft-t00o) — Segmented and inclined members, advanced profiles and more opening hosts

- **Increment:** 3 · **Priority:** 3 · **Size:** 15 days · **Requirements:** 14 (0 M / 11 S / 3 C / 0 done)
- **Labels:** increment-3, archicraft-model, archicraft-geometry, archicraft-app, structure
- **Depends on:** `profiles-columns-beams`, `host-openings`, `curved-geometry`, `terrain-and-site`, `free-form-solids`, `solid-kernel-ops`
- **SRS IDs:** COL-004 COL-005 BEA-003 BEA-005 BEA-006 BEA-007 BEA-009 OPN-012 OPN-010 PRF-006 PRF-007 PRF-008 PRF-010 WAL-034

**Goal.** Column inclination and segments, beam end cuts as arbitrary planes, segments with tapered sizes and segment operations, notches and holes in beams, curved beams, openings hosted by beams, columns, free-form solids and terrain, profiles from existing geometry, stretch zones, offset parameters, profiled walls and railings with per-edge materials, vertical voids and shared-profile openings.

**Acceptance.** Fixtures F4 (9.80 m double-pitch beam with four segments) and F5 (6.00 m corbel column with a 5.10 m flexible body) as geometry tests added to the NFR-MNT-005 suite together with the COL-005 and BEA-005 acceptance values; segment insert, remove, reorder and split with undo; a profiled wall following a storey-linked height scales only its stretch zone; a chimney from a holed profile is hollow in plan, section and 3D and joins the surrounding walls; a notch in a beam through an opening. F4 and F5 are registered in the fixture registry.

**Design notes.** archicraft-model segment lists on Column and Beam, profile stretch zones and offset parameters; archicraft-geometry lofted (tapered) segment sweeps merged with the kernel; archicraft-app segment palette on PartRef. Openings in beams, columns, solids and terrain subtract through the SolidKernel.

**Risks.** BEA-005 and COL-005 are extra large; tapered lofts keep planar faces only.

### 38. `perf-tuning-rm-m-l` (archicraft-a3ug) — Reference models RM-M and RM-L, AS-07 and performance tuning

- **Increment:** 4 · **Priority:** 2 · **Size:** 15 days · **Requirements:** 7 (5 M / 2 S / 0 C / 0 done)
- **Labels:** increment-4, archicraft-app, archicraft-geometry, archicraft-model, performance
- **Depends on:** `reference-models`, `curtain-walls`
- **SRS IDs:** NFR-PERF-002 NFR-PERF-003 NFR-PERF-004 NFR-PERF-007 NFR-PERF-008 NFR-PERF-009 VIEW-054

**Goal.** Generate RM-M (five storeys, stairs, layered walls, curtain-wall entrance, about 5,000 elements) and RM-L (repeated RM-M blocks, about 20,000) in-process from the seeded builder, run the AS-07 protocol with the harness of regression-and-status on the two documented hardware classes (open and save timing, frame rates for orbit, pan, zoom and walk, plan navigation, edit latency, memory, autosave blocking) and tune level of detail, culling, indexed GPU buffers, plan caches, background open/save with progress and cancellation until every NFR-PERF target is met.

**Acceptance.** The AS-07 results file docs/performance/<date>-<machine>.md records every measurement on both reference machines with all Must targets met: RM-M opens in <= 10 s and saves in <= 5 s, RM-L opens in <= 30 s, >= 30 fps on RM-M, >= 20 fps on RM-L and >= 30 fps on RM-S on minimum hardware, plan navigation >= 30 fps, a single edit visible in <= 200 ms, no UI freeze over 1 s with progress and cancellation for long operations, RM-L under 4 GB, autosave blocking <= 200 ms; RM-M and RM-L golden files pass IFX-006; a kittest shows the progress dialog while opening RM-L and the viewport stays navigable during save.

**Design notes.** samples::rm_m() and rm_l(blocks) through the seeded builder (no out-of-process interface needed); criterion benches in benches/ extended to the reference models; views/render3d.rs indexed buffers, frustum culling and screen-size LOD; plan-renderer caches; open and save on worker threads with a progress channel (NFR-PERF-007); autosave snapshot as Arc<Project> clone-on-write.

**Risks.** Targets may need rayon or the SQLite store (deferred candidates), added only after measuring; availability of minimum-class hardware (O-15 recorded in increment 1).

### 39. `release-candidate` (archicraft-x20e) — Three-OS acceptance, usability test, German translation, legal and security checks

- **Increment:** 4 · **Priority:** 2 · **Size:** 12 days · **Requirements:** 10 (9 M / 1 S / 0 C / 0 done)
- **Labels:** increment-4, archicraft-app, quality, release, l10n
- **Depends on:** `commands-and-l10n`, `reference-models`, `templates-presets`, `solid-kernel-ops`
- **SRS IDs:** NFR-PORT-001 NFR-PORT-002 NFR-PORT-003 NFR-PORT-006 SW-001 NFR-LEG-001 NFR-SEC-002 NFR-USE-001 NFR-USE-002 NFR-L10N-002

**Goal.** Verify the Must NFRs that are release properties: release builds for Windows 11, macOS (current and previous) and Ubuntu LTS with published minimum and reference hardware, .ifcx registration with consent at first start, the licence inventory gate, native-crash reports through a minidump handler (needed since the C++ kernel arrived), the usability test protocol and execution (NFR-USE-001, AS-08) with a five-day buffer for findings filed as beads, the interaction-consistency review, and the German translation.

**Acceptance.** CI produces release builds for the three operating systems and the release notes name minimum and reference hardware (NFR-PORT-006); scripted manual tests for AS-01..AS-05 exist under docs/test-plan/ and their automated parts run on all three OSes; the first start asks for consent and registers .ifcx (per-OS integration test); cargo deny check advisories licenses sources bans passes, THIRD-PARTY.md is generated and the schema-file licence is resolved; a simulated native crash leaves a minidump-based report whose path is shown at the next start and a network monitor shows no traffic except the consent-gated import fetch; AS-08: at least 4 of 5 participants complete the NFR-USE-001 task following docs/usability/protocol.md; the interaction-model checklist passes for every tool; the German catalogue has every key and switches the whole UI (kittest).

**Design notes.** A CI release workflow (no installer, decision D-12), per-OS file-association code (HKCU ProgId, CFBundleDocumentTypes, xdg-mime) behind a consent dialog, a minidump handler (minidumper/crash-handler, permissive) next to persistence/crash.rs, docs/usability/protocol.md, deny.toml advisories, docs/release.md checklist including the manual hosted-viewer check.

**Risks.** Usability findings may require late UI changes: five days reserved and findings filed as beads. Participant recruitment for AS-08. Native crash handlers vary per OS: Rust panic reports stay the baseline, minidump is best effort.

### 40. `could-scope-polish` (archicraft-5ok) — Could scope by user feedback: spaces, buildings, shells, gravity, walk mode, devices, toolbars, history, scripting

- **Increment:** 4 · **Priority:** 4 · **Size:** 15 days · **Requirements:** 12 (0 M / 0 S / 12 C / 0 done)
- **Labels:** increment-4, archicraft-model, archicraft-geometry, archicraft-ifcx, archicraft-app, could
- **Depends on:** `edit-reshape-palette`, `free-form-solids`, `roofs`, `terrain-and-site`, `commands-and-l10n`, `3d-input-navigation`, `plan-views-browser`
- **SRS IDs:** INP-019 SPC-001 SPC-002 STR-014 SW-004 UI-013 UND-004 ROF-013 ROF-014 HW-004 HW-005 VIEW-024

**Goal.** The Could requirements without a natural home in a feature slice, taken up by user feedback within a capped budget: spaces with drawn or detected boundaries and IfcSpace, several buildings per site, shells and faceted-surface-to-roof conversion, gravity placement, walk mode, 3D mouse and pen tablet support, configurable toolbars, a history panel and the out-of-process command interface. Each item is its own bead, scheduled only after feedback and cut first.

**Acceptance.** Each item taken into the release has its own model/geometry test and round-trip: a space detected inside a wall-bounded region shows name, number and area, is written as IfcSpace and its boundary follows a bounding wall; a second building has its own storeys and IfcBuilding node and the file passes IFX-006; a vault shell is modelled and converted into planar roofs per face; gravity drops a column onto the highest slab under the pointer; walk mode moves at eye height; a scripted client creates a wall through the command interface with explicit units and the same Command objects; untaken items are listed as deferred in docs/implementation-status.md with the feedback rationale.

**Design notes.** Space kind on the contour detection of edit-reshape-palette; StoreyList moved under Building with several IfcBuilding children and per-building grouping in the project browser of plan-views-browser (STR-014); gravity samples the evaluated slab, roof and terrain bodies under the pointer (INP-019); Shell kind on the half-edge and sweep machinery; toolbar model in ui/shell.rs; history panel over History's journal; SW-004 as a JSON-RPC over a local socket speaking serde-serialised Command values (NFR-MNT-001); HW-004 only with a permissive-licence HID crate after a health check.

**Risks.** STR-014 and SW-004 are extra-large Could items: explicitly optional, scheduled last and cut first. Spaces must not grow into the out-of-scope zones with calculation rules (Section 8).


## Changes relative to the winner (roadmap B)

- Manifold moved out of increment 1: B's `geometry-kernel` and `solid-ops-and-trims` are merged into `solid-kernel-ops`, the first epic of increment 2 (technology-choices: 'when the first case cannot be built from 2D cuts, at the latest in increment 2'); `junction-engine` now depends only on `layer-structures` and resolves junctions on 2D per-skin footprints with geo/i_overlay, so the F2/F6/F7 exit does not wait for a C++ toolchain.
- `host-openings` no longer depends on the kernel: single-host openings are 2D cuts in the wall plane or slab holes before extrusion (technology-choices); OPN-002 (multi-element cuts, a named Manifold case) moved into `solid-kernel-ops`, which depends on `host-openings` so the opening kind exists first. `host-openings` gained `3d-input-navigation` as a dependency because OPN-007 demands the live host preview in 3D views too (the Camera3d ViewSurface), a prerequisite B left implicit.
- Cross-kind 3D cleanup COL-006/BEA-004/SLB-007 left `junction-engine` (which carried three XL items among eleven requirements) and joined the trims SOL-030/SOL-031/SOL-032 and ROF-010 in a new increment-2 epic `cleanup-and-trims` (depends on `solid-kernel-ops`, `junction-engine`, `profiles-columns-beams`, `roofs`); SOL-027 (explicit operations prevail over automatic junctions) moved to `solid-kernel-ops` where the pipeline order is fixed, and SOL-033 (non-associative crop, which rewrites the element as a free-form solid) moved to `free-form-solids`, so the cleanup epic holds only the relation-driven cleanup and trim stages. This also removes the `roofs` dependency from solid operations, so `terrain-and-site` is no longer chained behind roofs.
- `stairs` (26 requirements, dependent on `roofs` for no SRS reason) split into `stair-solver-tool` (STA-001..011, STA-024; depends on framework and point input only) and `stair-body-plan` (flights, landings, plan symbol, STA-046 via the display range, compound parts; depends on `stair-solver-tool`, `selection-transforms`, `plan-views-browser`); STA-049 moved to `railings-stairs-adv` (with a `solid-kernel-ops` dependency) and STA-052 to `templates-presets` (with a `stair-solver-tool` dependency).
- IFX-016 (compound parts) moved from `roofs` into `element-kind-framework` as a generic parts codec proven on a test-only compound kind, so roofs and stairs consume one implementation instead of two epics writing compound children in the same increment.
- DW-019 (units in arc walls) moved from `doors-windows-core` (increment 1) and EDT-014 (fillet) from `edit-reshape-palette` into `curved-geometry`, which now depends on `doors-windows-core`, `edit-reshape-palette` and `drafting-trim-split`; no increment-1 acceptance needs an arc wall or an arc edge any more.
- WAL-022 (join/unjoin) moved from `drafting-trim-split` and WAL-021 (auto-join) from `selection-transforms` into `junction-engine`, where the join groups and junction relation (WAL-025) they need are built; WAL-008 (trapezoidal wall, fixture F1) moved from increment-2 `curved-geometry` into `junction-engine` so F1 lands in increment 1.
- `selection-transforms` now depends on `element-kind-framework` (it plans transforms through the registry's per-kind transform hook) and owns SEL-008: the PartRef/edit-mode infrastructure is introduced in increment 1 and proven on slab nodes and wall skins before stairs, segments, grid axes, curtain walls and free-form solids plug into it (moved out of `free-form-solids`). EDT-007's acceptance names the mechanism (the hook returns Unsupported with a message) and the epics that assert the positive cases (`free-form-solids`, `ifcx-interop`, `library-dw-exchange`), each of which now states that clause.
- INP-018 (contour detection) moved from increment-4 `release-polish` into increment-1 `edit-reshape-palette`, where SLB-003 hole tracing lives and which ANN-002, SLB-001 and ROF-004 build on; `drafting-trim-split` (ANN-002 names 2D elements as tracing input), `roofs` and `could-scope-polish` (spaces) depend on it.
- DSP-012 and DSP-017 (skin separator lines, layered display option) moved from `plan-cut-generator` into `plan-renderer`, which now depends on `plan-attributes` and `layer-structures`, so pens and skins exist before separator lines are accepted; the VIEW-013 drawing-scale clause no longer claims saved-view persistence (that is VIEW-004 in `plan-views-browser`).
- `plan-cut-generator` (view range, display ranges, per-kind plan options) merged into `saved-views-browser` as `plan-views-browser` (16 requirements, framework dependency only), freeing an epic slot while keeping all plan-view Musts in increment 1.
- `3d-view-restrictions` redesigned as GPU clip planes with a stencil cap pass in shader.wgsl (display-only, matching VIEW-047/IFX-068); its kernel dependency is gone, it depends only on `plan-views-browser` and moves to increment 1 so the Must items VIEW-040/042/043 are not deferred without technical reason; IFX-068 moved with the auxiliary flag ELM-014 into `solid-kernel-ops`.
- `terrain` became `terrain-and-site`: adds `drafting-trim-split` as a dependency (TER-002/TER-004 consume ANN-001 polylines as contours) and absorbs the georeferencing requirements STR-012, STR-015, IFX-070 and NFR-ACC-004 from the dissolved `ifcx-semantics-location`; `3d-display-styles` depends on it for the sun position from latitude/longitude.
- `ifcx-semantics-location` dissolved into natural homes: IFX-024 to `element-kind-framework` (D.5 classes and predefined types), IFX-007/IFX-008/IFX-091/IFX-092 to `regression-and-status` (determinism, indented output, format evolution), IFX-040/IFX-041/MAT-007/IFX-023 to `ifcx-interop`, STR-007/STR-011 (reference levels, datum display) to `properties-and-dialogs`, georeferencing to `terrain-and-site`.
- `third-party-ifcx` became `ifcx-interop` and moved from increment 3 to 2 as a risk slice (R2 composition semantics compared programmatically against compose3 right after the reader generalisation); PRJ-008 (reader never drops silently, an increment-0 residual) moved from it into increment-1 `regression-and-status`.
- `regression-and-status` turned from a done-only holder into the verification-infrastructure epic (15 days): it owns IFX-006 (headless Playwright viewer run on every golden and all three runners), NFR-SEC-003/NFR-REL-007 (nightly cargo-fuzz with D.9 limit tests), IFX-007/IFX-008, IFX-091/IFX-092, PRJ-008, NFR-PERF-001 (performance harness, CI smoke-mode regression gate, hardware decision), the D.7 fixture registry, the AS-05 end-to-end run with an injectable autosave clock, the Appendix D defaults table test and the per-merge status check; all moved from `reference-models` (increment 2) or `performance-harness` (increment 4), none of which they needed.
- `reference-models` now depends only on increment-2 epics (no `curved-geometry`), owns NFR-MNT-003 plus NFR-USE-006 (the RM-S builder demonstrates that template attributes suffice for AS-01), registers F3 and F9 and automates AS-01/AS-03 so the increment-2 exit has its asset; NFR-USE-006 came from `templates-presets`.
- `roofs` depends on `junction-engine` (ROF-004 pivot polygon from the junction engine's outer boundary) and `edit-reshape-palette` (pick a closed polygon through contour detection, per-edge handles); plane bodies are prisms clipped by neighbouring planes with own code, so the kernel dependency is dropped.
- `grids` acceptance now commits the AS-02 asset `samples::structural_frame` (4x3 bays, columns at all intersections on three storeys, beams, slabs, pad footings as IfcFooting) with its alignment and conformance run.
- Performance harness front-loaded: criterion benches, frame and edit-latency instrumentation, synthetic 1,000/5,000/20,000-element stand-ins (builder in `element-kind-framework`), a CI smoke-mode bench run failing on >20 % regressions, the docs/performance results convention and the NFR-PERF-001 hardware decision all land in increment 1; `performance-harness` became `perf-tuning-rm-m-l` (RM-M/RM-L generated in-process, AS-07, tuning) with NFR-PERF-001 removed.
- `release-polish` split: `release-candidate` (the Must/Should release properties incl. a five-day buffer for usability findings, a minidump handler tied to the C++ kernel via a `solid-kernel-ops` dependency, scripted manual procedures; the redundant `doors-windows-core` and `roofs` dependencies B carried are dropped because `reference-models` implies them) and `could-scope-polish` (Could items by feedback, priority 4) which also received HW-004, HW-005 and VIEW-024 from `3d-input-navigation` and depends on `edit-reshape-palette`, `free-form-solids`, `roofs`, `terrain-and-site`, `commands-and-l10n`, `3d-input-navigation` and `plan-views-browser` (browser grouping for STR-014); UI-016 moved to `templates-presets`.
- `library-objects-dw` became `library-dw-exchange` and absorbed PRJ-011, SW-003 and PRF-009 (multi-window and cross-project paste with resource bundles) from `templates-presets`, with `profiles-columns-beams` and `curved-geometry` (DW-020) as added dependencies; its object rotation clause now follows OBJ-003/EDT-007 (vertical axis only) instead of B's 'any axis'.
- `drafting-trim-split` introduces the exact Arc/PolyCurve type and the single tessellate(curve, settings) with bit-identical vertex lists asserted, so `curved-geometry` and F8 are not a retrofit; it depends on `edit-reshape-palette` because ANN-002 makes 2D elements tracing input for contour detection and the cutters reuse the Outline operations.
- `curtain-walls` depends on `cleanup-and-trims` (CW-008 applies the Trim relation to frames and panels) instead of B's `solid-ops-and-trims` and `free-form-solids`; its edit mode comes from `selection-transforms`.
- `sections-elevations-dims` and `3d-display-styles` dropped the kernel dependency (arbitrary-plane slicing generalises slice.rs; face overrides are per-face parts); sections gained `drafting-trim-split` and the rule that dimension bindings use element id plus a semantic feature key, never mesh indices.
- `profiles-columns-beams` cuts beam ends with its own convex plane clipper instead of kernel clipping, keeping increment 1 free of the C++ dependency.
- `commands-and-l10n` acceptance extended: the literal-string lint also covers Command::label and CommandError in archicraft-model, and a test checks every ToolState prompt against the keys handled in that state (NFR-USE-003).
- `plan-renderer` acceptance names the software adapter (lavapipe/WARP) and the documented per-pixel tolerance in docs/testing.md; `templates-presets` adds the template_contents test that loads the shipped .ifcx template and asserts every D.1/D.2 value.
- Labels now name the real crates (archicraft-model, archicraft-geometry, archicraft-ifcx, archicraft-app) after the increment label and before the area label; priorities are derived mechanically from the requirement table (an epic takes its highest open priority, increment-1 epics are 1).
- An explicit verification-asset table (F1..F9, AS-01..AS-08, RM-S/RM-M/RM-L, harness, fuzz corpus, defaults tests, status enforcement) and the increment-1 critical path were added to the ordering section; sizes re-balanced (increment 1: 19 epics, increment 2: 11, increment 3: 7, increment 4: 3; 40 epics in total).

## Coverage check

All 567 live requirement IDs appear exactly once across the 40 epics (checked mechanically by build_final.py: 0 missing, 0 duplicates, 0 unknown, 0 dangling dependencies, 0 cycles; every dependency sits in the same or an earlier increment). Epics per increment: 1 -> 19, 2 -> 11, 3 -> 7, 4 -> 3. Priorities: 1: 19, 2: 16, 3: 4, 4: 1.
