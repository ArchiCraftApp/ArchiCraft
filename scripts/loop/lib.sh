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
# and land.sh compare against it, so any edit after the gate or the review invalidates both.
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

# One bead as a JSON object (bd show prints an array).
loop_bead_json() { bd show "$1" --json 2>/dev/null | jq -c 'if type == "array" then .[0] else . end'; }
loop_bead_has_label() { loop_bead_json "$1" | jq -e --arg l "$2" '(.labels // []) | index($l) != null' >/dev/null; }
