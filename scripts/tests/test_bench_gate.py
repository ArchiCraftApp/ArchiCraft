#!/usr/bin/env python3
"""Tests of scripts/bench-gate.py, the CI regression gate of the benches (SRS NFR-PERF-001).

Each test writes canned criterion results (benchmark.json, base/new estimates and the change
estimates of a `--baseline-lenient base` run) into a temporary directory and runs the gate on
them. Python 3 standard library only: python3 scripts/tests/test_bench_gate.py
"""

import importlib.util
import io
import json
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

GATE = Path(__file__).resolve().parent.parent / "bench-gate.py"
spec = importlib.util.spec_from_file_location("bench_gate", GATE)
bench_gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench_gate)


def estimates(mean, lower=None, upper=None):
    """A criterion estimates.json with only the parts the gate reads."""
    lower = mean if lower is None else lower
    upper = mean if upper is None else upper
    return {
        "mean": {
            "confidence_interval": {"confidence_level": 0.95, "lower_bound": lower, "upper_bound": upper},
            "point_estimate": mean,
            "standard_error": 0.0,
        }
    }


class CannedCriterion:
    """A criterion output directory with canned results."""

    def __init__(self, root):
        self.root = Path(root)

    def bench(self, full_id, head_ns, base_ns=None, change=None):
        """One bench: `change` is (mean, lower, upper) as relative changes, None when the
        bench has no baseline (new on this branch)."""
        group, function = full_id.rsplit("/", 1)
        directory = self.root / group.replace("/", "_") / function
        meta = {"group_id": group, "function_id": function, "full_id": full_id}
        self.write(directory / "new" / "benchmark.json", meta)
        self.write(directory / "new" / "estimates.json", estimates(head_ns))
        if base_ns is not None:
            self.write(directory / "base" / "benchmark.json", meta)
            self.write(directory / "base" / "estimates.json", estimates(base_ns))
        if change is not None:
            self.write(directory / "change" / "estimates.json", estimates(*change))

    @staticmethod
    def write(path, data):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(data))


class BenchGateTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.criterion = CannedCriterion(self.tmp.name)
        self.summary = Path(self.tmp.name) / "summary.md"

    def tearDown(self):
        self.tmp.cleanup()

    def run_gate(self, *extra):
        out = io.StringIO()
        with redirect_stdout(out):
            code = bench_gate.main(["--criterion-dir", self.tmp.name, "--summary", str(self.summary), *extra])
        summary = self.summary.read_text() if self.summary.exists() else ""
        return code, out.getvalue(), summary

    def test_a_regression_of_25_percent_above_the_threshold_fails(self):
        """NFR-PERF-001: +25 % with the confidence interval's lower bound above +20 % fails."""
        self.criterion.bench("geometry/evaluate/tiled_shell", 1_250_000, 1_000_000, (0.25, 0.22, 0.28))
        code, out, summary = self.run_gate()
        self.assertEqual(code, 1)
        self.assertIn("geometry/evaluate/tiled_shell", out)
        self.assertIn("regression", out.lower())
        self.assertIn("| `geometry/evaluate/tiled_shell` | 1.00 ms | 1.25 ms | +25.0 %", summary)

    def test_a_noisy_25_percent_whose_lower_bound_is_below_the_threshold_passes(self):
        """The gate looks at the lower bound, so noise on a shared runner does not fail it."""
        self.criterion.bench("geometry/evaluate/tiled_shell", 1_250_000, 1_000_000, (0.25, 0.12, 0.38))
        code, _, _ = self.run_gate()
        self.assertEqual(code, 0)

    def test_a_change_of_15_percent_passes(self):
        """NFR-PERF-001: +15 % is below the 20 % threshold."""
        self.criterion.bench("ifcx/save/two_storey_shell", 115_000, 100_000, (0.15, 0.12, 0.18))
        code, _, summary = self.run_gate()
        self.assertEqual(code, 0)
        self.assertIn("| `ifcx/save/two_storey_shell` | 100.0 µs | 115.0 µs | +15.0 %", summary)

    def test_an_improvement_of_30_percent_passes(self):
        """NFR-PERF-001: a faster bench never fails."""
        self.criterion.bench("ifcx/load/tiled_shell", 70_000_000, 100_000_000, (-0.30, -0.33, -0.27))
        code, _, summary = self.run_gate()
        self.assertEqual(code, 0)
        self.assertIn("| `ifcx/load/tiled_shell` | 100.0 ms | 70.0 ms | −30.0 %", summary)

    def test_a_bench_missing_in_the_base_is_reported_and_does_not_fail(self):
        """A bench new on this branch has nothing to compare with: listed, not a failure."""
        self.criterion.bench("ifcx/save/two_storey_shell", 100_000, 100_000, (0.0, -0.02, 0.02))
        self.criterion.bench("geometry/new_bench/two_storey_shell", 50_000)
        code, out, summary = self.run_gate()
        self.assertEqual(code, 0)
        self.assertIn("geometry/new_bench/two_storey_shell", out)
        self.assertIn("| `geometry/new_bench/two_storey_shell` | – | 50.0 µs | new, not in the base |", summary)

    def test_the_accepted_label_overrides_a_regression_with_a_message(self):
        """A reviewer's perf-regression-accepted label passes the gate and names the
        accepted regressions in the log and the job summary."""
        self.criterion.bench("geometry/evaluate/tiled_shell", 1_250_000, 1_000_000, (0.25, 0.22, 0.28))
        code, out, summary = self.run_gate("--accepted")
        self.assertEqual(code, 0)
        for text in (out, summary):
            self.assertIn("perf-regression-accepted", text)
            self.assertIn("geometry/evaluate/tiled_shell", text)

    def test_the_summary_lists_every_bench_sorted_by_id(self):
        """The job summary publishes the numbers of every bench of the run."""
        self.criterion.bench("ifcx/save/two_storey_shell", 100_000, 100_000, (0.0, -0.02, 0.02))
        self.criterion.bench("geometry/plan/two_storey_shell", 2_000, 2_000, (0.0, -0.02, 0.02))
        _, _, summary = self.run_gate()
        rows = [line for line in summary.splitlines() if line.startswith("| `")]
        self.assertEqual(
            [r.split("`")[1] for r in rows], ["geometry/plan/two_storey_shell", "ifcx/save/two_storey_shell"]
        )

    def test_list_regressions_names_only_the_failing_benches(self):
        """bench-ci.sh measures the failing benches once more: --list-regressions names them,
        one per line, and does not fail."""
        self.criterion.bench("geometry/evaluate/tiled_shell", 1_250_000, 1_000_000, (0.25, 0.22, 0.28))
        self.criterion.bench("ifcx/save/two_storey_shell", 115_000, 100_000, (0.15, 0.12, 0.18))
        out = io.StringIO()
        with redirect_stdout(out):
            code = bench_gate.main(["--criterion-dir", self.tmp.name, "--list-regressions"])
        self.assertEqual(code, 0)
        self.assertEqual(out.getvalue(), "geometry/evaluate/tiled_shell\n")
        self.assertFalse(self.summary.exists())

    def test_no_results_is_an_error(self):
        """A run whose benches did not run must not pass as 'no regression'."""
        code, out, _ = self.run_gate()
        self.assertEqual(code, 2)
        self.assertIn("no bench results", out)


if __name__ == "__main__":
    unittest.main()
