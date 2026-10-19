#!/usr/bin/env bash
# Self-test of the bead loop's hooks and policy scripts: drives each hook with Claude Code hook
# input in a throwaway repository, with a stub bd. Runs locally and in CI (no cargo, no bd).
# Usage: scripts/loop/selftest.sh
set -uo pipefail
src="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
work="$(mktemp -d "${TMPDIR:-/tmp}/loop-selftest.XXXXXX")"
trap 'rm -rf "$work"' EXIT
repo="$work/repo"
passed=0
failed=0

# --- fixture: a tiny repository with the loop scripts and a stub bd --------------------------
mkdir -p "$repo/scripts" "$work/bin" "$repo/crates/demo/src" "$repo/crates/demo/tests/golden" "$repo/docs"
cp -R "$src" "$repo/scripts/loop"
cat >"$work/bin/bd" <<'EOF'
#!/usr/bin/env bash
# Stub: bd show <id> --json answers from FAKE_BD_STATUS and FAKE_BD_LABELS; the rest succeeds.
if [ "$1" = show ]; then
    jq -n --arg id "$2" --arg s "${FAKE_BD_STATUS:-in_progress}" --arg l "${FAKE_BD_LABELS:-}" \
        '[{id: $id, status: $s, issue_type: "task", labels: ($l | split(",") | map(select(. != "")))}]'
fi
exit 0
EOF
chmod +x "$work/bin/bd"
export PATH="$work/bin:$PATH"
cd "$repo" || exit 2
git init -q -b main
git config user.email selftest@example.invalid
git config user.name selftest
echo 'pub fn demo() {}' >crates/demo/src/lib.rs
echo '{}' >crates/demo/tests/golden/a.ifcx
echo '# doc' >docs/a.md
git add -A && git commit -qm init
# shellcheck source=lib.sh
source scripts/loop/lib.sh

# --- helpers -----------------------------------------------------------------------------------
hook() { # hook <name> <json>: the hook's stdout; a crash or stderr output counts as a failure
    local out err code
    err="$(mktemp "$work/err.XXXXXX")"
    out="$(printf '%s' "$2" | bash "scripts/loop/hooks/$1.sh" 2>"$err")"
    code=$?
    if [ "$code" -ne 0 ] || [ -s "$err" ]; then
        echo "$1" >>"$work/crashes" # hook runs in a subshell; counted at the end
        printf 'FAIL hook %s exited %s:\n%s\n' "$1" "$code" "$(cat "$err")" >&2
    fi
    rm -f "$err"
    printf '%s' "$out"
}
edit_input() { jq -nc --arg cwd "$repo" --arg f "$repo/$1" --arg a "${2:-}" \
    '{cwd: $cwd, tool_name: "Edit", tool_input: {file_path: $f}} + (if $a == "" then {} else {agent_type: $a} end)'; }
bash_input() { jq -nc --arg cwd "$repo" --arg c "$1" --arg a "${2:-}" \
    '{cwd: $cwd, tool_name: "Bash", tool_input: {command: $c}} + (if $a == "" then {} else {agent_type: $a} end)'; }
check() { # check <description> <expected: deny|allow|block|pass> <output>
    local got=allow
    if jq -e '.hookSpecificOutput.permissionDecision == "deny"' <<<"$3" >/dev/null 2>&1; then got=deny; fi
    if jq -e '.decision == "block"' <<<"$3" >/dev/null 2>&1; then got=block; fi
    if [ "$got" = "$2" ]; then
        passed=$((passed + 1))
    else
        failed=$((failed + 1))
        printf 'FAIL %s: expected %s, got %s\n%s\n' "$1" "$2" "$got" "$3"
    fi
}
stamp() { loop_stamp_write "$1" "$(loop_tree_hash)" test; }
reset_state() { rm -rf "$(loop_dir)"; git checkout -q -- . 2>/dev/null; git clean -qfd crates docs; }

# --- guard-edit --------------------------------------------------------------------------------
check "code edit on main" deny "$(hook guard-edit "$(edit_input crates/demo/src/lib.rs)")"
check "doc edit on main" allow "$(hook guard-edit "$(edit_input docs/a.md)")"
check "new code file in a new directory on main" deny "$(hook guard-edit "$(edit_input crates/demo/src/new/mod.rs)")"
check "golden edit on main" deny "$(hook guard-edit "$(edit_input crates/demo/tests/golden/a.ifcx)")"
check "loop edit on a non-bead branch" allow "$(hook guard-edit "$(edit_input scripts/loop/gate.sh)")"
git switch -q -c bead/archicraft-ab12.1-demo
check "code edit on a bead in progress" allow "$(hook guard-edit "$(edit_input crates/demo/src/lib.rs)")"
rm -f "$(loop_dir)/bead"
check "code edit on an open bead" deny "$(FAKE_BD_STATUS=open hook guard-edit "$(edit_input crates/demo/src/lib.rs)")"
check "code edit by the reviewer" deny "$(hook guard-edit "$(edit_input crates/demo/src/lib.rs bead-reviewer)")"
check "golden edit on a bead" deny "$(hook guard-edit "$(edit_input crates/demo/tests/golden/a.ifcx)")"
check "loop edit in a feature bead" deny "$(hook guard-edit "$(edit_input .claude/settings.json)")"
check "loop edit in a loop bead" allow "$(FAKE_BD_LABELS=loop hook guard-edit "$(edit_input scripts/loop/gate.sh)")"
check "relative path edit" deny "$(hook guard-edit "$(jq -nc --arg cwd "$repo" '{cwd: $cwd, tool_input: {file_path: "crates/demo/tests/golden/a.ifcx"}}')")"

# --- guard-bash --------------------------------------------------------------------------------
check "force push" deny "$(hook guard-bash "$(bash_input 'git push --force origin bead/x')")"
check "short force push" deny "$(hook guard-bash "$(bash_input 'git push -uf origin bead/x')")"
check "plus-refspec push" deny "$(hook guard-bash "$(bash_input 'git push origin +bead/x')")"
check "force-with-lease push of a bead branch" allow "$(hook guard-bash "$(bash_input 'git push --force-with-lease -u origin bead/x')")"
check "push to main" deny "$(hook guard-bash "$(bash_input 'git push origin HEAD:main')")"
check "push then rm -rf" allow "$(hook guard-bash "$(bash_input 'git push -u origin bead/x && rm -rf /tmp/x')")"
check "merge without auto" deny "$(hook guard-bash "$(bash_input 'gh pr merge 12 --squash')")"
check "merge with admin" deny "$(hook guard-bash "$(bash_input 'gh pr merge 12 --auto --admin')")"
check "merge with auto" allow "$(hook guard-bash "$(bash_input 'gh pr merge 12 --auto --squash')")"
check "writing a stamp" deny "$(hook guard-bash "$(bash_input "echo x > $(loop_dir)/review.ok")")"
check "reading a gate log" allow "$(hook guard-bash "$(bash_input "tail -n 50 $(loop_dir)/gate-test.log")")"
check "reviewer runs tests" allow "$(hook guard-bash "$(bash_input 'cargo test -p demo walls 2>&1 | tail -n 40' bead-reviewer)")"
check "reviewer greps an arrow" allow "$(hook guard-bash "$(bash_input 'grep -n "fn area() ->" crates/demo/src/lib.rs' bead-reviewer)")"
check "reviewer edits with sed" deny "$(hook guard-bash "$(bash_input 'sed -i "" s/a/b/ crates/demo/src/lib.rs' bead-reviewer)")"
check "reviewer redirects" deny "$(hook guard-bash "$(bash_input 'echo x > notes.txt' bead-reviewer)")"
check "reviewer commits" deny "$(hook guard-bash "$(bash_input 'git commit -am x' bead-reviewer)")"

echo 'pub fn demo() { let _x = 1; }' >crates/demo/src/lib.rs
check "code commit without stamps" deny "$(hook guard-bash "$(bash_input 'git add -A && git commit -m x')")"
stamp gate.ok
check "code commit with the gate only" deny "$(hook guard-bash "$(bash_input 'git commit -am x')")"
stamp review.ok
check "code commit with gate and review" allow "$(hook guard-bash "$(bash_input 'git commit -am x')")"
echo '// later edit' >>crates/demo/src/lib.rs
check "code commit after a later edit" deny "$(hook guard-bash "$(bash_input 'git commit -am x')")"
reset_state
git switch -q main
echo 'pub fn demo() { let _y = 2; }' >crates/demo/src/lib.rs
check "code commit on main" deny "$(hook guard-bash "$(bash_input 'git commit -am x')")"
check "push while on main" deny "$(hook guard-bash "$(bash_input 'git push')")"
reset_state
echo 'more' >>docs/a.md
check "doc commit on main" allow "$(hook guard-bash "$(bash_input 'git commit -am docs')")"
reset_state

# --- stop-gate ---------------------------------------------------------------------------------
stop_input="$(jq -nc --arg cwd "$repo" '{cwd: $cwd, stop_hook_active: false}')"
check "stop on main" allow "$(hook stop-gate "$stop_input")"
git switch -q bead/archicraft-ab12.1-demo
check "stop on a clean bead branch" allow "$(hook stop-gate "$stop_input")"
echo 'pub fn demo() { let _z = 3; }' >crates/demo/src/lib.rs
stamp gate.ok
stamp review.ok
check "stop with reviewed, uncommitted code" block "$(hook stop-gate "$stop_input")"
rm -f "$(loop_dir)/review.ok"
check "stop with gated, unreviewed code" block "$(hook stop-gate "$stop_input")"
rm -f "$(loop_dir)/gate.ok" "$(loop_dir)/stop-blocks"
stamp gate.fail
echo "GATE FAIL: test" >"$(loop_dir)/gate-summary.txt"
out="$(hook stop-gate "$stop_input")"
check "stop with a failed gate" block "$out"
grep -q 'GATE FAIL: test' <<<"$out" && passed=$((passed + 1)) || { failed=$((failed + 1)); echo "FAIL stop reason lacks the gate summary"; }
hook stop-gate "$stop_input" >/dev/null
hook stop-gate "$stop_input" >/dev/null
check "stop after three blocks" allow "$(hook stop-gate "$stop_input")"
rm -f "$(loop_dir)/stop-blocks"
stamp pause
check "stop when paused" allow "$(hook stop-gate "$stop_input")"

# --- review-verdict ----------------------------------------------------------------------------
reset_state
echo 'pub fn demo() { let _w = 4; }' >crates/demo/src/lib.rs
verdict() { jq -nc --arg cwd "$repo" --arg m "$1" --arg a "${2:-bead-reviewer}" \
    '{cwd: $cwd, agent_type: $a, last_assistant_message: $m}'; }
expect_review() { # <description> <yes|no>
    local got=no
    loop_stamp_valid review.ok && got=yes
    if [ "$got" = "$2" ]; then passed=$((passed + 1)); else failed=$((failed + 1)); echo "FAIL $1: review stamp $got"; fi
}
hook review-verdict "$(verdict $'## Findings\nnone\nVERDICT: PASS')" >/dev/null
expect_review "PASS without a gate" no
stamp gate.ok
hook review-verdict "$(verdict $'VERDICT: PASS' general-purpose)" >/dev/null
expect_review "PASS from another agent" no
hook review-verdict "$(verdict $'## Findings\nnone\nVERDICT: PASS')" >/dev/null
expect_review "PASS on a gated tree" yes
hook review-verdict "$(verdict $'1. Blocking\nVERDICT: FAIL')" >/dev/null
expect_review "FAIL after a PASS" no

# --- ci-check ----------------------------------------------------------------------------------
reset_state
base="$(git rev-parse main)"
ci() { HEAD_REF="$1" BASE_SHA="$base" PR_TITLE="$2" PR_LABELS="${3:-}" bash scripts/loop/ci-check.sh >/dev/null 2>&1 && echo pass || echo fail; }
expect() { if [ "$2" = "$3" ]; then passed=$((passed + 1)); else failed=$((failed + 1)); echo "FAIL $1: expected $2, got $3"; fi; }
echo 'pub fn demo() { let _v = 5; }' >crates/demo/src/lib.rs
git add -A && git commit -qm "bead change"
expect "bead PR with its id in the title" pass "$(ci bead/archicraft-ab12.1-demo 'archicraft-ab12.1: Demo change')"
expect "bead PR without its id" fail "$(ci bead/archicraft-ab12.1-demo 'Demo change')"
echo '{"x":1}' >crates/demo/tests/golden/a.ifcx
git commit -qam golden
expect "golden change without label" fail "$(ci bead/archicraft-ab12.1-demo 'archicraft-ab12.1: x')"
expect "golden change with label" pass "$(ci bead/archicraft-ab12.1-demo 'archicraft-ab12.1: x' format-change)"
echo '# tweak' >>scripts/loop/gate.sh
git commit -qam loop
expect "loop change in a feature bead" fail "$(ci bead/archicraft-ab12.1-demo 'archicraft-ab12.1: x' format-change)"
expect "loop change in a loop bead" pass "$(ci bead/archicraft-ab12.1-demo 'archicraft-ab12.1: x' format-change,loop)"
expect "loop change outside the loop" pass "$(ci claude/setup 'Set up the loop' format-change)"

[ -f "$work/crashes" ] && failed=$((failed + $(wc -l <"$work/crashes")))
echo "loop selftest: $passed passed, $failed failed"
[ "$failed" -eq 0 ]
