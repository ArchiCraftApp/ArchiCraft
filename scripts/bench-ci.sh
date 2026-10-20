#!/usr/bin/env bash
# The bench regression gate (SRS NFR-PERF-001, docs/performance/README.md): runs the benches in
# smoke mode on <base-rev> (checked out in a temporary git worktree) and on this working tree,
# on the same machine, and fails when a bench got more than 20 % slower
# (scripts/bench-gate.py). CI's bench job runs it with the pull request's merge base; locally it
# compares the working tree, uncommitted changes included, with any revision:
#
#   scripts/bench-ci.sh origin/main
#
# Base and head are measured on one machine in one run, never against numbers stored from other
# runs, and interleaved bench by bench (the base's run of a bench, then the head's), so a change
# in the machine's load during the job shifts both sides of a comparison alike. A bench that
# fails is measured once more, base then head, and fails the gate only if it fails again.
#
# Options after the revision go to bench-gate.py (--accepted, --summary FILE). The criterion
# results land in $CRITERION_HOME (default target/criterion), which is emptied first.
set -euo pipefail

if [ $# -lt 1 ]; then
    echo "usage: scripts/bench-ci.sh <base-rev> [bench-gate.py options]" >&2
    exit 2
fi
base_rev="$(git rev-parse --verify "$1^{commit}")"
shift

root="$(git rev-parse --show-toplevel)"
cd "$root"
gate="$root/scripts/bench-gate.py"
# One target directory for both builds: the dependencies build once, and criterion writes the
# baseline and the comparison into the same tree.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/target}"
export CRITERION_HOME="${CRITERION_HOME:-$CARGO_TARGET_DIR/criterion}"
# Smoke mode: only the bench models flagged ci (samples::BENCH_CI_ENV).
export ARCHICRAFT_BENCH_CI=1
smoke=(--warm-up-time 1 --measurement-time 3 --sample-size 20)
packages=(-p archicraft-geometry -p archicraft-ifcx)

rm -rf "$CRITERION_HOME"
worktree="$(mktemp -d)/base"
cleanup() {
    git worktree remove --force "$worktree" 2>/dev/null || true
    rm -rf "$(dirname "$worktree")"
}
trap cleanup EXIT
git worktree add --quiet --detach "$worktree" "$base_rev"

# The bench executables of the checkout in the current directory, one per line. Both checkouts
# build into one target directory, where cargo would take the other checkout's build of the
# workspace crates as up to date (it compares file times, and the paths are the same relative to
# the workspace): clean them first, so only the dependencies are shared.
bench_executables() {
    local members
    members="$(cargo metadata --locked --no-deps --format-version 1 |
        python3 -c 'import json, sys; print(" ".join("-p " + p["name"] for p in json.load(sys.stdin)["packages"]))')"
    # shellcheck disable=SC2086 # "-p <name>" words
    cargo clean --release --quiet $members
    cargo bench --locked --no-run --message-format=json "${packages[@]}" |
        python3 -c 'import json, sys
for line in sys.stdin:
    m = json.loads(line)
    if m.get("reason") == "compiler-artifact" and "bench" in m["target"]["kind"] and m.get("executable"):
        print(m["executable"])'
}

# "<executable> <bench id>" for every bench of the given executables.
bench_ids() {
    local exe
    for exe in "$@"; do
        "$exe" --bench --list | sed -n "s|: benchmark\$||p" | sed "s|^|$exe |"
    done
}

lists="$(dirname "$worktree")"
echo "== base: $(git log -1 --format='%h %s' "$base_rev")"
# A base whose benches do not build leaves every bench without a baseline: reported as new by
# the gate, not a failure of this change.
if (cd "$worktree" && bench_executables) >"$lists/base-exes"; then
    # Cargo names a bench executable the same in both checkouts of the workspace, so the head's
    # build would overwrite the base's: copy them aside first.
    mkdir "$lists/base-bin"
    while read -r exe; do cp "$exe" "$lists/base-bin/"; done <"$lists/base-exes"
    bench_ids "$lists"/base-bin/* >"$lists/base-ids"
else
    echo "::warning::the benches do not build on the base $base_rev; nothing is compared"
    : >"$lists/base-ids"
fi
echo "== head: $(git log -1 --format='%h %s')$(git diff --quiet HEAD -- || echo ' (with uncommitted changes)')"
bench_executables >"$lists/head-exes"
# shellcheck disable=SC2046
bench_ids $(cat "$lists/head-exes") >"$lists/head-ids"
[ -s "$lists/head-ids" ] || { echo "no benches on the head" >&2; exit 2; }

# Measures one bench: the base (saved as baseline "base"), then the head against it.
measure() {
    local id="$1" exe="$2" base_exe
    base_exe="$(awk -v id="$id" '$2 == id { print $1; exit }' "$lists/base-ids")"
    if [ -n "$base_exe" ]; then
        (cd "$worktree" && "$base_exe" --bench --exact "$id" "${smoke[@]}" --save-baseline base </dev/null)
    fi
    "$exe" --bench --exact "$id" "${smoke[@]}" --baseline-lenient base </dev/null
}

while read -r exe id; do measure "$id" "$exe"; done <"$lists/head-ids"

regressions="$(python3 "$gate" --criterion-dir "$CRITERION_HOME" --list-regressions)"
if [ -n "$regressions" ]; then
    echo "== measuring again: $(tr '\n' ' ' <<<"$regressions")"
    while read -r exe id; do
        if grep -qxF "$id" <<<"$regressions"; then measure "$id" "$exe"; fi
    done <"$lists/head-ids"
fi

python3 "$gate" --criterion-dir "$CRITERION_HOME" "$@"
