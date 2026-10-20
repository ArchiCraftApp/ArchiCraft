// The spatial tree SRS IFX-006 (b) expects the reference viewer to show:
// project → site → building → storeys → elements with a mesh body and a colour.
//
// Shared by the Node step (check.ts, on the output of the viewer's compose3) and the
// headless browser step (viewer.spec.ts, on the tree the viewer itself builds). Both
// hand in nodes named by their viewer path, so every failure names the node.

export type TreeNode = { name: string; attributes?: { [key: string]: any }; children?: TreeNode[] };

export type TreeSummary = { storeys: number; elements: number; bodies: number };

export function describeTree(summary: TreeSummary): string {
    return `${summary.storeys} storeys, ${summary.elements} elements, ${summary.bodies} bodies`;
}

function classOf(node: TreeNode): string | undefined {
    return node.attributes?.["bsi::ifc::class::code"];
}

function fail(message: string): never {
    throw new Error(message);
}

export function checkSpatialTree(root: TreeNode): TreeSummary {
    const roots = root.children ?? [];
    if (roots.length !== 1) fail(`expected one root in the viewer tree, found ${roots.length}`);
    const project = roots[0];
    if (classOf(project) !== "IfcProject") fail(`root ${project.name} is ${classOf(project)}, not IfcProject`);
    const sites = (project.children ?? []).filter((c) => classOf(c) === "IfcSite");
    if (sites.length !== 1) fail(`expected one IfcSite, found ${sites.length}`);
    const buildings = (sites[0].children ?? []).filter((c) => classOf(c) === "IfcBuilding");
    if (buildings.length !== 1) fail(`expected one IfcBuilding, found ${buildings.length}`);
    const storeys = (buildings[0].children ?? []).filter((c) => classOf(c) === "IfcBuildingStorey");
    if (storeys.length === 0) fail(`the building ${buildings[0].name} has no storeys`);
    let elements = 0;
    let bodies = 0;
    for (const storey of storeys) {
        if (!storey.attributes?.["usd::xformop::transform"]) fail(`${storey.name} has no usd::xformop placement`);
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
                if (!Array.isArray(points) || !Array.isArray(indices)) fail(`${mesh.name} has no triangle list`);
                // The viewer stores the indices in a Uint16Array.
                if (points.length > 65535) fail(`${mesh.name} has more than 65,535 points`);
                if (indices.length % 3 !== 0) fail(`${mesh.name} has ${indices.length} indices, not a multiple of 3`);
                const bad = indices.find((i: unknown) => !Number.isInteger(i) || (i as number) < 0 || (i as number) >= points.length);
                if (bad !== undefined) fail(`${mesh.name} has the index ${bad}, out of range for ${points.length} points`);
                if (!Array.isArray(mesh.attributes!["bsi::ifc::presentation::diffuseColor"])) {
                    fail(`${mesh.name} has no diffuseColor`);
                }
            }
        }
    }
    return { storeys: storeys.length, elements, bodies };
}
