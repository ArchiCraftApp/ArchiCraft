# The bead loop

How ArchiCraft is implemented: one bead at a time, each in a fresh agent session, and nothing
counts until it is merged on green CI. The first implementation run (2026-10-17) fanned out
parallel implementers and left three WIP branches of 1,300 to 2,400 lines that were never
built or reviewed. This loop is the answer to that: small slices, mechanical gates, a
reviewer that did not write the code, and hooks that make the steps unavoidable.

## One bead

The procedure an agent follows is the `/bead` skill (`.claude/skills/bead/SKILL.md`):

| Step | What happens | Tool |
|---|---|---|
| 0 Sweep | Close beads whose PR merged and split parents (label `loop-split`) whose children are done; report closes bd refused, waiting and queued PRs, PRs the merge queue dropped (`ATTENTION`), beads for a human, epics due for a demo, a failed newest Windows run on `main` (`ATTENTION`); fast-forward the main checkout to `origin/main` when it is on `main` (git hooks off), or report as `stale` why not. The copy on `origin/main` runs, since a new worktree may be cut from an older `main` | `scripts/loop/sweep.sh` |
| 1 Claim | Claim the bead, branch `bead/<id>-<slug>` from `origin/main`, clone the warm build cache. It re-executes the copy on `origin/main` when the worktree's `scripts/loop` differs | `scripts/loop/start-bead.sh <id>` |
| 2 Slice | Split into child beads above 480 minutes, above 800 non-test source lines (a top-level `#[cfg(test)] mod tests` does not count), or with separable criteria | `bd create --parent` |
| 3 Tests first | Every acceptance criterion becomes a failing test | |
| 4 Implement | CLAUDE.md conventions; rustfmt runs after every edit | |
| 5 Gate | fmt, clippy, tests, layering, golden files, diff budget, `.beads/`, loop guardrails only under label `loop`, implementation status (`scripts/status.py` check and merge-check with the bead's `SRS:` line), conformance, licences | `scripts/loop/gate.sh` |
| 6 Review | Fresh-context `bead-reviewer` agent; its verdict becomes the review stamp | `.claude/agents/bead-reviewer.md` |
| 7 Land | Commit `<id>: <summary>`, push, PR, squash auto-merge: on green CI the PR joins the merge queue, which merges it once CI passes on `main` with the PRs ahead of it (a PR that changes `.github/workflows/` or `.claude/settings.json` waits for the user) | `scripts/loop/land.sh` |
| 8 Close | Close note (tier, review rounds, gate failures); bead closes when the PR merges; follow-ups and lessons recorded | `sweep.sh`, `bd remember` |

The gate and the review stamp the exact tree they checked (`loop_tree_hash` in
`scripts/loop/lib.sh`). Any later edit invalidates the gate stamp. The review stamp belongs to
the branch's bead and survives only edits to prose (`docs/` and top-level Markdown files,
`loop_review_valid`, `LOOP_REVIEW_CARRY_PATHS`; not `CLAUDE.md`, `AGENTS.md` or `docs/SRS.md`,
which instruct agents or define requirements): a trivial doc fix for a minor finding after
a `PASS` needs a new gate run but no new review, and every other minor finding becomes a
follow-up bead instead of another review round. Stamps, logs and counters
live in the worktree's git dir (`git rev-parse --git-path loop`), never in the repository.
`scripts/loop/status.sh` shows where a worktree stands.

**Stop rules.** Three gate failures on the same problem or two failed reviews: the session runs
`scripts/loop/escalate.sh "<reason>"`. The first time, a bead below tier `max` moves one tier
up (`medium`, `high`, `xhigh`, `max`; Tiers below): its `tier:` label changes, it gets the
label `escalated` and a note, and its claim is released. The script saves the session's
uncommitted work as a patch in the git dir all worktrees share (`loop_escalation_patch`) and
detaches the worktree from the bead branch, so the orchestrator's next batch resumes the branch
in a fresh session on the stronger tier, where `start-bead.sh` names the patch. The second
time, or at tier `max` or `fable`, the bead gets the label `human` and the session pauses
(`pause.sh`); the orchestrator reports the bead in its `Needs you:` line and carries on with
other beads. A decision that belongs to the user skips the escalation: the bead gets the label
`human` and the session pauses (`scripts/loop/pause.sh "<reason>"`).

## Several beads

`/bead-loop [count] [parallel] [epic] [loop]` (`.claude/skills/bead-loop/SKILL.md`) runs in an
orchestrator session that never edits code, and only one such session runs at a time: two
pick the same beads and coordinate through the user. In the desktop app, sessions start only
in a turn the user triggered, never in one a sub-session's report triggered (user decisions of
2026-10-20). So each user turn (`/bead-loop` or "continue") sweeps and starts a whole batch at
once: up to `parallel` (default 3) ready beads, each in its own fresh session and worktree with
`/bead <id>`, whose crate labels overlap neither each other nor beads still in progress. Beads
labelled `loop` are picked only when the user passes `loop`, at most one per batch and none
while another loop bead's PR is open, because loop beads edit the same files. A red `main` (the
sweep's `ATTENTION windows` line or a failed CI run on `main`) becomes a P0 bead labelled
`red-main` that starts first in the next batch, without asking.

**Tiers: model and effort per bead.** Every bead runs at a tier, its label `tier:<x>`:
`medium`, `high`, `xhigh` and `max` are Claude Opus 5.5 at that effort, `fable` is Claude
Fable 5.1 (2.5 times Opus 5.5's price per token). A bead without the label gets one from the
rubric in `/bead-loop` before it starts: `xhigh` for `archicraft-geometry` algorithms, IFCX
format, reader or writer changes, beads labelled `loop` and `red-main`; `medium` for estimates
of 30 minutes or less (tidy-ups, test tightening, docs, chores); `high` otherwise. `max` and
`fable` come only from an escalation (stop rules above) or from slicing, which marks a child so
for an estimate of 360 minutes or more or a memory or architecture redesign; slicing gives
every child a tier. The orchestrator passes the tier's model and effort to `start_session`,
which takes only an effort at or below the caller's own and ignores a model above it, so the
orchestrator must run on Opus 5.5 at effort `max`, or the higher tiers cannot start. It never
starts a `fable` bead: it cannot raise the model, and `set_session_model` applies only from a
session's next turn, while `/bead` runs in the first. The bead goes into the `Needs you:` line
with the way to start it (`/bead <id>` in a new session with Claude Fable 5.1 picked). The
reviewer runs at effort `xhigh` whatever the tier (`effort` in
`.claude/agents/bead-reviewer.md`), because the gate and the review are what make a cheaper
implementer safe. The field works: on Claude Code 2.1.299 (2026-10-21) a probe subagent with
`effort: xhigh` ran at `xhigh` under a parent at `medium` and at `max` (session transcripts
record `effort` per message). Each bead's close note (`Close: tier <x>, <n> review rounds, <m>
gate failures, …`) is the data to calibrate the rubric with. Multi-agent workflows stay out of
the loop: one session per bead.

Turns triggered by the bead sessions' reports only verify each PR and auto-merge and report.
Nobody waits on a merge: the app's PR monitor wakes a session on a CI failure, a conflict or
a review comment, never on a merge. A bead counts once its PR is open with auto-merge on, is in
the merge queue, or waits for the user (see below); a bead blocked by a bead whose PR is still open is not ready
and goes into a later batch, and the sweep closes merged beads. The run stops after `count`
beads or when an epic's tasks are all closed.

**Reports.** A bead session ends with at most 4 lines:
`<id> ✅ PR #N (auto-merge|waits for you|blocked, <tier>): <what changed>`, where `auto-merge`
means on its way through the merge queue (⏫ for an
escalated bead, ⏸ for one labelled `human`), then `Needs you: -` or the one thing the user has
to do. The orchestrator reports one line per bead and a `Needs you:` line only when something
does, without restating the sessions' reports. Details only on request. A review finding
becomes a loop bead (priority 3) only when it caused a real defect; anything less goes into
`bd remember`.

**Demo checkpoint.** An epic closes only after the user tried it: the orchestrator builds and
launches the app from `main`, lists what the epic delivered, and closes the epic on the user's
OK or files the findings as beads under it.

## Enforcement

Instructions are soft; these are not:

| Where | Rule |
|---|---|
| Hook `guard-edit` (PreToolUse Edit/Write) | Code under `crates/`, `conformance/` and the workspace manifests is edited only on a bead branch whose bead is in progress; golden files are never edited by hand; the reviewer never edits; a feature bead never edits the loop's own files |
| Hook `format-rust` (PostToolUse) | rustfmt on every edited `.rs` file; syntax errors reported back |
| Hook `guard-bash` (PreToolUse Bash) | No force push, no push to `main`, no PR merge without `--auto`, no `--admin`, no auto-merge for a PR that changes `.github/workflows/` or `.claude/settings.json`; code commits only on a bead branch with a gate on the exact tree and a review that covers it; stamps cannot be written by hand; the reviewer runs read-only commands |
| Hook `review-verdict` (SubagentStop) | The reviewer's `VERDICT: PASS` stamps the tree, only when the gate passed on it |
| Hook `stop-gate` (Stop) | A bead session does not end with uncommitted code that has not passed the gate and the review; it runs the gate, blocks at most three times per tree, and `escalate.sh` (the stop rules) and `pause.sh` are the explicit ways out |
| Hook `session-context` (SessionStart) | New, resumed and compacted sessions see the loop state |
| CI job `Loop policy` | Bead PR titles start with the bead id; bead PRs never change `.beads/`; golden files change only with the PR label `format-change`; loop files change only in a bead labelled `loop` (labels read live at run time, since `gh pr create` attaches them after the PR is opened); headless layering; the hooks' self-test |
| CI workflow `User review` | A PR that changes `.github/workflows/` or `.claude/settings.json` (or `LOOP_USER_REVIEW_PATHS`) gets the failing status `User review`, decided by the base's `scripts/loop/ci-user-review.sh` (`pull_request_target`), never by the PR's own scripts |
| Rulesets on `main` | Every PR merges through the merge queue (squash), which needs the seven required CI checks on Linux and the status `User review` on the PR and again on `main` with the PRs ahead of it (The merge queue, below); admins bypass it, nobody deletes or force-pushes `main`; Windows runs on `main`, nightly and by hand, macOS only for releases and the nightly conformance matrix (below) |

**A bead PR runs CI once, and once more in the merge queue.** `ci.yml` runs for the
`pull_request` event; its `push` trigger covers only `main` and version tags `v*`, so pushing a
bead branch does not start a second run (before 2026-10-20 every bead PR ran it twice, in two
concurrency groups). Its `merge_group` trigger runs it on the PR's merge group (below).

**Windows runs on `main` and nightly.** No Windows job runs on a pull request, and none is a
required check: the Windows test job took 10 to 17 minutes where Linux took 1.5 to 9, and every
bead waited for it (user decision of 2026-10-20; the conformance matrix below follows it). The
workflow `.github/workflows/windows.yml` builds and tests on `windows-latest`, with
`core.autocrlf=true` as a Windows developer checks out (NFR-PORT-004), on every push to `main`,
nightly and when started by hand (`gh workflow run windows.yml --ref <branch>`). So a
Windows-only failure lands on `main` and shows up after the merge: `sweep.sh` looks at the
newest finished run on `main` that was not cancelled and prints `ATTENTION windows: …` with its
link when it did not pass. `/bead-loop` turns that line into a P0 bead labelled `red-main` and
starts it first in its next batch. The ruleset `main` names its required checks, so a change to
that list in `setup-github.sh` takes effect only when the user runs the script again (Setup):
for the change that dropped the Windows check, before its PR merges, since that PR's own run no
longer reports `Test (windows-latest)`.

**macOS runs only for releases and the nightly conformance matrix.** No macOS job runs on a
push or pull request, and none is a required check. On 2026-10-19 GitHub's hosted macOS runners
blocked every merge: the Apple silicon jobs (`macos-latest`) were never picked up and GitHub
cancelled them after 15 minutes, and an Intel job (`macos-15-intel`) took 16 minutes where
Linux took one. The workflow `.github/workflows/macos.yml` builds and tests on Apple silicon
when a version tag `v*` is pushed and when started by hand (`gh workflow run macos.yml --ref
<branch>`). It runs macOS 26 and 15, the two newest major versions GitHub's hosted runners
offer at this date (NFR-PORT-002 asks for Apple's current and previous one; a newer Apple
release is covered only once GitHub adds it). The one other macOS job is the nightly run of the
conformance matrix (`.github/workflows/conformance-matrix.yml`, IFX-006 on three platforms); on
pull requests that matrix runs Linux alone, and none of its jobs is required. So a Windows- or
macOS-only conformance failure shows up the next night, and any other macOS-only failure at the
next release at the latest: run the workflow by hand before tagging one, and file a red run as
a bead. When GitHub adds the next macOS major version, move both labels in `macos.yml` up by
one ([actions/runner-images](https://github.com/actions/runner-images#available-images) lists
the current labels).

**The merge queue.** Until 2026-10-21 branch protection required the CI checks but not that a
PR be up to date with `main` (`strict: false`), and `/bead-loop` lands up to three PRs at once.
Each PR's CI ran on the PR merged into `main` as it was at the PR's last push, never on the
combination, so two PRs that each passed could merge into a `main` that did not compile. That
happened twice that day, and each time every open bead PR was blocked until a P0 `red-main` bead
fixed it: #86 added `IfcClass::BuildingElementProxy` while #84 added `IfcClass::label_key()`
without that arm (archicraft-o363), and #97 added the field `Element::snap_excluded` while #98
added a builder in `crates/archicraft-model/src/samples.rs` that does not set it
(archicraft-va4v). Requiring up-to-date branches would have caught both, but every merge would
then leave every other open PR stale, to be updated and tested again by hand, and the loop is
meant to run for hours without the user.

So every PR merges through GitHub's merge queue (archicraft-9uh3). Auto-merge
(`gh pr merge --auto --squash` in `land.sh`) no longer merges a PR itself: once the PR's own
required checks pass, GitHub puts it into the queue (GitHub admits no PR whose checks have not
passed). For each PR the queue builds a merge group, a commit on the branch
`gh-readonly-queue/main/pr-<number>-<sha>` that holds `main`, the PRs ahead in the queue and this
PR, each squashed. CI runs on it (the event `merge_group`), and only when every required check
passed there does `main` fast-forward to that very commit. When a group fails, the queue drops its
PR and rebuilds the groups behind it without it, and `main` stays green. The settings, in the
ruleset `main` that `setup-github.sh` writes: squash; ALLGREEN, so each PR's own group must pass;
up to 3 groups built at once (one `/bead-loop` batch) and up to 3 PRs merged in one push; and 30
minutes for a required check to report on a group before the queue counts it as failed (the
slowest required job, the Linux tests, takes 3 to 9 minutes; the rest covers waiting for a
runner).

Every required check must report on a merge group, or the queue waits for it until that timeout
and drops the PR. `ci.yml` runs on `merge_group` and all its required jobs run there; the bench,
which is not required, skips it, since the PR's run compared it with the merge base already.
`selftest.sh` checks that every required check of the ruleset is a job of `ci.yml` without a
condition, or the status `User review`, and that every job without a condition is required: the
`Implementation status` job was not, and PRs #140, #141 and #143 merged with it failing
(archicraft-erpe). Windows, macOS, the conformance matrix, the fuzzing run and
the announcement are not required and do not run on merge groups; Windows and the announcement
run when the queue has pushed to `main`. The status `User review` comes from `user-review.yml`,
which runs on `merge_group` too: there it checks out `main`, not the group's commit, and `main`'s
`ci-user-review.sh` judges everything the group adds to `main` and posts the status on the
group's commit, so the queue neither skips the rule nor waits for the status. A PR that waits for
the user never gets into the queue (its `User review` fails on the PR), and if one did, its group
would fail the same check. For a merge group GitHub takes `user-review.yml` from the group's
commit rather than from `main`; that is safe because every PR in the queue passed the check as
`main` defines it first, so none of them changed that file.

A PR the queue dropped stays open with auto-merge off (GitHub clears it), and its own checks stay
green, so nothing on the PR wakes the bead's session. `scripts/loop/pr-state.sh` reads where a PR
stands (`queued <position>`, `auto-merge`, `left-queue <reason>`, `waits-for-user`, `open`,
`merged`, `closed`), with GitHub's reason for a drop (`failed_checks`, `merge_conflict`,
`checks_timed_out`, `manual`). The sweep prints a dropped PR as `ATTENTION <id>: <url> left the
merge queue (<reason>, <the failed merge group run>)`; the orchestrator hands that line to the
bead's session (or resumes the bead in a new one), which rebases onto `origin/main`, fixes the
combination, runs the gate and the review again and lands with `land.sh --rebased`, and auto-merge
puts the PR back in the queue. A PR in the queue shows as `queued` with its position, one whose
CI still runs as `waiting` (`CI pending, then the merge queue`), and the reports count both as on
their way (`auto-merge`). A PR that nothing moves on, open without auto-merge, outside the queue
and not waiting for the user, shows as `ATTENTION <id>: <url> is open without auto-merge and not
in the merge queue` and goes into the orchestrator's `Needs you:` line.

The user merges a PR that waits for the review as before, as an admin past the failing status
`User review` and past the queue ("Merge without waiting for requirements to be met (bypass
rules)" on GitHub). Admins bypass the ruleset `main`, which also lets them repair `main` and push
to it directly; the ruleset `main history` forbids deleting and force-pushing `main` for everyone,
since a bypass covers every rule of its ruleset. `setup-github.sh` replaces the classic branch
protection of `main` with these two rulesets: GitHub configures a merge queue only through
rulesets, and the classic protection's list of required checks would apply to merge groups as
well. GitHub offers the merge queue to public repositories owned by an organization, on every
plan, which this one is (checked 2026-10-21); a private one needs GitHub Enterprise Cloud, and
there `setup-github.sh` stops without changing anything. The fallback there would be
`strict: true`, under which auto-merge waits until a PR is up to date with `main`, and the sweep
would have to bring each stale PR up to date (`gh pr update-branch <pr>`) so that CI runs again
on the combination.

The hooks are wired in `.claude/settings.json`. Claude Code reads that file, the skills and
the agents from the main checkout and starts the hook commands there, also for a session in a
worktree (the desktop app passes the main checkout as `--project-config-root`). So each hook
command runs the main checkout's `scripts/loop/hooks/dispatch.sh`, which takes the session's
working directory from the hook input (`cwd`) and runs the hook script of that tree: a worktree
session runs its own branch's hook scripts, and `selftest.sh` checks this with a worktree. What
still comes from the main checkout is the wiring, the dispatcher, `/bead`, `/bead-loop` and the
`bead-reviewer` agent; a change to those applies once the main checkout is updated. Loop PRs
merge by themselves, so `sweep.sh` updates it at the start of every bead: a fast-forward to
`origin/main` while it is on `main`, with git hooks off (the beads post-merge hook can block bd
for minutes). Git refuses when local changes would be overwritten or local `main` has commits
of its own; the sweep then reports a `stale` line and carries on, and the user pulls by hand
([loop-tutorial.md](loop-tutorial.md), section 3). Command-line matching is a guardrail, not a
sandbox: CI and the rulesets on `main` are the gates that cannot be bypassed.

`guard-edit` sees only the Edit and Write tools. A file changed through Bash (`sed -i`, a
heredoc, a script) bypasses it, on `main` as on a bead branch. Only the stamps catch such a
change locally (a code commit needs a gate on the exact tree and a review that covers it), and
CI catches it in the pull request.

`guard-bash` judges all its rules (pushes, PR merges, commits, writes to the stamps, the
reviewer's commands) by the commands a line runs (`scripts/loop/hooks/shell-commands.awk`),
including those in `$(...)`, `find -exec`, behind wrappers such as `xargs`, `sudo` or
`timeout`, in `sh -c`, `eval` and `parallel`, and in scripts fed to a shell; a git command's
checks run in the directory its `-C` options name. Other quoted arguments and heredoc bodies are
data: a grep pattern, a commit message or a bead note may name `git push --force`,
`gh pr merge`, `git commit`, `rm` or the state dir (the state dir only on a line that writes no
file, since a path can reach a write through a variable or a pipe). A file counts as written by
a redirection, `rm`, `mv`, `cp`, `touch`, `tee`, `truncate`, `ln`, `chmod`, `install`, `mkdir`,
`rmdir`, `patch`, `dd of=`, `sed -i`, `perl -i` and `find -delete`, and for the reviewer also by
the git subcommands that change the work tree or the index (`git apply`, `git merge-file`,
`git checkout-index`, ...); calling the loop's own `loop_stamp_write` is refused whatever its
arguments.

What command-line matching cannot see: code that a program runs from its own arguments or
files. An interpreter's script (`python3 -c`, `node -e`, `ruby -e`, `perl -e` without `-i`), a
write inside a quoted program (`sed`'s `w` command, `awk '{print > "f"}'`), a command run
through `su -c` or `script -c`, a script file the line runs, and build tools that run project
code (`make`, `cargo run`, build scripts, `npm install`) can all write files, the stamps among
them, without the guard noticing. A stamp written that way still names a tree, so it covers only
that exact tree; CI runs the gate again on every pull request.

Beads labelled `loop` change the loop itself. Every review finding that a script could have
caught becomes such a bead, so the gate gets stricter over time. Since the user's decision of
2026-10-19 their PRs auto-merge like a feature bead's (squash, on green CI, after a passing
review), so the loop runs for hours without the user. The exception is a PR that changes
`.github/workflows/` or `.claude/settings.json` (`LOOP_USER_REVIEW_PATHS` in
`scripts/loop/lib.sh`): they hold the CI gate and the hook wiring, the gates that cannot be
bypassed. `land.sh` leaves auto-merge off for such a PR, also when an earlier run turned it on,
names the files and writes the user's steps into its description (review it, merge it as an
admin, and run `setup-github.sh` after the merge when the PR changes it, a step it writes for a
PR that auto-merges too); `guard-bash` refuses turning it on by hand; the user reviews and merges it.
Both judge by the pattern as `origin/main` defines it as well as the branch's copy, so a branch
that narrows `LOOP_USER_REVIEW_PATHS` still waits for every file the base names, and both also
make a PR wait when it changes the pattern itself (naming `scripts/loop/lib.sh`).
Both still run the branch's own copies, though, so a branch that rewrites `land.sh` or
`lib.sh` could turn auto-merge on for its own workflow change. CI closes that gap
(archicraft-9nlk): `.github/workflows/user-review.yml` runs on `pull_request_target`, so GitHub
takes the workflow and its script `scripts/loop/ci-user-review.sh` from `main`; it checks out
`main`, reads the PR's commits as data and posts the commit status `User review` on the PR's
head, failing when the PR changes a file of `main`'s `LOOP_USER_REVIEW_PATHS` or the pattern
itself. The ruleset `main` requires that status, so auto-merge does not merge such a PR even
when a branch whose scripts skip the rule turned it on; the user merges it after the review as
an admin, past the failing status and the merge queue ("Merge without waiting for requirements
to be met (bypass rules)", or `gh pr merge --admin`, which the hooks refuse to agents). The
ruleset ties every required check to GitHub Actions (`integration_id` 15368 in
`setup-github.sh`), so a status of the same name posted through `gh api` with a personal token
does not count; one that the PR's own workflow posts with its `GITHUB_TOKEN` still could
overwrite it, and closing that is archicraft-t9zs.
A CODEOWNERS review cannot enforce this while Claude works under the user's GitHub account,
because GitHub does not let an account approve its own PR.

## Setup

Once per machine: `brew install gh && gh auth login` (PRs and auto-merge), optionally
`cargo install cargo-deny` (the gate skips the licence check without it; CI runs it).

Once per repository, by an admin: `scripts/loop/setup-github.sh` (auto-merge, labels, the
rulesets on `main` with the merge queue and the required checks). Run it again after a change to
its list of required checks or the queue's settings: `land.sh` writes that step into the
description of every PR that changes the script. A PR
opened before the status `User review` became required gets it with its next push (or a close
and reopen).

After changing a hook or loop script: `scripts/loop/selftest.sh`.

## Announcements

Every change that reaches `main` is announced in the project's Signal group (user decision of
2026-10-19): `.github/workflows/announce.yml` runs `scripts/announce-signal.sh` on each push to
`main` and posts one short message with each merged PR's title (bead id left out) and link.
PRs merge only on green CI, so nothing red is announced. Signal has no bot API, so the message
goes through a self-hosted [signal-cli-rest-api](https://github.com/bbernhard/signal-cli-rest-api)
whose account (a number of its own, or a device linked to an existing one) is a member of the
group. GitHub's runners must reach it; put it behind a reverse proxy with TLS and a token.

Once, by an admin, with the group id from `GET <url>/v1/groups/<number>` (`group.…`):

```bash
gh secret set SIGNAL_API_URL             # https://signal.example.org
gh secret set SIGNAL_NUMBER              # the sending account, +43…
gh secret set SIGNAL_GROUP_ID            # group.…
gh secret set SIGNAL_API_AUTHORIZATION   # optional: "Bearer …" for the proxy
```

Without the first three secrets the job only leaves a notice. Preview a message without
sending: `GITHUB_REPOSITORY=ArchiCraftApp/ArchiCraft BEFORE=<sha> AFTER=<sha>
scripts/announce-signal.sh --dry-run`. To test the setup, start the workflow by hand
(`gh workflow run announce.yml`); it announces the newest commit on `main` again.
