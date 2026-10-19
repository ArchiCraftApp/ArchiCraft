#!/usr/bin/env bash
# Stop: a bead session does not end with uncommitted code that has not passed the gate and the
# review. Runs the gate when the tree has no result yet (a failure is remembered per tree, so
# stopping again without edits does not rerun it). After three blocks on the same tree it lets
# the session stop and asks for the bead to be flagged. scripts/loop/pause.sh stops on purpose.
set -uo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

hook_enter_repo "$HOOK_CWD" || exit 0
bead="$(loop_bead_from_branch)" || exit 0
loop_code_dirty || exit 0

dir="$(loop_dir)"
tree="$(loop_tree_hash)"
loop_stamp_valid pause "$tree" && exit 0

count=0
[ -f "$dir/stop-blocks" ] && read -r last_tree last_count <"$dir/stop-blocks"
[ "${last_tree:-}" = "$tree" ] && count="${last_count:-0}"
if [ "$count" -ge 3 ]; then
    jq -n --arg m "Bead $bead stops with uncommitted code that has not passed the gate and the review (blocked 3 times). Flag it: bd update $bead --add-label human --append-notes \"<what blocks>\"" \
        '{systemMessage: $m}'
    exit 0
fi
echo "$tree $((count + 1))" >"$dir/stop-blocks"

way_out="If you must stop here (a question for the user, a blocker), run scripts/loop/pause.sh \"<reason>\" first."
if loop_stamp_valid gate.ok "$tree"; then
    if loop_stamp_valid review.ok "$tree"; then
        hook_block "Bead $bead passed the gate and the review but is not committed. Commit it and run scripts/loop/land.sh (step 7 of /bead). $way_out"
    fi
    hook_block "Bead $bead passed the gate; the review is missing. Launch the bead-reviewer agent with 'Review bead $bead' (step 6 of /bead). $way_out"
fi
if ! loop_stamp_valid gate.fail "$tree"; then
    "$HOOK_ROOT/scripts/loop/gate.sh" >/dev/null 2>&1
    if loop_stamp_valid gate.ok "$tree"; then
        hook_block "The gate passed for bead $bead; the review is missing. Launch the bead-reviewer agent with 'Review bead $bead' (step 6 of /bead). $way_out"
    fi
fi
summary="$(head -c 6000 "$dir/gate-summary.txt" 2>/dev/null || echo "The gate failed; run scripts/loop/gate.sh for details.")"
hook_block "Bead $bead has uncommitted code that fails the gate. Fix the causes, then run scripts/loop/gate.sh again. $way_out

$summary"
