#!/usr/bin/env bash
# Where this worktree stands in the bead loop: branch, bead, and whether the gate and the
# review cover the current tree.
# Usage: scripts/loop/status.sh
set -uo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
cd "$(loop_root)" || exit 2

branch="$(loop_branch)"
if ! bead="$(loop_bead_from_branch)"; then
    echo "Bead loop: '$branch' is not a bead branch. Implementation work starts with /bead (docs/loop.md)."
    exit 0
fi
tree="$(loop_tree_hash)"
state() { if loop_stamp_valid "$1" "$tree"; then echo "$2"; else echo "$3"; fi; }
echo "Bead loop: $bead on $branch ($(loop_bead_json "$bead" | jq -r '.status // "unknown"'))"
echo "  uncommitted code: $(loop_code_dirty && echo yes || echo no)"
echo "  gate:   $(state gate.ok "passed on this tree" "$(state gate.fail "failed on this tree" "not run on this tree")")"
if loop_stamp_valid review.ok "$tree"; then
    review="passed on this tree"
elif loop_review_valid "$tree"; then
    review="passed before changes outside code: $(loop_changed_since_review "$tree" | paste -sd ' ' -)"
else
    review="not passed on this tree"
fi
echo "  review: $review"
echo "  paused: $(state pause yes no)"
pr="$(loop_bead_json "$bead" | jq -r '.external_ref // ""')"
[ -n "$pr" ] && echo "  PR:     $pr"
exit 0
