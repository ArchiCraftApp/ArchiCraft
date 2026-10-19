#!/usr/bin/env bash
# PostToolUse (Edit|Write|MultiEdit): rustfmt every edited Rust file (rustfmt.toml applies)
# and tell Claude when it cannot, which almost always means a syntax error.
set -uo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

file="$(hook_field .tool_input.file_path)"
case "$file" in *.rs) ;; *) exit 0 ;; esac
case "$file" in /*) ;; *) file="$HOOK_CWD/$file" ;; esac
[ -f "$file" ] || exit 0
hook_enter_repo "$(dirname "$file")" || exit 0

if ! out="$(rustfmt "$file" 2>&1)"; then
    hook_context PostToolUse "rustfmt could not format ${file#"$HOOK_ROOT"/} (likely a syntax error):
$(tail -n 20 <<<"$out")"
fi
exit 0
