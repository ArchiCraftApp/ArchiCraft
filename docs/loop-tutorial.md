# Working with the bead loop

A practical guide for the person who runs ArchiCraft's implementation with Claude Code: how
to start work, what happens meanwhile, and when you are needed. How the loop works inside
(scripts, hooks, CI rules) is described in [loop.md](loop.md).

## In short

The backlog is a set of small tasks, called beads, in `bd`. Claude works them one bead per
session: each in its own branch, tests first, checked by a script, reviewed by a
second Claude that did not write the code, and then a pull request that GitHub merges by
itself once CI is green on Linux (Windows is tested on `main` and nightly, macOS when a
version is released).

You start runs, try the app when a feature area (an epic) is complete, decide when Claude
is stuck, and review the rare pull request that changes CI or the hook wiring. Everything
else runs without you.

## Words you will see

| Word | Meaning |
|---|---|
| bead | One task in the backlog, for example `archicraft-ooz9`. Pieces of a split bead get a suffix: `archicraft-ooz9.1`. |
| epic | A group of beads for one feature area, for example `archicraft-0n41`. The order of the epics is in [roadmap.md](roadmap.md). |
| gate | `scripts/loop/gate.sh`: formatting, lint, all tests and the project rules. |
| review | The `bead-reviewer` agent checks the change against the bead's acceptance criteria. |
| sweep | `scripts/loop/sweep.sh`: closes beads whose pull request merged and reports what waits. |
| label `human` | On a bead: Claude is stuck and needs your decision. |
| label `loop` | On a bead or pull request: it changes the loop's own rules. It merges by itself like any other, unless it changes CI or the hook wiring; then you review it. |
| label `loop-split` | A bead that was split into smaller ones; it closes when they are done. |
| label `tier:<x>` | The model and effort a bead runs at: `medium`, `high`, `xhigh` or `max` on Claude Opus 5.5, or `fable` on Claude Fable 5.1. The coordinator picks it when a bead has none. |
| label `escalated` | A bead whose first run got stuck; it moved one tier up and runs again in the next batch. |

## 1. Start a run

1. In the Claude desktop app, start a new Code session for the ArchiCraft project. The app
   gives it its own worktree.
2. For `/bead-loop`, pick Claude Opus 5.5 and effort `max` in the model menu: the coordinator
   can start bead sessions only at its own effort or below, and each bead runs at the effort of
   its tier (small ones at `medium`, the default at `high`).
3. Type one of these:

| Command | What it does |
|---|---|
| `/bead-loop 5` | Works the next five ready beads, highest priority first, across all epics. |
| `/bead-loop 20 archicraft-0n41` | Works only on that epic, until it is complete or 20 beads are done. |
| `/bead-loop 6 2` | Works six beads, at most two at a time. |
| `/bead-loop 3 loop` | Also picks beads labelled `loop` (changes to the loop itself), which are skipped otherwise. |
| `/bead` | Works the single next ready bead in this session. |
| `/bead archicraft-ooz9.2` | Works exactly that bead in this session. |

With `/bead-loop`, the session you typed it in coordinates. It never writes code itself: it
starts a batch of up to three beads at once, one session each (they appear in the sidebar),
whose crates do not overlap, and checks each result. Run only one coordinator at a time. When
a batch has reported, it lists one line per bead and a `Needs you:` line when something does;
type `continue` to start the next batch.

Start with small numbers (3 to 5). Each bead is a full working session, so you see cost and
quality before you let a run go longer. The coordinator never waits for a merge: a bead
counts once its pull request is open with auto-merge on, and beads that build on it go into a
later batch.

## 2. While it runs

You do not need to watch. If you want to:

- open a bead session in the sidebar and read along;
- look at the pull requests on GitHub (or `gh pr list`); their titles start with the bead id,
  and they merge themselves when CI passes;
- ask the backlog in a terminal:

  ```bash
  bd ready                         # what can start next
  bd list --status in_progress     # being worked on, or waiting for its merge
  bd epic status                   # progress per epic
  ```

**When the coordinator pauses**, it says why:

- *Batch reported*: type `continue` for the next batch, whenever you like.
- *Nothing ready*: every ready bead depends on a pull request that is still in CI. Type
  `continue` later; there is no need to watch for the merge. This happens often when you
  restrict a run to one epic, because the beads of an epic usually build on each other.
- *Count reached*: start the next run when you like.
- One of the cases in the next section.

To stop a run, tell the coordinator "stop after the current bead", or use the stop button.
A bead that was cut off keeps its branch; open its session again and type `/bead` to resume.

## 3. When you are needed

### An epic is complete: the demo

The coordinator builds and starts the app from the latest `main`, lists what the epic
delivered and gives you a short script of things to try.

- Try them.
- Everything works: answer "OK". The epic is closed.
- Something is off: describe each problem in your own words ("the wall preview flickers when
  I snap to a corner"). Claude files each one as a bug bead under the epic; the epic stays
  open and the next run picks them up.

This is your main quality check. Tests prove behaviour; only you can tell whether the CAD
tool feels right.

### A bead waits for Claude Fable 5.1

A bead labelled `tier:fable` (a memory or architecture redesign, or your choice after a bead
got stuck at `max`) runs on Claude Fable 5.1, which costs 2.5 times as much per token as Opus
5.5. The coordinator never starts it: it names it in its `Needs you:` line. To run it, start a
new Code session, pick Claude Fable 5.1 (effort `high`) in the model menu and type
`/bead <id>`. To run it on Opus instead:
`bd update <id> --remove-label tier:fable --add-label tier:max`.

### A bead is flagged `human`

Claude stopped because it cannot continue on its own: an unclear or contradicting
requirement, or a gate or review that kept failing after the bead moved one tier up (label
`escalated`) or at tier `max`. The coordinator names the bead; the details are in its notes:

```bash
bd show archicraft-xxxx
```

Decide, and answer in the coordinator or in the bead's session, for example "follow the SRS
text, the note is outdated" or "split this into two beads". Then remove the flag and let it
continue:

```bash
bd update archicraft-xxxx --remove-label human
```

To put the bead aside instead: `bd defer archicraft-xxxx`. A bead stuck at tier `max` can
also try Claude Fable 5.1: pick it in that session's model menu and type `continue`.

### A pull request that waits for your review

Pull requests of `loop` beads change the loop's own rules. Since your decision of 2026-10-19
they merge by themselves on green CI like any other, except one that changes the CI workflows
(`.github/workflows/`) or the hook wiring (`.claude/settings.json`): those hold the checks
Claude cannot get around, so that pull request waits for you. Its description names the
files, and the coordinator tells you about it.

1. Read the pull request on GitHub; its description says what changes and why.
2. Merge it, or comment on it.

Claude Code reads the hook wiring, the skills and the reviewer from the main checkout (each
session runs the hook scripts of its own worktree), so it has to follow `main` after a `loop`
pull request merged. You do not need to pull it: the sweep at the start of every bead
fast-forwards it to `origin/main` while it is on `main`, without git hooks. Your own
uncommitted changes there stay as they are. Only when the update would overwrite one of them,
the main checkout is on another branch, or its `main` has commits of its own, does the sweep
leave it alone and print a line starting with `stale`; the coordinator passes it on and
carries on. Then sort out that change, branch or commit and pull by hand:

```bash
git -C ~/Code/06-archicraft pull --ff-only
```

Or tell any Claude session "pull the main checkout".

## 4. Steering

| You want | Do |
|---|---|
| A bead done sooner | `bd update <id> --priority 0` |
| A bead not done now | `bd defer <id>` |
| A new task | Tell Claude "file a bead for …", or use `bd create` |
| Focus on one feature area | `/bead-loop <count> <epic-id>` |
| See the plan | [roadmap.md](roadmap.md), `bd epic status` |
| See progress against the SRS | [implementation-status.md](implementation-status.md) |

## 5. If something looks wrong

| Symptom | Cause and fix |
|---|---|
| Claude says "Code changes need a bead" | A guard: code changes happen only inside a bead. Use `/bead`. |
| A pull request stays open with a red check | The bead's session usually fixes it on its own (Auto-fix). If not, the checks tab on GitHub shows the failure; tell that session, or resume it with `/bead`. |
| The loop behaves like an older version | The main checkout is behind: the sweep could not fast-forward it and reported a `stale` line (section 3). Fix the cause and pull it by hand. |
| A session takes minutes to stop | The stop guard runs the gate on unfinished code; that is intended. To stop anyway, Claude runs `scripts/loop/pause.sh "<reason>"`. |
| `bd` is very slow in a new session | The beads checkout import is on. Turn it off: `bd config set import.auto false`. |

## 6. Setting up another computer

```bash
brew install gh jq && gh auth login
bd config set import.auto false
```

You also need Rust (rustup), Node.js (for the IFCX conformance check) and `bd`. GitHub is
already set up (auto-merge, labels, required checks); `scripts/loop/setup-github.sh` would
redo it. For fast first builds in new worktrees, build once in the main checkout:

```bash
cargo clippy --workspace --all-targets && cargo test --workspace --no-run
```
