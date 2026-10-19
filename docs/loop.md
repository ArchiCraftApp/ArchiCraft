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
| 0 Sweep | Close beads whose PR merged and split parents (label `loop-split`) whose children are done; report closes bd refused, waiting PRs, beads for a human, epics due for a demo. The copy on `origin/main` runs, since a new worktree may be cut from an older `main` | `scripts/loop/sweep.sh` |
| 1 Claim | Claim the bead, branch `bead/<id>-<slug>` from `origin/main`, clone the warm build cache | `scripts/loop/start-bead.sh <id>` |
| 2 Slice | Split into child beads above 480 minutes, above 800 non-test source lines (a top-level `#[cfg(test)] mod tests` does not count), or with separable criteria | `bd create --parent` |
| 3 Tests first | Every acceptance criterion becomes a failing test | |
| 4 Implement | CLAUDE.md conventions; rustfmt runs after every edit | |
| 5 Gate | fmt, clippy, tests, layering, golden files, diff budget, `.beads/`, conformance, licences | `scripts/loop/gate.sh` |
| 6 Review | Fresh-context `bead-reviewer` agent; its verdict becomes the review stamp | `.claude/agents/bead-reviewer.md` |
| 7 Land | Commit `<id>: <summary>`, push, PR, squash auto-merge on green CI | `scripts/loop/land.sh` |
| 8 Close | Bead closes when the PR merges; follow-ups and lessons recorded | `sweep.sh`, `bd remember` |

The gate and the review stamp the exact tree they checked (`loop_tree_hash` in
`scripts/loop/lib.sh`). Any later edit invalidates the gate stamp. The review stamp survives
only edits outside code and the loop's own files (`loop_review_valid`): a trivial doc fix for
a minor finding after a `PASS` needs a new gate run but no new review, and every other minor
finding becomes a follow-up bead instead of another review round. Stamps, logs and counters
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
a time; two only on request and only when they share no crate label.

**Demo checkpoint.** An epic closes only after the user tried it: the orchestrator builds and
launches the app from `main`, lists what the epic delivered, and closes the epic on the user's
OK or files the findings as beads under it.

## Enforcement

Instructions are soft; these are not:

| Where | Rule |
|---|---|
| Hook `guard-edit` (PreToolUse Edit/Write) | Code under `crates/`, `conformance/` and the workspace manifests is edited only on a bead branch whose bead is in progress; golden files are never edited by hand; the reviewer never edits; a feature bead never edits the loop's own files |
| Hook `format-rust` (PostToolUse) | rustfmt on every edited `.rs` file; syntax errors reported back |
| Hook `guard-bash` (PreToolUse Bash) | No force push, no push to `main`, no PR merge without `--auto`, no `--admin`; code commits only on a bead branch with a gate on the exact tree and a review that covers it; stamps cannot be written by hand; the reviewer runs read-only commands |
| Hook `review-verdict` (SubagentStop) | The reviewer's `VERDICT: PASS` stamps the tree, only when the gate passed on it |
| Hook `stop-gate` (Stop) | A bead session does not end with uncommitted code that has not passed the gate and the review; it runs the gate, blocks at most three times per tree, and `pause.sh` is the explicit way out |
| Hook `session-context` (SessionStart) | New, resumed and compacted sessions see the loop state |
| CI job `Loop policy` | Bead PR titles start with the bead id; bead PRs never change `.beads/`; golden files change only with the PR label `format-change`; loop files change only in a bead labelled `loop`; headless layering; the hooks' self-test |
| Branch protection on `main` | Auto-merge waits for all seven CI checks on three operating systems |

The hooks are wired in `.claude/settings.json`; each resolves its script from the git
top level of the session's working directory, so worktree sessions run their own branch's
version. Command-line matching is a guardrail, not a sandbox: CI and branch protection are
the gates that cannot be bypassed.

`guard-edit` sees only the Edit and Write tools. A file changed through Bash (`sed -i`, a
heredoc, a script) bypasses it, on `main` as on a bead branch. Only the stamps catch such a
change locally (a code commit needs a gate on the exact tree and a review that covers it), and
CI catches it in the pull request.

`guard-bash` judges the reviewer's commands and writes to the stamps by the commands a line
runs (`scripts/loop/hooks/shell-commands.awk`), including those in `$(...)`, `find -exec`,
behind wrappers such as `xargs`, `sudo` or `timeout`, in `sh -c` and `eval`, and in scripts
fed to a shell. Other quoted arguments and heredoc bodies are data: a grep pattern, a commit
message or a bead note may name `git merge`, `rm` or the state dir (the state dir only on a
line that writes no file, since a path can reach a write through a variable or a pipe).

Beads labelled `loop` change the loop itself. Their PRs do not auto-merge; the user reviews
them. Every review finding that a script could have caught becomes such a bead, so the gate
gets stricter over time.

## Setup

Once per machine: `brew install gh && gh auth login` (PRs and auto-merge), optionally
`cargo install cargo-deny` (the gate skips the licence check without it; CI runs it).

Once per repository, by an admin: `scripts/loop/setup-github.sh` (auto-merge, labels,
required checks on `main`).

After changing a hook or loop script: `scripts/loop/selftest.sh`.
