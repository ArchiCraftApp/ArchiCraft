#!/usr/bin/env bash
# PreToolUse (Bash): the git and GitHub rules of the bead loop.
# - stamps are written only by gate.sh, the review hook and pause.sh;
# - the reviewer runs read-only commands;
# - no force pushes, no pushes to main, no PR merges without --auto;
# - a code commit happens only on a bead branch whose tree passed the gate and is covered by a
#   passing review (loop_review_valid);
# - no auto-merge for a PR that changes the CI workflows or the hook wiring: it waits for the user.
# All of them are judged by the commands the line runs (shell-commands.awk): quoted arguments and
# heredoc bodies that mention a command or the loop's state dir do not count.
# Pattern matching on the command line is a guardrail, not a sandbox; CI is the final gate.
set -uo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

cmd="$(hook_field .tool_input.command)"
[ -n "$cmd" ] || exit 0
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
    # git's global options, between git and its subcommand.
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

# The push and commit rules for one git command: git_checks <dir> <subcommand> <argument>...
# Runs in a subshell, since it enters the repository the command runs in.
git_checks() {
    hook_enter_repo "$1" || return 0
    local sub="$2" a bead tree
    shift 2
    case "$sub" in
    push)
        [ "$(loop_branch)" != main ] ||
            hook_deny "Nothing is pushed to main. Changes reach main only through a pull request that merges on green CI (scripts/loop/land.sh)."
        for a in "$@"; do
            if [[ "$a" =~ ^(--force|-[a-zA-Z]*f[a-zA-Z]*|\+.+)$ ]]; then
                hook_deny "Force pushes are refused. After a rebase of a bead branch use scripts/loop/land.sh --rebased (it pushes with --force-with-lease)."
            fi
            if [[ "$a" =~ ^(.*:)?(refs/heads/)?(main|master)$ ]]; then
                hook_deny "Nothing is pushed to main. Changes reach main only through a pull request that merges on green CI (scripts/loop/land.sh)."
            fi
        done
        ;;
    commit)
        loop_code_dirty || return 0
        bead="$(loop_bead_from_branch)" ||
            hook_deny "Code changes are committed only on a bead branch, after the gate and the review (docs/loop.md). This worktree is on '$(loop_branch)'; run /bead first."
        tree="$(loop_tree_hash)"
        loop_stamp_valid gate.ok "$tree" ||
            hook_deny "No passing gate for the current tree of $bead. Run scripts/loop/gate.sh (any edit after the gate needs a new run)."
        loop_review_valid "$tree" ||
            hook_deny "No passing review for the current tree of $bead. Launch the bead-reviewer agent with 'Review bead $bead' (any edit after the review other than to docs/ or a top-level Markdown file needs a new one)."
        ;;
    esac
}

# Every git command the line runs, in the directory its -C options name (relative ones in turn,
# from the hook's cwd). A commit message, a bead note or a grep pattern that names git push or
# git commit runs neither.
while IFS= read -r rec; do
    IFS="$t" read -r -a w <<<"$rec"
    dir="$HOOK_CWD"
    i=1
    while [ "$i" -lt "${#w[@]}" ] && [[ "${w[i]}" == -* ]]; do
        case "${w[i]}" in
        -C) case "${w[i + 1]:-}" in /*) dir="${w[i + 1]}" ;; *) dir="$dir/${w[i + 1]:-}" ;; esac; i=$((i + 2)) ;;
        -c | --git-dir | --work-tree) i=$((i + 2)) ;;
        *) i=$((i + 1)) ;;
        esac
    done
    [ "$i" -lt "${#w[@]}" ] || continue
    out="$(git_checks "$dir" "${w[@]:i}")"
    [ -z "$out" ] || { printf '%s\n' "$out"; exit 0; }
done < <(grep -E -- "^git($t|\$)" <<<"$cmds")

hook_enter_repo "$HOOK_CWD" || exit 0

# gh pr merge, judged by its own words.
while IFS= read -r rec; do
    IFS="$t" read -r -a w <<<"$rec"
    auto=""
    for a in "${w[@]:3}"; do
        case "$a" in
        --admin) hook_deny "PRs never merge with --admin; they merge when the required CI checks pass." ;;
        --auto | --auto=*) auto=on ;;
        --disable-auto) [ -n "$auto" ] || auto=off ;;
        esac
    done
    [ -n "$auto" ] ||
        hook_deny "PRs merge only through auto-merge on green CI: gh pr merge <pr> --auto --squash (scripts/loop/land.sh does this)."
    # Auto-merge stays off for a PR that changes the CI gate or the hook wiring, whoever turns it
    # on (LOOP_USER_REVIEW_PATHS; an older lib.sh without it keeps the older rules).
    [ "$auto" = on ] && [ -n "${LOOP_USER_REVIEW_PATHS:-}" ] || continue
    pr="${w[3]:-}"
    case "$pr" in -*) pr="" ;; esac # no PR named: the current branch's
    case "$pr" in *[\$\`\"\'\(]*) hook_deny "Name the PR literally (number, URL or branch) when turning on auto-merge, so the guard can check its files; or run scripts/loop/land.sh." ;; esac
    files="$(gh pr diff ${pr:+"$pr"} --name-only 2>/dev/null)" ||
        hook_deny "Could not list the files of PR '${pr:-of this branch}' (gh pr diff), so auto-merge stays off. scripts/loop/land.sh decides it from the branch."
    waits="$(grep -E "$LOOP_USER_REVIEW_PATHS" <<<"$files" | paste -sd ' ' - || true)"
    [ -z "$waits" ] ||
        hook_deny "This PR changes the CI workflows or the hook wiring ($waits), so it waits for the user's review and auto-merge stays off (user decision of 2026-10-19, docs/loop.md)."
done < <(grep -E -- "^gh${t}pr${t}merge($t|\$)" <<<"$cmds")
exit 0
