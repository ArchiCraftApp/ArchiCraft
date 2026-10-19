#!/usr/bin/env bash
# CI half of the bead loop (docs/loop.md): rules a pull request keeps whatever happened locally.
# Environment: HEAD_REF (PR branch), BASE_SHA (PR base), PR_TITLE, PR_LABELS (comma-separated).
# Usage: scripts/loop/ci-check.sh
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
cd "$(loop_root)"

: "${HEAD_REF:?}" "${BASE_SHA:?}" "${PR_TITLE:=}" "${PR_LABELS:=}"
changed="$(git diff --name-only "$BASE_SHA"...HEAD)"
labelled() { [[ ",$PR_LABELS," == *",$1,"* ]]; }
status=0
error() { echo "::error::$1"; status=1; }

golden="$(grep -E "$LOOP_GOLDEN_PATHS" <<<"$changed" || true)"
if [ -n "$golden" ] && ! labelled format-change; then
    error "Golden files changed without the label format-change: $(tr '\n' ' ' <<<"$golden")"
fi

if bead="$(loop_bead_from_branch "$HEAD_REF")"; then
    [[ "$PR_TITLE" == "$bead: "* ]] || error "A bead PR's title starts with '$bead: ' (it becomes the squash commit subject)."
    grep -q '^\.beads/' <<<"$changed" && error "A bead PR does not change .beads/ (the export conflicts across parallel PRs)."
    loop_self="$(grep -E "$LOOP_SELF_PATHS" <<<"$changed" || true)"
    if [ -n "$loop_self" ] && ! labelled loop; then
        error "Only a bead labelled loop changes the loop's guardrails: $(tr '\n' ' ' <<<"$loop_self")"
    fi
fi

[ "$status" -eq 0 ] && echo "Loop policy: ok"
exit "$status"
