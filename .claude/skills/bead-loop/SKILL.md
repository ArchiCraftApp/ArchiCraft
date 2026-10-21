---
name: bead-loop
description: Orchestrate the bead loop over several beads — sweep, start a batch of ready beads with disjoint crates, each in its own fresh session and worktree with /bead at the model and effort of its tier, verify what they landed, never wait on a merge, report blockers in one line, and stop after N beads or at an epic boundary for the user's demo.
argument-hint: "[count] [parallel] [epic-id] [loop]"
disable-model-invocation: true
---

# Orchestrating the bead loop

This session coordinates; it never edits code and never works a bead itself. Every bead
runs in its own fresh session with `/bead <id>` (`.claude/skills/bead/SKILL.md`), so each
starts with a clean context. The design is in `docs/loop.md`.

Arguments (`$ARGUMENTS`): the number of beads to run (default 3), the number of sessions per
batch (default 3), optionally an epic id that restricts the picks to its descendants, and the
word `loop` when the user wants beads labelled `loop` picked too.

**One orchestrator.** Run at most one `/bead-loop` session at a time. Two orchestrators pick
the same beads and the same crates and end up coordinating through the user. If another
orchestrator session is still working (`mcp__ccd_session_mgmt__list_sessions`), say so in one
line and stop.

**Opus 5.5 at effort max.** Run this session on Claude Opus 5.5 at effort `max` (the app's
model menu). `start_session` gives a new session at most the caller's own effort and ignores a
model above the caller's, so only an orchestrator at `max` can start every Opus tier (Tiers
below). `mcp__ccd_session_mgmt__get_session` with `self` shows this session's `model` and
`effort`. A bead whose tier is above this session's effort is not started: it goes into the
`Needs you:` line with "switch the orchestrator to Opus 5.5, effort max".

The user's time is the budget: ask nothing that a rule below answers, keep every report to
one line per bead, and never end a turn by waiting for a merge.

## Tiers

Every bead runs at a tier, named by its label `tier:<x>`:

| Tier | Model | Effort |
|---|---|---|
| `medium` | Claude Opus 5.5 (`claude-opus-5-5`) | `medium` |
| `high` | Claude Opus 5.5 | `high` |
| `xhigh` | Claude Opus 5.5 | `xhigh` |
| `max` | Claude Opus 5.5 | `max` |
| `fable` | Claude Fable 5.1 (`claude-fable-5-1`), 2.5 times Opus 5.5's price per token | `high` |

A `tier:` label on the bead wins: slicing, an escalation or the user set it. A bead without
one gets the first tier of this rubric that fits, and the label before it starts
(`bd update <id> --add-label tier:<x>`):

1. `xhigh`: a bead labelled `red-main` or `loop`; an `archicraft-geometry` algorithm (wall
   junctions, plan cuts, meshing, solid operations); an IFCX format, reader or writer change
   (label `format-change`, or an `archicraft-ifcx` bead that changes what is read or written).
2. `medium`: an estimate of 30 minutes or less (`estimated_minutes` in `bd show <id> --json`):
   tidy-ups, test tightening, docs, chores.
3. `high`: everything else, also a bead without an estimate.

`max` and `fable` never come from the rubric. A bead gets them by escalation (Stop rules in
`/bead`) or when slicing marked it so: `max` for an estimate of 360 minutes or more, `fable`
for a memory or architecture redesign that later beads build on. The reviewer runs at effort
`xhigh` whatever the tier (`.claude/agents/bead-reviewer.md`): the gate and the review are what
make a cheaper implementer safe.

**A `fable` bead is never started from here.** `start_session` cannot raise the model, and
`set_session_model` applies only from a session's next turn, while `/bead` runs in its first.
The bead goes into the `Needs you:` line instead, with the way to start it: "`<id>` (tier
fable): new Code session, pick Claude Fable 5.1 (effort high) in the model menu, send
`/bead <id>`". Repeat it each turn until the bead is in progress; it takes no batch slot.

**Escalation.** A bead session that hits the stop rule (two failed reviews or three gate
failures) runs `scripts/loop/escalate.sh`. The first time, a bead below `max` moves one tier
up (new `tier:` label, label `escalated`, a note) and is ready again; its next session resumes
the bead branch and finds the unreviewed work of the run before as a patch. A second stop, or
one at `max` or `fable`, gives it the label `human` as before.

## A user turn: start a batch

The app starts sessions only in a turn the user triggered (the `/bead-loop` command or the
user's "continue"); in a turn a bead session's report triggered, `start_session` is refused.
So each user turn starts a whole batch at once:

1. **Sweep.** Run the sweep as it is on `origin/main` (this checkout's copy may be older),
   the same way as step 0 of `/bead`:

   ```bash
   git fetch --quiet origin main
   main_loop="$(mktemp -d)"
   git archive origin/main scripts/loop | tar -x -C "$main_loop"
   bash "$main_loop/scripts/loop/sweep.sh"; rm -rf "$main_loop"
   ```

   An epic ready for its demo stops the loop (see Demo checkpoint). Beads under "needs a
   human", `ATTENTION <id>: … was closed without merging` and `ATTENTION <id>: … is open
   without auto-merge and not in the merge queue` lines, `refused` lines (bd would not
   close a bead) and `stale` lines (the main checkout could not fast-forward; the user pulls
   it by hand) do not stop it: they go into the `Needs you:` line. Never commit, stash or
   discard the main checkout's local changes yourself. `queued` and `waiting` lines are PRs on
   their way through the merge queue (`docs/loop.md`, "The merge queue"): nothing to do.

   An `ATTENTION <id>: <url> left the merge queue (<reason>, …)` line needs no user either: the
   PR failed or conflicted together with `main` and the PRs ahead of it, nothing merged and
   `main` stayed green. Hand it back to its bead: send the line to the bead's session
   (`SendMessage` to its `local_…` id from the linked sessions, title `<id>: …`) with "Your PR
   left the merge queue: rebase onto origin/main, fix it, gate, review and land it again (/bead,
   step 7)". When that session no longer exists, start the bead again in this batch, first after
   a `red-main` bead, at its tier with prompt `/bead <id>` (`start-bead.sh` resumes the bead
   branch); it takes a batch slot.
2. **Red main first.** `main` is red when the sweep prints `ATTENTION windows`, or when the
   newest finished CI run on `main` failed:

   ```bash
   gh run list --workflow ci.yml --branch main --limit 5 --json status,conclusion,url,headSha \
     | jq -r '[.[] | select(.status == "completed" and .conclusion != "cancelled")] | first // empty
       | select(.conclusion != "success") | "\(.conclusion) \(.headSha[0:7]) \(.url)"'
   ```

   Do not ask the user. Unless an open bead labelled `red-main` already covers that run,
   create one: `bd create --type bug --priority 0 --labels red-main,<crate labels the failing
   job points at> --title "Fix red main: <check> at <sha>" --description "<run url, the failing
   step>" --acceptance "<the check> passes on main"`. Start it first in this batch, whatever
   else is ready.
3. **Pick the batch.** `bd ready --json --limit 0 --exclude-type epic --exclude-label human,loop`
   (only `--exclude-label human` when the user passed `loop`; add `--parent <epic>` when an
   epic was given). `bd ready` leaves out beads in progress and beads blocked by an open
   bead, so a bead whose blocker's PR has not merged yet waits for a later batch by itself.
   Walk the list in its order (priority), a bead labelled `escalated` first after a `red-main`
   one, and take a bead when its crate labels (`archicraft-model`, `archicraft-geometry`,
   `archicraft-ifcx`, `archicraft-app`) overlap neither those of the beads already taken nor
   those of beads still in progress (`bd list --status in_progress --json`), until the batch
   holds `parallel` beads or the remaining count is reached. Give each bead you take its tier
   (Tiers), labelling it when it has none. A `fable` bead, or one whose tier is above this
   session's effort, is not taken: it goes into the `Needs you:` line and takes no slot. At
   most one loop bead per batch, and none while another loop bead's PR is open
   (`gh pr list --state open --label loop`), because loop beads edit the same files. If
   nothing is ready, say in one line which open PRs the remaining work waits on and end the
   turn; the user's next "continue" sweeps again.
4. **Start.** In the Claude desktop app, load `mcp__ccd_session__start_session` with
   ToolSearch, mark a chapter for the batch (`mcp__ccd_session__mark_chapter`), and start every bead of the batch in the same
   message: `initiation` "user_asked" (the user ran /bead-loop or said continue), `context`
   "fresh", `use_worktree` true, `model` "claude-opus-5-5", `effort` the bead's tier
   (`medium`, `high`, `xhigh` or `max`), title `<id>: <bead title>`, prompt `/bead <id>`.
   Outside the desktop app, give the user the command per bead
   (`claude --worktree bead-<id> --model claude-opus-5-5 --effort <tier> "/bead <id>"`) and
   stop.
5. **End the turn.** One line: the beads started, each with its tier. The app continues this
   session when the bead sessions report.

## A report turn: verify and report

When the app continues this session with the bead sessions' reports, start nothing (the app
would refuse it) and do not restate the reports. For each bead that reported:

1. **Verify, do not trust the report.** `bd show <id>`: the bead has a PR in `external_ref`,
   the label `human`, or the label `escalated` with a higher `tier:` label and status open
   (`escalate.sh` moved it up a tier; its note says so). With a PR,
   `git show origin/main:scripts/loop/pr-state.sh | bash -s <url>` says where it stands:
   `queued <position>` (in the merge queue), `auto-merge` (it joins the queue once its CI
   passes), `merged`, or `waits-for-user`: it changes `.github/workflows/` or
   `.claude/settings.json` and waits for the user's review (user decision of 2026-10-19);
   `gh pr view <url> --json files` and `land.sh` name the files. An `open` PR that changes one
   of them waits for the user too: its status `User review` is not posted yet. `left-queue
   <reason>`: the queue dropped it, and the next sweep hands it back to the bead (step 1 of a
   user turn).
2. **Count, never wait.** The bead counts once its PR is `queued`, `auto-merge`, `merged` or
   `waits-for-user`. Do not wait for a merge: the app's PR monitor wakes a bead session on a CI
   failure, a conflict or a review comment, never on a merge or on a merge group's failure, and
   the next sweep closes merged beads and reports dropped PRs. An escalated bead does not count
   yet: it starts again in the next batch, and nobody needs to do anything for it, nor for a
   `left-queue` PR, which the next sweep hands back to its bead. A bead without a PR, with a PR
   that is otherwise `open` (auto-merge off for any other reason), or with the label `human`
   does not count; it goes into the `Needs you:` line with its session link. A `human` bead at
   tier `max` names Claude Fable 5.1 as the other way out: "decide, or pick Claude Fable 5.1 in
   its session's model menu and say continue".

Then report, one line per bead and nothing else:

`auto-merge` covers a PR in the merge queue and one whose CI runs before it joins it.

```
<id> ✅ PR #<n> (auto-merge, <tier>): <one line: what changed>
<id> ✅ PR #<n> (waits for you, <tier>): <one line>
<id> ⏫ escalated to <tier>: <the reason>; it starts again in the next batch
<id> ⏸ blocked (<tier>): <the reason, session link>
Needs you: <the one thing, or leave the line out>
```

Add `Needs you:` only when something does: a PR that waits for the user's review, a bead
labelled `human`, a `fable` bead or one above this session's effort (Tiers), a sweep line from
step 1. When beads of the batch are still working, end the turn; the app continues this
session when they report. When the whole batch reported and the
count is not reached, end with "continue" starting the next batch. When the count is reached,
the last report is the run's summary.

Never poll CI: the bead sessions have Auto-fix, and the sweep checks each PR once.

## Demo checkpoint (epic boundary)

When the sweep lists an epic whose tasks are all closed, the loop stops for the user's
demo, because automated tests do not judge how a CAD tool feels:

1. Update a worktree to the latest `origin/main`, build and launch the app
   (`cargo run --release -p archicraft-app`), and take a screenshot if a screen tool is
   available.
2. Tell the user what the epic delivered (its closed beads with SRS IDs, from
   `bd children <epic>`) and give a short script of what to try.
3. On the user's OK: `bd close <epic> --reason "Demo accepted <date>"`. On findings: file
   each as a bead under the epic (`bd create --parent <epic> --type bug …`); the epic stays
   open and the loop picks them up next.

## Budget

Each bead session is a full working session, priced by its tier: small beads run at `medium`,
and Claude Fable 5.1 runs only when the user starts it. Start no more beads than the count the
user gave and no more at once than `parallel`, never fan out further, and report a blocker in
the `Needs you:` line instead of retrying it.
