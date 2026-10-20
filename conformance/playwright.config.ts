// Step (b2) of the IFX-006 conformance check: buildingSMART's reference viewer in headless
// Chromium (viewer.spec.ts). Run through run.sh, which builds the harness and passes the files.

import { defineConfig } from "@playwright/test";

export default defineConfig({
    testDir: ".",
    testMatch: "viewer.spec.ts",
    outputDir: "test-results",
    reporter: [["./reporter.ts"]],
    fullyParallel: true,
    forbidOnly: true,
    retries: 0,
    timeout: 60_000,
    use: {
        browserName: "chromium",
        viewport: { width: 1280, height: 800 },
        deviceScaleFactor: 1,
        // WebGL without a GPU: SwiftShader, as on the CI runners.
        launchOptions: { args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"] },
    },
});
