#!/usr/bin/env bash
# Runs the bead loop's hook <name> from the tree the session works in.
# Claude Code starts hook commands in the project's config root. For a session in a worktree of
# the Claude desktop app that is the main checkout (the app passes --project-config-root), so a
# hook resolved from the process directory would run the main checkout's copy. The hook input's
# cwd is the session's working directory: the hook script of that tree runs, so a worktree session
# runs its own branch's hooks. When cwd is outside a loop-enabled tree, or that tree has no such
# hook yet, the copy next to this dispatcher runs.
# Usage (.claude/settings.json): bash scripts/loop/hooks/dispatch.sh <name>, hook input on stdin.
set -uo pipefail
name="${1:-}"
[[ "$name" =~ ^[a-z][a-z-]*$ ]] || exit 0
input="$(cat)"
script="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$name.sh"
cwd="$(jq -r '.cwd // empty' <<<"$input" 2>/dev/null)"
if [ -n "$cwd" ] && root="$(git -C "$cwd" rev-parse --show-toplevel 2>/dev/null)" &&
    [ -f "$root/scripts/loop/hooks/$name.sh" ]; then
    script="$root/scripts/loop/hooks/$name.sh"
fi
[ -f "$script" ] || exit 0
exec bash "$script" <<<"$input"
