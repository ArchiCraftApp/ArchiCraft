---
name: bead-loop
description: Orchestrate the bead loop over several beads — sweep, start a batch of ready beads with disjoint crates, each in its own fresh session and worktree with /bead at the model and effort of its tier, verify what they landed and start the next batch when the whole batch reported, never wait on a merge, ask the user through AskUserQuestion (and merge a PR that waits for their review once they approve it), and stop after N beads or at an epic boundary for the user's demo.
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
`effort`. A bead whose tier is above this session's effort is not started: the report lists it
with "switch the orchestrator to Opus 5.5, effort max".

The user's time is the budget: ask nothing that a rule below answers, keep every report to
one line per bead, and never end a turn by waiting for a merge. What does need the user is asked
with `AskUserQuestion` in this session (Asking the user), never left in the report for the user
to find.

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
The user is asked instead (Asking the user, kind 4), each turn until the bead is in progress or
moved to another tier; it takes no batch slot.

**Escalation.** A bead session that hits the stop rule (two failed reviews or three gate
failures) runs `scripts/loop/escalate.sh`. The first time, a bead below `max` moves one tier
up (new `tier:` label, label `escalated`, a note) and is ready again; its next session resumes
the bead branch and finds the unreviewed work of the run before as a patch. A second stop, or
one at `max` or `fable`, gives it the label `human` as before.

## Starting a batch

A batch starts in a user turn (the `/bead-loop` command or the user's "continue") and in a
report turn once the whole batch before it reported (A report turn, below), so the loop runs
on without the user's "continue" (user decision of 2026-11-09). The app lets a turn that the
bead sessions' reports triggered start sessions only when it follows the user's own turn or
another such turn; after many report turns in a row without the user, or after a turn of this
session that was stopped or failed, it refuses `start_session`. Then the batch waits for the
user: the turn ends with "continue" starting the next batch. The app continues this session only
when no session it started is still working, so a slot that frees up early stays empty until
the whole batch reported. Each batch starts at once:

1. **Sweep.** Run the sweep as it is on `origin/main` (this checkout's copy may be older),
   the same way as step 0 of `/bead`:

   ```bash
   git fetch --quiet origin main
   main_loop="$(mktemp -d)"
   git archive origin/main scripts/loop | tar -x -C "$main_loop"
   bash "$main_loop/scripts/loop/sweep.sh"; rm -rf "$main_loop"
   ```

   An epic ready for its demo stops the loop (see Demo checkpoint). Beads under "needs a
   human" (asked, kind 2 of Asking the user), `ATTENTION <id>: … was closed without merging`
   and `ATTENTION <id>: … is open without auto-merge and not in the merge queue` lines,
   `refused` lines (bd would not close a bead) and `stale` lines (the main checkout could not
   fast-forward; the user pulls it by hand) do not stop it: the report lists them. A `waits`
   line, a PR that waits for the user's review, is asked (kind 1). Never commit, stash or
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
   (Tiers), labelling it when it has none. A `fable` bead (asked, kind 4) or one whose tier is
   above this session's effort (listed in the report) is not taken and takes no slot. At
   most one loop bead per batch, and none while another loop bead's PR is open
   (`gh pr list --state open --label loop`), because loop beads edit the same files. If
   nothing is ready, say in one line which open PRs the remaining work waits on and end the
   turn; the user's next "continue" sweeps again (no bead session is working, so no report
   turn follows).
4. **Start.** In the Claude desktop app, load `mcp__ccd_session__start_session` with
   ToolSearch, mark a chapter for the batch (`mcp__ccd_session__mark_chapter`), and start every bead of the batch in the same
   message: `initiation` "user_asked" (the user ran /bead-loop for `count` beads or said
   continue), `context`
   "fresh", `use_worktree` true, `model` "claude-opus-5-5", `effort` the bead's tier
   (`medium`, `high`, `xhigh` or `max`), title `<id>: <bead title>`, prompt `/bead <id>`.
   Outside the desktop app, give the user the command per bead
   (`claude --worktree bead-<id> --model claude-opus-5-5 --effort <tier> "/bead <id>"`) and
   stop.
5. **Ask.** Once the batch's sessions are started, ask what needs the user (Asking the user)
   and act on the answers, so no bead waits for them.
6. **End the turn.** One line: the beads started, each with its tier, and a line for each thing
   the report lists. The app continues this session when the bead sessions report.

If `start_session` is refused (a report turn the app no longer lets start sessions), do not
retry it and do not start the beads another way: the beads picked stay ready, and the turn ends
with the report and "continue" starting the next batch.

## A report turn: verify, report and start the next batch

When the app continues this session with the bead sessions' reports, do not restate the
reports. For each bead that reported:

1. **Verify, do not trust the report.** `bd show <id>`: the bead has a PR in `external_ref`,
   the label `human`, or the label `escalated` with a higher `tier:` label and status open
   (`escalate.sh` moved it up a tier; its note says so). With a PR,
   `git show origin/main:scripts/loop/pr-state.sh | bash -s <url>` says where it stands:
   `queued <position>` (in the merge queue), `auto-merge` (it joins the queue once its CI
   passes), `merged`, or `waits-for-user`: it changes `.github/workflows/` or
   `.claude/settings.json` and waits for the user's review (user decision of 2026-10-19);
   `gh pr view <url> --json files` and `land.sh` name the files. An `open` PR that changes one
   of them waits for the user too: its status `User review` is not posted yet. `left-queue
   <reason>`: the queue dropped it, and the next sweep hands it back to the bead (step 1 of
   Starting a batch).
2. **Count, never wait.** The bead counts once its PR is `queued`, `auto-merge`, `merged` or
   `waits-for-user`. Do not wait for a merge: the app's PR monitor wakes a bead session on a CI
   failure, a conflict or a review comment, never on a merge or on a merge group's failure, and
   the next sweep closes merged beads and reports dropped PRs. An escalated bead does not count
   yet: it starts again in the next batch, and nobody needs to do anything for it, nor for a
   `left-queue` PR, which the next sweep hands back to its bead. A bead without a PR, with a PR
   that is otherwise `open` (auto-merge off for any other reason), or with the label `human`
   does not count. A `human` bead is asked about (kind 2); the report lists the others with
   their session links.
3. **Next batch.** When the whole batch reported and fewer than `count` beads counted so far
   in this run, start the next batch now: Starting a batch, steps 1 to 4, sized to the beads
   still to run. An epic ready for its demo stops the loop there as in a user turn. When beads of
   the batch are still working (a note arrived early), start nothing and end the turn after the
   report; the app continues this session when they report.
4. **Ask** what needs the user (Asking the user): a PR that waits for the user's review, a
   `human` bead, and what the sweep of step 3 found, after the next batch's sessions are
   started.

Then report, one line per bead and nothing else:

`auto-merge` covers a PR in the merge queue and one whose CI runs before it joins it.

```
<id> ✅ PR #<n> (auto-merge, <tier>): <one line: what changed>
<id> ✅ PR #<n> (merged on your OK, <tier>): <one line>
<id> ✅ PR #<n> (waits for you, <tier>): <one line>
<id> ⏫ escalated to <tier>: <the reason>; it starts again in the next batch
<id> ⏸ blocked (<tier>): <the reason, session link>
```

`waits for you` is a PR the user answered `Later` or `Changes needed` for. Below the bead lines,
one line each for what the user has to do by hand and nothing here can answer: a `stale`,
`refused` or `ATTENTION` sweep line from step 1, a bead above this session's effort (Tiers), a
`human` bead that waits on a third party, a step after a merge that the PR's description gives
the user (`setup-github.sh`). Then one line for the next batch: the beads started, each with
its tier; or, when `start_session` was refused, "continue" starting the next batch; or, when
nothing is ready, which open PRs the remaining work waits on. When the count is reached, the
last report is the run's summary.

Never poll CI: the bead sessions have Auto-fix, and the sweep checks each PR once.

## Asking the user

The user answers here, in this session, through `AskUserQuestion` (user request of 2026-11-04):
nobody has to look for a line in a report or merge a PR by hand. Ask at the end of a turn's
work, after its sessions are started (a user turn, and a report turn that starts the next
batch) or its reports verified (a report turn that starts none), so
no bead waits for the answer; act on the answers in the same turn. At most four questions per
call, PRs that wait for the review first; ask the rest in a second call. Never fill in the
call's `answers` yourself: the `user-merge` hook refuses such a call, and only the user's own
answer counts. Each kind of question and its options:

1. **A PR that waits for the user's review** (a `waits` sweep line, `waits-for-user`, or `open`
   and changing `.github/workflows/` or `.claude/settings.json`). Ask once its required checks
   other than `User review` passed on its head (`gh pr checks <n> --required`, read once, never
   polled), since the merge needs them within 60 minutes of the answer: while one is pending,
   ask after the next sweep, and a failing one wakes the bead's session through Auto-fix. Read
   its head and files: `gh pr view <n> --json headRefOid,url,files`. The question starts with
   `PR #<n> at <the first 7 characters of headRefOid>` and its bead id, says in two or three
   sentences what changes and why (from the bead and the PR description), names the changed
   workflow and hook-wiring files and links the diff (`<url>/files`); the header is `PR #<n>`.
   Options, one choice:
   - `Merge #<n>`: merge it now, as below.
   - `Changes needed`: send the user's text (their note on the choice, or their text under
     Other) to the bead's session with `SendMessage`: "The user's review of PR #<n>: <text>. Fix
     it on the branch, gate, review and land it again (/bead, step 7)". An answer under Other is
     `Changes needed` with that text. When the session no longer exists, append the text to the
     bead's notes and start the bead again in the next batch (`start-bead.sh` resumes its
     branch); it takes a batch slot.
   - `Later`: ask again after the next sweep.

   After `Merge #<n>`, merge it in this turn, the PR and the full head sha written out (the
   guard reads the command line, not variables):

   ```bash
   gh pr merge <n> --admin --squash --match-head-commit <the 40 characters of headRefOid>
   ```

   The answer wrote the stamp `user-merge.<n>` (hook `user-merge`), and `guard-bash` lets
   exactly this command through while the stamp is at most 60 minutes old, the PR's head is
   still that commit and every required check but `User review` passed on it; the merge uses
   the stamp up (docs/loop.md, "The user's approval of a merge"). When it refuses, its message
   says why: the head moved (ask again, the user reviews what changed), a check failed or is
   still pending (a failure wakes the bead's session through Auto-fix; merge once it passed, or
   ask again after the next sweep), or the answer is older than 60 minutes (ask again). A
   refusal with no reason after its first sentence means the command is not exactly the one
   above: check the PR number, `--squash`, the 40-character sha and that no other flag is there,
   and run it again. Only when it is exactly that command does the refusal come from a
   `guard-bash` older than this rule (this checkout lags behind `main`): then the report tells
   the user to merge it on GitHub. If the PR's description lists a step after the merge
   (`setup-github.sh`), the report names it.
2. **A bead labelled `human` that needs a decision.** The question is the one its notes ask,
   with the bead id; the options are the choices the notes name (two to four, the bead's
   recommendation first; at most three for a bead at tier `max`, which gets a fourth below).
   Record the answer verbatim with the date and remove the label:
   `bd update <id> --append-notes "<YYYY-MM-DD>, the user (AskUserQuestion): '<answer>'"
   --remove-label human`, then send the answer to the bead's session (`SendMessage`: "The
   user decided: '<answer>'. Continue (/bead)"); when the session no longer exists, the bead
   starts again in the next batch. A bead at tier `max` adds the option `Claude Fable 5.1`:
   then swap its `tier:max` label for `tier:fable` and ask kind 4. A `human` bead that waits on
   a third party (an upstream issue, a licence answer) has nothing to decide: the report lists
   it, it is not asked.
3. **The demo checkpoint**: see Demo checkpoint.
4. **A `fable` bead** (until archicraft-5vau0 lets this session start it): "`<id>` (<title>)
   runs at tier fable, Claude Fable 5.1 at 2.5 times Opus 5.5's price per token." Options:
   `Start it myself` (its description gives the way: new Code session, pick Claude Fable 5.1,
   effort high, in the model menu, send `/bead <id>`), `Run at max`
   (`bd update <id> --remove-label tier:fable --add-label tier:max`; it starts in the next
   batch) and `Later` (asked again next turn).

## Demo checkpoint (epic boundary)

When the sweep lists an epic whose tasks are all closed, the loop stops for the user's
demo, because automated tests do not judge how a CAD tool feels:

1. Update a worktree to the latest `origin/main`, build and launch the app
   (`cargo run --release -p archicraft-app`), and take a screenshot if a screen tool is
   available.
2. Tell the user what the epic delivered (its closed beads with SRS IDs, from
   `bd children <epic>`) and give a short script of what to try.
3. Ask with `AskUserQuestion` (Asking the user): "Demo of `<epic>` (<title>): does it work as
   it should?" Options: `Accept` (`bd close <epic> --reason "Demo accepted <date>"`),
   `Findings` (the user's text, their note on the choice or their text under Other, which
   counts as findings too: file each finding as a bead under the epic,
   `bd create --parent <epic> --type bug …`; the epic stays open and the loop picks them up
   next) and `Later` (asked again after the next sweep).

## Budget

Each bead session is a full working session, priced by its tier: small beads run at `medium`,
and Claude Fable 5.1 runs only when the user starts it. Start no more beads than the count the
user gave and no more at once than `parallel`, never fan out further, and ask about a blocker
(Asking the user) or list it in the report instead of retrying it.
