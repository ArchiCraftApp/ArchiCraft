#!/usr/bin/env bash
# Start of every loop iteration (docs/loop.md): closes in-progress beads whose PR has merged,
# closes loop-split parents whose children are all closed, and reports what still waits: open PRs
# (in the merge queue, waiting for CI, waiting for the user's review), PRs the merge queue dropped,
# PRs that nothing moves on and PRs closed without merging (ATTENTION lines), beads flagged for a
# human, epics ready for their demo, and a failed newest Windows run on main (an ATTENTION line).
# A close that bd refuses (a blocked bead) is reported and the sweep carries on.
# Last, it fast-forwards the main checkout to origin/main (fetched by the caller), since Claude
# Code reads the hook wiring, the skills and the reviewer from there; when git cannot, it reports
# why and the sweep carries on.
# Read-only towards GitHub; it checks each PR once and never polls.
# Usage: scripts/loop/sweep.sh
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
command -v gh >/dev/null || { echo "gh is not installed: brew install gh && gh auth login"; exit 1; }

# close_bead <id> <reason> <what>: closes the bead, or reports bd's refusal.
close_bead() {
    local out
    if out="$(bd close "$1" --reason "$2" 2>&1)"; then
        echo "closed   $1 ($3)"
    else
        echo "refused  $1: bd close failed: $(printf '%s\n' "$out" | head -n 1)"
    fi
}

# The failed merge group run of PR <url>, when there is one: the queue names a group's branch
# gh-readonly-queue/main/pr-<number>-<sha>.
queue_run() {
    gh run list --workflow ci.yml --event merge_group --limit 50 --json headBranch,conclusion,url 2>/dev/null |
        jq -r --arg b "gh-readonly-queue/main/pr-${1##*/}-" \
            '[.[] | select((.headBranch // "") | startswith($b)) | select(.conclusion == "failure")] | first // empty | .url' \
            2>/dev/null || true
}

# A PR on its way into main is in the merge queue or waits for its CI with auto-merge on; one the
# queue dropped did not merge, and main stays green without it: its bead fixes it (/bead, step 7).
bd list --status in_progress --json --limit 0 |
    jq -r '.[] | select((.external_ref // "") | test("/pull/[0-9]+$")) | "\(.id) \(.external_ref)"' |
    while read -r id url; do
        state="$(bash "$(dirname "${BASH_SOURCE[0]}")/pr-state.sh" "$url")"
        case "$state" in
            merged) close_bead "$id" "Merged in $url" "merged $url" ;;
            "queued "*) echo "queued   $id ($url, merge queue position ${state#queued })" ;;
            auto-merge) echo "waiting  $id ($url, CI pending, then the merge queue)" ;;
            "left-queue "*)
                run="$(queue_run "$url")"
                echo "ATTENTION $id: $url left the merge queue (${state#left-queue }${run:+, $run}): rebase it onto origin/main, fix it and land it again"
                ;;
            waits-for-user) echo "waits    $id ($url, waits for the user's review)" ;;
            open) echo "ATTENTION $id: $url is open without auto-merge and not in the merge queue" ;;
            closed) echo "ATTENTION $id: $url was closed without merging" ;;
            *) echo "unknown  $id ($url)" ;;
        esac
    done

# A bead the loop split (label loop-split; plain "split" is an area label of the split tool) is
# a container: it closes once it has children and all of them are closed.
bd list --label loop-split --json --limit 0 | jq -r '.[] | select(.status != "closed") | .id' |
    while read -r id; do
        read -r total open < <(bd children "$id" --json | jq -r '"\(length) \([.[] | select(.status != "closed")] | length)"')
        if [ "${total:-0}" -gt 0 ] && [ "$open" = 0 ]; then
            close_bead "$id" "All child beads closed" "split, children done"
        fi
    done

humans="$(bd list --label human --json --limit 0 | jq -r '.[] | select(.status != "closed") | "\(.id) \(.title)"')"
[ -n "$humans" ] && { echo "needs a human:"; sed 's/^/  /' <<<"$humans"; }

demo="$(bd epic close-eligible --dry-run 2>/dev/null | grep -E 'archicraft-' || true)"
[ -n "$demo" ] && { echo "epics with every task closed, ready for the demo:"; sed 's/^/  /' <<<"$demo"; }

# Windows tests run on main, nightly and by hand, not on PRs (.github/workflows/windows.yml), so
# a Windows-only failure surfaces here: the newest finished run on main that was not cancelled
# (a newer push cancels the older run). When gh cannot answer, nothing is reported.
windows="$(gh run list --workflow windows.yml --branch main --limit 20 \
    --json status,conclusion,url,headSha 2>/dev/null |
    jq -r '[.[] | select(.status == "completed" and .conclusion != "cancelled" and .conclusion != "skipped")]
        | first // empty | select(.conclusion != "success") | "\(.conclusion) \(.headSha[0:7]) \(.url)"' 2>/dev/null || true)"
if [ -n "$windows" ]; then
    read -r conclusion sha url <<<"$windows"
    echo "ATTENTION windows: the newest Windows run on main ($sha) ended in $conclusion: $url"
fi

# The main checkout is the repository's first worktree. Only a checkout on main moves, and only by
# a fast-forward, which git refuses when local changes would be overwritten (other local changes,
# such as an edited .beads/config.yaml, stay as they are) or when local main has commits of its
# own. Git hooks stay off: the beads post-merge hook can block bd for minutes, and start-bead.sh
# skips hooks for its own switch the same way.
main_checkout="$(git worktree list --porcelain | awk 'NR == 1 { sub(/^worktree /, ""); print }')"
if git rev-parse -q --verify refs/remotes/origin/main >/dev/null && [ -d "$main_checkout" ] &&
    [ "$(git -C "$main_checkout" rev-parse --is-bare-repository)" = false ]; then
    main_branch="$(git -C "$main_checkout" branch --show-current)"
    if [ "$main_branch" != main ]; then
        echo "stale    main checkout $main_checkout is on ${main_branch:-a detached HEAD}, not main; not updated"
    elif ! git -C "$main_checkout" merge-base --is-ancestor origin/main HEAD 2>/dev/null; then
        old="$(git -C "$main_checkout" rev-parse --short HEAD 2>/dev/null || echo unknown)"
        if out="$(git -C "$main_checkout" -c core.hooksPath=/dev/null merge --ff-only --quiet origin/main 2>&1)"; then
            echo "updated  main checkout $main_checkout: $old..$(git -C "$main_checkout" rev-parse --short HEAD)"
        else
            # git's own error line, not the hints it prints before it
            why="$(grep -m 1 -E '^(error|fatal):' <<<"$out" || grep -m 1 . <<<"$out" || echo 'git merge failed')"
            echo "stale    main checkout $main_checkout stays at $old: $why"
        fi
    fi
fi
exit 0
