#!/usr/bin/env bash
# SessionStart: one short block of loop state, so a new, resumed or compacted session knows
# whether it is in the middle of a bead.
set -uo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/common.sh"
hook_enter_repo "$HOOK_CWD" || exit 0
"$HOOK_ROOT/scripts/loop/status.sh"
loop_bead_from_branch >/dev/null && echo "Continue with /bead (it resumes this bead)."
exit 0
