#!/usr/bin/env python3
"""The CI regression gate of the benches (SRS NFR-PERF-001, docs/performance/README.md).

Reads the criterion results of one bench job: the benches ran on the merge base with
`--save-baseline base` and then on the pull request with `--baseline-lenient base`, on the same
runner (scripts/bench-ci.sh). Criterion wrote, per bench, `base/estimates.json` (merge base),
`new/estimates.json` (pull request) and `change/estimates.json` (the relative change of the mean
with its 95 % confidence interval).

A bench fails the gate when the lower bound of its mean change's confidence interval is above
the threshold (+20 %): the slowdown is then larger than the threshold even at the favourable end
of the interval, so runner noise alone does not fail it. A bench without a baseline (new on this
branch) is reported and does not fail. `--accepted` (the pull-request label
perf-regression-accepted) passes a run with regressions and names them in the log.

Writes a markdown table (bench id, base, head, change) to `--summary` (in CI the job summary,
$GITHUB_STEP_SUMMARY) and a plain report to stdout. Exit codes: 0 pass, 1 regression, 2 no
results. Python 3 standard library only.

Usage: scripts/bench-gate.py [--criterion-dir target/criterion] [--threshold 0.20]
                             [--accepted] [--summary FILE] [--list-regressions]
"""

import argparse
import json
import os
import sys
from pathlib import Path

ACCEPT_LABEL = "perf-regression-accepted"


def read_json(path):
    try:
        return json.loads(path.read_text())
    except (OSError, ValueError):
        return None


def collect(criterion_dir):
    """One entry per bench that ran on this branch, sorted by bench id."""
    results = []
    for meta_path in Path(criterion_dir).glob("**/new/benchmark.json"):
        bench_dir = meta_path.parent.parent
        meta = read_json(meta_path) or {}
        head = read_json(bench_dir / "new" / "estimates.json")
        if head is None:
            continue
        base = read_json(bench_dir / "base" / "estimates.json")
        change = read_json(bench_dir / "change" / "estimates.json")
        results.append(
            {
                "id": meta.get("full_id") or str(bench_dir.relative_to(criterion_dir)),
                "head_ns": head["mean"]["point_estimate"],
                "base_ns": base["mean"]["point_estimate"] if base else None,
                "change": change["mean"] if change else None,
            }
        )
    return sorted(results, key=lambda r: r["id"])


def duration(ns):
    """A time for the table, with three significant digits."""
    if ns is None:
        return "–"
    for unit, scale in (("s", 1e9), ("ms", 1e6), ("µs", 1e3)):
        if ns >= scale:
            value = ns / scale
            return f"{value:.2f} {unit}" if value < 10 else f"{value:.1f} {unit}"
    return f"{ns:.0f} ns"


def percent(fraction):
    sign = "+" if fraction >= 0 else "−"
    return f"{sign}{abs(fraction) * 100:.1f} %"


def judge(result, threshold):
    """'regression', 'ok' or 'new' (no comparison)."""
    change = result["change"]
    if change is None:
        return "new"
    return "regression" if change["confidence_interval"]["lower_bound"] > threshold else "ok"


def change_cell(result, verdict):
    change = result["change"]
    if change is None:
        if result["base_ns"] is None:
            return "new, not in the base"
        return "not compared (not run on this branch)"
    ci = change["confidence_interval"]
    cell = f"{percent(change['point_estimate'])} [{percent(ci['lower_bound'])}, {percent(ci['upper_bound'])}]"
    return f"{cell} **regression**" if verdict == "regression" else cell


def report(results, threshold, accepted):
    """(exit code, stdout lines, summary markdown)."""
    verdicts = [(r, judge(r, threshold)) for r in results]
    regressions = [r["id"] for r, v in verdicts if v == "regression"]
    new = [r["id"] for r, v in verdicts if v == "new"]
    limit = percent(threshold)

    lines = []
    for r, v in verdicts:
        lines.append(f"{v:10} {r['id']}: {duration(r['base_ns'])} -> {duration(r['head_ns'])}  {change_cell(r, v)}")
    if new:
        lines.append(f"Not compared (no baseline on the merge base): {', '.join(new)}")
    if regressions and accepted:
        verdict = (
            f"Regression accepted by the pull-request label {ACCEPT_LABEL}: {', '.join(regressions)} "
            f"(lower bound of the mean change above {limit})."
        )
        code = 0
    elif regressions:
        verdict = (
            f"Performance regression: {', '.join(regressions)} (lower bound of the mean change above "
            f"{limit}). A reviewer who accepts it adds the label {ACCEPT_LABEL} and re-runs the job."
        )
        code = 1
    else:
        verdict = f"No regression above {limit} in {len(results)} benches."
        code = 0
    lines.append(verdict)

    summary = [
        "## Benches: merge base against this commit",
        "",
        f"Mean time per iteration; the change is the mean's relative change with its 95 % confidence "
        f"interval. A bench fails when the interval's lower bound is above {limit}.",
        "",
        "| Bench | Base | Head | Change |",
        "|---|---|---|---|",
    ]
    for r, v in verdicts:
        summary.append(f"| `{r['id']}` | {duration(r['base_ns'])} | {duration(r['head_ns'])} | {change_cell(r, v)} |")
    summary += ["", f"**{verdict}**", ""]
    return code, lines, "\n".join(summary)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--criterion-dir", default="target/criterion")
    parser.add_argument("--threshold", type=float, default=0.20)
    parser.add_argument("--accepted", action="store_true", help=f"the pull request has the label {ACCEPT_LABEL}")
    parser.add_argument("--summary", default=os.environ.get("GITHUB_STEP_SUMMARY"))
    parser.add_argument(
        "--list-regressions", action="store_true", help="only print the ids of the failing benches, one per line"
    )
    args = parser.parse_args(argv)

    if args.list_regressions:
        for r in collect(args.criterion_dir):
            if judge(r, args.threshold) == "regression":
                print(r["id"])
        return 0

    results = collect(args.criterion_dir)
    if not results:
        print(f"bench-gate: no bench results in {args.criterion_dir}; did the benches run?")
        return 2
    code, lines, summary = report(results, args.threshold, args.accepted)
    print("\n".join(lines))
    if args.summary:
        with open(args.summary, "a", encoding="utf-8") as f:
            f.write(summary + "\n")
    return code


if __name__ == "__main__":
    sys.exit(main())
