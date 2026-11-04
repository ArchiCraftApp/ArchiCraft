#!/usr/bin/env bash
# PreToolUse and PostToolUse (AskUserQuestion): the user's own answer approves one admin merge of a
# PR that waits for the user's review (docs/loop.md, "The user's approval of a merge").
# - PostToolUse: a question that names exactly one 'PR #<n> at <sha>' (7 to 40 hex digits) and
#   offers the option 'Merge #<n>', answered with that option as the single choice, writes the stamp
#   user-merge.<n> holding "<sha> - <utc time>". guard-bash then allows
#   'gh pr merge <n> --admin --squash --match-head-commit <full sha>' once, within 60 minutes.
#   'Changes needed', 'Later', text under Other, a question without the marker and selections the
#   user did not send (an idle card, a follow-up, a typed reply) write nothing.
# - PreToolUse: a call that brings its own answers is refused, so only the user's question card
#   answers. Claude Code 2.1.309 drops such answers before the card anyway (probe of 2026-11-04).
# The answers come keyed by the question text in tool_response (Claude Code 2.1.309).
set -uo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

[ "$(hook_field .tool_name)" = AskUserQuestion ] || exit 0
hook_enter_repo "$HOOK_CWD" || exit 0
case "$(hook_field .hook_event_name)" in
PreToolUse)
    if jq -e '(.tool_input.answers // {}) | length > 0' <<<"$HOOK_INPUT" >/dev/null 2>&1; then
        hook_deny "AskUserQuestion takes the questions only: the answers come from the user's question card."
    fi
    exit 0
    ;;
PostToolUse) ;;
*) exit 0 ;;
esac

# "<n> <sha>" for each question the user approved a merge in. Only answers the user sent count:
# not the selections of a card that resolved itself while the user was idle (afkTimeoutMs), nor
# those of a user who asked for more questions (followUp) or typed a reply instead (response), as
# Claude Code 2.1.309 describes its result fields.
approved="$(jq -r '
    def sent: .afkTimeoutMs == null and ((.followUp // false) | not)
        and (((.response // "") | tostring | test("\\S")) | not);
    select((.tool_input // {} | sent) and (.tool_response // {} | sent))
    | .tool_response as $r | ($r.answers // {}) as $answers
    | $r.questions[]? | select((.multiSelect // false) | not)
    | .question as $q | [$q | scan("PR #([0-9]+) at ([0-9a-f]{7,40})\\b")] as $marks
    | select($marks | length == 1) | $marks[0][0] as $n | $marks[0][1] as $sha
    | select($answers[$q] == "Merge #\($n)" and any(.options[]?; .label == "Merge #\($n)"))
    | "\($n) \($sha)"' <<<"$HOOK_INPUT" 2>/dev/null)"
[ -n "$approved" ] || exit 0
message=""
while read -r n sha; do
    loop_stamp_write "user-merge.$n" "$sha"
    message+="The user approved merging PR #$n at $sha: run gh pr merge $n --admin --squash --match-head-commit <its full head sha, literally> within 60 minutes, while that is still the head and every required check but User review passed. "
done <<<"$approved"
hook_context PostToolUse "${message% }"
