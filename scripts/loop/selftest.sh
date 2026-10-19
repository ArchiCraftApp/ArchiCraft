#!/usr/bin/env bash
# Self-test of the bead loop's hooks and policy scripts: drives each hook with Claude Code hook
# input in a throwaway repository, with stubs for bd, gh and cargo. Runs locally and in CI
# (needs neither cargo nor bd).
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
# Stub: bd show <id> --json answers from FAKE_BD_STATUS and FAKE_BD_LABELS; list, children
# and close work on the JSON array in FAKE_BD_DB (closed ids go to FAKE_BD_DB.closed; close
# refuses the ids listed in FAKE_BD_REFUSE, as bd does for a blocked bead).
db="${FAKE_BD_DB:-}"
case "$1" in
    show)
        jq -n --arg id "$2" --arg s "${FAKE_BD_STATUS:-in_progress}" --arg l "${FAKE_BD_LABELS:-}" \
            '[{id: $id, status: $s, issue_type: "task", labels: ($l | split(",") | map(select(. != "")))}]'
        ;;
    list)
        label="" status=""
        while [ $# -gt 0 ]; do
            case "$1" in --label) label="$2"; shift ;; --status) status="$2"; shift ;; esac
            shift
        done
        [ -n "$db" ] || { echo '[]'; exit 0; }
        jq --arg l "$label" --arg s "$status" \
            '[.[] | select(($l == "" or ((.labels // []) | index($l))) and ($s == "" or .status == $s))]' "$db"
        ;;
    children)
        [ -n "$db" ] || { echo '[]'; exit 0; }
        jq --arg p "$2" '[.[] | select(.parent == $p)]' "$db"
        ;;
    close)
        if [[ ",${FAKE_BD_REFUSE:-}," == *",$2,"* ]]; then
            echo "Error: cannot close $2: blocked by open issues" >&2
            exit 1
        fi
        [ -n "$db" ] && echo "$2" >>"$db.closed"
        ;;
    epic) [ -n "$db" ] && echo "archicraft-epic1 Demo epic (2/2 closed)" ;;
esac
exit 0
EOF
cat >"$work/bin/gh" <<'EOF'
#!/usr/bin/env bash
# Stub: every call goes to FAKE_GH_LOG. pr view answers the open PR's url from FAKE_GH_URL
# (--json url,state), whether auto-merge is on from FAKE_GH_AUTO (--json autoMergeRequest) and
# the state from FAKE_GH_STATE otherwise; pr create prints a url; pr diff prints the files in
# FAKE_GH_FILES, or fails when FAKE_GH_FAIL is set.
[ -n "${FAKE_GH_LOG:-}" ] && echo "$*" >>"$FAKE_GH_LOG"
case "$1 $2" in
    "pr view")
        case "$*" in
            *url,state*) echo "${FAKE_GH_URL:-}" ;;
            *autoMergeRequest*) echo "${FAKE_GH_AUTO:-false}" ;;
            *) echo "${FAKE_GH_STATE:-}" ;;
        esac
        ;;
    "pr create") echo "https://github.com/o/r/pull/9" ;;
    "pr diff")
        [ -z "${FAKE_GH_FAIL:-}" ] || exit 1
        for f in ${FAKE_GH_FILES:-}; do echo "$f"; done
        ;;
esac
exit 0
EOF
# Stub: every cargo step passes, so gate.sh runs in seconds when the Stop hook starts it.
printf '#!/bin/sh\nexit 0\n' >"$work/bin/cargo"
chmod +x "$work/bin/bd" "$work/bin/gh" "$work/bin/cargo"
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
expect() { if [ "$2" = "$3" ]; then passed=$((passed + 1)); else failed=$((failed + 1)); echo "FAIL $1: expected $2, got $3"; fi; }
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
# Auto-merge stays off for a PR that changes the CI workflows or the hook wiring (LOOP_USER_REVIEW_PATHS).
check "auto-merge for a PR that changes a CI workflow" deny \
    "$(FAKE_GH_FILES='docs/a.md .github/workflows/ci.yml' hook guard-bash "$(bash_input 'gh pr merge 12 --auto --squash')")"
check "auto-merge for a PR that changes the hook wiring" deny \
    "$(FAKE_GH_FILES='.claude/settings.json' hook guard-bash "$(bash_input 'gh pr merge "https://github.com/o/r/pull/12" --squash --auto')")"
check "auto-merge for the branch's PR that changes the hook wiring" deny \
    "$(FAKE_GH_FILES='.claude/settings.json' hook guard-bash "$(bash_input 'git push && gh pr merge --auto --squash')")"
check "auto-merge for a PR that changes other loop files" allow \
    "$(FAKE_GH_FILES='scripts/loop/land.sh .claude/settings.local.json .github/CODEOWNERS' hook guard-bash "$(bash_input 'gh pr merge 12 --auto --squash')")"
check "auto-merge off for a PR that changes a CI workflow" allow \
    "$(FAKE_GH_FILES='.github/workflows/ci.yml' hook guard-bash "$(bash_input 'gh pr merge 12 --disable-auto')")"
check "auto-merge for a PR named by a variable" deny "$(hook guard-bash "$(bash_input 'gh pr merge "$url" --auto --squash')")"
check "auto-merge for a PR whose files cannot be listed" deny \
    "$(FAKE_GH_FAIL=1 hook guard-bash "$(bash_input 'gh pr merge 12 --auto --squash')")"
check "writing a stamp" deny "$(hook guard-bash "$(bash_input "echo x > $(loop_dir)/review.ok")")"
check "reading a gate log" allow "$(hook guard-bash "$(bash_input "tail -n 50 $(loop_dir)/gate-test.log")")"
check "reviewer runs tests" allow "$(hook guard-bash "$(bash_input 'cargo test -p demo walls 2>&1 | tail -n 40' bead-reviewer)")"
check "reviewer greps an arrow" allow "$(hook guard-bash "$(bash_input 'grep -n "fn area() ->" crates/demo/src/lib.rs' bead-reviewer)")"
check "reviewer edits with sed" deny "$(hook guard-bash "$(bash_input 'sed -i "" s/a/b/ crates/demo/src/lib.rs' bead-reviewer)")"
check "reviewer redirects" deny "$(hook guard-bash "$(bash_input 'echo x > notes.txt' bead-reviewer)")"
check "reviewer commits" deny "$(hook guard-bash "$(bash_input 'git commit -am x' bead-reviewer)")"
# The reviewer's guard and the stamp guard look at the commands a line runs; quoted arguments and
# heredoc bodies are data (hooks/shell-commands.awk).
reviewer() { hook guard-bash "$(bash_input "$1" bead-reviewer)"; }
check "reviewer runs git merge-base" allow "$(reviewer 'git diff $(git merge-base origin/main HEAD) && git status --short')"
check "reviewer greps for file utilities" allow "$(reviewer 'grep -rnE "touch|rm -rf|mv |cp |tee " scripts/loop | head')"
check "reviewer greps for unquoted words" allow "$(reviewer 'rg -nw touch scripts/loop; grep -c tee docs/a.md')"
check "reviewer greps for git commands" allow "$(reviewer "grep -n 'git rm\\|git merge' docs/a.md")"
check "reviewer greps for a redirect" allow "$(reviewer 'grep -n "echo x > f" docs/a.md 2>/dev/null')"
check "reviewer prints a heredoc" allow "$(reviewer $'cat <<\'EOF\'\nrm -rf x > y\nEOF')"
check "reviewer merges" deny "$(reviewer 'git merge origin/main')"
check "reviewer merges in another directory" deny "$(reviewer 'git -C . merge --ff-only origin/main')"
check "reviewer removes with git" deny "$(reviewer 'git rm crates/demo/src/lib.rs')"
check "reviewer touches a file" deny "$(reviewer 'touch crates/demo/src/x.rs')"
check "reviewer removes through xargs" deny "$(reviewer 'grep -rl demo crates | xargs -n 1 rm -f')"
check "reviewer removes through find -exec" deny "$(reviewer "find crates -name '*.rs' -exec rm {} \\;")"
check "reviewer deletes through find" deny "$(reviewer 'find crates -name x.rs -delete')"
check "reviewer writes with tee" deny "$(reviewer 'cargo test 2>&1 | tee out.txt')"
check "reviewer writes through a heredoc" deny "$(reviewer $'cat > notes.md <<\'EOF\'\nonly text\nEOF')"
check "reviewer writes in a substitution" deny "$(reviewer 'echo "$(touch x)"')"
check "reviewer writes through sh -c" deny "$(reviewer 'bash -c "echo x > notes.txt"')"
check "reviewer writes through a heredoc fed to a shell" deny "$(reviewer $'bash <<\'EOF\'\ntouch x\nEOF')"
check "reviewer pipes a script into a shell" deny "$(reviewer 'echo rm -rf crates | sh')"
check "reviewer runs the selftest and greps its output" allow \
    "$(reviewer "/bin/bash scripts/loop/selftest.sh 2>&1 | grep -E 'FAIL|touch|loop selftest'")"
check "reviewer checks out behind a wrapper" deny "$(reviewer 'timeout 60 git checkout -- .')"
check "reviewer fixes with clippy" deny "$(reviewer 'cargo clippy --fix --allow-dirty')"
check "reviewer greps for --fix" allow "$(reviewer 'grep -rn -- --fix scripts/loop')"
check "heredoc that quotes the stamp dir" allow \
    "$(hook guard-bash "$(bash_input $'cat > docs/a.md <<\'EOF\'\nStamps: $(git rev-parse --git-path loop) > gate.ok\nEOF')")"
check "bd note that quotes the stamp dir" allow \
    "$(hook guard-bash "$(bash_input "bd update x --append-notes 'denied: echo > \$(git rev-parse --git-path loop)/gate.ok'")")"
check "writing a stamp at a quoted git-path" deny \
    "$(hook guard-bash "$(bash_input 'echo x > "$(git rev-parse --git-path loop)/review.ok"')")"
check "touching a stamp" deny "$(hook guard-bash "$(bash_input 'touch "$(git rev-parse --git-path loop)/gate.ok"')")"
check "copying to a quoted stamp path" deny "$(hook guard-bash "$(bash_input "cp /tmp/x '$(loop_dir)/review.ok'")")"
check "writing a stamp through sh -c" deny "$(hook guard-bash "$(bash_input "sh -c 'printf x >$(loop_dir)/gate.ok'")")"
check "writing a stamp with tee" deny \
    "$(hook guard-bash "$(bash_input 'printf x | tee "$(git rev-parse --git-path loop)/review.ok" >/dev/null')")"
check "writing a stamp behind a wrapper" deny \
    "$(hook guard-bash "$(bash_input 'timeout 5 touch "$(git rev-parse --git-path loop)/gate.ok"')")"
check "writing in the state dir after a cd" deny "$(hook guard-bash "$(bash_input 'cd "$(git rev-parse --git-path loop)" && touch pause')")"
check "writing in the state dir through a variable" deny \
    "$(hook guard-bash "$(bash_input 'd=$(git rev-parse --git-path loop); echo x > "$d/pause"')")"
check "writing in the state dir after a cd through a variable" deny \
    "$(hook guard-bash "$(bash_input 'd=$(git rev-parse --git-path loop); cd "$d" && touch bead')")"
check "writing in the state dir through a pipe" deny \
    "$(hook guard-bash "$(bash_input 'git rev-parse --git-path loop | xargs -I{} touch {}/pause')")"
check "reading a log in the state dir after a cd" allow \
    "$(hook guard-bash "$(bash_input 'cd "$(git rev-parse --git-path loop)" && tail -n 20 gate-test.log')")"
commands() { printf '%s\n' "$1" | awk -f scripts/loop/hooks/shell-commands.awk | cut -f1 | LC_ALL=C sort | paste -sd ' ' -; }
expect "a commit message in a heredoc is data" "cat git" \
    "$(commands $'git commit -m "$(cat <<\'EOF\'\nSay "hi" > there; rm -rf x\nEOF\n)"')"
expect "wrappers and keywords are not the program" "> cp done for ln mv rm rm rm rm rm ssh touch watch" \
    "$(commands $'for f in a; do sudo -u root rm "$f"; done\nX=1 env -i nohup rm y 2>&1 >out
timeout -s KILL 5 stdbuf -o L ionice -c 3 setsid /bin/rm z\nfunction f { touch t; }; coproc mv a b
xargs --max-args 1 cp -t d; watch -n 1 "ln -s a b"; ssh -p 22 host rm w; strace -e trace=all rm v')"

echo 'pub fn demo() { let _x = 1; }' >crates/demo/src/lib.rs
check "code commit without stamps" deny "$(hook guard-bash "$(bash_input 'git add -A && git commit -m x')")"
stamp gate.ok
check "code commit with the gate only" deny "$(hook guard-bash "$(bash_input 'git commit -am x')")"
stamp review.ok
check "code commit with gate and review" allow "$(hook guard-bash "$(bash_input 'git commit -am x')")"
echo 'minor fix' >>docs/a.md
check "code commit after a doc fix, before a new gate" deny "$(hook guard-bash "$(bash_input 'git commit -am x')")"
stamp gate.ok
check "code commit after a doc fix and a new gate" allow "$(hook guard-bash "$(bash_input 'git commit -am x')")"
loop_review_valid && passed=$((passed + 1)) || { failed=$((failed + 1)); echo "FAIL review not carried over a doc fix"; }
echo '# minor fix' >>scripts/loop/gate.sh
stamp gate.ok
check "code commit after a loop-file fix and a new gate" deny "$(hook guard-bash "$(bash_input 'git commit -am x')")"
git checkout -q -- scripts/loop/gate.sh
mv crates/demo/tests/golden/a.ifcx docs/a.ifcx
stamp gate.ok
check "code commit after moving a code file to docs/ and a new gate" deny "$(hook guard-bash "$(bash_input 'git commit -am x')")"
mv docs/a.ifcx crates/demo/tests/golden/a.ifcx
echo '// later edit' >>crates/demo/src/lib.rs
check "code commit after a later edit" deny "$(hook guard-bash "$(bash_input 'git commit -am x')")"
stamp gate.ok
check "code commit after a code fix and a new gate" deny "$(hook guard-bash "$(bash_input 'git commit -am x')")"
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
# The hook runs the gate itself (cargo is a stub) on a tree without a gate result.
stamp gate.ok
stamp review.ok
echo 'typo fix' >>docs/a.md
out="$(hook stop-gate "$stop_input")"
check "stop after a doc fix following a PASS" block "$out"
grep -q 'passed the gate and the review but is not committed' <<<"$out" && passed=$((passed + 1)) ||
    { failed=$((failed + 1)); echo "FAIL stop after a doc fix asks for a new review: $out"; }
echo '// code fix' >>crates/demo/src/lib.rs
out="$(hook stop-gate "$stop_input")"
grep -q 'passed the gate; the review is missing' <<<"$out" && passed=$((passed + 1)) ||
    { failed=$((failed + 1)); echo "FAIL stop after a code fix following a PASS does not ask for a review: $out"; }
git checkout -q -- docs/a.md
echo 'pub fn demo() { let _z = 3; }' >crates/demo/src/lib.rs
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

# --- sweep -------------------------------------------------------------------------------------
export FAKE_BD_DB="$work/beads.json"
cat >"$FAKE_BD_DB" <<'EOF'
[
  {"id": "x-area", "status": "open", "labels": ["split"]},
  {"id": "x-blocked", "status": "in_progress", "labels": ["loop-split"]},
  {"id": "x-blocked.1", "status": "closed", "parent": "x-blocked"},
  {"id": "x-done", "status": "in_progress", "labels": ["loop-split"]},
  {"id": "x-done.1", "status": "closed", "parent": "x-done"},
  {"id": "x-busy", "status": "in_progress", "labels": ["loop-split"]},
  {"id": "x-busy.1", "status": "closed", "parent": "x-busy"},
  {"id": "x-busy.2", "status": "open", "parent": "x-busy"},
  {"id": "x-empty", "status": "in_progress", "labels": ["loop-split"]},
  {"id": "x-merged-blocked", "status": "in_progress", "external_ref": "https://github.com/o/r/pull/8"},
  {"id": "x-merged", "status": "in_progress", "external_ref": "https://github.com/o/r/pull/7"},
  {"id": "x-human", "status": "open", "labels": ["human"], "title": "Needs a decision"}
]
EOF
: >"$FAKE_BD_DB.closed"
FAKE_GH_STATE=MERGED FAKE_BD_REFUSE=x-blocked,x-merged-blocked bash scripts/loop/sweep.sh >"$work/sweep.out" 2>"$work/sweep.err" ||
    { failed=$((failed + 1)); echo "FAIL sweep exited non-zero after a refused close"; }
[ -s "$work/sweep.err" ] && { failed=$((failed + 1)); echo "FAIL sweep wrote to stderr:"; cat "$work/sweep.err"; }
expect "sweep closes merged beads and only loop-split parents whose children are all closed" "x-done x-merged" \
    "$(sort "$FAKE_BD_DB.closed" | tr '\n' ' ' | sed 's/ $//')"
expect "sweep reports each refused close" 2 "$(grep -cE '^refused  x-(blocked|merged-blocked): .*blocked by open issues' "$work/sweep.out")"
expect "sweep reports beads for a human after a refused close" 1 "$(grep -c '^  x-human Needs a decision$' "$work/sweep.out")"
expect "sweep reports epics due for a demo after a refused close" 1 \
    "$(grep -c '^  archicraft-epic1 Demo epic' "$work/sweep.out")"
unset FAKE_BD_DB

# --- diff budget -------------------------------------------------------------------------------
reset_state
printf 'pub fn demo() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn old() {}\n}\n' >crates/demo/src/lib.rs
git add -A && git commit -qm "budget base"
budget_base="$(git rev-parse HEAD)"
# Non-test lines: a new function, a test-only helper and a module with a nested test module that
# does not end the file (8 lines); the file's own test module grows by 2.
printf '%s\n' 'pub fn demo() {}' 'pub fn added() {}' '#[cfg(test)]' 'pub fn helper() {}' 'mod inner {' \
    '    #[cfg(test)]' '    mod tests {}' '    pub fn g() {}' '}' '' '#[cfg(test)]' '#[allow(dead_code)]' \
    'mod tests {' '    #[test]' '    fn old() {}' '    #[test]' '    fn new() {}' '}' >crates/demo/src/lib.rs
# A new source file (1 line before its test module); an out-of-line test module declaration and a
# test-only module not named tests, which clippy does not keep last, count like any other line (4);
# an integration test does not count.
printf 'pub fn extra() {}\n#[cfg(test)] mod tests { fn t() {} }\n' >crates/demo/src/extra.rs
printf '#[cfg(test)]\nmod tests;\n#[cfg(test)]\nmod fixtures {}\n' >crates/demo/src/other.rs
printf '#[test]\nfn it() {}\n' >crates/demo/tests/it.rs
expect "diff budget leaves out inline test modules and tests/" 13 "$(loop_budget_lines "$budget_base")"
reset_state

# --- land.sh helpers ---------------------------------------------------------------------------
srs() { printf '%b' "$1" | loop_srs_ids; }
expect "SRS ids from a notes line" "SEL-003, SEL-004" "$(srs 'SRS: SEL-003 SEL-004')"
expect "SRS ids up to the end of the sentence" "NFR-ACC-003" \
    "$(srs 'SRS: NFR-ACC-003. Reference only: wip/x, see WAL-020.\nPR https://x/pull/5')"
expect "SRS ids in one-paragraph notes" "WAL-020, NFR-PERF-005" \
    "$(srs 'Split from x. SRS: WAL-020 (partial, see 4.2), NFR-PERF-005. Depends on GEO-004.')"
expect "SRS ids with digits in the prefix" "NFR-L10N-003, NFR-A11Y-001" "$(srs 'SRS: NFR-L10N-003, NFR-A11Y-001\nFollow-up')"
expect "notes without SRS ids" "" "$(srs 'No requirements. Mentions WAL-020 only in passing.')"
printf '%s\n' 'Intro' '## Acceptance criteria' '- ok' '```bash' '# a comment' '```' '### Detail' '## Findings' \
    >"$work/review.md"
expect "review headings nest below the PR body's level-2 heading" \
    "Intro|### Acceptance criteria|- ok|\`\`\`bash|# a comment|\`\`\`|#### Detail|### Findings" \
    "$(loop_nest_headings 3 "$work/review.md" | paste -sd '|' -)"

# --- land.sh: auto-merge, or wait for the user's review ----------------------------------------
reset_state
git switch -q main
git init -q --bare "$work/origin.git"
git remote add origin "$work/origin.git"
git push -q origin main
export FAKE_GH_LOG="$work/gh.log"
land() { # land <labels>: land.sh's output for the committed tree, with gate and review stamped
    stamp gate.ok
    stamp review.ok
    : >"$FAKE_GH_LOG"
    FAKE_BD_LABELS="$1" bash scripts/loop/land.sh 2>&1
}
commit_on() { # commit_on <branch> <file>...: a new branch off main with a change to each file
    local b="$1" f
    shift
    git switch -q -c "$b" main
    for f in "$@"; do mkdir -p "$(dirname "$f")" && echo "# $b" >>"$f"; done
    git add -A && git commit -qm "${b#bead/}: change"
}
merge_calls() { grep '^pr merge ' "$FAKE_GH_LOG" | paste -sd '|' -; }
pr=https://github.com/o/r/pull/9
commit_on bead/archicraft-fe01-feature crates/demo/src/lib.rs
out="$(land "")"
expect "land.sh turns on auto-merge for a feature bead" "pr merge $pr --auto --squash" "$(merge_calls)"
commit_on bead/archicraft-lp01-loop scripts/loop/gate.sh .claude/skills/bead/SKILL.md .claude/settings.local.json
out="$(land loop)"
expect "land.sh turns on auto-merge for a loop bead" "pr merge $pr --auto --squash" "$(merge_calls)"
commit_on bead/archicraft-lp02-ci scripts/loop/gate.sh .github/workflows/ci.yml
out="$(land loop)"
expect "land.sh leaves auto-merge off for a loop bead that changes a CI workflow" "" "$(merge_calls)"
expect "land.sh names the CI workflow that makes the PR wait" 1 \
    "$(grep -c "waits for the user's review.*\`\.github/workflows/ci\.yml\`" <<<"$out")"
commit_on bead/archicraft-lp03-hooks docs/a.md .claude/settings.json
out="$(land loop)"
expect "land.sh leaves auto-merge off for a loop bead that changes the hook wiring" "" "$(merge_calls)"
expect "land.sh names the hook wiring that makes the PR wait" 1 \
    "$(grep -c "waits for the user's review.*\`\.claude/settings\.json\`" <<<"$out")"
out="$(FAKE_GH_URL=$pr FAKE_GH_AUTO=true land loop)"
expect "land.sh turns auto-merge off when a PR that had it changes the hook wiring" "pr merge $pr --disable-auto" "$(merge_calls)"
git switch -q main
mkdir -p .github/workflows && echo 'on: push' >.github/workflows/ci.yml
git add -A && git commit -qm workflows && git push -q origin main
git switch -q -c bead/archicraft-lp04-move main
git mv .github/workflows/ci.yml docs/ci.yml && git commit -qm "archicraft-lp04: move"
out="$(land loop)"
expect "land.sh leaves auto-merge off for a loop bead that moves a CI workflow away" "" "$(merge_calls)"
unset FAKE_GH_LOG

[ -f "$work/crashes" ] && failed=$((failed + $(wc -l <"$work/crashes")))
echo "loop selftest: $passed passed, $failed failed"
[ "$failed" -eq 0 ]
