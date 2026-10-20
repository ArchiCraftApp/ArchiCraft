// Conformance check for ArchiCraft's IFCX output (SRS IFX-006, NFR-MNT-003).
//
// For every file given on the command line:
//  (a) load it with schema validation enabled, the way buildingSMART's
//      reference implementation (src/ifcx-core at the commit SRS R2 pins)
//      loads a layer stack: imports resolved, schemas checked, composition
//      without cycles;
//  (b1) compose it with the code of the reference viewer (src/viewer) and
//      check the spatial tree it shows: project → site → building → storeys
//      → elements with a mesh body and a colour (tree-check.ts). Step (b2),
//      the viewer itself in headless Chromium, is viewer.spec.ts.
//
// Imports are served from ArchiCraft's bundled schema copies, so the check
// runs offline. Usage: node .build/check.mjs <schema dir> <file.ifcx>...

import * as fs from "fs";
import * as path from "path";
import { IfcxLayerStackBuilder } from "./.ifc5/src/ifcx-core/layers/layer-stack";
import { InMemoryLayerProvider, RemoteLayerProvider, StackedLayerProvider } from "./.ifc5/src/ifcx-core/layers/layer-providers";
import { IfcxFile } from "./.ifc5/src/ifcx-core/schema/schema-helper";
import { compose3 } from "./.ifc5/src/viewer/compose-flattened";
import { SCHEMA_FILES } from "./schemas";
import { checkSpatialTree, describeTree, type TreeNode } from "./tree-check";

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

function message(e: unknown): string {
    return e instanceof Error ? e.message || e.constructor.name : String(e);
}

// One line per file and step; (b2), the viewer in a headless browser, is viewer.spec.ts.
let failures = 0;
for (const file of files) {
    // By repository path (run.sh runs in conformance/): reference models and goldens share base names.
    const relative = path.relative("..", file);
    const name = relative.startsWith("..") ? file : relative;
    let ifcx: IfcxFile;
    try {
        ifcx = JSON.parse(fs.readFileSync(file, "utf8"));
    } catch (e) {
        // Neither step can run on a file that does not parse; both say so.
        failures++;
        console.log(`FAIL (a) ifcx-core ${name}: ${message(e)}`);
        console.log(`FAIL (b1) compose3 ${name}: ${message(e)}`);
        continue;
    }

    // (a) Reference implementation: layer stack with schema validation.
    try {
        const provider = new StackedLayerProvider([
            new InMemoryLayerProvider().AddAll([ifcx]),
            new BundledSchemaProvider(),
        ]);
        const stack = await new IfcxLayerStackBuilder(provider).FromId(ifcx.header.id).Build();
        if (stack instanceof Error) throw stack;
        const tree = stack.GetFullTree();
        if (tree.children.size !== 1) throw new Error(`expected one root, found ${tree.children.size}`);
        console.log(`PASS (a) ifcx-core ${name}: schema validation and composition`);
    } catch (e) {
        failures++;
        console.log(`FAIL (a) ifcx-core ${name}: ${message(e)}`);
    }

    // (b1) Reference viewer composition and spatial tree, in Node.
    try {
        const composed = (await compose3([ifcx])) as TreeNode;
        console.log(`PASS (b1) compose3 ${name}: ${describeTree(checkSpatialTree(composed))}`);
    } catch (e) {
        failures++;
        console.log(`FAIL (b1) compose3 ${name}: ${message(e)}`);
    }
}
process.exit(failures === 0 ? 0 : 1);
