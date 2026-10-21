#!/usr/bin/env bash
# CI half of the user-review rule (docs/loop.md): a pull request that changes the CI workflows or
# the hook wiring (LOOP_USER_REVIEW_PATHS), or that pattern itself, waits for the user. land.sh
# and guard-bash run the branch's own copies of themselves and lib.sh, so a branch that rewrites
# them could turn auto-merge on for its own workflow change (archicraft-9nlk). This check runs as
# the base defines it: .github/workflows/user-review.yml starts it on pull_request_target, which
# checks out the base and never runs the PR's code; the PR's commits are only read as data.
# It reports the commit status "User review" on the PR's head, failure when the PR waits. Branch
# protection requires that status (setup-github.sh), so auto-merge never merges such a PR; the
# user merges it after the review, as an admin past the failing status and the merge queue.
# The merge queue requires the status on the merge group's commit too: there the workflow runs
# this script in a checkout of main with HEAD_SHA the group's head, so it judges everything the
# group adds to main, every PR ahead in the queue included (docs/loop.md, "The merge queue").
# Environment: HEAD_SHA (the PR's or the merge group's head commit, fetched); GH_REPO and GH_TOKEN
# to post the status (without GH_REPO it only prints the result). Exits 1 when the change waits.
# Usage: scripts/loop/ci-user-review.sh   (run in the base's checkout)
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
cd "$(loop_root)"

: "${HEAD_SHA:?}"
# The PR's change as GitHub shows it, from the fork point on: what the base received since does
# not count. --no-renames lists both paths of a move (archicraft-95eq).
fork="$(git merge-base HEAD "$HEAD_SHA")"
waits="$({
    git diff --no-renames --name-only "$fork" "$HEAD_SHA" | grep -E "$LOOP_USER_REVIEW_PATHS" || true
    [ "$(loop_user_review_paths_at "$HEAD_SHA")" = "$(loop_user_review_paths_at "$fork")" ] || echo scripts/loop/lib.sh
} | sort -u | paste -sd ' ' -)"

if [ -n "$waits" ]; then
    state=failure
    message="Waits for the user's review: changes the CI workflows, the hook wiring or which files wait: $waits"
else
    state=success
    message="Changes neither the CI workflows nor the hook wiring"
fi
echo "User review: $message"
if [ -n "${GH_REPO:-}" ]; then
    # GitHub cuts a status description at 140 characters.
    status=(-f state="$state" -f context="User review" -f description="${message:0:140}")
    [ -z "${GITHUB_RUN_ID:-}" ] ||
        status+=(-f target_url="${GITHUB_SERVER_URL:-https://github.com}/$GH_REPO/actions/runs/$GITHUB_RUN_ID")
    gh api -X POST "repos/$GH_REPO/statuses/$HEAD_SHA" "${status[@]}" >/dev/null
fi
[ "$state" = success ] || { echo "::error::$message"; exit 1; }
