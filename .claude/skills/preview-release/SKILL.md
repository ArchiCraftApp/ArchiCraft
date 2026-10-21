---
name: preview-release
description: Publish an in-between (preview) release of ArchiCraft on GitHub — a macOS DMG, a Linux tarball and a Windows zip of one pushed commit, as a pre-release — by starting .github/workflows/preview-release.yml and reporting the release link. Use when the user asks for a preview, in-between, test or nightly-style build or release, for installers or executables of the current state for testers, or invokes /preview-release.
argument-hint: "[branch or commit] [notes for testers]"
---

# Publishing a preview release

`.github/workflows/preview-release.yml` builds the commit at the tip of a branch on macOS
(Apple silicon), Linux and Windows (x86_64) and publishes the three packages with
`SHA256SUMS.txt` as a GitHub pre-release tagged `preview-<version>-<yyyymmdd>-<sha7>`. It is
never marked as the latest release and starts none of the `v*` release workflows. Every package
takes its icon from `ArchiCraft_Logo.png` in the repository root. The repository is public: a
preview is visible to everyone, so start one only when the user asked for it.

Arguments (`$ARGUMENTS`): optionally a branch (default `main`), then optionally the notes for
testers. Do not invent notes: GitHub lists the merged pull requests since the previous preview
on its own.

## Steps

1. **The commit.** `git fetch --quiet origin` and resolve the branch on the remote:
   `git rev-parse origin/<branch>`. The workflow builds the remote tip, never local work. When
   the user means the branch of this worktree and it has commits that are not pushed
   (`git log origin/<branch>..HEAD`), say so and stop: pushing is the user's call (CLAUDE.md,
   conservative profile).
2. **CI on that commit.**
   `gh run list --workflow ci.yml --commit <sha> --limit 1 --json status,conclusion,url`.
   - `completed` / `success`: go on.
   - still running: say so with the run's link, watch it in the background
     (`gh run watch <id> --exit-status`) and go on when it passes.
   - failed or no run: stop and report it. Only when the user says to publish it anyway, add
     `-f allow_untested=true` below; the release then carries a CI warning in its run.
3. **Start the workflow.** GitHub starts it only once the workflow file is on `main`; a run
   uses the chosen branch's copy of it:

   ```bash
   gh workflow run preview-release.yml --ref <branch> -f notes="<notes or empty>"
   ```

4. **Find the run.** `gh run list --workflow preview-release.yml --event workflow_dispatch
   --limit 3 --json databaseId,headSha,status,url,createdAt` and take the newest run whose
   `headSha` is the commit of step 1 (it can take a few seconds to appear; list again).
   Tell the user in one line: the commit, the run's link, and that it takes about 20 to 40
   minutes (a cold release build; macOS runners can be slow to start).
5. **Wait without polling.** Run `gh run watch <id> --exit-status >/dev/null` with
   `run_in_background` and end the turn; you are notified when it exits.
6. **Report.**
   - Success: `gh release view preview-<id> --json url,assets --jq '.url, .assets[].name'`,
     and give the user the release link and the package names in two or three lines.
   - Failure: `gh run view <id> --log-failed | tail -n 80`, name the failed job and the error
     in a few lines. A failure in `Name and CI status` is usually a preview of the same commit
     from today or a CI run that did not pass; one in `Package (…)` is a build or packaging
     error of that OS (`scripts/package-macos.sh`, `scripts/package-linux.sh`,
     `scripts/package-windows.sh`). Propose the fix; do not retry blindly.

## Cleaning up

`gh release list --limit 50 | grep preview-` lists the previews. Delete one only when the user
asks for that release by name or says to remove old previews, and confirm the list first:
`gh release delete <tag> --cleanup-tag --yes` removes the release and its tag for good.
