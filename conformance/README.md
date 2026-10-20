# IFCX conformance (SRS IFX-006)

Checks that ArchiCraft's IFCX files load and display in buildingSMART's reference code, at
the commit of [IFC5-development](https://github.com/buildingSMART/IFC5-development) that SRS
R2 pins (`1a63082ada967c683cfacee2005f8f749c8e1b79`, cloned into `.ifc5/`). These are test
tools under Node.js; nothing here ships with the product.

```bash
cargo run --locked -p archicraft-ifcx --example reference_models -- target/reference-models
cargo run --locked -p archicraft-ifcx --example reference_models -- --indented target/reference-models
conformance/run.sh target/reference-models/*.ifcx crates/archicraft-ifcx/tests/golden/*.ifcx
```

`run.sh` prints one `PASS` or `FAIL` line per file and step and exits non-zero if any step
failed. The steps:

| Step | What runs | Checks |
|---|---|---|
| (a) `ifcx-core` | `check.ts`: the reference implementation (`src/ifcx-core`) builds the layer stack | Imports resolve, schema validation passes, composition has no cycles and one root |
| (b1) `compose3` | `check.ts`: the viewer's composition (`src/viewer/compose-flattened.ts`) in Node | The spatial tree project → site → building → storeys → elements, each with a name and a mesh body that has a valid triangle list and a `diffuseColor` (`tree-check.ts`) |
| (b2) `viewer` | `viewer.spec.ts`: the viewer itself (`src/viewer/render.ts`) in headless Chromium, driven by Playwright | The same tree as the viewer shows it, storey and element counts equal to the file's, a render with visible geometry, and every material colour of the file in that render |

Step (b2) loads `viewer/harness.html`, whose `viewer/harness.ts` is bundled by esbuild with
the viewer's modules and three.js. `window.loadIfcx(json)` calls the viewer's `addModel()`,
which composes the file, builds the three.js scene, renders it and builds the model tree; the
harness reads the tree back from the viewer's model tree and attribute table and repeats none
of its logic. The test then screenshots the viewport and checks:

- geometry: at least 2 % of the viewport differs from the empty viewport;
- colours: each `diffuseColor` of the file explains at least 25 pixels best, within 3 levels
  per channel, under the viewer's lighting (ambient light 0xddeeff plus a fitted directional
  share; see `analyseRender`). Colours that differ only in brightness and render within a
  level of each other on some face can trade pixels there.

The screenshots land in `test-results/` (one PNG per file) for the manual check of each
release; CI uploads them as the `viewer-screenshots` artefact. Three broken copies of the
walls-and-slab golden (a storey without `usd::xformop`, a mesh without `diffuseColor`, a mesh
with an out-of-range index) must fail the step with a message naming the node.

All steps run offline. Steps (a) and (b1) serve the four `ifcx.dev` imports from
`crates/archicraft-ifcx/schemas` (`schemas.ts`); in (b2), Playwright's `page.route` serves
the harness and those imports, aborts every other request, and the test fails if any request
left `http://localhost` and the bundled schema URIs.

Playwright's Chromium is a test tool like Node. `run.sh` installs it on first use
(`npx playwright install chromium`); on Linux CI, `npx playwright install --with-deps chromium`
also installs its system libraries. WebGL runs on SwiftShader (`playwright.config.ts`), so no
GPU is needed. The hosted IFC5 viewer (SRS R4) is checked by hand once per release.

`.gitignore` keeps `.ifc5/`, `node_modules/`, `.build/`, `test-results/` and
`playwright-report/` out of git.
