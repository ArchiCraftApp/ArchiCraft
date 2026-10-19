#!/usr/bin/env bash
# SubagentStop (bead-reviewer): turns the reviewer's last line, VERDICT: PASS or VERDICT: FAIL,
# into the review stamp of the current tree. A PASS counts only on a tree that passed the gate.
# The reviewer cannot write stamps itself, and the main session cannot fake a review.
set -uo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

[ "$HOOK_AGENT" = bead-reviewer ] || exit 0
hook_enter_repo "$HOOK_CWD" || exit 0

dir="$(loop_dir)"
message="$(hook_field .last_assistant_message)"
verdict="$(grep -Eo 'VERDICT: (PASS|FAIL)' <<<"$message" | tail -n 1 | cut -d' ' -f2)"
tree="$(loop_tree_hash)"
bead="$(loop_bead_from_branch || echo -)"
rm -f "$dir/review.ok"
printf '%s\n' "$message" | grep -v '^VERDICT:' >"$dir/review.md"

if [ "$verdict" = PASS ] && loop_stamp_valid gate.ok "$tree"; then
    loop_stamp_write review.ok "$tree" "$bead"
    jq -n --arg m "Review PASS stamped for $bead (tree ${tree:0:12})." '{systemMessage: $m}'
elif [ "$verdict" = PASS ]; then
    jq -n --arg m "Review PASS not stamped for $bead: the tree has no passing gate (it changed after scripts/loop/gate.sh). Run the gate, then review again." '{systemMessage: $m}'
else
    jq -n --arg m "Review ${verdict:-without verdict} for $bead: fix the findings, run the gate and review again." '{systemMessage: $m}'
fi
exit 0
