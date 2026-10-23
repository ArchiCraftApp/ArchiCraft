#!/usr/bin/env bash
# Step 1 of the bead loop (docs/loop.md): claims a bead and puts this worktree on its branch
# bead/<id>-<slug>, cut from the latest main. Re-running it for a bead in progress resumes
# its existing branch. It runs as it is on origin/main: a worktree cut from an older main has
# outdated loop scripts until this switch moves it, so when its scripts/loop differs from
# origin/main's, it re-executes origin/main's copy.
# Usage: scripts/loop/start-bead.sh <bead-id>
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
cd "$(loop_root)"

# The copy removes its directory when done; a LOOP_START_BASE_COPY that does not hold this very
# script (one leaked into the environment) is ignored.
if [ "${LOOP_START_BASE_COPY:-}/scripts/loop" = "$(dirname "${BASH_SOURCE[0]}")" ]; then
    trap 'rm -rf "$LOOP_START_BASE_COPY"' EXIT
else
    git fetch --quiet origin main || echo "warning: could not fetch origin/main; branching from the local ref"
    if ! git diff --quiet "$(loop_base)" -- scripts/loop; then
        copy="$(mktemp -d "${TMPDIR:-/tmp}/loop-start-bead.XXXXXX")"
        git archive "$(loop_base)" scripts/loop | tar -x -C "$copy"
        LOOP_START_BASE_COPY="$copy" exec bash "$copy/scripts/loop/start-bead.sh" "$@"
    fi
fi

id="${1:?usage: scripts/loop/start-bead.sh <bead-id>}"
bead="$(loop_bead_json "$id")"
[ -n "$bead" ] || { echo "No bead $id."; exit 1; }

type="$(jq -r '.issue_type' <<<"$bead")"
status="$(jq -r '.status' <<<"$bead")"
title="$(jq -r '.title' <<<"$bead")"
if [ "$type" = epic ]; then
    echo "$id is an epic. Work its tasks: bd ready --parent $id --exclude-type epic"
    exit 1
fi
case "$status" in
    closed) echo "$id is closed."; exit 1 ;;
    in_progress) echo "$id is in progress; resuming it." ;;
    *)
        if ! bd ready --json --limit 0 --exclude-type epic | jq -e --arg id "$id" 'any(.[]; .id == $id)' >/dev/null; then
            echo "$id is not ready (blocked or deferred):"
            bd show "$id" | sed -n '/DEPENDS ON/,/^$/p'
            exit 1
        fi
        ;;
esac

if git status --porcelain | grep -v ' \.beads/' | grep -q .; then
    echo "The working tree has uncommitted changes; commit, stash or discard them first."
    git status --short
    exit 1
fi

# The beads post-checkout hook re-imports the whole JSONL export (minutes) into the database
# every worktree already shares; skip git hooks for this switch.
switch() { git -c core.hooksPath=/dev/null switch --quiet "$@"; }
existing="$(git for-each-ref --format='%(refname:short)' "refs/heads/bead/$id-*" "refs/heads/bead/$id" | head -n 1)"
if [ -n "$existing" ]; then
    switch "$existing"
    branch="$existing"
else
    slug="$(tr '[:upper:]' '[:lower:]' <<<"$title" | sed -E 's/[^a-z0-9]+/-/g; s/^-+//' | cut -c1-40 | sed -E 's/-+$//')"
    branch="bead/$id-$slug"
    switch -c "$branch" "$(loop_base)"
fi
[ "$status" = in_progress ] || bd update "$id" --claim >/dev/null

# Fresh per-worktree loop state for this bead.
dir="$(loop_dir)"
rm -f "$dir"/*.ok "$dir"/pause "$dir"/stop-blocks "$dir"/review.md "$dir"/gate-*.log
echo "$id" >"$dir/bead"

# Warm build cache: clone the main checkout's target dir copy-on-write (APFS) when this
# worktree has none (a new worktree, or a reused one whose target/ sweep.sh deleted once its work
# had landed). Concurrent builds must never share one target dir.
main_tree="$(git worktree list --porcelain | awk 'NR == 1 { print $2 }')"
if [ ! -d target ] && [ -d "$main_tree/target/debug" ] && [ "$(uname)" = Darwin ]; then
    cp -Rc "$main_tree/target" target && echo "Cloned the warm build cache from $main_tree/target."
fi

echo "On branch $branch for $id."
wip="$(jq -r '.notes // ""' <<<"$bead" | grep -Eo 'wip/[A-Za-z0-9._/-]+' | head -n 1 || true)"
if [ -n "$wip" ]; then
    echo "A stopped run left partial work on $wip (unbuilt, unreviewed). Use it as a reference:"
    echo "  git diff $(loop_base)...$wip --stat"
fi
patch="$(loop_escalation_patch "$id")"
if [ -f "$patch" ]; then
    echo "The run on the lower tier left its unreviewed work (it failed the gate or the review). Apply it"
    echo "as the starting point or read it as a reference:"
    echo "  git apply --index $patch"
fi
echo
bd show "$id"
