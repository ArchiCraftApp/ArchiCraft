#!/usr/bin/env bash
# The bead gate (docs/loop.md): every check a change must pass before review and commit.
# Runs all steps, prints the tail of each failing step and, when everything passes on an
# unchanged tree, writes the gate stamp the commit guard and land.sh require.
# Usage: scripts/loop/gate.sh
set -uo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
cd "$(loop_root)" || exit 2

dir="$(loop_dir)"
bead="$(loop_bead_from_branch || true)"
tree="$(loop_tree_hash)"
changed="$(loop_changed_files)"
rm -f "$dir/gate.ok" "$dir/gate.fail" "$dir"/gate-*.log "$dir/gate-summary.txt"
failures=()

# step <name> <command...>: runs the command, logs its output, records a failure.
step() {
    local name="$1" log="$dir/gate-$1.log"
    shift
    printf '== %s\n' "$name"
    if "$@" >"$log" 2>&1; then
        echo "   ok"
    else
        failures+=("$name")
        echo "   FAILED (full log: $log)"
        tail -n 60 "$log" | sed 's/^/   | /'
    fi
}
skip() { printf '== %s\n   skipped: %s\n' "$1" "$2"; }
has_label() { [ -n "$bead" ] && loop_bead_has_label "$bead" "$1"; }

step fmt cargo fmt --all --check
step clippy cargo clippy --workspace --all-targets --locked -- -D warnings
step test cargo test --workspace --locked
step layering "$(dirname "${BASH_SOURCE[0]}")/check-layering.sh"

golden_check() {
    local golden
    golden="$(grep -E "$LOOP_GOLDEN_PATHS" <<<"$changed" || true)"
    [ -z "$golden" ] && return 0
    if has_label format-change; then
        echo "golden files changed under label format-change:"
        echo "$golden"
        return 0
    fi
    echo "Golden files changed, but the bead has no label format-change:"
    echo "$golden"
    echo "Restore them, or, for an intended format change, label the bead format-change and"
    echo "regenerate them with ARCHICRAFT_BLESS=1 cargo test -p archicraft-ifcx --test roundtrip."
    return 1
}
step golden golden_check

budget_check() {
    local lines
    # Added plus deleted lines of Rust sources outside tests/, benches/, examples/ and the
    # top-level `#[cfg(test)] mod tests`, new untracked files included.
    lines="$(loop_budget_lines "$(git merge-base "$(loop_base)" HEAD)")"
    echo "non-test source lines changed: $lines (budget $LOOP_DIFF_BUDGET)"
    [ "$lines" -le "$LOOP_DIFF_BUDGET" ] && return 0
    has_label large-change && { echo "over budget, allowed by label large-change"; return 0; }
    echo "Over budget: split the bead into child beads (docs/loop.md, step 2), or label it"
    echo "large-change when the change is mechanical and cannot be split."
    return 1
}
step diff-budget budget_check

beads_check() {
    local base
    [ -z "$bead" ] && return 0
    base="$(git merge-base "$(loop_base)" HEAD)"
    git diff --quiet "$base" -- .beads && return 0
    echo "This bead branch changes .beads/. Bead PRs never carry the beads export (it conflicts"
    echo "across parallel PRs); restore it with: git checkout $base -- .beads"
    return 1
}
step beads-export beads_check

# The same rule as CI's Loop policy (ci-check.sh), checked before the PR exists (archicraft-tvy0).
loop_self_check() {
    local loop_self
    [ -z "$bead" ] && return 0
    loop_self="$(grep -E "$LOOP_SELF_PATHS" <<<"$changed" || true)"
    [ -z "$loop_self" ] && return 0
    if has_label loop; then
        echo "loop guardrails changed under label loop:"
        echo "$loop_self"
        return 0
    fi
    echo "Only a bead labelled loop changes the loop's guardrails (CI's Loop policy refuses it too):"
    echo "$loop_self"
    echo "Restore them and file a bead with label loop for the change (docs/loop.md), or, when"
    echo "this bead is about the loop, label it loop."
    return 1
}
step loop-guardrails loop_self_check

if grep -Eq '^(crates/archicraft-ifcx/|conformance/)' <<<"$changed"; then
    if command -v node >/dev/null; then
        conformance() {
            cargo run --locked -q -p archicraft-ifcx --example reference_models -- target/reference-models &&
                conformance/run.sh target/reference-models/*.ifcx crates/archicraft-ifcx/tests/golden/*.ifcx
        }
        step conformance conformance
    else
        skip conformance "node is not installed; CI runs it"
    fi
else
    skip conformance "no IFCX or conformance changes"
fi

if grep -Eq '(^|/)Cargo\.(toml|lock)$|^deny\.toml$' <<<"$changed"; then
    if command -v cargo-deny >/dev/null; then
        step licences cargo deny check licenses sources bans
    else
        skip licences "cargo-deny is not installed; CI runs it (cargo install cargo-deny)"
    fi
else
    skip licences "no manifest changes"
fi

if [ "${#failures[@]}" -gt 0 ]; then
    # Remembered per tree, so the Stop hook does not rerun a gate that already failed on it.
    loop_stamp_write gate.fail "$tree" "${bead:--}"
    {
        echo "GATE FAIL: ${failures[*]}"
        for f in "${failures[@]}"; do
            echo "-- $f (full log: $dir/gate-$f.log)"
            tail -n 25 "$dir/gate-$f.log"
        done
    } >"$dir/gate-summary.txt"
    echo
    echo "GATE FAIL: ${failures[*]}"
    exit 1
fi
if [ "$(loop_tree_hash)" != "$tree" ]; then
    echo
    echo "GATE FAIL: the working tree changed while the gate ran; run it again."
    exit 1
fi
loop_stamp_write gate.ok "$tree" "${bead:--}"
echo
echo "GATE PASS (tree $tree)"
