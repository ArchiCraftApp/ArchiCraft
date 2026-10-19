#!/usr/bin/env bash
# Start of every loop iteration (docs/loop.md): closes in-progress beads whose PR has merged,
# closes loop-split parents whose children are all closed, and reports what still waits: open PRs,
# PRs closed without merging, beads flagged for a human, and epics ready for their demo.
# Read-only towards GitHub; it checks each PR once and never polls.
# Usage: scripts/loop/sweep.sh
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
command -v gh >/dev/null || { echo "gh is not installed: brew install gh && gh auth login"; exit 1; }

bd list --status in_progress --json --limit 0 |
    jq -r '.[] | select((.external_ref // "") | test("/pull/[0-9]+$")) | "\(.id) \(.external_ref)"' |
    while read -r id url; do
        state="$(gh pr view "$url" --json state -q .state 2>/dev/null || echo UNKNOWN)"
        case "$state" in
            MERGED) bd close "$id" --reason "Merged in $url" >/dev/null && echo "closed   $id (merged $url)" ;;
            OPEN) echo "waiting  $id ($url, CI or auto-merge pending)" ;;
            CLOSED) echo "ATTENTION $id: $url was closed without merging" ;;
            *) echo "unknown  $id ($url)" ;;
        esac
    done

# A bead the loop split (label loop-split; plain "split" is an area label of the split tool) is
# a container: it closes once it has children and all of them are closed.
bd list --label loop-split --json --limit 0 | jq -r '.[] | select(.status != "closed") | .id' |
    while read -r id; do
        read -r total open < <(bd children "$id" --json | jq -r '"\(length) \([.[] | select(.status != "closed")] | length)"')
        if [ "${total:-0}" -gt 0 ] && [ "$open" = 0 ]; then
            bd close "$id" --reason "All child beads closed" >/dev/null && echo "closed   $id (split, children done)"
        fi
    done

humans="$(bd list --label human --json --limit 0 | jq -r '.[] | select(.status != "closed") | "\(.id) \(.title)"')"
[ -n "$humans" ] && { echo "needs a human:"; sed 's/^/  /' <<<"$humans"; }

demo="$(bd epic close-eligible --dry-run 2>/dev/null | grep -E 'archicraft-' || true)"
[ -n "$demo" ] && { echo "epics with every task closed, ready for the demo:"; sed 's/^/  /' <<<"$demo"; }
exit 0
