#!/usr/bin/env bash
# SubagentStop (bead-reviewer): turns the reviewer's last line, VERDICT: PASS or VERDICT: FAIL,
# into the review stamp of the current tree. A PASS counts only on a tree that passed the gate.
# The reviewer cannot write stamps itself, and the main session cannot fake a review.
# The report is the reviewer's last message or, when that holds no verdict, the report it handed
# back through SubagentHandback: Claude Code (2.1.299) makes every subagent deliver its report
# with that call, and a run that ends with it has an empty last message (archicraft-wjdta).
# Claude Code (2.1.317) buffers the transcript's rows for up to a second, so the row of the hand-back
# that ended the run can reach the transcript only after this hook started: the hook waits for it,
# up to LOOP_HANDBACK_WAIT seconds (default 10, within the hook's timeout of 30 s; archicraft-gdo0h).
# A stop without any verdict changes no stamp: it neither stamps nor takes away the PASS an earlier
# run earned, such as the plain-text PASS whose enforced hand-back stops once more.
set -uo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

[ "$HOOK_AGENT" = bead-reviewer ] || exit 0
hook_enter_repo "$HOOK_CWD" || exit 0

# The message of the last SubagentHandback call in the agent transcript (JSONL), only when it came
# after the run's last prompt (a user row that is not a tool result): a resumed reviewer's earlier
# hand-back is not this run's report. Rows that do not parse (one still being written) are skipped.
# Fails while the transcript holds no such hand-back.
handback_report() {
    [ -n "$1" ] && [ -f "$1" ] || return 1
    jq -Rrne 'reduce (inputs | fromjson? | select(.type == "user" or .type == "assistant")) as $row (null;
        ($row.message.content | if type == "array" then . else [{type: "text"}] end) as $items
        | if $row.type == "assistant" then
              ([$items[] | select(.type == "tool_use" and .name == "SubagentHandback") | .input.message // ""] | last) // .
          elif all($items[]; .type == "tool_result") then .
          else null end) // empty' "$1" 2>/dev/null
}
# <transcript> <seconds>: handback_report, retried while the hand-back's row is not there yet.
await_handback() {
    local deadline=$((SECONDS + $2))
    [ -n "$1" ] && [ -f "$1" ] || return 1
    until handback_report "$1"; do
        [ "$SECONDS" -lt "$deadline" ] || return 1
        sleep 0.2
    done
}
verdict_of() { grep -Eo 'VERDICT: (PASS|FAIL)' | tail -n 1 | cut -d' ' -f2; }

wait_limit="${LOOP_HANDBACK_WAIT:-10}"
[[ "$wait_limit" =~ ^[0-9]+$ ]] || wait_limit=10
dir="$(loop_dir)"
transcript="$(hook_field .agent_transcript_path)"
message="$(hook_field .last_assistant_message)"
verdict="$(verdict_of <<<"$message")"
handed_back=""
if [ -z "$verdict" ] && report="$(await_handback "$transcript" "$wait_limit")"; then
    handed_back=1
    if [ -n "$report" ]; then
        message="$report"
        verdict="$(verdict_of <<<"$message")"
    fi
fi
tree="$(loop_tree_hash)"
bead="$(loop_bead_from_branch || echo -)"
# A verdict replaces the review and its report; without one, review.md shows what the reviewer said
# last unless it belongs to a stamp.
if [ -n "$verdict" ] || [ ! -f "$dir/review.ok" ]; then
    rm -f "$dir/review.ok"
    printf '%s\n' "$message" | grep -v '^VERDICT:' >"$dir/review.md"
fi

if [ "$verdict" = PASS ] && loop_stamp_valid gate.ok "$tree"; then
    # With the fork point of the reviewed change, so a rebase that leaves it as it was keeps the
    # review (loop_review_rebased).
    loop_stamp_write review.ok "$tree" "$bead" "$(loop_fork_point)"
    jq -n --arg m "Review PASS stamped for $bead (tree ${tree:0:12})." '{systemMessage: $m}'
elif [ "$verdict" = PASS ]; then
    jq -n --arg m "Review PASS not stamped for $bead: the tree has no passing gate (it changed after scripts/loop/gate.sh). Run the gate, then review again." '{systemMessage: $m}'
elif [ "$verdict" = FAIL ]; then
    jq -n --arg m "Review FAIL for $bead: fix the findings, run the gate and review again." '{systemMessage: $m}'
else
    if [ -z "$transcript" ]; then
        why="the hook input names no agent transcript"
    elif [ ! -f "$transcript" ]; then
        why="its transcript $transcript does not exist"
    elif [ -n "$handed_back" ]; then
        why="its hand-back holds none"
    else
        why="no hand-back after its last prompt reached $transcript within $wait_limit s"
    fi
    if loop_review_valid "$tree"; then
        then_what="The earlier review PASS still covers this tree."
    else
        then_what="Nothing was stamped: ask the reviewer (SendMessage) to restate its verdict, or review again."
    fi
    jq -n --arg m "Review without verdict for $bead: no VERDICT: PASS or VERDICT: FAIL in the reviewer's last message, and $why. $then_what" \
        '{systemMessage: $m}'
fi
exit 0
