#!/usr/bin/env bash
# One-time GitHub setup for the bead loop (docs/loop.md), run by a repository admin:
# - squash auto-merge on, merged branches deleted, squash commits titled after the PR;
# - the labels the loop uses (format-change, large-change, loop);
# - main protected by the repository ruleset "main": the CI checks are required, and every PR
#   merges through the merge queue, which runs them again on main with the PRs ahead of it before
#   it merges (docs/loop.md, "The merge queue"). Without the queue two PRs that each passed CI
#   merged into a main that did not compile, twice on 2026-10-21 (archicraft-o363,
#   archicraft-va4v). Admins bypass it, so the owner can still repair main and merge a PR that
#   waits for the review. The ruleset "main history" forbids deleting and force-pushing main, for
#   admins too. They replace the classic branch protection of main, which the script deletes once
#   they are in place, so the required checks live in one place.
#   No Windows check is required: Windows runs on main, nightly and by hand
#   (.github/workflows/windows.yml), because its 10 to 17 minutes made every bead PR wait
#   (user decision of 2026-10-20); sweep.sh reports a failed run on main (docs/loop.md).
#   No macOS check is required: macOS runs only for releases (.github/workflows/macos.yml),
#   because GitHub's hosted macOS runners were too slow or never picked the jobs up
#   (2026-10-19) and no PR could merge (docs/loop.md).
# - main also requires the status "User review" (.github/workflows/user-review.yml), which fails
#   for a PR that changes the CI workflows or the hook wiring: auto-merge never merges it, the
#   owner merges it after the review (archicraft-9nlk).
# Usage: scripts/loop/setup-github.sh [--print-rulesets]   (--print-rulesets prints the rulesets
#        it would send and changes nothing)
set -euo pipefail

# The required checks: the CI job names in .github/workflows/ci.yml, each of which runs on
# merge_group too, and the commit status "User review" that user-review.yml posts on the PR's head
# and on the merge group's. A required check that does not report on a merge group keeps the queue
# waiting until check_response_timeout_minutes and then drops the PR (selftest.sh checks the list).
required=(
    "Test (ubuntu-24.04)"
    "Format and clippy"
    "IFCX conformance (buildingSMART reference code)"
    "Licences and sources"
    "Loop policy"
    "Repository guards"
    "User review"
)
# GitHub Actions, the app that reports all of them (the classic protection pinned them to it too).
actions_app=15368

# The rulesets of main. "main": the required checks and the merge queue, which admins bypass.
# The queue squashes (main fast-forwards to the commit its group tested), each PR's group must
# pass on its own (ALLGREEN), up to 3 groups build at once (as many as /bead-loop starts per batch),
# up to 3 PRs merge in one push, no wait for a fuller group, and a check has 30 minutes to report
# before the queue gives up on a group: the slowest required job, the Linux tests, takes 3 to 9
# minutes, and the rest covers waiting for a runner.
# "main history": no deletion and no force push, for admins too (a bypass covers every rule of its
# ruleset, so these live in a ruleset without one).
rulesets="$(jq -n --argjson app "$actions_app" --args '
    { ref_name: { include: ["refs/heads/main"], exclude: [] } } as $main
    | [{
        name: "main",
        target: "branch",
        enforcement: "active",
        conditions: $main,
        bypass_actors: [{ actor_id: 5, actor_type: "RepositoryRole", bypass_mode: "always" }],
        rules: [
            { type: "required_status_checks", parameters: {
                strict_required_status_checks_policy: false,
                do_not_enforce_on_create: false,
                required_status_checks: [$ARGS.positional[] | { context: ., integration_id: $app }]
            } },
            { type: "merge_queue", parameters: {
                merge_method: "SQUASH",
                grouping_strategy: "ALLGREEN",
                max_entries_to_build: 3,
                max_entries_to_merge: 3,
                min_entries_to_merge: 1,
                min_entries_to_merge_wait_minutes: 0,
                check_response_timeout_minutes: 30
            } }
        ]
    }, {
        name: "main history",
        target: "branch",
        enforcement: "active",
        conditions: $main,
        bypass_actors: [],
        rules: [{ type: "deletion" }, { type: "non_fast_forward" }]
    }]' "${required[@]}")"
if [ "${1:-}" = --print-rulesets ]; then
    printf '%s\n' "$rulesets"
    exit 0
fi

command -v gh >/dev/null || { echo "gh is not installed: brew install gh && gh auth login"; exit 1; }
repo="$(gh repo view --json nameWithOwner -q .nameWithOwner)"
echo "Configuring $repo"

# GitHub offers the merge queue to public repositories owned by an organization, on every plan
# (private ones need GitHub Enterprise Cloud). Elsewhere this script changes nothing (docs/loop.md).
read -r in_org visibility < <(gh repo view --json isInOrganization,visibility -q '"\(.isInOrganization) \(.visibility)"')
if [ "$in_org" != true ] || [ "$visibility" != PUBLIC ]; then
    echo "  The merge queue needs a public repository owned by an organization; $repo is not"
    echo "  (owned by an organization: $in_org, visibility: $visibility). Nothing changed: see"
    echo "  docs/loop.md, \"The merge queue\"."
    exit 1
fi

gh api -X PATCH "repos/$repo" \
    -F allow_auto_merge=true -F allow_squash_merge=true -F delete_branch_on_merge=true \
    -f squash_merge_commit_title=PR_TITLE -f squash_merge_commit_message=PR_BODY >/dev/null
echo "  auto-merge and squash merging on"

gh label create format-change --force --color d93f0b --description "Intended IFCX format change: golden files regenerated" >/dev/null
gh label create large-change --force --color fbca04 --description "Mechanical change above the bead diff budget" >/dev/null
gh label create loop --force --color 5319e7 --description "Changes the bead loop's own guardrails; reviewed by a human" >/dev/null
echo "  labels format-change, large-change, loop"

# Create each ruleset, or update it by name when an earlier run made it.
for i in 0 1; do
    ruleset="$(jq -c ".[$i]" <<<"$rulesets")"
    name="$(jq -r .name <<<"$ruleset")"
    id="$(gh api "repos/$repo/rulesets?includes_parents=false" -q ".[] | select(.name == \"$name\") | .id" | head -n 1)"
    if [ -n "$id" ]; then
        gh api -X PUT "repos/$repo/rulesets/$id" --input - <<<"$ruleset" >/dev/null
    else
        gh api -X POST "repos/$repo/rulesets" --input - <<<"$ruleset" >/dev/null
    fi
done
echo "  ruleset main: the merge queue (squash), the six CI checks (Linux) and the status User review"
echo "  ruleset main history: no deletion and no force push of main, for admins too"

# Only now that the rulesets protect main: the classic protection would keep a second list of
# required checks, which the merge queue would have to satisfy as well.
if gh api "repos/$repo/branches/main/protection" >/dev/null 2>&1; then
    gh api -X DELETE "repos/$repo/branches/main/protection" >/dev/null
    echo "  classic branch protection of main removed (the ruleset replaces it)"
fi
