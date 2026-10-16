// Conformance check for ArchiCraft's IFCX output (SRS IFX-006, NFR-MNT-003).
//
// For every file given on the command line:
//  (a) load it with schema validation enabled, the way buildingSMART's
//      reference implementation (src/ifcx-core at the commit SRS R2 pins)
//      loads a layer stack: imports resolved, schemas checked, composition
//      without cycles;
//  (b) compose it with the code of the reference viewer (src/viewer) and
//      check the spatial tree it shows: project → site → building → storeys
//      → elements with a mesh body and a colour.
//
// Imports are served from ArchiCraft's bundled schema copies, so the check
// runs offline. Usage: node .build/check.mjs <schema dir> <file.ifcx>...

import * as fs from "fs";
import * as path from "path";
import { IfcxLayerStackBuilder } from "./.ifc5/src/ifcx-core/layers/layer-stack";
import { InMemoryLayerProvider, RemoteLayerProvider, StackedLayerProvider } from "./.ifc5/src/ifcx-core/layers/layer-providers";
import { IfcxFile } from "./.ifc5/src/ifcx-core/schema/schema-helper";
import { compose3 } from "./.ifc5/src/viewer/compose-flattened";

const SCHEMA_FILES: { [uri: string]: string } = {
    "https://ifcx.dev/@standards.buildingsmart.org/ifc/core/ifc@v5a.ifcx": "ifc@v5a.ifcx",
    "https://ifcx.dev/@standards.buildingsmart.org/ifc/core/prop@v5a.ifcx": "prop@v5a.ifcx",
    "https://ifcx.dev/@standards.buildingsmart.org/ifc/ifc-mat/ifc-mat@v1.0.0.ifcx": "ifc-mat@v1.0.0.ifcx",
    "https://ifcx.dev/@openusd.org/usd@v1.ifcx": "usd@v1.ifcx",
};

const [schemaDir, ...files] = process.argv.slice(2);
if (!schemaDir || files.length === 0) {
    console.error("usage: check.mjs <schema dir> <file.ifcx>...");
    process.exit(2);
}

const bundled = new Map<string, IfcxFile>();
for (const [uri, name] of Object.entries(SCHEMA_FILES)) {
    bundled.set(uri, JSON.parse(fs.readFileSync(path.join(schemaDir, name), "utf8")));
}

class BundledSchemaProvider implements RemoteLayerProvider {
    async GetLayerByURI(uri: string): Promise<IfcxFile | Error> {
        return bundled.get(uri) ?? new Error(`import "${uri}" is not bundled`);
    }
}

// The viewer fetches imports over the network; serve them from the bundle.
const realFetch = globalThis.fetch;
globalThis.fetch = (async (input: any, init?: any) => {
    const url = String(input);
    const file = bundled.get(url);
    if (file) {
        return new Response(JSON.stringify(file), { status: 200, headers: { "content-type": "application/json" } });
    }
    return realFetch(input, init);
}) as typeof fetch;

type Composed = { name: string; attributes?: { [key: string]: any }; children?: Composed[] };

function classOf(node: Composed): string | undefined {
    return node.attributes?.["bsi::ifc::class::code"];
}

function fail(message: string): never {
    throw new Error(message);
}

function checkViewerTree(root: Composed): string {
    const roots = root.children ?? [];
    if (roots.length !== 1) fail(`expected one root in the viewer tree, found ${roots.length}`);
    const project = roots[0];
    if (classOf(project) !== "IfcProject") fail(`root is ${classOf(project)}, not IfcProject`);
    const sites = (project.children ?? []).filter((c) => classOf(c) === "IfcSite");
    if (sites.length !== 1) fail(`expected one IfcSite, found ${sites.length}`);
    const buildings = (sites[0].children ?? []).filter((c) => classOf(c) === "IfcBuilding");
    if (buildings.length !== 1) fail(`expected one IfcBuilding, found ${buildings.length}`);
    const storeys = (buildings[0].children ?? []).filter((c) => classOf(c) === "IfcBuildingStorey");
    if (storeys.length === 0) fail("the building has no storeys");
    let elements = 0;
    let bodies = 0;
    for (const storey of storeys) {
        if (!storey.attributes?.["usd::xformop::transform"]) fail(`${storey.name} has no placement`);
        for (const element of storey.children ?? []) {
            elements++;
            if (!classOf(element)) fail(`${element.name} has no IFC class`);
            if (typeof element.attributes?.["bsi::ifc::prop::Name"] !== "string") fail(`${element.name} has no name`);
            const meshes = (element.children ?? []).filter((c) => c.attributes?.["usd::usdgeom::mesh::points"]);
            if (meshes.length === 0) fail(`${element.name} has no body`);
            for (const mesh of meshes) {
                bodies++;
                const points = mesh.attributes!["usd::usdgeom::mesh::points"];
                const indices = mesh.attributes!["usd::usdgeom::mesh::faceVertexIndices"];
                if (points.length > 65535) fail(`${mesh.name} has more than 65,535 points`);
                if (indices.length % 3 !== 0 || indices.some((i: number) => i >= points.length)) {
                    fail(`${mesh.name} has an invalid triangle list`);
                }
                if (!Array.isArray(mesh.attributes!["bsi::ifc::presentation::diffuseColor"])) {
                    fail(`${mesh.name} has no colour`);
                }
            }
        }
    }
    return `${storeys.length} storeys, ${elements} elements, ${bodies} bodies`;
}

let failures = 0;
for (const file of files) {
    const name = path.basename(file);
    try {
        const ifcx: IfcxFile = JSON.parse(fs.readFileSync(file, "utf8"));

        // (a) Reference implementation: layer stack with schema validation.
        const provider = new StackedLayerProvider([
            new InMemoryLayerProvider().AddAll([ifcx]),
            new BundledSchemaProvider(),
        ]);
        const stack = await new IfcxLayerStackBuilder(provider).FromId(ifcx.header.id).Build();
        if (stack instanceof Error) throw stack;
        const tree = stack.GetFullTree();
        if (tree.children.size !== 1) fail(`expected one root, found ${tree.children.size}`);

        // (b) Reference viewer composition and spatial tree.
        const composed = (await compose3([ifcx])) as Composed;
        const summary = checkViewerTree(composed);
        console.log(`PASS ${name}: ${summary}`);
    } catch (e) {
        failures++;
        console.log(`FAIL ${name}: ${e instanceof Error ? e.message || e.constructor.name : e}`);
    }
}
process.exit(failures === 0 ? 0 : 1);
