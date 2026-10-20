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
| 0 Sweep | Close beads whose PR merged and split parents (label `loop-split`) whose children are done; report closes bd refused, waiting PRs, beads for a human, epics due for a demo, a failed newest Windows run on `main` (`ATTENTION`); fast-forward the main checkout to `origin/main` when it is on `main` (git hooks off), or report as `stale` why not. The copy on `origin/main` runs, since a new worktree may be cut from an older `main` | `scripts/loop/sweep.sh` |
| 1 Claim | Claim the bead, branch `bead/<id>-<slug>` from `origin/main`, clone the warm build cache. It re-executes the copy on `origin/main` when the worktree's `scripts/loop` differs | `scripts/loop/start-bead.sh <id>` |
| 2 Slice | Split into child beads above 480 minutes, above 800 non-test source lines (a top-level `#[cfg(test)] mod tests` does not count), or with separable criteria | `bd create --parent` |
| 3 Tests first | Every acceptance criterion becomes a failing test | |
| 4 Implement | CLAUDE.md conventions; rustfmt runs after every edit | |
| 5 Gate | fmt, clippy, tests, layering, golden files, diff budget, `.beads/`, loop guardrails only under label `loop`, conformance, licences | `scripts/loop/gate.sh` |
| 6 Review | Fresh-context `bead-reviewer` agent; its verdict becomes the review stamp | `.claude/agents/bead-reviewer.md` |
| 7 Land | Commit `<id>: <summary>`, push, PR, squash auto-merge on green CI (a PR that changes `.github/workflows/` or `.claude/settings.json` waits for the user) | `scripts/loop/land.sh` |
| 8 Close | Bead closes when the PR merges; follow-ups and lessons recorded | `sweep.sh`, `bd remember` |

The gate and the review stamp the exact tree they checked (`loop_tree_hash` in
`scripts/loop/lib.sh`). Any later edit invalidates the gate stamp. The review stamp belongs to
the branch's bead and survives only edits to prose (`docs/` and top-level Markdown files,
`loop_review_valid`, `LOOP_REVIEW_CARRY_PATHS`; not `CLAUDE.md`, `AGENTS.md` or `docs/SRS.md`,
which instruct agents or define requirements): a trivial doc fix for a minor finding after
a `PASS` needs a new gate run but no new review, and every other minor finding becomes a
follow-up bead instead of another review round. Stamps, logs and counters
live in the worktree's git dir (`git rev-parse --git-path loop`), never in the repository.
`scripts/loop/status.sh` shows where a worktree stands.

**Stop rules.** Three gate failures on the same problem or two failed reviews: the bead gets
the label `human`, the session pauses (`scripts/loop/pause.sh "<reason>"`) and the loop stops.
The same applies when a decision belongs to the user.

## Several beads

`/bead-loop [count] [epic]` (`.claude/skills/bead-loop/SKILL.md`) runs in an orchestrator
session that never edits code. It starts each ready bead in its own fresh session and worktree
with `/bead <id>`, verifies the PR and auto-merge after the session reports, and stops after
`count` beads, at the first blocker, or when an epic's tasks are all closed. Beads run one at
a time; two only on request and only when they share no crate label. A loop bead's PR with
auto-merge on counts like a feature bead's. One that waits for the user (see below) is reported,
and the run carries on with beads not labelled `loop`. No loop bead starts while another loop
bead's PR is open, merging by itself or waiting, because loop beads edit the same files.

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
| Hook `stop-gate` (Stop) | A bead session does not end with uncommitted code that has not passed the gate and the review; it runs the gate, blocks at most three times per tree, and `pause.sh` is the explicit way out |
| Hook `session-context` (SessionStart) | New, resumed and compacted sessions see the loop state |
| CI job `Loop policy` | Bead PR titles start with the bead id; bead PRs never change `.beads/`; golden files change only with the PR label `format-change`; loop files change only in a bead labelled `loop` (labels read live at run time, since `gh pr create` attaches them after the PR is opened); headless layering; the hooks' self-test |
| CI workflow `User review` | A PR that changes `.github/workflows/` or `.claude/settings.json` (or `LOOP_USER_REVIEW_PATHS`) gets the failing status `User review`, decided by the base's `scripts/loop/ci-user-review.sh` (`pull_request_target`), never by the PR's own scripts |
| Branch protection on `main` | Auto-merge waits for the six required CI checks on Linux and the status `User review`; Windows runs on `main`, nightly and by hand, macOS only for releases and the nightly conformance matrix (below) |

**A bead PR runs CI once.** `ci.yml` runs for the `pull_request` event; its `push` trigger
covers only `main` and version tags `v*`, so pushing a bead branch does not start a second run
(before 2026-10-20 every bead PR ran it twice, in two concurrency groups).

**Windows runs on `main` and nightly.** No Windows test job runs on a pull request, and no
Windows job is a required check: the Windows test job took 10 to 17 minutes where Linux took
1.5 to 9, and every bead waited for it (user decision of 2026-10-20). The one Windows job on a
pull request is the Windows leg of the conformance matrix (below), on pull requests that change
the model, geometry, IFCX or conformance code; it is not required either, so no bead waits for
it. The workflow `.github/workflows/windows.yml` builds and tests on `windows-latest`, with
`core.autocrlf=true` as a Windows developer checks out (NFR-PORT-004), on every push to `main`,
nightly and when started by hand (`gh workflow run windows.yml --ref <branch>`). So a
Windows-only failure lands on `main` and shows up after the merge: `sweep.sh` looks at the
newest finished run on `main` that was not cancelled and prints `ATTENTION windows: …` with its
link when it did not pass. `/bead-loop` stops at that line, as at every `ATTENTION`, and the
fix becomes a P0 bead that runs next. Branch protection names its required checks, so a change
to that list in `setup-github.sh` takes effect only when the user runs the script again
(Setup): for the change that dropped the Windows check, before its PR merges, since that PR's
own run no longer reports `Test (windows-latest)`.

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
pull requests that matrix runs Linux and Windows only, and none of its jobs is required. So a
macOS-only conformance failure shows up the next night, and any other macOS-only failure at the
next release at the latest: run the workflow by hand before tagging one, and file a red run as
a bead. When GitHub adds the next macOS major version, move both labels in `macos.yml` up by
one ([actions/runner-images](https://github.com/actions/runner-images#available-images) lists
the current labels).

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
sandbox: CI and branch protection are the gates that cannot be bypassed.

`guard-edit` sees only the Edit and Write tools. A file changed through Bash (`sed -i`, a
heredoc, a script) bypasses it, on `main` as on a bead branch. Only the stamps catch such a
change locally (a code commit needs a gate on the exact tree and a review that covers it), and
CI catches it in the pull request.

`guard-bash` judges all its rules (pushes, PR merges, commits, writes to the stamps, the
reviewer's commands) by the commands a line runs (`scripts/loop/hooks/shell-commands.awk`),
including those in `$(...)`, `find -exec`, behind wrappers such as `xargs`, `sudo` or
`timeout`, in `sh -c` and `eval`, and in scripts fed to a shell; a git command's checks run in
the directory its `-C` options name. Other quoted arguments and heredoc bodies are data: a grep
pattern, a commit message or a bead note may name `git push --force`, `gh pr merge`,
`git commit`, `rm` or the state dir (the state dir only on a line that writes no file, since a
path can reach a write through a variable or a pipe).

Beads labelled `loop` change the loop itself. Every review finding that a script could have
caught becomes such a bead, so the gate gets stricter over time. Since the user's decision of
2026-10-19 their PRs auto-merge like a feature bead's (squash, on green CI, after a passing
review), so the loop runs for hours without the user. The exception is a PR that changes
`.github/workflows/` or `.claude/settings.json` (`LOOP_USER_REVIEW_PATHS` in
`scripts/loop/lib.sh`): they hold the CI gate and the hook wiring, the gates that cannot be
bypassed. `land.sh` leaves auto-merge off for such a PR, also when an earlier run turned it on,
and names the files; `guard-bash` refuses turning it on by hand; the user reviews and merges it.
Both judge by the pattern as `origin/main` defines it as well as the branch's copy, so a branch
that narrows `LOOP_USER_REVIEW_PATHS` still waits for every file the base names, and both also
make a PR wait when it changes the pattern itself (naming `scripts/loop/lib.sh`).
Both still run the branch's own copies, though, so a branch that rewrites `land.sh` or
`lib.sh` could turn auto-merge on for its own workflow change. CI closes that gap
(archicraft-9nlk): `.github/workflows/user-review.yml` runs on `pull_request_target`, so GitHub
takes the workflow and its script `scripts/loop/ci-user-review.sh` from `main`; it checks out
`main`, reads the PR's commits as data and posts the commit status `User review` on the PR's
head, failing when the PR changes a file of `main`'s `LOOP_USER_REVIEW_PATHS` or the pattern
itself. Branch protection requires that status, so auto-merge does not merge such a PR even
when a branch whose scripts skip the rule turned it on; the user merges it after the review as
an admin, past the failing status ("Merge without waiting for requirements", or
`gh pr merge --admin`, which the hooks refuse to agents). Protection names the status by its
context only, so a status of the same name from another source (the PR's own workflow, a
`gh api` call) could still overwrite it; closing that is archicraft-t9zs.
A CODEOWNERS review cannot enforce this while Claude works under the user's GitHub account,
because GitHub does not let an account approve its own PR.

## Setup

Once per machine: `brew install gh && gh auth login` (PRs and auto-merge), optionally
`cargo install cargo-deny` (the gate skips the licence check without it; CI runs it).

Once per repository, by an admin: `scripts/loop/setup-github.sh` (auto-merge, labels,
required checks on `main`). Run it again after a change to its list of required checks. A PR
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
