#!/usr/bin/env bash
# The stop rule of the bead loop (docs/loop.md): the gate failed three times on the same problem
# or the review failed twice. The first time, a bead below tier max moves one tier up (medium,
# high, xhigh, max: the tier:<x> label) and goes back to the ready beads, so that the next batch
# of /bead-loop resumes its branch in a fresh session on the stronger tier. That session finds
# this run's unreviewed work as a patch (loop_escalation_patch); this worktree lets go of the
# branch (detached HEAD) and keeps its changes. A bead at tier max or fable, or one escalated
# before, gets the label human instead. Either way this tree may stop (pause.sh records why).
# Usage: scripts/loop/escalate.sh "<what blocks, what was tried>"
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$here/lib.sh"
cd "$(loop_root)"

reason="${1:?usage: scripts/loop/escalate.sh \"<what blocks, what was tried>\"}"
id="$(loop_bead_from_branch)" || { echo "Not on a bead branch (bead/<id>-<slug>)."; exit 1; }
branch="$(loop_branch)"
labels="$(loop_bead_json "$id" | jq -r '(.labels // [])[]')"
tier="$(sed -n 's/^tier://p' <<<"$labels" | head -n 1)"
next=""
if ! grep -qx escalated <<<"$labels"; then
    # A bead without a tier label ran at the default tier, high.
    case "${tier:-high}" in medium) next=high ;; high) next=xhigh ;; xhigh) next=max ;; esac
fi

bash "$here/pause.sh" "$reason"

if [ -z "$next" ]; then
    bd update "$id" --add-label human >/dev/null
    why="it ran at tier ${tier:-high}"
    grep -qx escalated <<<"$labels" && why="it was escalated before"
    echo "$id gets the label human: $why."
    exit 0
fi

# The work of this run, untracked files included (intent-to-add puts them into the diff).
patch="$(loop_escalation_patch "$id")"
mkdir -p "$(dirname "$patch")"
git add --intent-to-add -- . ':(exclude).beads'
git diff --binary HEAD -- . ':(exclude).beads' >"$patch"
if [ -s "$patch" ]; then
    saved="Unreviewed work of this run: $patch."
else
    rm -f "$patch"
    saved="This run left no uncommitted work."
fi

update=(--add-label "tier:$next" --add-label escalated)
[ -n "$tier" ] && update+=(--remove-label "tier:$tier")
bd update "$id" "${update[@]}" --append-notes "Escalated $(date -u +%Y-%m-%d) from tier ${tier:-high} to tier $next; the next session resumes $branch. $saved" >/dev/null
bd unclaim "$id" >/dev/null
# Another worktree can check the branch out only once this one lets go of it. The beads
# post-checkout hook would re-import the whole JSONL export (minutes); skip git hooks.
git -c core.hooksPath=/dev/null switch --quiet --detach

echo "$id moves up from tier ${tier:-high} to tier $next and is ready again: the next /bead-loop batch resumes $branch in a fresh session."
echo "This worktree let go of the branch (detached HEAD) and keeps its changes. $saved"
