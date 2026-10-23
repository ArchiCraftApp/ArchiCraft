#!/usr/bin/env bash
# SubagentStop (bead-reviewer): turns the reviewer's last line, VERDICT: PASS or VERDICT: FAIL,
# into the review stamp of the current tree. A PASS counts only on a tree that passed the gate.
# The reviewer cannot write stamps itself, and the main session cannot fake a review.
# The report is the reviewer's last message or, when that holds no verdict, the report it handed
# back through SubagentHandback: Claude Code (2.1.299) makes every subagent deliver its report
# with that call, and a run that ends with it has an empty last message (archicraft-wjdta).
set -uo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

[ "$HOOK_AGENT" = bead-reviewer ] || exit 0
hook_enter_repo "$HOOK_CWD" || exit 0

# The message of the last SubagentHandback call in the agent transcript (JSONL), only when it came
# after the run's last prompt (a user row that is not a tool result): a resumed reviewer's earlier
# hand-back is not this run's report. Rows that do not parse (one still being written) are skipped.
handback_report() {
    [ -n "$1" ] && [ -f "$1" ] || return 0
    jq -Rrn 'reduce (inputs | fromjson? | select(.type == "user" or .type == "assistant")) as $row (null;
        ($row.message.content | if type == "array" then . else [{type: "text"}] end) as $items
        | if $row.type == "assistant" then
              ([$items[] | select(.type == "tool_use" and .name == "SubagentHandback") | .input.message // ""] | last) // .
          elif all($items[]; .type == "tool_result") then .
          else null end) // empty' "$1" 2>/dev/null
}
verdict_of() { grep -Eo 'VERDICT: (PASS|FAIL)' | tail -n 1 | cut -d' ' -f2; }

dir="$(loop_dir)"
message="$(hook_field .last_assistant_message)"
verdict="$(verdict_of <<<"$message")"
if [ -z "$verdict" ]; then
    report="$(handback_report "$(hook_field .agent_transcript_path)")"
    if [ -n "$report" ]; then
        message="$report"
        verdict="$(verdict_of <<<"$message")"
    fi
fi
tree="$(loop_tree_hash)"
bead="$(loop_bead_from_branch || echo -)"
rm -f "$dir/review.ok"
printf '%s\n' "$message" | grep -v '^VERDICT:' >"$dir/review.md"

if [ "$verdict" = PASS ] && loop_stamp_valid gate.ok "$tree"; then
    loop_stamp_write review.ok "$tree" "$bead"
    jq -n --arg m "Review PASS stamped for $bead (tree ${tree:0:12})." '{systemMessage: $m}'
elif [ "$verdict" = PASS ]; then
    jq -n --arg m "Review PASS not stamped for $bead: the tree has no passing gate (it changed after scripts/loop/gate.sh). Run the gate, then review again." '{systemMessage: $m}'
else
    jq -n --arg m "Review ${verdict:-without verdict} for $bead: fix the findings, run the gate and review again." '{systemMessage: $m}'
fi
exit 0
