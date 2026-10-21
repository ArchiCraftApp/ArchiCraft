#!/usr/bin/env bash
# Where a bead PR stands on its way into main through the merge queue (docs/loop.md, "The merge
# queue"), read once from GitHub, never polled. sweep.sh, land.sh and the /bead-loop report use it,
# so that a queued PR counts as on its way and a PR the queue dropped is not mistaken for one whose
# CI is still pending. Prints one line:
#   merged               merged into main
#   closed               closed without merging
#   queued <position>    in the merge queue; position 1 merges next
#   auto-merge           auto-merge on: GitHub puts it into the merge queue once its CI passes
#   left-queue <reason>  the queue dropped it (GitHub's reason: failed_checks, merge_conflict, ...):
#                        main with the PRs ahead of it and this one failed, so nothing merged; it
#                        needs a rebase onto main, a fix and land.sh again
#   waits-for-user       the status "User review" failed: the PR waits for the user's review,
#                        whatever the queue did with an earlier commit of it
#   open                 open, neither queued nor with auto-merge on nor waiting for the user:
#                        nothing moves it on (a workflow PR right after land.sh, before its status
#                        "User review" is posted, also shows as open)
#   unknown              gh gave no answer
# Needs only gh and jq, so it also runs as `git show origin/main:scripts/loop/pr-state.sh | bash -s <url>`.
# Usage: scripts/loop/pr-state.sh <pr-url>
set -uo pipefail
url="${1:?usage: scripts/loop/pr-state.sh <pr-url>}"

# The newest of the events that move a PR into the queue or out of it decides whether the queue
# dropped it: only auto-merge turned on again after the removal (land.sh after a fix) or a new
# entry into the queue starts over. A disabled auto-merge does not count: it must not turn a
# dropped PR into a merely open one.
query='query($url: URI!) {
  resource(url: $url) {
    ... on PullRequest {
      state
      autoMergeRequest { enabledAt }
      mergeQueueEntry { position }
      timelineItems(last: 1, itemTypes: [ADDED_TO_MERGE_QUEUE_EVENT, REMOVED_FROM_MERGE_QUEUE_EVENT,
          AUTO_MERGE_ENABLED_EVENT]) {
        nodes { __typename ... on RemovedFromMergeQueueEvent { reason } }
      }
      commits(last: 1) { nodes { commit { status { context(name: "User review") { state } } } } }
    }
  }
}'
answer="$(gh api graphql -F url="$url" -f query="$query" 2>/dev/null)" || answer=""
jq -r '
    .data.resource as $pr
    | ($pr.timelineItems.nodes // [] | last) as $event
    | ($pr.commits.nodes // [] | last | .commit.status.context.state // "") as $review
    | if ($pr | type) != "object" or $pr.state == null then "unknown"
      elif $pr.state == "MERGED" then "merged"
      elif $pr.state == "CLOSED" then "closed"
      elif $pr.mergeQueueEntry != null then "queued \($pr.mergeQueueEntry.position)"
      elif $review == "FAILURE" or $review == "ERROR" then "waits-for-user"
      elif $pr.autoMergeRequest != null then "auto-merge"
      elif $event.__typename == "RemovedFromMergeQueueEvent" and $event.reason != "merged"
      then "left-queue \($event.reason // "unknown" | gsub("\\s+"; "_"))"
      else "open" end' <<<"${answer:-null}" 2>/dev/null || echo unknown
