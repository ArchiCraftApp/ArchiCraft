# Project Instructions for AI Agents

This file provides instructions and context for AI coding agents working on this project.

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:1105d646 -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/core-concepts/sync-concepts.md for details and anti-patterns.

## Agent Context Profiles

The managed Beads block is task-tracking guidance, not permission to override repository, user, or orchestrator instructions.

- **Conservative (default)**: Use `bd` for task tracking. Do not run git commits, git pushes, or Dolt remote sync unless explicitly asked. At handoff, report changed files, validation, and suggested next commands.
- **Minimal**: Keep tool instruction files as pointers to `bd prime`; use the same conservative git policy unless active instructions say otherwise.
- **Team-maintainer**: Only when the repository explicitly opts in, agents may close beads, run quality gates, commit, and push as part of session close. A current "do not commit" or "do not push" instruction still wins.

## Session Completion

This protocol applies when ending a Beads implementation workflow. It is subordinate to explicit user, repository, and orchestrator instructions.

1. **File issues for remaining work** - Create beads for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **Handle git/sync by active profile**:
   ```bash
   # Conservative/minimal/default: report status and proposed commands; wait for approval.
   git status

   # Team-maintainer opt-in only, unless current instructions forbid it:
   git pull --rebase
   git push
   git status
   ```
5. **Hand off** - Summarize changes, validation, issue status, and any blocked sync/commit/push step

**Critical rules:**
- Explicit user or orchestrator instructions override this Beads block.
- Do not commit or push without clear authority from the active profile or the current user request.
- If a required sync or push is blocked, stop and report the exact command and error.
<!-- END BEADS INTEGRATION -->


## Implementation loop

All implementation work goes through the bead loop (`docs/loop.md`): one bead per session
with `/bead [id]` (`.claude/skills/bead/SKILL.md`), several beads in fresh sessions with
`/bead-loop [count] [epic]`. Hooks in `.claude/settings.json` enforce it: code under `crates/`
is edited only on a `bead/<id>-<slug>` branch and committed only after `scripts/loop/gate.sh`
passed on the exact tree and a passing `bead-reviewer` review covers it.

Git authority on `bead/*` branches: the user authorised (2026-10-19) committing, pushing the
branch, opening the PR and enabling squash auto-merge on green CI, all through
`scripts/loop/land.sh`. Nothing is pushed to `main` directly, and PRs of beads labelled `loop`
wait for the user's review. Everywhere else the conservative profile applies.

## Build & Test

```bash
cargo build --workspace
cargo test --workspace                                   # unit, golden-file and UI tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
conformance/run.sh crates/archicraft-ifcx/tests/golden/*.ifcx   # IFX-006, needs Node.js
ARCHICRAFT_BLESS=1 cargo test -p archicraft-ifcx --test roundtrip  # after an intended format change
scripts/loop/gate.sh                                     # the bead gate: all of the above plus policy checks
scripts/loop/selftest.sh                                 # after changing the loop's hooks or scripts
```

## Architecture Overview

Rust workspace (see `docs/technology-choices.md` and the SRS in `docs/SRS.md`):

- `crates/archicraft-model`: domain model; every change is a `Command` executed by `History` (undo/redo).
- `crates/archicraft-geometry`: meshes from parameters, wall junctions, plan cuts sliced from the 3D bodies.
- `crates/archicraft-ifcx`: IFCX read/write, versioned per IFCX version (`alpha/`), bundled schema files.
- `crates/archicraft-app`: eframe/egui application; wgpu 3D views; persistence (atomic save, autosave, recovery, crash reports).

## Conventions & Patterns

- Cite SRS requirement IDs (for example `WAL-020`) in doc comments where code implements them.
- The model, geometry and IFCX crates stay headless (no UI dependencies).
- Lengths are metres and angles radians in `f64`; units only affect display and input.
