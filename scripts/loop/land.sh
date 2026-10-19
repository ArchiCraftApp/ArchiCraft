#!/usr/bin/env bash
# Step 7 of the bead loop (docs/loop.md): pushes the committed bead branch, opens or updates
# its pull request and turns on auto-merge, so GitHub merges it once the required CI checks
# pass. Records the PR on the bead, which stays in progress until sweep.sh sees the merge.
# Usage: scripts/loop/land.sh [--rebased]   (--rebased pushes with --force-with-lease)
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
cd "$(loop_root)"

id="$(loop_bead_from_branch)" || { echo "Not on a bead branch (bead/<id>-<slug>)."; exit 1; }
branch="$(loop_branch)"
command -v gh >/dev/null || { echo "gh is not installed: brew install gh && gh auth login"; exit 1; }

if git status --porcelain --untracked-files=all | grep -v ' \.beads/' | grep -q .; then
    echo "Uncommitted changes; commit everything the gate and review saw first."
    git status --short
    exit 1
fi
tree="$(loop_tree_hash)"
loop_stamp_valid gate.ok "$tree" || { echo "No passing gate for this tree: scripts/loop/gate.sh"; exit 1; }
loop_review_valid "$tree" || { echo "No passing review for this tree: run the bead-reviewer agent."; exit 1; }

if [ "${1:-}" = --rebased ]; then
    git push --quiet --force-with-lease -u origin "$branch"
else
    git push --quiet -u origin "$branch"
fi

bead="$(loop_bead_json "$id")"
title="$(git log -1 --format=%s)"
body="$(mktemp "${TMPDIR:-/tmp}/loop-pr.XXXXXX")"
{
    echo "Bead: \`$id\` — $(jq -r '.title' <<<"$bead")"
    srs="$(jq -r '.notes // ""' <<<"$bead" | loop_srs_ids || true)"
    [ -n "$srs" ] && echo "SRS: $srs"
    echo
    echo "## Acceptance criteria"
    echo
    jq -r '.acceptance_criteria // "(none)"' <<<"$bead"
    echo
    echo "## Review"
    echo
    if [ -s "$(loop_dir)/review.md" ]; then loop_nest_headings 3 "$(loop_dir)/review.md"; else echo "(no review notes)"; fi
    echo
    since="$(loop_changed_since_review "$tree" | sed 's/.*/`&`/' | paste -sd ' ' - || true)"
    if [ -z "$since" ]; then
        echo "Local gate and review passed on tree \`$tree\`."
    else
        echo "Local gate passed on tree \`$tree\`; the review passed before these changes outside code: $since."
    fi
    echo
    echo "🤖 Generated with [Claude Code](https://claude.com/claude-code)"
} >"$body"

# Bead labels the CI loop-policy job reads: set on creation, so the first CI run sees them.
create=(--base main --head "$branch" --title "$title" --body-file "$body")
edit=(--body-file "$body")
for l in format-change large-change loop; do
    if loop_bead_has_label "$id" "$l"; then
        create+=(--label "$l")
        edit+=(--add-label "$l")
    fi
done
if url="$(gh pr view "$branch" --json url,state -q 'select(.state == "OPEN") | .url' 2>/dev/null)" && [ -n "$url" ]; then
    gh pr edit "$url" "${edit[@]}" >/dev/null
    echo "Updated $url"
else
    url="$(gh pr create "${create[@]}")"
    echo "Opened $url"
fi
rm -f "$body"

if loop_bead_has_label "$id" loop; then
    # Changes to the loop's own guardrails wait for the user's review instead of auto-merging.
    merge="the user reviews and merges it (label loop)"
else
    gh pr merge "$url" --auto --squash >/dev/null
    merge="auto-merge (squash) on green CI"
fi
echo "Merge: $merge."

if [ "$(jq -r '.external_ref // ""' <<<"$bead")" != "$url" ]; then
    bd update "$id" --external-ref "$url" --append-notes "PR $url, $merge" >/dev/null
fi
echo "$url"
