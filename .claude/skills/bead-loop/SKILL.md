---
name: bead-loop
description: Orchestrate the bead loop over several beads — sweep, start each ready bead in its own fresh session and worktree with /bead, verify what it landed, and stop after N beads, at an epic boundary for the user's demo, or at the first blocker.
argument-hint: "[count] [epic-id]"
disable-model-invocation: true
---

# Orchestrating the bead loop

This session coordinates; it never edits code and never works a bead itself. Every bead
runs in its own fresh session with `/bead <id>` (`.claude/skills/bead/SKILL.md`), so each
starts with a clean context. The design is in `docs/loop.md`.

Arguments (`$ARGUMENTS`): the number of beads to run (default 3) and optionally an epic id
that restricts the picks to its descendants.

## Each iteration

1. **Sweep.** Run the sweep as it is on `origin/main` (this checkout's copy may be older),
   the same way as step 0 of `/bead`:

   ```bash
   git fetch --quiet origin main
   main_loop="$(mktemp -d)"
   git archive origin/main scripts/loop | tar -x -C "$main_loop"
   bash "$main_loop/scripts/loop/sweep.sh"; rm -rf "$main_loop"
   ```

   Stop the loop and report to the user when it prints `ATTENTION`, lists beads under
   "needs a human", or lists an epic ready for its demo (see Demo checkpoint). A `refused`
   line (bd would not close a bead, for example a blocked one) does not stop the loop; include
   it in your report to the user.
2. **Pick.** `bd ready --json --limit 0 --exclude-type epic --exclude-label human`
   (add `--parent <epic>` when an epic was given) and take the first entry. Loop beads edit
   the same files, so no loop bead starts while another loop bead's PR is open, whether it
   merges by itself or waits for the user: when `gh pr list --state open --label loop` lists
   one, pick with `--exclude-label human,loop` instead. If nothing is ready, the remaining work
   waits on open PRs: list them from the sweep output, end your turn, and tell the user to say
   "continue" once they merged.
3. **Start.** In the Claude desktop app, load `mcp__ccd_session__start_session` with
   ToolSearch and start the bead: `initiation` "user_asked" (the user ran /bead-loop),
   `context` "fresh", `use_worktree` true, title `<id>: <bead title>`, prompt `/bead <id>`.
   Run one bead at a time. Run two at once only when the user asked for it and the two
   beads share no crate label (`archicraft-model`, `archicraft-geometry`, `archicraft-ifcx`,
   `archicraft-app`), and never two beads labelled `loop`. Outside the desktop app, give the user the command for a new session
   in a fresh worktree instead (`claude --worktree bead-<id> "/bead <id>"`) and stop.
4. **Wait.** End your turn. The app continues this session when the bead session reports.
5. **Verify, do not trust the report.** `bd show <id>`: the bead has a PR in
   `external_ref`, or the label `human`. With a PR, `gh pr view <url> --json state,autoMergeRequest,files`
   shows it open with auto-merge on, or already merged, or open with auto-merge off because
   it changes `.github/workflows/` or `.claude/settings.json`: such a PR waits for the user's
   review (user decision of 2026-10-19), and `land.sh` named the files. Anything else (no PR,
   auto-merge off without those files, the label `human`) stops the loop: report it to the
   user with the session link.
6. **Count.** The bead counts once its PR is open with auto-merge on, or merged; a loop
   bead's PR counts like a feature bead's. A PR that waits for the user's review counts too:
   tell the user in one line that it waits for them, and carry on (step 2 skips loop beads
   until it merged). Continue with the next iteration until the count is reached, then
   report: the beads, their PRs, what waits, and which PRs wait for the user's review.

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
gave, never fan out further, and stop at the first blocker rather than skipping past it.
