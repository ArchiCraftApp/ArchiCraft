// Step (b2) of the IFX-006 conformance check: every file in CONFORMANCE_FILES (one path per
// line, set by run.sh) is loaded into buildingSMART's reference viewer (src/viewer at the
// commit SRS R2 pins) in headless Chromium through viewer/harness.html. The test checks the
// spatial tree the viewer shows (project → site → building → storeys → elements with bodies
// and colours), compares its storey and element counts with the file, renders a frame and
// finds visible geometry and every material colour of the file in it. The screenshot is
// written to test-results/<path-of-the-file>.png for the manual release check.
//
// Everything is served by page.route: the harness from this directory and the four schema
// imports from crates/archicraft-ifcx/schemas. Any other request is aborted and fails the
// test before the other checks, so the step runs offline and says so when it does not. A
// request for a localhost path the harness does not serve fails the test the same way.
//
// One case is not step (b2): "broken copy: a copy that does not parse" runs check.ts, steps (a)
// and (b1), on a cut-off copy of a golden file, since a file that does not parse never reaches
// the viewer. It checks that both steps fail on that file with its JSON parse error.

import { expect, test, type Page } from "@playwright/test";
import { spawnSync } from "child_process";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import { SCHEMA_FILES } from "./schemas";
import { checkSpatialTree, describeTree, type TreeNode } from "./tree-check";

const here = path.dirname(fileURLToPath(import.meta.url));
const schemaDir = path.join(here, "..", "crates", "archicraft-ifcx", "schemas");
const golden = path.join(here, "..", "crates", "archicraft-ifcx", "tests", "golden");

const ORIGIN = "http://localhost";
const SERVED: { [pathname: string]: [string, string] } = {
    "/": ["viewer/harness.html", "text/html"],
    "/harness.mjs": [".build/harness.mjs", "text/javascript"],
    // The environment map the viewer loads at start-up.
    "/images/wildflower_field_1k.hdr": [".ifc5/web/viewer/images/wildflower_field_1k.hdr", "application/octet-stream"],
};

// A pixel belongs to the model when a channel differs from the empty viewport by more than
// this many levels; the render must cover at least MIN_GEOMETRY of the viewport.
const BACKGROUND_TOLERANCE = 24;
const MIN_GEOMETRY = 0.02;
// A material colour is found when at least MIN_COLOUR_PIXELS model pixels match it best and
// within COLOUR_TOLERANCE levels per channel under the viewer's lighting (see analyseRender).
const COLOUR_TOLERANCE = 3;
const MIN_COLOUR_PIXELS = 25;

const SPATIAL = new Set(["IfcProject", "IfcSite", "IfcBuilding", "IfcBuildingStorey"]);

type Ifcx = { header: { id: string }; data: { path: string; children?: { [name: string]: string | null }; inherits?: { [name: string]: string | null }; attributes?: { [key: string]: any } }[] };

function readIfcx(file: string): Ifcx {
    return JSON.parse(fs.readFileSync(file, "utf8"));
}

/** Storeys, elements and distinct material colours as the file states them. */
function expectedFromFile(ifcx: Ifcx) {
    const classes = new Map<string, string>();
    const colours = new Map<string, number[]>();
    for (const node of ifcx.data) {
        const code = node.attributes?.["bsi::ifc::class"]?.code;
        if (typeof code === "string") classes.set(node.path, code);
        const colour = node.attributes?.["bsi::ifc::presentation::diffuseColor"];
        if (Array.isArray(colour)) colours.set(JSON.stringify(colour), colour);
    }
    const codes = [...classes.values()];
    return {
        storeys: codes.filter((c) => c === "IfcBuildingStorey").length,
        elements: codes.filter((c) => !SPATIAL.has(c)).length,
        colours: [...colours.values()],
    };
}

type Session = { outside: string[]; unserved: string[]; console: string[] };

/**
 * Serves the harness and the bundled schemas; records every request that leaves both and
 * every localhost path the harness does not serve.
 */
async function openViewer(page: Page): Promise<Session> {
    const outside: string[] = [];
    const unserved: string[] = [];
    const consoleErrors: string[] = [];
    page.on("request", (request) => {
        const url = request.url();
        if (!url.startsWith(`${ORIGIN}/`) && !(url in SCHEMA_FILES) && !/^(data|blob):/.test(url)) outside.push(url);
    });
    page.on("console", (message) => {
        if (message.type() === "error") consoleErrors.push(message.text());
    });
    page.on("pageerror", (error) => consoleErrors.push(error.message));
    await page.route("**/*", async (route) => {
        const url = route.request().url();
        const schema = SCHEMA_FILES[url];
        if (schema) {
            return route.fulfill({
                path: path.join(schemaDir, schema),
                contentType: "application/json",
                headers: { "access-control-allow-origin": "*" },
            });
        }
        if (url.startsWith(`${ORIGIN}/`)) {
            const served = SERVED[new URL(url).pathname];
            if (served) return route.fulfill({ path: path.join(here, served[0]), contentType: served[1] });
            unserved.push(url);
            return route.fulfill({ status: 404 });
        }
        return route.abort("blockedbyclient");
    });
    await page.goto(`${ORIGIN}/`);
    await page.waitForFunction(() => typeof window.loadIfcx === "function");
    return { outside, unserved, console: consoleErrors };
}

/** Fails when the viewer reached beyond what the harness serves (IFX-006: offline). */
function checkOffline(session: Session) {
    if (session.outside.length > 0) throw new Error(`requests left the machine: ${session.outside.join(", ")}`);
    if (session.unserved.length > 0) throw new Error(`the harness does not serve ${session.unserved.join(", ")}`);
}

type RenderAnalysis = { geometry: number; colourPixels: number[] };

/**
 * Runs in the page: decodes both screenshots, counts the pixels that differ from the empty
 * viewport and assigns each of them to the material colour that explains it best.
 *
 * The viewer shades with MeshLambertMaterial, and three.js treats diffuseColor as linear: a
 * pixel is colour × (ambient + d) in linear terms, where ambient is the viewer's ambient
 * light (0xddeeff at intensity 0.4, divided by π for the Lambert BRDF) and d ≥ 0 the
 * directional light the face receives. d is fitted per pixel and colour by least squares;
 * the colour with the smallest error after sRGB encoding gets the pixel if that error is
 * within the tolerance. Colours that render within a level of each other on some face
 * (greys of nearly the same hue) can still trade pixels there.
 */
async function analyseRender(input: {
    background: string;
    render: string;
    colours: number[][];
    backgroundTolerance: number;
    colourTolerance: number;
}): Promise<RenderAnalysis> {
    async function decode(base64: string): Promise<Uint8ClampedArray> {
        const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
        const bitmap = await createImageBitmap(new Blob([bytes], { type: "image/png" }));
        const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
        const context = canvas.getContext("2d")!;
        context.drawImage(bitmap, 0, 0);
        return context.getImageData(0, 0, bitmap.width, bitmap.height).data;
    }
    const toLinear = (v: number) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
    const toSrgb = (v: number) => 255 * (v <= 0.0031308 ? v * 12.92 : 1.055 * Math.max(v, 0) ** (1 / 2.4) - 0.055);
    const ambient = [0xdd, 0xee, 0xff].map((v) => (0.4 * toLinear(v / 255)) / Math.PI);
    const dot = (x: number[], y: number[]) => x[0] * y[0] + x[1] * y[1] + x[2] * y[2];

    const background = await decode(input.background);
    const render = await decode(input.render);
    let geometry = 0;
    const colourPixels = input.colours.map(() => 0);
    for (let i = 0; i < render.length; i += 4) {
        const pixel = [render[i], render[i + 1], render[i + 2]];
        if (pixel.every((v, c) => Math.abs(v - background[i + c]) <= input.backgroundTolerance)) continue;
        geometry++;
        const linear = pixel.map((v) => toLinear(v / 255));
        const errors = input.colours.map((colour) => {
            const ambientPart = colour.map((c, ch) => c * ambient[ch]);
            const directional = linear.map((l, ch) => l - ambientPart[ch]);
            const d = Math.max(dot(colour, colour) > 0 ? dot(colour, directional) / dot(colour, colour) : 0, 0);
            return Math.max(...pixel.map((p, ch) => Math.abs(toSrgb(ambientPart[ch] + colour[ch] * d) - p)));
        });
        const best = Math.min(...errors);
        errors.forEach((error, k) => {
            if (error <= input.colourTolerance && error === best) colourPixels[k]++;
        });
    }
    return { geometry: geometry / (render.length / 4), colourPixels };
}

/**
 * Loads one file into the viewer and checks it; returns the summary line or throws with a
 * message that names the failing node.
 */
async function checkInViewer(page: Page, ifcx: Ifcx, screenshot?: string): Promise<string> {
    const session = await openViewer(page);
    const viewport = page.locator(".viewport");
    const background = await viewport.screenshot({ scale: "css" });
    const loaded = await page.evaluate((json) => window.loadIfcx(json), ifcx as unknown).then(
        (tree: TreeNode) => ({ tree }),
        (error: Error) => ({ error }),
    );
    // A request the harness cannot answer explains whatever fails after it, so it comes first.
    checkOffline(session);
    if ("error" in loaded) throw loaded.error;
    const tree = loaded.tree;
    const render = await viewport.screenshot({ scale: "css", path: screenshot });

    const summary = checkSpatialTree(tree);
    const expected = expectedFromFile(ifcx);
    if (summary.storeys !== expected.storeys || summary.elements !== expected.elements) {
        throw new Error(
            `the viewer shows ${summary.storeys} storeys and ${summary.elements} elements, the file has ${expected.storeys} and ${expected.elements}`,
        );
    }
    const analysis = await page.evaluate(analyseRender, {
        background: background.toString("base64"),
        render: render.toString("base64"),
        colours: expected.colours,
        backgroundTolerance: BACKGROUND_TOLERANCE,
        colourTolerance: COLOUR_TOLERANCE,
    });
    if (analysis.geometry < MIN_GEOMETRY) {
        throw new Error(`the render shows almost no geometry (${(analysis.geometry * 100).toFixed(1)} % of the viewport)`);
    }
    const missing = expected.colours.filter((_, k) => analysis.colourPixels[k] < MIN_COLOUR_PIXELS);
    if (missing.length > 0) {
        throw new Error(`the render does not show the material colours ${missing.map((c) => JSON.stringify(c)).join(", ")}`);
    }
    checkOffline(session);
    if (session.console.length > 0) throw new Error(`the viewer logged errors: ${session.console.join("; ")}`);
    return (
        `${describeTree(summary)} (file: ${expected.storeys} storeys, ${expected.elements} elements), ` +
        `${(analysis.geometry * 100).toFixed(0)} % geometry, ${expected.colours.length}/${expected.colours.length} colours`
    );
}

const files = (process.env.CONFORMANCE_FILES ?? "").split("\n").filter((f) => f.trim() !== "");

for (const file of files) {
    // Reference models and golden files share base names, so tests go by repository path, with
    // forward slashes on every platform so the lines compare equal (compare-runs.sh).
    const relative = path.relative(path.join(here, ".."), file).split(path.sep).join("/");
    const name = relative.startsWith("..") ? file : relative;
    test(name, async ({ page }, testInfo) => {
        const screenshot = path.join(testInfo.project.outputDir, `${name.replace(/\.ifcx$/, "").replace(/[\\/:]+/g, "-").replace(/^-/, "")}.png`);
        const summary = await checkInViewer(page, readIfcx(file), screenshot);
        testInfo.annotations.push({ type: "summary", description: summary });
    });
}

// The checks above must catch broken files and name the node or URL at fault. Each case breaks
// one node or import of a copy of the walls-and-slab golden; the last one cuts the copy off so
// it does not parse, which steps (a) and (b1) must report.
test.describe("broken copy", () => {
    const base = path.join(golden, "walls-and-slab.ifcx");

    /** The viewer path of a node below its root, such as "/Site_…/Building_…/Storey_…". */
    function viewerPath(ifcx: Ifcx, nodePath: string): string {
        const parent = new Map<string, [string, string]>();
        for (const node of ifcx.data) {
            for (const [key, child] of Object.entries(node.children ?? {})) if (child) parent.set(child, [node.path, key]);
        }
        let result = "";
        for (let at = parent.get(nodePath); at; at = parent.get(at[0])) result = `/${at[1]}${result}`;
        return result;
    }

    function nodes(ifcx: Ifcx, predicate: (node: Ifcx["data"][number]) => boolean) {
        return ifcx.data.filter(predicate);
    }

    async function expectRejected(page: Page, ifcx: Ifcx, message: string, testInfo: { annotations: { type: string; description?: string }[] }) {
        const error = await checkInViewer(page, ifcx).then(
            () => null,
            (e: Error) => e,
        );
        expect(error, "the broken copy passed").not.toBeNull();
        expect(error!.message).toContain(message);
        testInfo.annotations.push({ type: "summary", description: `rejected: ${error!.message}` });
    }

    test("a storey without usd::xformop", async ({ page }, testInfo) => {
        const ifcx = readIfcx(base);
        const storey = nodes(ifcx, (n) => n.attributes?.["bsi::ifc::class"]?.code === "IfcBuildingStorey")[0];
        for (const node of nodes(ifcx, (n) => n.path === storey.path)) delete node.attributes?.["usd::xformop"];
        await expectRejected(page, ifcx, `${viewerPath(ifcx, storey.path)} has no usd::xformop placement`, testInfo);
    });

    test("a mesh without diffuseColor", async ({ page }, testInfo) => {
        const ifcx = readIfcx(base);
        const meshes = nodes(ifcx, (n) => !!n.attributes?.["usd::usdgeom::mesh"]);
        // A mesh whose material another mesh shares, so the material stays in the tree.
        const mesh = meshes.find((m) => meshes.some((o) => o !== m && o.inherits?.material === m.inherits?.material))!;
        delete mesh.inherits;
        await expectRejected(page, ifcx, `${viewerPath(ifcx, mesh.path)} has no diffuseColor`, testInfo);
    });

    test("a mesh with an out-of-range index", async ({ page }, testInfo) => {
        const ifcx = readIfcx(base);
        const mesh = nodes(ifcx, (n) => !!n.attributes?.["usd::usdgeom::mesh"])[0];
        const geometry = mesh.attributes!["usd::usdgeom::mesh"];
        geometry.faceVertexIndices[0] = geometry.points.length;
        await expectRejected(page, ifcx, `${viewerPath(ifcx, mesh.path)} has the index ${geometry.points.length}, out of range`, testInfo);
    });

    test("an import that is not bundled", async ({ page }, testInfo) => {
        const ifcx = readIfcx(base) as Ifcx & { imports: { uri: string }[] };
        const uri = "https://ifcx.dev/@example.org/not-bundled@v1.ifcx";
        ifcx.imports.push({ uri });
        await expectRejected(page, ifcx, `requests left the machine: ${uri}`, testInfo);
    });

    test("an import from a localhost path the harness does not serve", async ({ page }, testInfo) => {
        const ifcx = readIfcx(base) as Ifcx & { imports: { uri: string }[] };
        const uri = `${ORIGIN}/not-served@v1.ifcx`;
        ifcx.imports.push({ uri });
        await expectRejected(page, ifcx, `the harness does not serve ${uri}`, testInfo);
    });

    // Steps (a) and (b1) run in check.ts, before this spec (run.sh builds it into .build/).
    test("a copy that does not parse, in steps (a) and (b1)", async ({}, testInfo) => {
        const text = fs.readFileSync(base, "utf8");
        const file = testInfo.outputPath("does-not-parse.ifcx");
        fs.writeFileSync(file, text.slice(0, text.length / 2));
        const run = spawnSync(process.execPath, [path.join(here, ".build", "check.mjs"), schemaDir, file], { cwd: here, encoding: "utf8" });
        const lines = run.stdout.split("\n");
        expect(run.status, run.stdout + run.stderr).toBe(1);
        // Each step names the cut-off file and the parse error, so a missing file or another
        // failure does not pass for it.
        const failLine = (step: string) => new RegExp(`^FAIL ${step.replace(/[()]/g, "\\$&")} \\S*/does-not-parse\\.ifcx: .*JSON`);
        for (const step of ["(a) ifcx-core", "(b1) compose3"]) {
            expect(lines.some((l) => failLine(step).test(l)), `no FAIL ${step} line for does-not-parse.ifcx with a JSON parse error in:\n${run.stdout}`).toBe(true);
        }
        expect(lines.some((l) => l.startsWith("PASS ")), run.stdout).toBe(false);
        testInfo.annotations.push({ type: "summary", description: "rejected: FAIL (a) and FAIL (b1), exit status 1" });
    });
});
