#!/usr/bin/env bash
# One-time GitHub setup for the bead loop (docs/loop.md), run by a repository admin:
# - squash auto-merge on, merged branches deleted, squash commits titled after the PR;
# - the labels the loop uses (format-change, large-change, loop);
# - main protected by the CI checks, so auto-merge waits for green CI on Linux and Windows.
#   Admins are not bound by it (enforce_admins false), so the owner can still repair main.
#   No macOS check is required: macOS runs only for releases (.github/workflows/macos.yml),
#   because GitHub's hosted macOS runners were too slow or never picked the jobs up
#   (2026-10-19) and no PR could merge (docs/loop.md).
# Usage: scripts/loop/setup-github.sh
set -euo pipefail
command -v gh >/dev/null || { echo "gh is not installed: brew install gh && gh auth login"; exit 1; }
repo="$(gh repo view --json nameWithOwner -q .nameWithOwner)"
echo "Configuring $repo"

gh api -X PATCH "repos/$repo" \
    -F allow_auto_merge=true -F allow_squash_merge=true -F delete_branch_on_merge=true \
    -f squash_merge_commit_title=PR_TITLE -f squash_merge_commit_message=PR_BODY >/dev/null
echo "  auto-merge and squash merging on"

gh label create format-change --force --color d93f0b --description "Intended IFCX format change: golden files regenerated" >/dev/null
gh label create large-change --force --color fbca04 --description "Mechanical change above the bead diff budget" >/dev/null
gh label create loop --force --color 5319e7 --description "Changes the bead loop's own guardrails; reviewed by a human" >/dev/null
echo "  labels format-change, large-change, loop"

# The check names are the CI job names in .github/workflows/ci.yml.
gh api -X PUT "repos/$repo/branches/main/protection" --input - >/dev/null <<'EOF'
{
  "required_status_checks": {
    "strict": false,
    "contexts": [
      "Test (ubuntu-24.04)",
      "Test (windows-latest)",
      "Format and clippy",
      "IFCX conformance (buildingSMART reference code)",
      "Licences and sources",
      "Loop policy",
      "Repository guards"
    ]
  },
  "enforce_admins": false,
  "required_pull_request_reviews": null,
  "restrictions": null,
  "allow_force_pushes": false,
  "allow_deletions": false
}
EOF
echo "  main requires the six CI checks"
