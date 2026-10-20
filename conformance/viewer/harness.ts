// Harness for buildingSMART's reference viewer (src/viewer at the commit SRS R2 pins) in a
// headless browser, for SRS IFX-006 (b).
//
// window.loadIfcx(json) hands the file to the viewer's own entry point, addModel() of
// render.ts, which composes it (compose3), builds the three.js scene, renders it and builds
// the model tree. The tree model returned is read back from what the viewer shows: the
// nesting of its model tree and, for every node, the attribute table the viewer fills when
// the node is clicked. Nothing of the viewer's logic is repeated here.

import addModel from "../.ifc5/src/viewer/render";
import type { TreeNode } from "../tree-check";

declare global {
    interface Window {
        loadIfcx(json: unknown): Promise<TreeNode>;
    }
}

const errors: string[] = [];
window.addEventListener("error", (e) => errors.push(e.message));
window.addEventListener("unhandledrejection", (e) => errors.push(String(e.reason)));

function frame(): Promise<void> {
    return new Promise((resolve) => requestAnimationFrame(() => resolve()));
}

// The attribute table prints arrays as "(a,b,(c,d))" and objects as JSON; anything else is
// the text the viewer shows.
function parseValue(text: string): unknown {
    if (text.startsWith("(") || text.startsWith("{")) {
        try {
            return JSON.parse(text.startsWith("(") ? text.replace(/\(/g, "[").replace(/\)/g, "]") : text);
        } catch {
            return text;
        }
    }
    return text;
}

function readAttributeTable(): { [key: string]: unknown } {
    const attributes: { [key: string]: unknown } = {};
    for (const row of document.querySelectorAll(".attributes .table tr")) {
        const [key, value] = Array.from(row.children, (cell) => cell.textContent ?? "");
        if (key !== "name") attributes[key] = parseValue(value);
    }
    return attributes;
}

function readNode(element: HTMLElement): TreeNode {
    const name = element.dataset.path ?? "";
    const node: TreeNode = { name };
    // The root has no attributes, and the viewer's click handler fails on it.
    if (name !== "") {
        element.click();
        node.attributes = readAttributeTable();
    }
    const children = Array.from(element.children).filter((c): c is HTMLDivElement => c instanceof HTMLDivElement);
    if (children.length > 0) node.children = children.map(readNode);
    return node;
}

window.loadIfcx = async (json: unknown): Promise<TreeNode> => {
    await addModel("model.ifcx", json as any);
    const root = document.querySelector<HTMLElement>(".tree > div");
    if (!root) throw new Error("the viewer built no model tree");
    const tree = readNode(root);
    // Clear the selection (and its red highlight) the way the viewer does: a click on the
    // canvas that hits nothing.
    document
        .querySelector(".viewport canvas")
        ?.dispatchEvent(new MouseEvent("click", { clientX: -1e5, clientY: -1e5 }));
    if (document.querySelector(".tree .selected")) throw new Error("the viewer kept a selection");
    await frame();
    await frame();
    if (errors.length > 0) throw new Error(`the viewer reported errors: ${errors.join("; ")}`);
    return tree;
};
