---
name: bead-loop
description: Orchestrate the bead loop over several beads — sweep, start a batch of ready beads with disjoint crates, each in its own fresh session and worktree with /bead, verify what they landed, never wait on a merge, report blockers in one line, and stop after N beads or at an epic boundary for the user's demo.
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

The user's time is the budget: ask nothing that a rule below answers, keep every report to
one line per bead, and never end a turn by waiting for a merge.

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
   human", `ATTENTION <id>` lines (a PR closed without merging), `refused` lines (bd would not
   close a bead) and `stale` lines (the main checkout could not fast-forward; the user pulls
   it by hand) do not stop it: they go into the `Needs you:` line. Never commit, stash or
   discard the main checkout's local changes yourself.
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
   Walk the list in its order (priority) and take a bead when its crate labels
   (`archicraft-model`, `archicraft-geometry`, `archicraft-ifcx`, `archicraft-app`) overlap
   neither those of the beads already taken nor those of beads still in progress
   (`bd list --status in_progress --json`), until the batch holds `parallel` beads or the
   remaining count is reached. At most one loop bead per batch, and none while another loop
   bead's PR is open (`gh pr list --state open --label loop`), because loop beads edit the
   same files. If nothing is ready, say in one line which open PRs the remaining work waits
   on and end the turn; the user's next "continue" sweeps again.
4. **Start.** In the Claude desktop app, load `mcp__ccd_session__start_session` with
   ToolSearch, mark a chapter for the batch (`mcp__ccd_session__mark_chapter`), and start every bead of the batch in the same
   message: `initiation` "user_asked" (the user ran /bead-loop or said continue), `context`
   "fresh", `use_worktree` true, title `<id>: <bead title>`, prompt `/bead <id>`. Outside the
   desktop app, give the user the command per bead
   (`claude --worktree bead-<id> "/bead <id>"`) and stop.
5. **End the turn.** One line: the beads started. The app continues this session when the
   bead sessions report.

## A report turn: verify and report

When the app continues this session with the bead sessions' reports, start nothing (the app
would refuse it) and do not restate the reports. For each bead that reported:

1. **Verify, do not trust the report.** `bd show <id>`: the bead has a PR in `external_ref`,
   or the label `human`. With a PR, `gh pr view <url> --json state,autoMergeRequest,files`
   shows it open with auto-merge on, or already merged, or open with auto-merge off because
   it changes `.github/workflows/` or `.claude/settings.json`: such a PR waits for the user's
   review (user decision of 2026-10-19), and `land.sh` named the files.
2. **Count, never wait.** The bead counts once its PR is open with auto-merge on, merged, or
   waiting for the user's review. Do not wait for a merge: the app's PR monitor wakes a bead
   session on a CI failure, a conflict or a review comment, never on a merge, and the next
   sweep closes merged beads. A bead without a PR, with auto-merge off for any other reason,
   or with the label `human` does not count; it goes into the `Needs you:` line with its
   session link.

Then report, one line per bead and nothing else:

```
<id> ✅ PR #<n> (auto-merge): <one line: what changed>
<id> ✅ PR #<n> (waits for you): <one line>
<id> ⏸ blocked: <the reason, session link>
Needs you: <the one thing, or leave the line out>
```

Add `Needs you:` only when something does: a PR that waits for the user's review, a bead
labelled `human`, a sweep line from step 1. When beads of the batch are still working, end the
turn; the app continues this session when they report. When the whole batch reported and the
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

Each bead session is a full working session. Start no more beads than the count the user
gave and no more at once than `parallel`, never fan out further, and report a blocker in the
`Needs you:` line instead of retrying it.
