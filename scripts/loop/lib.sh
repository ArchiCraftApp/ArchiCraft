# Shared helpers for the bead loop (docs/loop.md). Source it; do not run it.
# Requires bash, git and jq; bd for the bead lookups.

# Paths that count as code: changing them needs a bead branch, the gate and a review.
LOOP_CODE_PATHS='^(crates/|conformance/|Cargo\.toml$|Cargo\.lock$|rust-toolchain\.toml$|rustfmt\.toml$|clippy\.toml$|deny\.toml$)'
# Golden files change only through ARCHICRAFT_BLESS on a bead labelled format-change.
LOOP_GOLDEN_PATHS='^crates/[^/]+/tests/golden/'
# The loop's own guardrails: a feature bead never changes them, only a bead labelled loop.
LOOP_SELF_PATHS='^(scripts/loop/|\.claude/settings\.json$|\.claude/skills/bead|\.claude/agents/bead-reviewer\.md$|\.github/workflows/)'
# The CI gate and the hook wiring: a PR that changes them never auto-merges, it waits for the
# user's review (user decision of 2026-10-19). Every other bead PR auto-merges on green CI.
LOOP_USER_REVIEW_PATHS='^(\.github/workflows/|\.claude/settings\.json$)'
# Prose a passing review carries over: a change to these files alone after a PASS needs a new
# gate run, not a new review (loop_review_prose_only), and a rebase compares the bead's change
# outside them (loop_change_id). Everything else, the build and packaging scripts and other
# agents' hook files included, needs a new review.
LOOP_REVIEW_CARRY_PATHS='^(docs/|[^/]+\.md$)'
# Markdown that is not just prose and needs a new review after all: the agent instructions and
# the requirements the review checks the change against.
LOOP_REVIEW_NO_CARRY_PATHS='^(CLAUDE\.md|AGENTS\.md|docs/SRS\.md)$'
# Non-test source lines a bead branch may change before it must be split (label large-change lifts it).
LOOP_DIFF_BUDGET="${LOOP_DIFF_BUDGET:-800}"
# The Rust sources the diff budget counts (`#[cfg(test)] mod tests` left out, see loop_budget_lines).
LOOP_BUDGET_PATHS=(':(glob)crates/**/*.rs' ':(exclude,glob)crates/*/tests/**'
    ':(exclude,glob)crates/*/benches/**' ':(exclude,glob)crates/*/examples/**')

loop_root() { git rev-parse --show-toplevel; }

# Per-worktree state directory inside the git dir: stamps, logs, counters. Never committed.
loop_dir() {
    local d
    d="$(git rev-parse --path-format=absolute --git-path loop)"
    mkdir -p "$d"
    printf '%s\n' "$d"
}

loop_branch() { git branch --show-current; }

# Prints the bead id of a bead branch (bead/<id>-<slug>), fails on any other branch.
loop_bead_from_branch() {
    local b="${1:-$(loop_branch)}"
    [[ "$b" =~ ^bead/([a-z0-9]+-[a-z0-9]+(\.[0-9]+)*)(-|$) ]] || return 1
    printf '%s\n' "${BASH_REMATCH[1]}"
}

# The ref bead branches start from and merge into.
loop_base() {
    if git rev-parse -q --verify refs/remotes/origin/main >/dev/null; then
        echo origin/main
    else
        echo main
    fi
}

# Tree id of the working tree as it would be committed (tracked and untracked, ignored files
# left out) with .beads/ held at HEAD. Gate and review stamps record it; the commit guard
# and land.sh compare against it, so any edit after the gate invalidates the gate stamp and
# any edit after the review other than to prose invalidates the review (loop_review_valid).
loop_tree_hash() {
    local idx tree
    idx="$(mktemp "${TMPDIR:-/tmp}/loop-index.XXXXXX")"
    rm -f "$idx"
    GIT_INDEX_FILE="$idx" git read-tree HEAD &&
        GIT_INDEX_FILE="$idx" git add -A -- . ':(exclude).beads' &&
        tree="$(GIT_INDEX_FILE="$idx" git write-tree)"
    rm -f "$idx"
    [ -n "${tree:-}" ] && printf '%s\n' "$tree"
}

# Files changed on this branch since it left the base, committed or not, including untracked;
# both paths of a move count.
loop_changed_files() {
    local base
    base="$(git merge-base "$(loop_base)" HEAD 2>/dev/null || git rev-parse HEAD)"
    { git diff --no-renames --name-only "$base"; git ls-files --others --exclude-standard; } | sort -u
}

loop_code_changed() { loop_changed_files | grep -Eq "$LOOP_CODE_PATHS"; }

# LOOP_USER_REVIEW_PATHS as scripts/loop/lib.sh at <rev> defines it; empty when it has no such line.
loop_user_review_paths_at() {
    git show "$1:scripts/loop/lib.sh" 2>/dev/null |
        sed -n "s/^LOOP_USER_REVIEW_PATHS='\(.*\)'\$/\1/p" | head -n 1
}

# The pattern of the files that make a PR wait for the user: LOOP_USER_REVIEW_PATHS as the base
# defines it or as this tree does. A branch's own copy alone does not decide, so a branch that
# narrows it still waits for every file the base names (archicraft-3c2i).
loop_user_review_pattern() {
    local base
    base="$(loop_user_review_paths_at "$(loop_base)")"
    printf '%s\n' "${base:+$base|}$LOOP_USER_REVIEW_PATHS"
}

# The files of this branch's change that make its PR wait for the user (loop_user_review_pattern),
# one per line, plus scripts/loop/lib.sh when the branch changes LOOP_USER_REVIEW_PATHS: which
# files wait is the user's to review too. Prints nothing when the PR may auto-merge.
loop_user_review_files() {
    local fork
    fork="$(git merge-base "$(loop_base)" HEAD 2>/dev/null || git rev-parse HEAD)"
    {
        loop_changed_files | grep -E "$(loop_user_review_pattern)"
        [ "$(loop_user_review_paths_at "$fork")" = "$LOOP_USER_REVIEW_PATHS" ] || echo scripts/loop/lib.sh
    } | sort -u
}

# A Rust source on stdin without its inline test module: the first top-level `#[cfg(test)]`
# (further attributes allowed) followed by `mod tests {`, up to the module's closing `}` in
# column 0, as rustfmt writes it (or the end of its line when it closes there). Whatever follows
# the module counts: clippy's items_after_test_module, denied by the gate, stays silent when a
# module follows it. Nested test modules, out-of-line `mod tests;`, test-only modules with other
# names and other #[cfg(test)] items such as helpers still count. Strings are not parsed: a
# column-0 `}` inside a multi-line string literal of the test module ends the cut early and the
# rest of the module counts, which errs towards a larger diff (none in crates/ as of 2026-10-20).
loop_strip_test_module() {
    awk '
        function open_module(line, rest) {
            rest = line
            if (gsub(/[{]/, "", rest) != gsub(/[}]/, "", line)) cut = 1
            done = 1
        }
        cut { if ($0 ~ /^[}]/) cut = 0; next }
        held != "" {
            if ($0 ~ /^#\[/) { held = held $0 "\n"; next }
            if (!done && $0 ~ /^(pub(\([^)]*\))?[[:space:]]+)?mod[[:space:]]+tests[[:space:]]*[{]/) {
                held = ""
                open_module($0)
                next
            }
            printf "%s", held
            held = ""
        }
        !done && /^#\[cfg\(test\)\][[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?mod[[:space:]]+tests[[:space:]]*[{]/ { open_module($0); next }
        !done && /^#\[cfg\(test\)\][[:space:]]*$/ { held = $0 "\n"; next }
        { print }
        END { printf "%s", held }
    '
}

# Added plus deleted lines since <base> in LOOP_BUDGET_PATHS, untracked files included, both
# versions of each file cut by loop_strip_test_module. Run it from the top level.
loop_budget_lines() {
    local base="$1" tmp
    tmp="$(mktemp -d "${TMPDIR:-/tmp}/loop-budget.XXXXXX")" || return 1
    {
        git diff --name-status -M "$base" -- "${LOOP_BUDGET_PATHS[@]}"
        git ls-files --others --exclude-standard -- "${LOOP_BUDGET_PATHS[@]}" | awk '{ print "A\t" $0 }'
    } | while IFS=$'\t' read -r change old new; do
        case "$change" in R*) ;; *) new="$old" ;; esac
        git show "$base:$old" 2>/dev/null | loop_strip_test_module >"$tmp/old"
        if [ -f "$new" ]; then loop_strip_test_module <"$new"; fi >"$tmp/new"
        diff "$tmp/old" "$tmp/new" || :
    done | awk '/^[<>]/ { n++ } END { print n + 0 }'
    rm -rf "$tmp"
}

# The requirement IDs that a bead's notes (stdin) cite after "SRS:", up to the end of that
# sentence, comma-separated: requirement IDs such as WAL-020 or NFR-PERF-005, scenario IDs such as
# AS-03 and decision IDs such as D-11. Prints nothing when there are none.
loop_srs_ids() {
    grep -Eo 'SRS:.*' | head -n 1 | sed -E 's/\.([[:space:]].*)?$//' | tr -c 'A-Za-z0-9-' '\n' |
        grep -Ex '[A-Z][A-Z0-9]{0,4}(-[A-Z][A-Z0-9]{0,4})?-[0-9]{2,3}' |
        awk 'NR > 1 { printf ", " } { printf "%s", $0 } END { if (NR) print "" }'
}

# The lines of a bead PR's body for the bead's notes (stdin), as merge-check reads them: `SRS:` with
# the IDs of loop_srs_ids that have a row in docs/status/requirements.json, each of which the PR
# must change, and `Also cites:` with the others, scenario, decision and open-question IDs such as
# AS-03, D-11 or O-12, which have no row and would fail merge-check (archicraft-erpe). Prints
# nothing when the notes cite no ID. Run it from the top level.
loop_srs_lines() {
    local ids
    ids="$(loop_srs_ids)" || return 0
    jq -r '.requirements[]?.id' docs/status/requirements.json 2>/dev/null |
        awk -v ids="$ids" '
            { row[$0] = 1 }
            END {
                n = split(ids, id, /, /)
                for (i = 1; i <= n; i++) {
                    if (id[i] in row) srs = srs (srs == "" ? "" : ", ") id[i]
                    else also = also (also == "" ? "" : ", ") id[i]
                }
                if (srs != "") print "SRS: " srs
                if (also != "") print "Also cites: " also
            }'
}

# The Markdown <file> with its headings shifted down so the top one has <level> (code blocks
# untouched): the review notes nest below the PR body's "## Review".
loop_nest_headings() {
    awk -v want="$1" '
        FNR == 1 { fence = 0 }
        /^[[:space:]]*(```|~~~)/ { fence = !fence; if (NR != FNR) print; next }
        NR == FNR {
            if (!fence && match($0, /^#+[[:space:]]/) && (!top || RLENGTH - 1 < top)) top = RLENGTH - 1
            next
        }
        !fence && top && top < want && /^#+[[:space:]]/ { $0 = substr("######", 1, want - top) $0 }
        { print }
    ' "$2" "$2"
}

# Uncommitted code changes (tracked or untracked) in the working tree.
loop_code_dirty() {
    git status --porcelain --untracked-files=all | cut -c4- | sed 's/.* -> //' | grep -Eq "$LOOP_CODE_PATHS"
}

# Stamp files hold "<tree> <bead> <utc time>", the review stamp also "<fork point>": the commit
# the reviewed change left the base at (loop_review_rebased). A stamp is valid while its tree is
# the current one.
loop_stamp_write() { printf '%s %s %s%s\n' "$2" "${3:--}" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "${4:+ $4}" >"$(loop_dir)/$1"; }
loop_stamp_tree() { [ -f "$(loop_dir)/$1" ] && cut -d' ' -f1 "$(loop_dir)/$1"; }
# Seconds since a stamp was written (its UTC time), negative for one from the future; fails without
# the stamp or a time GNU or BSD date can read.
loop_stamp_age() {
    local at written
    at="$(cut -d' ' -f3 "$(loop_dir)/$1" 2>/dev/null)" && [ -n "$at" ] || return 1
    written="$(date -u -d "$at" +%s 2>/dev/null || date -u -j -f %Y-%m-%dT%H:%M:%SZ "$at" +%s 2>/dev/null)" || return 1
    echo $(($(date -u +%s) - written))
}
loop_stamp_valid() {
    local want="${2:-$(loop_tree_hash)}"
    [ -n "$want" ] && [ "$(loop_stamp_tree "$1" 2>/dev/null)" = "$want" ]
}

# Files changed between the reviewed tree and [tree] (default: the current one).
loop_changed_since_review() {
    local reviewed tree="${1:-$(loop_tree_hash)}"
    reviewed="$(loop_stamp_tree review.ok 2>/dev/null)" && [ -n "$reviewed" ] && [ -n "$tree" ] &&
        git diff --no-renames --name-only "$reviewed" "$tree" 2>/dev/null # both paths of a move
}

# The review stamp belongs to this branch's bead: one that another bead left behind (after a
# manual git switch; start-bead.sh clears the stamps) covers nothing here.
loop_review_ours() {
    local bead
    bead="$(loop_bead_from_branch)" && [ -f "$(loop_dir)/review.ok" ] &&
        [ "$(cut -d' ' -f2 "$(loop_dir)/review.ok")" = "$bead" ]
}

# The review of this branch's bead covers [tree] (default: the current one) when it passed on
# exactly that tree, on one that differs from it only in prose (loop_review_prose_only), or on the
# same change before a rebase (loop_review_rebased): a minor doc fix after a PASS, or a rebase
# whose only conflicts were status rows, needs a new gate run, not a new review (/bead steps 6, 7).
loop_review_valid() {
    local tree="${1:-$(loop_tree_hash)}"
    loop_review_ours || return 1
    loop_stamp_valid review.ok "$tree" || loop_review_prose_only "$tree" || loop_review_rebased "$tree"
}

# The reviewed tree and [tree] differ only in prose (LOOP_REVIEW_CARRY_PATHS but not
# LOOP_REVIEW_NO_CARRY_PATHS).
loop_review_prose_only() {
    local changed
    changed="$(loop_changed_since_review "${1:-$(loop_tree_hash)}")" || return 1
    ! grep -Evq "$LOOP_REVIEW_CARRY_PATHS" <<<"$changed" && ! grep -Eq "$LOOP_REVIEW_NO_CARRY_PATHS" <<<"$changed"
}

# The commit this branch left the base at.
loop_fork_point() { git merge-base "$(loop_base)" HEAD 2>/dev/null; }

# The patch from <from> to <to> for the given paths, from plumbing with fixed options: the user's
# diff configuration (color, an external diff tool or textconv, the context size) changes nothing,
# and paths are literal.
loop_patch() {
    local from="$1" to="$2"
    shift 2
    git --literal-pathspecs diff-tree -p -r -U3 --no-renames --no-color --no-ext-diff --no-textconv \
        --full-index "$from" "$to" -- "$@"
}

# The patch id (git patch-id --verbatim: line numbers ignored, whitespace and context lines
# compared) of the change from <from> to <to> in the files a review does not carry over, the ones
# loop_review_prose_only does not let through; "none" when the change has no such file. A failed
# diff, or a patch without an id, fails: it never counts as "none".
loop_change_id() {
    local names f files=() patch id
    names="$(git -c core.quotePath=false diff-tree -r --no-renames --name-only "$1" "$2")" || return 1
    while IFS= read -r f; do
        [ -n "$f" ] || continue
        if ! grep -Eq "$LOOP_REVIEW_CARRY_PATHS" <<<"$f" || grep -Eq "$LOOP_REVIEW_NO_CARRY_PATHS" <<<"$f"; then
            files+=("$f")
        fi
    done <<<"$names"
    [ "${#files[@]}" -gt 0 ] || { echo none; return 0; }
    patch="$(loop_patch "$1" "$2" "${files[@]}")" && [ -n "$patch" ] || return 1
    id="$(git patch-id --verbatim <<<"$patch" | cut -d' ' -f1)" && [ -n "$id" ] || return 1
    printf '%s\n' "$id"
}

# After a rebase the review covers [tree] when the bead's change from the new fork point to [tree]
# is the reviewed one, from the fork point the review stamp records to the reviewed tree, hunk for
# hunk outside the prose (loop_change_id): the rebase moved the change and did not alter it. The
# rebases of 2026-11 that conflicted only in status rows each paid a new review (archicraft-qpy17).
# Prose may differ, as after a doc fix (loop_rebased_prose_changes names it).
loop_review_rebased() {
    local tree="${1:-$(loop_tree_hash)}" reviewed fork_then fork_now then_id now_id
    reviewed="$(loop_stamp_tree review.ok 2>/dev/null)" && [ -n "$reviewed" ] || return 1
    fork_then="$(cut -d' ' -f4 "$(loop_dir)/review.ok")" && [ -n "$fork_then" ] || return 1
    fork_now="$(loop_fork_point)" && [ -n "$fork_now" ] && [ "$fork_now" != "$fork_then" ] || return 1
    then_id="$(loop_change_id "$fork_then" "$reviewed")" && now_id="$(loop_change_id "$fork_now" "$tree")" &&
        [ "$then_id" = "$now_id" ]
}

# The prose files whose change from the fork point differs from the one the review saw before a
# rebase (loop_review_rebased), one per line: status rows merged with main's, a doc fix.
loop_rebased_prose_changes() {
    local tree="${1:-$(loop_tree_hash)}" reviewed fork_then fork_now names f
    reviewed="$(loop_stamp_tree review.ok 2>/dev/null)" && fork_then="$(cut -d' ' -f4 "$(loop_dir)/review.ok")" &&
        fork_now="$(loop_fork_point)" && [ -n "$reviewed" ] && [ -n "$fork_then" ] && [ -n "$fork_now" ] || return 1
    names="$(git -c core.quotePath=false diff-tree -r --no-renames --name-only "$fork_then" "$reviewed" &&
        git -c core.quotePath=false diff-tree -r --no-renames --name-only "$fork_now" "$tree")" || return 1
    sort -u <<<"$names" | while IFS= read -r f; do
        [ -n "$f" ] && grep -Eq "$LOOP_REVIEW_CARRY_PATHS" <<<"$f" && ! grep -Eq "$LOOP_REVIEW_NO_CARRY_PATHS" <<<"$f" ||
            continue
        [ "$(loop_patch "$fork_then" "$reviewed" "$f" | git patch-id --verbatim)" = \
            "$(loop_patch "$fork_now" "$tree" "$f" | git patch-id --verbatim)" ] || printf '%s\n' "$f"
    done
}

# One bead as a JSON object (bd show prints an array).
loop_bead_json() { bd show "$1" --json 2>/dev/null | jq -c 'if type == "array" then .[0] else . end'; }
loop_bead_has_label() { loop_bead_json "$1" | jq -e --arg l "$2" '(.labels // []) | index($l) != null' >/dev/null; }
# Where escalate.sh keeps the unreviewed work of a bead's run on the lower tier: in the git dir
# every worktree shares, so the session that resumes the bead in another worktree finds it.
loop_escalation_patch() { printf '%s/bead-escalations/%s.diff\n' "$(git rev-parse --path-format=absolute --git-common-dir)" "$1"; }
