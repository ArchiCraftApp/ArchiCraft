// Prints one PASS or FAIL line per test of viewer.spec.ts, in the format check.ts uses for
// steps (a) and (b1), so run.sh reports every file and step the same way (SRS IFX-006).

import type { FullResult, Reporter, TestCase, TestError, TestResult } from "@playwright/test/reporter";

function firstLine(text: string | undefined): string {
    // eslint-disable-next-line no-control-regex
    return (text ?? "").replace(/\u001b\[[0-9;]*m/g, "").split("\n").find((l) => l.trim() !== "") ?? "";
}

export default class StepReporter implements Reporter {
    onTestEnd(test: TestCase, result: TestResult): void {
        const name = test.parent.type === "describe" ? `${test.parent.title}: ${test.title}` : test.title;
        const annotations = [...(result.annotations ?? []), ...test.annotations];
        const detail =
            result.status === "passed"
                ? annotations.find((a) => a.type === "summary")?.description ?? ""
                : firstLine(result.error?.message) || result.status;
        console.log(`${result.status === "passed" ? "PASS" : "FAIL"} (b2) viewer ${name}: ${detail}`);
    }

    onError(error: TestError): void {
        console.log(`FAIL (b2) viewer: ${firstLine(error.message)}`);
    }

    onEnd(result: FullResult): void {
        if (result.status !== "passed") console.log(`(b2) viewer: ${result.status}`);
    }

    printsToStdio(): boolean {
        return true;
    }
}
