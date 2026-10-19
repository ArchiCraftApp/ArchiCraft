# Shared preamble for the Claude Code hooks of the bead loop (wired in .claude/settings.json).
# Reads the hook input from stdin and changes into the repository the session works in.
# Outside a repository that has the loop, every hook allows everything.

HOOK_INPUT="$(cat)"
hook_field() { jq -r "$1 // empty" <<<"$HOOK_INPUT"; }

# PreToolUse: refuse the tool call; the reason is shown to Claude.
hook_deny() {
    jq -n --arg r "$1" '{hookSpecificOutput: {hookEventName: "PreToolUse", permissionDecision: "deny", permissionDecisionReason: $r}}'
    exit 0
}
# Stop: keep Claude working; the reason is shown to Claude.
hook_block() {
    jq -n --arg r "$1" '{decision: "block", reason: $r}'
    exit 0
}
# Any event that supports it: extra context for Claude's next request.
hook_context() {
    jq -n --arg e "$1" --arg c "$2" '{hookSpecificOutput: {hookEventName: $e, additionalContext: $c}}'
    exit 0
}

# Nearest existing directory of a path (a Write may create new directories).
hook_existing_dir() {
    local d="$1"
    while [ -n "$d" ] && [ ! -d "$d" ]; do d="$(dirname "$d")"; done
    printf '%s\n' "${d:-/}"
}

# cd into the repository root of the given directory; return 1 outside a loop-enabled repo.
hook_enter_repo() {
    local root
    root="$(git -C "$1" rev-parse --show-toplevel 2>/dev/null)" || return 1
    [ -f "$root/scripts/loop/lib.sh" ] || return 1
    cd "$root" || return 1
    # shellcheck source=../lib.sh
    source "$root/scripts/loop/lib.sh"
    HOOK_ROOT="$root"
}

HOOK_CWD="$(hook_field .cwd)"
HOOK_CWD="${HOOK_CWD:-$PWD}"
HOOK_AGENT="$(hook_field .agent_type)"
