#!/usr/bin/env bash
# Lets a bead session stop with a failing or unreviewed tree, for example to ask the user a
# question or to hand the bead to a human. The Stop hook lets this exact tree stop; any later
# edit re-arms it. Records the reason on the bead.
# Usage: scripts/loop/pause.sh "<reason>"
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
cd "$(loop_root)"

reason="${1:?usage: scripts/loop/pause.sh \"<reason>\"}"
loop_stamp_write pause "$(loop_tree_hash)" "$(loop_bead_from_branch || echo -)"
if id="$(loop_bead_from_branch)"; then
    bd update "$id" --append-notes "Paused $(date -u +%Y-%m-%d): $reason" >/dev/null
fi
echo "Paused: this tree may stop. Reason recorded."
