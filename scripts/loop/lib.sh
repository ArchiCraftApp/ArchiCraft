# Shared helpers for the bead loop (docs/loop.md). Source it; do not run it.
# Requires bash, git and jq; bd for the bead lookups.

# Paths that count as code: changing them needs a bead branch, the gate and a review.
LOOP_CODE_PATHS='^(crates/|conformance/|Cargo\.toml$|Cargo\.lock$|rust-toolchain\.toml$|rustfmt\.toml$|clippy\.toml$|deny\.toml$)'
# Golden files change only through ARCHICRAFT_BLESS on a bead labelled format-change.
LOOP_GOLDEN_PATHS='^crates/[^/]+/tests/golden/'
# The loop's own guardrails: a feature bead never changes them, only a bead labelled loop.
LOOP_SELF_PATHS='^(scripts/loop/|\.claude/settings\.json$|\.claude/skills/bead|\.claude/agents/bead-reviewer\.md$|\.github/workflows/)'
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
# any edit to code or the loop's files after the review invalidates the review (loop_review_valid).
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

# Files changed on this branch since it left the base, committed or not, including untracked.
loop_changed_files() {
    local base
    base="$(git merge-base "$(loop_base)" HEAD 2>/dev/null || git rev-parse HEAD)"
    { git diff --name-only "$base"; git ls-files --others --exclude-standard; } | sort -u
}

loop_code_changed() { loop_changed_files | grep -Eq "$LOOP_CODE_PATHS"; }

# A Rust source on stdin without its inline test module: everything from the first top-level
# `#[cfg(test)]` (further attributes allowed) followed by `mod tests {`. Clippy's
# items_after_test_module, denied by the gate, keeps that module last in the file unless other
# modules follow it. Nested test modules, out-of-line `mod tests;`, test-only modules with other
# names and other #[cfg(test)] items such as helpers still count.
loop_strip_test_module() {
    awk '
        cut { next }
        held != "" {
            if ($0 ~ /^#\[/) { held = held $0 "\n"; next }
            if ($0 ~ /^(pub(\([^)]*\))?[[:space:]]+)?mod[[:space:]]+tests[[:space:]]*[{]/) { cut = 1; next }
            printf "%s", held
            held = ""
        }
        /^#\[cfg\(test\)\][[:space:]]*(pub(\([^)]*\))?[[:space:]]+)?mod[[:space:]]+tests[[:space:]]*[{]/ { cut = 1; next }
        /^#\[cfg\(test\)\][[:space:]]*$/ { held = $0 "\n"; next }
        { print }
        END { if (!cut) printf "%s", held }
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
# sentence, comma-separated. Prints nothing when there are none.
loop_srs_ids() {
    grep -Eo 'SRS:.*' | head -n 1 | sed -E 's/\.([[:space:]].*)?$//' |
        grep -Eo '[A-Z][A-Z0-9]{1,4}(-[A-Z][A-Z0-9]{1,4})?-[0-9]{3}' |
        awk 'NR > 1 { printf ", " } { printf "%s", $0 } END { if (NR) print "" }'
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

# Stamp files hold "<tree> <bead> <utc time>"; a stamp is valid while its tree is the current one.
loop_stamp_write() { printf '%s %s %s\n' "$2" "${3:--}" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >"$(loop_dir)/$1"; }
loop_stamp_tree() { [ -f "$(loop_dir)/$1" ] && cut -d' ' -f1 "$(loop_dir)/$1"; }
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

# The review covers [tree] (default: the current one) when it passed on exactly that tree, or on
# one that differs from it only outside code and the loop's own files: a minor doc fix after a
# PASS needs a new gate run, not a new review (/bead step 6).
loop_review_valid() {
    local tree="${1:-$(loop_tree_hash)}" changed
    loop_stamp_valid review.ok "$tree" && return 0
    changed="$(loop_changed_since_review "$tree")" || return 1
    ! grep -Eq "$LOOP_CODE_PATHS|$LOOP_SELF_PATHS" <<<"$changed"
}

# One bead as a JSON object (bd show prints an array).
loop_bead_json() { bd show "$1" --json 2>/dev/null | jq -c 'if type == "array" then .[0] else . end'; }
loop_bead_has_label() { loop_bead_json "$1" | jq -e --arg l "$2" '(.labels // []) | index($l) != null' >/dev/null; }
