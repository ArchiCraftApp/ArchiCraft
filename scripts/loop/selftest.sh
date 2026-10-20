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
# The gate's layering step runs the repository guard next to the loop scripts.
cp "$src/../check-layering.sh" "$repo/scripts/"
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
# the state from FAKE_GH_STATE otherwise; pr create prints a url; pr diff answers only for the PR
# in FAKE_GH_PR (as named, HEAD for the branch's PR, "<repo>#<pr>" with -R) and prints a diff of the
# files in FAKE_GH_FILES ("<old>=><new>" is a move; --name-only lists only the new paths, like gh),
# each changing line x into FAKE_GH_LINE (default y), or fails when FAKE_GH_FAIL is set; pr view --json labels prints FAKE_GH_LABELS, or fails when
# FAKE_GH_FAIL is set. pr create and pr edit copy the PR body (--body-file) to FAKE_GH_BODY when
# it is set.
[ -n "${FAKE_GH_LOG:-}" ] && echo "$*" >>"$FAKE_GH_LOG"
if [ -n "${FAKE_GH_BODY:-}" ]; then
    prev=""
    for a in "$@"; do [ "$prev" = --body-file ] && cp "$a" "$FAKE_GH_BODY"; prev="$a"; done
fi
case "$1 $2" in
    "pr view")
        case "$*" in
            *url,state*) echo "${FAKE_GH_URL:-}" ;;
            *"--json labels"*) [ -z "${FAKE_GH_FAIL:-}" ] || exit 1; echo "${FAKE_GH_LABELS:-}" ;;
            *autoMergeRequest*) echo "${FAKE_GH_AUTO:-false}" ;;
            *) echo "${FAKE_GH_STATE:-}" ;;
        esac
        ;;
    "pr create") echo "https://github.com/o/r/pull/9" ;;
    "pr diff")
        [ -z "${FAKE_GH_FAIL:-}" ] || exit 1
        shift 2
        pr="" repo="" names=""
        while [ $# -gt 0 ]; do
            case "$1" in
                -R | --repo) repo="$2"; shift ;;
                --color) shift ;;
                --name-only) names=1 ;;
                -*) ;;
                *) pr="$1" ;;
            esac
            shift
        done
        [ "${repo:+$repo#}${pr:-HEAD}" = "${FAKE_GH_PR:-}" ] || { echo "no pull requests found" >&2; exit 1; }
        for f in ${FAKE_GH_FILES:-}; do
            old="${f%%=>*}" new="${f#*=>}"
            if [ -n "$names" ]; then echo "$new"; continue; fi
            echo "diff --git a/$old b/$new"
            if [ "$old" != "$new" ]; then
                printf 'similarity index 100%%\nrename from %s\nrename to %s\n' "$old" "$new"
            else
                printf -- '--- a/%s\n+++ b/%s\n@@ -1 +1 @@\n-x\n+%s\n' "$old" "$new" "${FAKE_GH_LINE:-y}"
            fi
        done
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
stamp() { loop_stamp_write "$1" "$(loop_tree_hash)" "$(loop_bead_from_branch || echo -)"; }
expect() { if [ "$2" = "$3" ]; then passed=$((passed + 1)); else failed=$((failed + 1)); echo "FAIL $1: expected $2, got $3"; fi; }
reset_state() { rm -rf "$(loop_dir)"; git checkout -q -- . 2>/dev/null; git clean -qfd crates docs; }

# --- dispatch ----------------------------------------------------------------------------------
# Claude Code starts the hooks in the main checkout; dispatch.sh runs the copy of the tree named by
# the hook input's cwd, so a worktree session runs its own branch's hooks.
wt="$work/wt"
git worktree add -q -b wt-probe "$wt" main
printf 'source "$(dirname "${BASH_SOURCE[0]}")/common.sh"\nhook_deny "the worktree copy ran"\n' \
    >"$wt/scripts/loop/hooks/guard-bash.sh"
dispatch() { # dispatch <name> <cwd>: the dispatcher of the main checkout, as settings.json runs it
    jq -nc --arg cwd "$2" '{cwd: $cwd, tool_name: "Bash", tool_input: {command: "ls"}}' |
        bash scripts/loop/hooks/dispatch.sh "$1"
}
check "dispatch from a worktree runs the worktree's hook" deny "$(dispatch guard-bash "$wt")"
check "dispatch from a worktree subdirectory runs the worktree's hook" deny "$(dispatch guard-bash "$wt/docs")"
check "dispatch from the main checkout runs its own hook" allow "$(dispatch guard-bash "$repo")"
check "dispatch from outside a repository runs the dispatcher's copy" allow "$(dispatch guard-bash "$work")"
rm "$wt/scripts/loop/hooks/guard-bash.sh"
check "dispatch from a tree without the hook runs the dispatcher's copy" allow "$(dispatch guard-bash "$wt")"
git worktree remove --force "$wt" && git branch -q -D wt-probe
# Every loop hook in the real settings.json goes through the dispatcher to a hook that exists.
settings="$src/../../.claude/settings.json"
if [ -f "$settings" ]; then
    wiring="$(jq -r '.hooks[][].hooks[].command | select(test("scripts/loop"))' "$settings")"
    expect "settings.json wires loop hooks" true "$([ -n "$wiring" ] && echo true || echo false)"
    while read -r line; do
        name="$(sed -nE 's|.*exec bash "\$d/scripts/loop/hooks/dispatch\.sh" ([a-z-]+); exit 0$|\1|p' <<<"$line")"
        expect "settings.json runs a dispatched hook: $line" true \
            "$([ -n "$name" ] && [ -f "$src/hooks/$name.sh" ] && echo true || echo false)"
    done <<<"$wiring"
fi

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
check "merge with auto" allow "$(FAKE_GH_PR=12 hook guard-bash "$(bash_input 'gh pr merge 12 --auto --squash')")"
# Auto-merge stays off for a PR that changes the CI workflows or the hook wiring (LOOP_USER_REVIEW_PATHS).
# merge_with <pr> <files> <command>: the guard's answer when gh pr diff knows only PR <pr> (see the stub).
merge_with() { FAKE_GH_PR="$1" FAKE_GH_FILES="$2" hook guard-bash "$(bash_input "$3")"; }
ci='docs/a.md .github/workflows/ci.yml'
check "auto-merge for a PR that changes a CI workflow" deny "$(merge_with 12 "$ci" 'gh pr merge 12 --auto --squash')"
check "auto-merge for a PR that changes the hook wiring" deny \
    "$(merge_with https://github.com/o/r/pull/12 .claude/settings.json 'gh pr merge "https://github.com/o/r/pull/12" --squash --auto')"
check "auto-merge for the branch's PR that changes the hook wiring" deny \
    "$(merge_with HEAD .claude/settings.json 'git push && gh pr merge --auto --squash')"
check "auto-merge for a PR that changes other loop files" allow \
    "$(merge_with 12 'scripts/loop/land.sh .claude/settings.local.json .github/CODEOWNERS' 'gh pr merge 12 --auto --squash')"
check "auto-merge off for a PR that changes a CI workflow" allow \
    "$(merge_with 12 .github/workflows/ci.yml 'gh pr merge 12 --disable-auto')"
check "auto-merge for a PR named by a variable" deny "$(hook guard-bash "$(bash_input 'gh pr merge "$url" --auto --squash')")"
check "auto-merge for a PR whose files cannot be listed" deny \
    "$(FAKE_GH_PR=12 FAKE_GH_FAIL=1 hook guard-bash "$(bash_input 'gh pr merge 12 --auto --squash')")"
# The PR is the first word after merge that is not a flag or a flag's value, in the repository -R names.
check "auto-merge for a PR named after the flags" allow "$(merge_with 12 docs/a.md 'gh pr merge --squash --auto 12')"
check "auto-merge for a PR named after the flags that changes a CI workflow" deny \
    "$(merge_with 12 "$ci" 'gh pr merge --squash --auto 12')"
check "auto-merge for a PR named after flags with values" allow \
    "$(merge_with 12 docs/a.md 'gh pr merge -t "Fix 7" --body=8 -b 9 -F body.md -A a@b.c --match-head-commit abc -sd --auto 12')"
check "auto-merge for a PR named after clustered short flags with a value" allow \
    "$(merge_with 12 docs/a.md 'gh pr merge -dt 7 -sb8 --auto 12')"
check "auto-merge for a PR named after --" allow "$(merge_with 12 docs/a.md 'gh pr merge --auto -- 12')"
check "auto-merge for a PR in another repository" allow "$(merge_with 'o/r#12' docs/a.md 'gh pr merge -R o/r 12 --auto')"
check "auto-merge for a PR in another repository that changes a CI workflow" deny \
    "$(merge_with 'o/r#12' "$ci" 'gh pr merge -R o/r 12 --auto')"
check "auto-merge for a PR in a repository named by --repo=" allow "$(merge_with 'o/r#12' docs/a.md 'gh pr merge --repo=o/r --auto 12')"
check "auto-merge for a PR in a repository named in a short cluster" allow "$(merge_with 'o/r#12' docs/a.md 'gh pr merge -sRo/r --auto 12')"
# gh pr diff --name-only lists only the new path of a move; the guard reads both sides of the diff.
check "auto-merge for a PR that moves a CI workflow away" deny \
    "$(merge_with 12 '.github/workflows/ci.yml=>docs/ci.yml' 'gh pr merge 12 --auto --squash')"
check "auto-merge for a PR that renames the hook wiring" deny \
    "$(merge_with 12 '.claude/settings.json=>.claude/settings.old.json' 'gh pr merge 12 --auto --squash')"
check "auto-merge for a PR that moves a file within docs" allow \
    "$(merge_with 12 'docs/a.md=>docs/b.md' 'gh pr merge 12 --auto --squash')"
# A branch's narrower LOOP_USER_REVIEW_PATHS does not let its own workflow change through: the
# guard judges by the base's pattern too, and a PR that changes the pattern waits (archicraft-3c2i).
narrow_review_paths() { sed -i.bak "s|^LOOP_USER_REVIEW_PATHS=.*|LOOP_USER_REVIEW_PATHS='^\\\\.claude/settings\\\\.json\$'|" scripts/loop/lib.sh && rm scripts/loop/lib.sh.bak; }
narrow_review_paths
check "auto-merge for a PR that narrows the review paths and changes a CI workflow" deny \
    "$(merge_with 12 'scripts/loop/lib.sh .github/workflows/ci.yml' 'gh pr merge 12 --auto --squash')"
git checkout -q -- scripts/loop/lib.sh
check "auto-merge for a PR that changes the review paths" deny \
    "$(FAKE_GH_LINE="LOOP_USER_REVIEW_PATHS='^x$'" merge_with 12 scripts/loop/lib.sh 'gh pr merge 12 --auto --squash')"
check "auto-merge for a PR that changes other lines of lib.sh" allow \
    "$(merge_with 12 scripts/loop/lib.sh 'gh pr merge 12 --auto --squash')"
# The push, merge and commit rules also look at the commands a line runs: a bead note, a commit
# message or a grep pattern may name them (hooks/shell-commands.awk).
main_bash() { hook guard-bash "$(bash_input "$1")"; }
check "force push in another directory" deny "$(main_bash 'git -C crates push --force origin bead/x')"
check "force push behind global options" deny "$(main_bash 'git -c push.default=current push -f')"
check "force push through sh -c" deny "$(main_bash "sh -c 'git push -uf origin bead/x'")"
check "force push in a substitution" deny "$(main_bash 'echo "$(git push origin +bead/x)"')"
check "push to main by its full ref" deny "$(main_bash 'git push origin HEAD:refs/heads/main')"
check "push to a quoted main" deny "$(main_bash 'git push origin "HEAD:main"')"
check "merge without auto, with --auto elsewhere on the line" deny "$(main_bash 'gh pr merge 12 --squash; echo --auto')"
check "merge without auto through sh -c" deny "$(main_bash "bash -c 'gh pr merge 12 --squash'")"
check "bd note that quotes a force push" allow "$(main_bash "bd update x --append-notes 'denied: git push --force origin main'")"
check "force push after a bd note that quotes git -C" deny \
    "$(main_bash "bd update x --append-notes 'ran git -C /nowhere status' && git push -f origin bead/x")"
check "bd note that quotes a merge without auto" allow \
    "$(main_bash 'bd update x --append-notes "gh pr merge 12 --squash --admin was refused"')"
check "bd note that quotes a merge with auto" allow \
    "$(main_bash 'bd update x --append-notes "ran gh pr merge https://github.com/o/r/pull/12 --auto --squash"')"
check "commit message that quotes a force push and a merge" allow \
    "$(main_bash 'git commit -qm "Refuse git push --force and gh pr merge 12 --squash"')"
check "commit message in a heredoc that quotes a force push" allow \
    "$(main_bash $'git commit -F - <<\'EOF\'\ngit push -f origin main\ngh pr merge 12 --admin\nEOF')"
check "grep for a force push and a merge" allow "$(main_bash "grep -rnE 'git push (-f|--force)|gh pr merge [0-9]+ --squash' docs")"
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
check "code commit in another directory without stamps" deny "$(main_bash 'git -C crates/demo commit -am x')"
check "code commit through sh -c without stamps" deny "$(main_bash "sh -c 'git commit -am x'")"
check "grep for git commit with ungated code" allow "$(main_bash 'grep -rn "git commit" scripts/loop | head')"
check "bd note that quotes a commit with ungated code" allow \
    "$(main_bash "bd update x --append-notes 'git commit -am x was refused: no gate'")"
git init -q "$work/plain"
check "commit in a repository without the loop" allow "$(main_bash "git -C '$work/plain' commit -am x")"
stamp gate.ok
check "code commit with the gate only" deny "$(hook guard-bash "$(bash_input 'git commit -am x')")"
stamp review.ok
check "code commit with gate and review" allow "$(hook guard-bash "$(bash_input 'git commit -am x')")"
echo 'minor fix' >>docs/a.md
check "code commit after a doc fix, before a new gate" deny "$(hook guard-bash "$(bash_input 'git commit -am x')")"
stamp gate.ok
check "code commit after a doc fix and a new gate" allow "$(hook guard-bash "$(bash_input 'git commit -am x')")"
loop_review_valid && passed=$((passed + 1)) || { failed=$((failed + 1)); echo "FAIL review not carried over a doc fix"; }
expect "status.sh shows a review carried over a doc fix" "  review: passed before changes to prose only: docs/a.md" \
    "$(bash scripts/loop/status.sh | grep '^  review:')"
loop_stamp_write review.ok "$(loop_stamp_tree review.ok)" archicraft-zz99
check "code commit after a doc fix with another bead's review" deny "$(hook guard-bash "$(bash_input 'git commit -am x')")"
git checkout -q -- docs/a.md
stamp gate.ok
loop_stamp_write review.ok "$(loop_tree_hash)" archicraft-zz99
check "code commit with another bead's review of this tree" deny "$(hook guard-bash "$(bash_input 'git commit -am x')")"
expect "status.sh does not count another bead's review" "  review: not passed on this tree" \
    "$(bash scripts/loop/status.sh | grep '^  review:')"
stamp review.ok
expect "status.sh shows a review of this tree" "  review: passed on this tree" "$(bash scripts/loop/status.sh | grep '^  review:')"
for f in scripts/package-macos.sh .codex/hooks.json .cursor/hooks.json; do
    mkdir -p "$(dirname "$f")" && echo '# minor fix' >"$f"
    stamp gate.ok
    check "code commit after a fix to $f and a new gate" deny "$(hook guard-bash "$(bash_input 'git commit -am x')")"
    rm -f "$f"
done
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
# In CI (PR_NUMBER set) the labels come live from gh, not from the event payload: land.sh's
# gh pr create attaches them after the opened event (archicraft-z9yp).
ci_live() { # ci_live <live labels> <payload labels> [fail]: ci-check with PR_NUMBER
    FAKE_GH_LABELS="$1" FAKE_GH_FAIL="${3:-}" PR_NUMBER=9 \
        ci bead/archicraft-ab12.1-demo 'archicraft-ab12.1: x' "$2"
}
expect "labels attached after opening, read live" pass "$(ci_live format-change,loop '')"
expect "loop change without the live label loop" fail "$(ci_live format-change format-change,loop)"
expect "golden change without the live label format-change" fail "$(ci_live loop format-change,loop)"
expect "labels that cannot be read live" fail "$(ci_live format-change,loop format-change,loop fail)"
# A pull_request run checks out the PR merged into the base as it is now (refs/pull/N/merge),
# while BASE_SHA stays at the base the PR was opened against.
pr_merge() { # pr_merge <branch>: detach at the merge of <branch> into moved-base
    git switch -q --detach moved-base && git merge -q --no-ff --no-edit "$1"
}
git switch -q -c moved-base "$base"
echo '{"x":2}' >crates/demo/tests/golden/a.ifcx
git commit -qam "base moves: golden"
git switch -q -c bead/archicraft-ab13-docs "$base"
echo '# more' >>docs/a.md
git commit -qam "docs change"
pr_merge bead/archicraft-ab13-docs
expect "PR without golden changes after the base got one" pass "$(ci bead/archicraft-ab13-docs 'archicraft-ab13: Docs')"
git switch -q -c bead/archicraft-ab14-golden "$base"
echo '{"y":1}' >crates/demo/tests/golden/b.ifcx
git add -A && git commit -qm "golden change"
pr_merge bead/archicraft-ab14-golden
expect "PR merge commit with a golden change without label" fail "$(ci bead/archicraft-ab14-golden 'archicraft-ab14: x')"
expect "PR merge commit with a golden change with label" pass \
    "$(ci bead/archicraft-ab14-golden 'archicraft-ab14: x' format-change)"
# A move out of the guarded paths still changes them: git diff's rename detection would print only
# the new path (archicraft-95eq).
git switch -q -c bead/archicraft-ab15-move "$base"
git mv scripts/loop/gate.sh docs/gate.sh
git commit -qm "move a loop file"
expect "loop file moved out in a feature bead" fail "$(ci bead/archicraft-ab15-move 'archicraft-ab15: x')"
pr_merge bead/archicraft-ab15-move
expect "PR merge commit moving a loop file out in a feature bead" fail \
    "$(ci bead/archicraft-ab15-move 'archicraft-ab15: x')"
git switch -q -c bead/archicraft-ab16-move "$base"
git mv crates/demo/tests/golden/a.ifcx docs/a.ifcx
git commit -qm "move a golden file"
expect "golden file moved out without label" fail "$(ci bead/archicraft-ab16-move 'archicraft-ab16: x')"
expect "golden file moved out with label" pass "$(ci bead/archicraft-ab16-move 'archicraft-ab16: x' format-change)"
git switch -q bead/archicraft-ab12.1-demo

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
    'mod tests {' '    #[test]' '    fn old() {' '    }' '    #[test]' '    fn new() {}' '}' \
    'mod after {' '    pub fn h() {}' '}' >crates/demo/src/lib.rs
# A module after the test module, which clippy's items_after_test_module allows, counts (3).
# A new source file (1 line before its one-line test module, 1 after); an out-of-line test module declaration and a
# test-only module not named tests, which clippy does not keep last, count like any other line (4);
# an integration test does not count.
printf 'pub fn extra() {}\n#[cfg(test)] mod tests { fn t() {} }\npub fn after() {}\n' >crates/demo/src/extra.rs
printf '#[cfg(test)]\nmod tests;\n#[cfg(test)]\nmod fixtures {}\n' >crates/demo/src/other.rs
printf '#[test]\nfn it() {}\n' >crates/demo/tests/it.rs
expect "diff budget leaves out inline test modules and tests/, not what follows them" 17 \
    "$(loop_budget_lines "$budget_base")"
reset_state

# --- land.sh helpers ---------------------------------------------------------------------------
srs() { printf '%b' "$1" | loop_srs_ids; }
expect "SRS ids from a notes line" "SEL-003, SEL-004" "$(srs 'SRS: SEL-003 SEL-004')"
expect "SRS ids up to the end of the sentence" "NFR-ACC-003" \
    "$(srs 'SRS: NFR-ACC-003. Reference only: wip/x, see WAL-020.\nPR https://x/pull/5')"
expect "SRS ids in one-paragraph notes" "WAL-020, NFR-PERF-005" \
    "$(srs 'Split from x. SRS: WAL-020 (partial, see 4.2), NFR-PERF-005. Depends on GEO-004.')"
expect "SRS ids with digits in the prefix" "NFR-L10N-003, NFR-A11Y-001" "$(srs 'SRS: NFR-L10N-003, NFR-A11Y-001\nFollow-up')"
expect "scenario and decision ids with a one-letter prefix or two digits" "AS-03, D-11, O-07, WAL-020" \
    "$(srs 'SRS: AS-03, D-11 (decision), O-07/WAL-020.')"
expect "SRS ids only as whole words" "D-11" "$(srs 'SRS: D-11 with ISO-8601 dates, WAL-0200, UTF-8, xWAL-020.')"
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
expect "land.sh names the CI workflow a move takes away" 1 \
    "$(grep -c "waits for the user's review.*\`\.github/workflows/ci\.yml\`" <<<"$out")"
# A branch that narrows LOOP_USER_REVIEW_PATHS runs land.sh with its own lib.sh: the base's pattern
# still makes its workflow change wait, and changing the pattern alone waits too (archicraft-3c2i).
git switch -q -c bead/archicraft-lp05-narrow main
narrow_review_paths && echo '# lp05' >>.github/workflows/ci.yml
git add -A && git commit -qm "archicraft-lp05: narrow"
out="$(land loop)"
expect "land.sh leaves auto-merge off for a branch that narrows the review paths and changes a CI workflow" "" \
    "$(merge_calls)"
expect "land.sh names the CI workflow and the narrowed review paths" 1 \
    "$(grep -c "waits for the user's review.*\`\.github/workflows/ci\.yml\` \`scripts/loop/lib\.sh\`" <<<"$out")"
git switch -q -c bead/archicraft-lp06-paths main
narrow_review_paths && git commit -qam "archicraft-lp06: narrow"
out="$(land loop)"
expect "land.sh leaves auto-merge off for a branch that only changes the review paths" "" "$(merge_calls)"
commit_on bead/archicraft-lp07-lib scripts/loop/lib.sh
out="$(land loop)"
expect "land.sh turns on auto-merge for a branch that changes lib.sh but not the review paths" \
    "pr merge $pr --auto --squash" "$(merge_calls)"
commit_on bead/archicraft-fe02-docfix crates/demo/src/lib.rs
stamp review.ok
echo 'typo fix' >>docs/a.md && git commit -qam "archicraft-fe02: typo"
stamp gate.ok
: >"$FAKE_GH_LOG"
FAKE_GH_BODY="$work/pr-body.md" bash scripts/loop/land.sh >/dev/null 2>&1
expect "land.sh names the doc fixes a carried-over review did not see" 1 \
    "$(grep -c "the review passed before these changes to prose only: \`docs/a\.md\`\.$" "$work/pr-body.md")"
expect "land.sh turns on auto-merge after a carried-over review" "pr merge $pr --auto --squash" "$(merge_calls)"
unset FAKE_GH_LOG

# --- start-bead.sh runs as it is on origin/main ------------------------------------------------
git switch -q main
# origin/main's start-bead.sh says where it ran from; the worktree's copy, one commit older, does not.
{ sed -n 1p scripts/loop/start-bead.sh; echo 'echo "start-bead ran from $(dirname "${BASH_SOURCE[0]}")"'
    sed 1d scripts/loop/start-bead.sh; } >"$work/start-bead.sh"
cat "$work/start-bead.sh" >scripts/loop/start-bead.sh
git commit -qam "start-bead marker" && git push -q origin main
git switch -q -c stale HEAD~1
mkdir -p "$work/tmp"
start_bead() { FAKE_BD_STATUS=in_progress TMPDIR="$work/tmp" bash scripts/loop/start-bead.sh "$1" 2>&1; }
out="$(start_bead archicraft-sb01)"
expect "start-bead.sh in a worktree cut from an older main runs origin/main's copy, once" 1 \
    "$(grep -c "^start-bead ran from $work/tmp/loop-start-bead\.[^/]*/scripts/loop$" <<<"$out")"
expect "start-bead.sh cuts the bead branch from origin/main" "$(git rev-parse origin/main)" "$(git rev-parse HEAD)"
expect "start-bead.sh removes origin/main's copy when done" "" "$(ls -A "$work/tmp")"
mkdir -p "$work/keep"
out="$(LOOP_START_BASE_COPY="$work/keep" start_bead archicraft-sb01)"
expect "start-bead.sh runs its own copy when it matches origin/main" 1 "$(grep -c '^start-bead ran from scripts/loop$' <<<"$out")"
expect "start-bead.sh keeps a directory that a leaked LOOP_START_BASE_COPY names" yes \
    "$([ -d "$work/keep" ] && echo yes || echo no)"

# --- sweep fast-forwards the main checkout ------------------------------------------------------
# A clone of origin is the main checkout; the sweep runs in one of its worktrees, after the fetch.
main_co="$work/main-checkout" sweep_wt="$work/sweep-wt"
git clone -q -b main "$work/origin.git" "$main_co" # the bare origin's HEAD follows init.defaultBranch
main_co="$(cd "$main_co" && pwd -P)" # as git worktree list prints it
printf '#!/bin/sh\ntouch "%s"\n' "$work/hook-ran" >"$main_co/.git/hooks/post-merge"
chmod +x "$main_co/.git/hooks/post-merge"
git -C "$main_co" worktree add -q -b sweep-wt "$sweep_wt" main
upstream() { # upstream <content of docs/a.md>: a new commit on origin/main, fetched
    echo "$1" >"$sweep_wt/docs/a.md"
    git -C "$sweep_wt" -c user.email=s@example.invalid -c user.name=s commit -qam "$1"
    git -C "$sweep_wt" push -q origin HEAD:main && git -C "$sweep_wt" fetch -q origin main
}
sweep_main() { # the sweep's lines about the main checkout; a crash or stderr output counts as a failure
    local out code
    out="$(cd "$sweep_wt" && bash "$src/sweep.sh" 2>"$work/sweep.err")"
    code=$?
    if [ "$code" -ne 0 ] || [ -s "$work/sweep.err" ]; then
        echo sweep >>"$work/crashes" # runs in a subshell; counted at the end
        printf 'FAIL sweep exited %s:\n%s\n' "$code" "$(cat "$work/sweep.err")" >&2
    fi
    grep 'main checkout' <<<"$out"
}
upstream "# doc 2"
echo 'pub fn local() {}' >"$main_co/crates/demo/src/lib.rs"
out="$(sweep_main)"
expect "sweep fast-forwards the main checkout on main" "$(git -C "$sweep_wt" rev-parse origin/main)" "$(git -C "$main_co" rev-parse HEAD)"
expect "sweep reports the fast-forward" 1 "$(grep -c "^updated  main checkout $main_co: [0-9a-f]*\.\.[0-9a-f]*$" <<<"$out")"
expect "sweep keeps unrelated local changes of the main checkout" 'pub fn local() {}' "$(cat "$main_co/crates/demo/src/lib.rs")"
expect "sweep fast-forwards without git hooks" no "$([ -e "$work/hook-ran" ] && echo yes || echo no)"
expect "sweep is silent about an up-to-date main checkout" "" "$(sweep_main)"
before="$(git -C "$main_co" rev-parse HEAD)"
upstream "# doc 3"
echo '# local edit' >"$main_co/docs/a.md"
out="$(sweep_main)"
expect "sweep leaves a main checkout whose local changes the merge would overwrite" "$before" "$(git -C "$main_co" rev-parse HEAD)"
expect "sweep reports why it left the main checkout" 1 \
    "$(grep -c "^stale    main checkout $main_co stays at [0-9a-f]*: error: Your local changes" <<<"$out")"
expect "sweep keeps the local change the merge would overwrite" '# local edit' "$(cat "$main_co/docs/a.md")"
git -C "$main_co" checkout -q -- docs/a.md
git -C "$main_co" switch -q -c elsewhere
out="$(sweep_main)"
expect "sweep leaves a main checkout on another branch" "$before" "$(git -C "$main_co" rev-parse HEAD)"
expect "sweep reports a main checkout on another branch" \
    "stale    main checkout $main_co is on elsewhere, not main; not updated" "$out"
git -C "$main_co" switch -q main
upstream "# doc 4"
git -C "$main_co" merge -q --ff-only origin/main
expect "a plain fast-forward runs the planted hook (control for the test without hooks)" yes \
    "$([ -e "$work/hook-ran" ] && echo yes || echo no)"
git -C "$main_co" -c user.email=s@example.invalid -c user.name=s commit -qam "local commit on main"
before="$(git -C "$main_co" rev-parse HEAD)"
upstream "# doc 5"
out="$(sweep_main)"
expect "sweep leaves a main checkout with commits of its own" "$before" "$(git -C "$main_co" rev-parse HEAD)"
expect "sweep reports a main checkout it cannot fast-forward" 1 \
    "$(grep -c "^stale    main checkout $main_co stays at [0-9a-f]*: fatal: " <<<"$out")"

[ -f "$work/crashes" ] && failed=$((failed + $(wc -l <"$work/crashes")))
echo "loop selftest: $passed passed, $failed failed"
[ "$failed" -eq 0 ]
