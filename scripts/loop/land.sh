#!/usr/bin/env bash
# Step 7 of the bead loop (docs/loop.md): pushes the committed bead branch, opens or updates
# its pull request and turns on auto-merge, so GitHub merges it once the required CI checks
# pass. A PR that changes the CI workflows or the hook wiring (LOOP_USER_REVIEW_PATHS, as the
# base or the branch defines it) or that pattern itself waits for the user's review instead.
# Records the PR on the bead, which stays in progress until sweep.sh sees the merge.
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
# The files that make the PR wait for the user, as `a` `b`; empty when it may auto-merge.
waits="$(loop_user_review_files | sed 's/.*/`&`/' | paste -sd ' ' - || true)"
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
        echo "Local gate passed on tree \`$tree\`; the review passed before these changes to prose only: $since."
    fi
    if [ -n "$waits" ]; then
        echo
        echo "Waits for the user's review (no auto-merge): it changes the CI workflows, the hook wiring or which files wait, $waits."
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

# Every bead PR auto-merges, a loop bead's too, except one that changes the CI gate or the hook
# wiring: those are the gates that cannot be bypassed, so the user reviews it (2026-10-19).
if [ -n "$waits" ]; then
    # An earlier run may have turned auto-merge on, before the branch changed these files.
    if [ "$(gh pr view "$url" --json autoMergeRequest -q '.autoMergeRequest != null')" = true ]; then
        gh pr merge "$url" --disable-auto >/dev/null
    fi
    merge="waits for the user's review, as it changes the CI workflows, the hook wiring or which files wait: $waits"
else
    gh pr merge "$url" --auto --squash >/dev/null
    merge="auto-merge (squash) on green CI"
fi
echo "Merge: $merge."

if [ "$(jq -r '.external_ref // ""' <<<"$bead")" != "$url" ]; then
    bd update "$id" --external-ref "$url" --append-notes "PR $url, $merge" >/dev/null
fi
echo "$url"
