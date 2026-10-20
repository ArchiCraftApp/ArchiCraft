#!/usr/bin/env bash
# CI half of the bead loop (docs/loop.md): rules a pull request keeps whatever happened locally.
# Environment: HEAD_REF (PR branch), BASE_SHA (PR base), PR_TITLE, and either PR_NUMBER (CI: the
# labels are read live with gh, which needs GH_TOKEN) or PR_LABELS (comma-separated).
# Usage: scripts/loop/ci-check.sh
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
cd "$(loop_root)"

: "${HEAD_REF:?}" "${BASE_SHA:?}" "${PR_TITLE:=}" "${PR_LABELS:=}"
# The labels as they are now, not as the event payload had them: gh pr create (land.sh) attaches
# the labels in a second call after the PR exists, so the payload of the opened event may have
# none (archicraft-z9yp). This also lets a re-run of the job see a label added since.
if [ -n "${PR_NUMBER:-}" ]; then
    PR_LABELS="$(gh pr view "$PR_NUMBER" --json labels -q '[.labels[].name] | join(",")')" ||
        { echo "::error::Cannot read the labels of pull request #$PR_NUMBER."; exit 1; }
fi
# A pull_request run checks out refs/pull/N/merge, the PR merged into the base as it is now, while
# BASE_SHA stays at the base the PR was opened against: diffing the merge against its first parent
# leaves out what the base received since. Elsewhere (no merge commit) BASE_SHA is the base.
# --no-renames lists both paths of a move: rename detection prints only the new one, which lets a
# file moved out of a guarded path pass (archicraft-95eq).
if git rev-parse --verify --quiet HEAD^2 >/dev/null; then
    changed="$(git diff --no-renames --name-only HEAD^1 HEAD)"
else
    changed="$(git diff --no-renames --name-only "$BASE_SHA"...HEAD)"
fi
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
