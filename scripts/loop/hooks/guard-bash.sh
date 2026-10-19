#!/usr/bin/env bash
# PreToolUse (Bash): the git and GitHub rules of the bead loop.
# - stamps are written only by gate.sh, the review hook and pause.sh;
# - the reviewer runs read-only commands;
#   (both judged by the commands the line runs, see shell-commands.awk: quoted arguments and
#   heredoc bodies that mention a command or the loop's state dir do not count)
# - no force pushes, no pushes to main, no PR merges without --auto;
# - a code commit happens only on a bead branch whose tree passed the gate and is covered by a
#   passing review (loop_review_valid).
# Pattern matching on the command line is a guardrail, not a sandbox; CI is the final gate.
set -uo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

cmd="$(hook_field .tool_input.command)"
[ -n "$cmd" ] || exit 0
has() { grep -Eq -- "$1" <<<"$cmd"; }
# The commands the line runs: one per line, words separated by tabs, a write redirection as
# ">	<target>".
cmds="$(printf '%s\n' "$cmd" | awk -f "$(dirname "${BASH_SOURCE[0]}")/shell-commands.awk")"
t=$'\t'
runs() { grep -Eq -- "$1" <<<"$cmds"; }
# Commands that write files: a redirection, a file utility, sed -i, find -delete.
write_re="^(>|rm|mv|cp|touch|tee|truncate|ln|chmod)($t|\$)|^sed$t(.*$t)?(-[a-zA-Z]*i|--in-place)|^find$t(.*$t)?-delete($t|\$)"
writes="$(grep -E -- "$write_re" <<<"$cmds")"
# A line that writes and runs any command naming the state dir: the write itself, a cd into it, or
# a variable or pipe that carries the path to the write. A heredoc body that is data never counts;
# a quoted mention (a bd note, a commit message) counts only on a line that writes.
state_re='\.git/([^[:space:]]*/)?loop|git-path[[:space:]]+loop|(gate|review)\.(ok|fail)|stop-blocks'

if [ -n "$writes" ] && grep -Eq -- "$state_re" <<<"$cmds"; then
    hook_deny "The loop's stamps are written only by scripts/loop/gate.sh, the review hook and scripts/loop/pause.sh. Reading the gate logs is fine."
fi

if [ "$HOOK_AGENT" = bead-reviewer ]; then
    git_opts="($t(-C|-c|--git-dir|--work-tree)$t[^$t]*|$t-[^$t]*)*"
    # --fix rewrites files in any tool (cargo clippy --fix); only a search pattern may name it.
    fixes="$(grep -E -- "$t--fix(=|$t|\$)" <<<"$cmds" | grep -Ev -- "^(grep|egrep|fgrep|rg)$t")"
    if [ -n "$writes" ] || [ -n "$fixes" ] ||
        runs "^git$git_opts$t(add|commit|push|reset|checkout|switch|restore|stash|clean|rebase|merge|cherry-pick|rm|mv)($t|\$)" ||
        runs "^cargo($t[-+][^$t]*)*$t(fmt|fix|add|remove|update)($t|\$)" ||
        runs "^bd($t-[^$t]*)*$t(update|close|create|delete|dep|label|remember|forget|human)($t|\$)|^gh($t|\$)"; then
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
    loop_review_valid "$tree" ||
        hook_deny "No passing review for the current tree of $bead. Launch the bead-reviewer agent with 'Review bead $bead' (any edit to code or the loop's files after the review needs a new one)."
fi
exit 0
