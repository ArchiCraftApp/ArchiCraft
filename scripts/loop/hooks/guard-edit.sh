#!/usr/bin/env bash
# PreToolUse (Edit|Write|MultiEdit|NotebookEdit): no code edits without a claimed bead, no
# hand-edited golden files, no edits by the reviewer, no loop changes inside a feature bead.
set -uo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

file="$(hook_field '.tool_input.file_path // .tool_input.notebook_path')"
[ -n "$file" ] || exit 0
case "$file" in /*) ;; *) file="$HOOK_CWD/$file" ;; esac
existing="$(hook_existing_dir "$(dirname "$file")")"
hook_enter_repo "$existing" || exit 0
# Repository-relative path, also for files in directories the edit is about to create.
rel="$(cd "$existing" && pwd -P)${file#"$existing"}"
rel="${rel#"$(pwd -P)"/}"

[ "$HOOK_AGENT" = bead-reviewer ] && hook_deny "The bead reviewer reports findings; it never edits files."

if grep -Eq "$LOOP_GOLDEN_PATHS" <<<"$rel"; then
    hook_deny "Golden files are never edited by hand. For an intended format change, label the bead format-change and regenerate them with ARCHICRAFT_BLESS=1 cargo test -p archicraft-ifcx --test roundtrip."
fi

bead="$(loop_bead_from_branch 2>/dev/null)"

if grep -Eq "$LOOP_SELF_PATHS" <<<"$rel" && [ -n "$bead" ] && ! loop_bead_has_label "$bead" loop; then
    hook_deny "$rel belongs to the loop's guardrails. A feature bead never changes them; file a bead with label loop for the change (docs/loop.md)."
fi

grep -Eq "$LOOP_CODE_PATHS" <<<"$rel" || exit 0
if [ -z "$bead" ]; then
    hook_deny "Code changes need a bead. This worktree is on '$(loop_branch)'. Run /bead (scripts/loop/start-bead.sh <id>) to claim a bead and switch to its bead/<id>-<slug> branch first."
fi
# start-bead.sh caches the claimed bead; only an unknown bead costs a bd lookup.
[ "$(cat "$(loop_dir)/bead" 2>/dev/null)" = "$bead" ] && exit 0
status="$(loop_bead_json "$bead" | jq -r '.status // "missing"')"
[ "$status" = in_progress ] || hook_deny "Bead $bead is $status, not in progress. Claim it with scripts/loop/start-bead.sh $bead."
echo "$bead" >"$(loop_dir)/bead"
exit 0
