---
name: bead
description: Implement one ArchiCraft bead end to end through the enforced loop — sweep, claim and branch, slice, tests first, implement, gate, fresh-context review, PR with auto-merge, close after merge. Use for ANY implementation work in this repository (a bead id like archicraft-3rbv, "implement", "next task", "work on <feature>", a bug fix under crates/) and when resuming a bead/* branch.
argument-hint: "[bead-id]"
---

# The bead loop

One bead per session, from claim to a PR that merges itself when CI is green. The full
design is in `docs/loop.md`. Hooks enforce the hard parts, so work with them, not around
them:

- Editing `crates/`, `conformance/` or the workspace manifests is refused unless this
  worktree is on `bead/<id>-<slug>` and that bead is in progress.
- Editing golden files under `crates/*/tests/golden/` is refused (they change only through
  `ARCHICRAFT_BLESS=1` on a bead labelled `format-change`).
- `git commit` of code is refused unless the gate passed on the current tree and a passing
  review covers it. Any edit invalidates the gate stamp; the review stamp survives only edits
  to `docs/` and top-level Markdown files (step 6).
- Pushing to `main`, force pushes and merging a PR without `--auto` are refused.
- Stopping with uncommitted code runs the gate and blocks while it fails (`pause.sh` below
  is the explicit way out).

**Git authority.** On `bead/*` branches the user has authorised (2026-10-19) committing,
pushing the branch, opening the PR and enabling auto-merge on green CI as part of this loop,
for beads labelled `loop` too (user decision of 2026-10-19). The exception is a PR that changes
`.github/workflows/` or `.claude/settings.json`: it waits for the user's review, and nobody
turns its auto-merge on. Everywhere else the conservative profile in CLAUDE.md applies.

## 0. Sweep and pick

```bash
git fetch --quiet origin main
main_loop="$(mktemp -d)"
git archive origin/main scripts/loop | tar -x -C "$main_loop"
bash "$main_loop/scripts/loop/sweep.sh"; rm -rf "$main_loop"
```

This runs the sweep as it is on `origin/main`: a new worktree can be cut from an older `main`,
and its own copy of the scripts may be out of date until step 1 moves it to `origin/main`.
The sweep closes beads whose PR merged and reports PRs still waiting, beads flagged `human`
and epics whose tasks are all closed. A close that bd refuses (a blocked bead) shows as
`refused`; mention it in your report, it does not stop the sweep. Then choose the bead:

- the id in `$ARGUMENTS`, if given;
- otherwise, if this worktree is already on a `bead/*` branch, resume that bead;
- otherwise the first entry of
  `bd ready --json --limit 0 --exclude-type epic --exclude-label human` (sorted by priority).

## 1. Claim and branch

```bash
scripts/loop/start-bead.sh <id>
```

It refuses epics and blocked beads, cuts `bead/<id>-<slug>` from the latest `origin/main`
(or resumes it), claims the bead and clones the warm build cache. Like the sweep it runs as it
is on `origin/main`: when this worktree's `scripts/loop` differs, it re-executes that copy.
`scripts/loop/status.sh` shows where a resumed bead stands. The claim's lease
(`expires in 4 mins`) needs no renewal: beads run one at a time, and the lease only matters
when two sessions might claim the same bead. Then read, in full:

- `bd show <id>`: description (why and what), acceptance criteria, design, notes. The notes
  line `SRS: …` lists the requirement IDs.
- The text of every cited requirement in `docs/SRS.md` (`grep -n '<ID>' docs/SRS.md`) and its
  row in `docs/implementation-status.md`.
- Every file and line the description names. If the notes name a `wip/…` branch from the
  stopped first run, read its diff as a reference only: it was never built or reviewed, so
  nothing from it lands without passing this loop.

## 2. Slice before coding

Split the bead first when any of these holds:

- its estimate is above 480 minutes;
- you expect more than 800 changed non-test source lines (the gate's diff budget: Rust
  sources outside `tests/`, `benches/` and `examples/`, without a top-level
  `#[cfg(test)] mod tests` at the end of a file);
- the acceptance criteria cover separable concerns (a new type plus several consumers, a
  refactor plus a feature).

To split: create child beads with
`bd create --parent <id> --type task --no-inherit-labels --labels <labels>`, each with its own
`--description`, `--acceptance` (a subset of the parent's criteria, together covering all of
them), `--estimate` and a notes line `SRS: …` for the IDs it serves. A child takes only the
crate labels (`archicraft-model`, `archicraft-geometry`, `archicraft-ifcx`, `archicraft-app`)
of the crates it touches, because `/bead-loop` runs two beads at once only when they share no
crate label; it keeps the parent's other labels where they apply (`format-change` only on the
child that changes the format). Each child must leave `main` green on its own; order them
with `bd dep add <later> <earlier>`. A cross-cutting test (a scan or grep over all crates)
goes into the first child scoped to the crate that child changes, and each later child widens
the scope to its own crates, so `main` stays green after every child. Then mark the parent as
a container and move to the first child:

```bash
bd update <id> --add-label loop-split --append-notes "Split into <child ids>"
scripts/loop/start-bead.sh <first-child>
git branch -D bead/<id>-<slug>   # the parent's branch, still without commits
```

The parent stays in progress and `sweep.sh` closes it once all children are closed. Use
exactly the label `loop-split`: plain `split` is the area label of the split tool's beads.

## 3. Tests first

Turn every acceptance criterion into a test before the implementation: name it after the
behaviour, cite the SRS ID in its doc comment, run it and watch it fail for the right
reason. Extend existing suites where they fit (`crates/*/tests/`, unit tests next to the
code). UI behaviour gets an `egui_kittest` test in `crates/archicraft-app/tests/`; when
it renders snapshots, they are part of the evidence the reviewer looks at.

## 4. Implement

- Follow CLAUDE.md: SRS IDs in doc comments where code implements them; the model, geometry
  and IFCX crates stay headless; lengths in metres and angles in radians as `f64`; every
  model change is a `Command` executed by `History` and is undoable; match the surrounding
  code.
- Compile and test in small steps: `cargo check -p <crate>`, `cargo test -p <crate> <filter>`.
  rustfmt runs on every edited `.rs` file automatically.
- Stay in scope. Anything else you notice becomes its own bead:
  `bd create … --deps discovered-from:<id>`.
- When the change moves a requirement's status, update its row in
  `docs/implementation-status.md` (status and gap text).

## 5. Gate

```bash
scripts/loop/gate.sh
```

fmt, clippy `-D warnings`, the workspace tests, headless layering, golden files, the diff
budget, a clean `.beads/`, IFCX conformance when IFCX code changed and licences when
manifests changed. Fix each failure at its cause. Never weaken or delete a test, add an
`#[allow]`, bless golden files or edit the loop scripts to get green.

## 6. Review

Launch the `bead-reviewer` agent in the foreground (`run_in_background: false`) with the
prompt `Review bead <id>`, and do not edit anything while it runs. It starts from a fresh
context, checks every acceptance criterion against evidence, the conventions and the scope,
and ends with `VERDICT: PASS` or `VERDICT: FAIL`; a hook stamps the tree on a PASS
(`scripts/loop/status.sh` shows it). On `FAIL`, fix the findings, run the gate again and
ask for a new review.

A `PASS` can come with minor findings. Do not re-review for them:

- Fix a trivial one that touches only `docs/` or a top-level Markdown file (a typo, a status
  row): run the gate again, then commit without a new review. The review stamp still
  covers the tree, and `land.sh` names the changed files in the PR.
- File every other minor finding as a follow-up bead
  (`bd create … --deps discovered-from:<id>`) and land the reviewed tree as it is.

Any other edit between a passing review and the commit needs a new review.

## 7. Land

```bash
git add -A -- . ':!.beads'
git commit -m "<id>: <imperative summary>" -m "<what changed and why, SRS IDs>"
scripts/loop/land.sh
```

`land.sh` pushes the branch, opens the PR (title from the commit subject, body from the
bead, the review and the gate), turns on squash auto-merge and records the PR on the bead.
When the branch changes `.github/workflows/` or `.claude/settings.json`, it leaves auto-merge
off and names those files instead: the PR waits for the user's review, so say that in your
report.
In the Claude desktop app, also bind the PR to this session if `get_status` does not show
it and turn on Auto-fix and archive-on-close with the `ccd_pr` tools, so CI failures wake
this session. Never poll CI yourself.

When CI fails or `main` moved under the PR: fix on the same branch, gate, review, commit,
then `scripts/loop/land.sh` again (after a rebase onto `origin/main`:
`scripts/loop/land.sh --rebased`).

## 8. Close

The bead closes when its PR merges: `sweep.sh` does that at the start of the next
iteration. Before you end the session:

- `bd remember` anything a later bead needs and the code does not say.
- File every follow-up as a bead (`discovered-from:<id>`).
- If the review found something a script could have caught, file a bead with label `loop`
  that adds the check to the gate: the loop gets stricter with every repeated finding.
- Report: bead id, PR link, what changed, which tests prove it, anything skipped.

## Stop rules

- The gate fails three times on the same problem, or the review fails twice: stop. Flag the
  bead and pause:

  ```bash
  bd update <id> --add-label human --append-notes "<what blocks, what you tried>"
  scripts/loop/pause.sh "<one-line reason>"
  ```

- A decision belongs to the user (ambiguous requirement, conflicting criteria): pause the
  same way and ask.
- Do not start a second bead in the same session; the next bead gets a fresh context.
- Changes to the loop itself (scripts, hooks, skills, the reviewer) are their own bead with
  label `loop`, never part of a feature bead.
