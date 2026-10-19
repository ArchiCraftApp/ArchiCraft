#!/usr/bin/env bash
# PreToolUse (Bash): the git and GitHub rules of the bead loop.
# - stamps are written only by gate.sh, the review hook and pause.sh;
# - the reviewer runs read-only commands;
# - no force pushes, no pushes to main, no PR merges without --auto;
# - a code commit happens only on a bead branch whose tree passed the gate and the review.
# Pattern matching on the command line is a guardrail, not a sandbox; CI is the final gate.
set -uo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

cmd="$(hook_field .tool_input.command)"
[ -n "$cmd" ] || exit 0
has() { grep -Eq -- "$1" <<<"$cmd"; }
# The command without harmless redirections (2>&1, >/dev/null), for write detection.
plain="$(sed -E 's/[0-9]*>&[0-9]+//g; s#[0-9]*>>?[[:space:]]*/dev/null##g' <<<"$cmd")"
writes() { grep -Eq '(^|[^-=])>|(^|[;&|[:space:]])(rm|mv|cp|touch|tee|truncate|ln|chmod)[[:space:]]|sed[[:space:]]+(-[a-zA-Z]*[[:space:]]+)*-i' <<<"$plain"; }

if has '\.git/([^[:space:]]*/)?loop|git-path[[:space:]]+loop|(gate|review)\.(ok|fail)|stop-blocks' && writes; then
    hook_deny "The loop's stamps are written only by scripts/loop/gate.sh, the review hook and scripts/loop/pause.sh. Reading the gate logs is fine."
fi

if [ "$HOOK_AGENT" = bead-reviewer ]; then
    if writes || has 'git[[:space:]]+(add|commit|push|reset|checkout|switch|restore|stash|clean|rebase|merge|cherry-pick|rm|mv)|cargo[[:space:]]+(fmt|fix|add|remove|update)|--fix|bd[[:space:]]+(update|close|create|delete|dep|label|remember|forget|human)|gh[[:space:]]'; then
        hook_deny "The bead reviewer only reads: git diff/log/show/status, bd show, grep, cargo check/test. Report the problem as a finding instead."
    fi
    exit 0
fi

# git -C <dir> runs elsewhere; follow it so the checks see the right repository.
dir="$HOOK_CWD"
if [[ "$cmd" =~ git[[:space:]]+-C[[:space:]]+(\"[^\"]+\"|\'[^\']+\'|[^[:space:]]+) ]]; then
    d="${BASH_REMATCH[1]}"
    d="${d%\"}"; d="${d#\"}"; d="${d%\'}"; d="${d#\'}"
    case "$d" in /*) dir="$d" ;; *) dir="$HOOK_CWD/$d" ;; esac
fi
hook_enter_repo "$dir" || exit 0

git_cmd='git([[:space:]]+-C[[:space:]]+[^[:space:]]+)?[[:space:]]+'
push="$(grep -Eo "${git_cmd}push[^;&|]*" <<<"$cmd" || true)"
if [ -n "$push" ]; then
    if grep -Eq '[[:space:]](--force|-f|-[a-zA-Z]*f[a-zA-Z]*)([[:space:]]|$)|[[:space:]]\+[A-Za-z0-9_./-]+' <<<"$push"; then
        hook_deny "Force pushes are refused. After a rebase of a bead branch use scripts/loop/land.sh --rebased (it pushes with --force-with-lease)."
    fi
    if grep -Eq '[[:space:]:](main|master)([[:space:]]|$)' <<<"$push" || [ "$(loop_branch)" = main ]; then
        hook_deny "Nothing is pushed to main. Changes reach main only through a pull request that merges on green CI (scripts/loop/land.sh)."
    fi
fi

if has 'gh[[:space:]]+pr[[:space:]]+merge'; then
    has '--admin' && hook_deny "PRs never merge with --admin; they merge when the required CI checks pass."
    has '--auto|--disable-auto' || hook_deny "PRs merge only through auto-merge on green CI: gh pr merge <pr> --auto --squash (scripts/loop/land.sh does this)."
fi

if has "${git_cmd}commit"; then
    loop_code_dirty || exit 0
    bead="$(loop_bead_from_branch)" ||
        hook_deny "Code changes are committed only on a bead branch, after the gate and the review (docs/loop.md). This worktree is on '$(loop_branch)'; run /bead first."
    tree="$(loop_tree_hash)"
    loop_stamp_valid gate.ok "$tree" ||
        hook_deny "No passing gate for the current tree of $bead. Run scripts/loop/gate.sh (any edit after the gate needs a new run)."
    loop_stamp_valid review.ok "$tree" ||
        hook_deny "No passing review for the current tree of $bead. Launch the bead-reviewer agent with 'Review bead $bead' (any edit after the review needs a new one)."
fi
exit 0
