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
  to `docs/` and top-level Markdown files other than `CLAUDE.md`, `AGENTS.md` and
  `docs/SRS.md` (step 6).
- Pushing to `main`, force pushes and merging a PR without `--auto` are refused.
- Stopping with uncommitted code runs the gate and blocks while it fails (`escalate.sh` and
  `pause.sh` in the stop rules below are the explicit ways out).

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
`refused`; mention it in your report, it does not stop the sweep. It also fast-forwards the
main checkout to `origin/main` (`updated`), or says why it could not (`stale`): mention a
`stale` line in your report too. Then choose the bead:

- the id in `$ARGUMENTS`, if given;
- otherwise, if this worktree is already on a `bead/*` branch, resume that bead;
- otherwise the first entry of
  `bd ready --json --limit 0 --exclude-type epic --exclude-label human,loop` (sorted by
  priority). Beads labelled `loop` (changes to the loop itself) are picked only when the user
  names one or asks for loop beads.

## 1. Claim and branch

```bash
scripts/loop/start-bead.sh <id>
```

It refuses epics and blocked beads, cuts `bead/<id>-<slug>` from the latest `origin/main`
(or resumes it), claims the bead and clones the warm build cache. Like the sweep it runs as it
is on `origin/main`: when this worktree's `scripts/loop` differs, it re-executes that copy.
`scripts/loop/status.sh` shows where a resumed bead stands. The claim's lease
(`expires in 4 mins`) needs no renewal: a single orchestrator hands out distinct beads, and the
lease only matters when two sessions might claim the same bead. Then read, in full:

- `bd show <id>`: description (why and what), acceptance criteria, design, notes. The notes
  line `SRS: …` lists the requirement IDs. The label `tier:<x>` is the model and effort
  `/bead-loop` started this session at (Tiers in `.claude/skills/bead-loop/SKILL.md`); the
  report, the close note and an escalation start from it. A bead started by hand may have
  none: then label it with this session's own tier (`mcp__ccd_session_mgmt__get_session` with
  `self`: its effort on Claude Opus 5.5, `fable` on Claude Fable 5.1).
- When `start-bead.sh` names an escalation patch, an earlier run on a lower tier hit the stop
  rule and left its unreviewed work there; the notes say what blocked it. Apply it as the
  starting point (`git apply --index <patch>`) or read it as a reference.
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
them), `--estimate`, a notes line `SRS: …` for the IDs it serves and a `tier:<x>` label from
the rubric in `.claude/skills/bead-loop/SKILL.md` (`medium` up to 30 minutes, `high` by
default, `xhigh` for a geometry algorithm, an IFCX format, reader or writer change or a loop
change). Slicing may also mark a child `max` (an estimate of 360 minutes or more) or `fable` (a
memory or architecture redesign that later beads build on). A child takes only the
crate labels (`archicraft-model`, `archicraft-geometry`, `archicraft-ifcx`, `archicraft-app`)
of the crates it touches, because `/bead-loop` runs beads at once only when they share no
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
When the first child's tier is above this session's (the order is `medium`, `high`, `xhigh`,
`max`, `fable`), do not start it here: end after the split and report it, and `/bead-loop`
starts each child at its tier.

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
- When the change moves a requirement's status, and for every ID on the bead's `SRS:` line that
  has a row (`land.sh` puts those on the PR's), change its row in
  `docs/status/requirements.json` (status, effort, evidence or gap) and run
  `python3 scripts/status.py generate`. Never edit the generated tables of
  `docs/implementation-status.md` by hand (docs/testing.md, "Implementation status").

## 5. Gate

```bash
scripts/loop/gate.sh
```

fmt, clippy `-D warnings`, the workspace tests, headless layering, golden files, the diff
budget, a clean `.beads/`, no change to the loop's guardrails (`LOOP_SELF_PATHS`) without the
label `loop`, the implementation status (`scripts/status.py` check, and merge-check with the
bead's `SRS:` line), IFCX conformance when IFCX code changed and licences when manifests
changed. Fix each failure at its cause. Never weaken or delete a test, add an `#[allow]`, bless golden files
or edit the loop scripts to get green.

## 6. Review

Launch the `bead-reviewer` agent in the foreground (`run_in_background: false`) with the
prompt `Review bead <id>`, and do not edit anything while it runs. It starts from a fresh
context, checks every acceptance criterion against evidence, the conventions and the scope,
and ends with `VERDICT: PASS` or `VERDICT: FAIL`; a hook stamps the tree on a PASS
(`scripts/loop/status.sh` shows it). It runs at effort `xhigh` whatever this session's tier
(`effort` in its frontmatter), so a cheaper implementer still gets a strong review. On
`FAIL`, fix the findings, run the gate again and ask for a new review.

A `PASS` can come with minor findings. Do not re-review for them:

- Fix a trivial one that touches only `docs/` or a top-level Markdown file (a typo, a status
  row; not `CLAUDE.md`, `AGENTS.md` or `docs/SRS.md`): run the gate again, then commit without a new review. The review stamp still
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
Auto-merge puts the PR into the merge queue once its CI passes, and the queue merges it once CI
passes again on `main` with the PRs ahead of it (`docs/loop.md`, "The merge queue").
When the branch changes `.github/workflows/` or `.claude/settings.json`, it leaves auto-merge
off, names those files and writes the user's steps into the PR description instead: the PR
waits for the user's review, so say that in your report.
In the Claude desktop app, also bind the PR to this session if `get_status` does not show
it and turn on Auto-fix and archive-on-close with the `ccd_pr` tools, so CI failures wake
this session. Never poll CI yourself.

When CI fails or `main` moved under the PR: fix on the same branch, gate, review, commit,
then `scripts/loop/land.sh` again (after a rebase onto `origin/main`:
`scripts/loop/land.sh --rebased`).

When the PR left the merge queue (the sweep's `ATTENTION <id>: <url> left the merge queue
(<reason>, <run>)` line, or the orchestrator's message with it): its merge group, `main` with the
PRs ahead of it and this PR, failed (`failed_checks`, `checks_timed_out`) or did not merge
(`merge_conflict`), so nothing merged and auto-merge is off. Read the failed group run
(`gh run view <run> --log-failed`), rebase onto `origin/main`
(`git fetch origin main && git -c core.hooksPath=/dev/null rebase origin/main`), fix what the
combination broke, then gate, review, commit and `scripts/loop/land.sh --rebased`, which turns
auto-merge on again.

## 8. Close

The bead closes when its PR merges: `sweep.sh` does that at the start of the next
iteration. Before you end the session:

- `bd remember` anything a later bead needs and the code does not say.
- File every follow-up as a bead (`discovered-from:<id>`).
- If the review found something a script could have caught, and it caused a real defect
  (a bug, a broken requirement or a convention the code violated, not a style or wording
  point), file a bead with label `loop` at priority 3 that adds the check to the gate. Record
  anything less with `bd remember`. Loop beads cost the user reviews, so they stay rare.
- Write the close note, so the rubric can be calibrated later: the tier, the review rounds
  (bead-reviewer runs in this session) and the gate failures (failed gate runs, the Stop
  hook's included), and how the session ended (`PR #N`, or `stopped` and `paused` from the
  stop rules):

  ```bash
  bd update <id> --append-notes "Close: tier <x>, <n> review rounds, <m> gate failures, <PR #N|stopped|paused>"
  ```

- Report in at most 4 lines, nothing else (details only when the user asks):

  ```
  <id> ✅ PR #<n> (auto-merge, <tier>): <one line: what changed>
  Needs you: -
  ```

  The status is ✅ (PR open or merged), ⏫ (escalated one tier up, see Stop rules),
  ⏸ (paused, label `human`) or ❌ (no PR); the bracket says `auto-merge` (on its way through
  the merge queue), `waits for you` (`land.sh` left auto-merge off) or `blocked`, then the
  tier.
  `Needs you:` names the one thing the user has to do, or `-`. Two more lines at most, only
  for what the user must know: a `refused` or `stale` sweep line, a skipped criterion, a
  follow-up bead.

## Stop rules

- The gate fails three times on the same problem, or the review fails twice: stop. Write the
  close note (step 8, `stopped`), then:

  ```bash
  scripts/loop/escalate.sh "<what blocks, what you tried>"
  ```

  The first time, a bead below tier `max` moves one tier up: `escalate.sh` swaps its `tier:`
  label, adds the label `escalated` and a note, saves this run's uncommitted work as a patch,
  releases the claim and detaches this worktree from the branch. The next `/bead-loop` batch
  resumes the branch in a fresh session on the stronger tier. Report `<id> ⏫ escalated to
  <tier>: <reason>` with `Needs you: -`. A bead at `max` or `fable`, or one escalated before,
  gets the label `human` instead and stays paused here; report it as ⏸. When the user
  switches this session to Claude Fable 5.1 and says continue, relabel the bead
  (`bd update <id> --remove-label human --remove-label tier:<x> --add-label tier:fable`) and
  carry on.
- A decision belongs to the user (ambiguous requirement, conflicting criteria): a stronger
  model does not settle it. Write the close note (`paused`), flag the bead, pause and ask:

  ```bash
  bd update <id> --add-label human --append-notes "<the question, what you tried>"
  scripts/loop/pause.sh "<one-line reason>"
  ```
- Do not start a second bead in the same session; the next bead gets a fresh context.
- Changes to the loop itself (scripts, hooks, skills, the reviewer) are their own bead with
  label `loop`, never part of a feature bead.
