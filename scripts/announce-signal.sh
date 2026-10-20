#!/usr/bin/env bash
# Posts a short announcement of the changes that reached main to a Signal group, through a
# self-hosted signal-cli-rest-api (https://github.com/bbernhard/signal-cli-rest-api).
# Run by .github/workflows/announce.yml on every push to main; one message per push, naming each
# merged PR by its title (bead id left out) and link. Setup: docs/loop.md, Announcements.
#
# Environment:
#   BEFORE, AFTER              the pushed range; BEFORE empty or all zeros announces AFTER alone
#   GITHUB_REPOSITORY          owner/name, for the PR lookup and the links
#   GH_TOKEN                   token for gh api (the workflow's GITHUB_TOKEN)
#   SIGNAL_API_URL             base URL of the REST API, for example https://signal.example.org
#   SIGNAL_NUMBER              the sending account, as registered in signal-cli (+43...)
#   SIGNAL_GROUP_ID            the group as the REST API names it (group.<base64>, GET /v1/groups/<number>)
#   SIGNAL_API_AUTHORIZATION   optional Authorization header value for a proxy in front of the API
# Usage: scripts/announce-signal.sh [--dry-run]   (--dry-run prints the message, sends nothing)
set -euo pipefail

dry_run=false
[ "${1:-}" = --dry-run ] && dry_run=true
: "${AFTER:?}" "${GITHUB_REPOSITORY:?}"
before="${BEFORE:-}"

if ! $dry_run && { [ -z "${SIGNAL_API_URL:-}" ] || [ -z "${SIGNAL_NUMBER:-}" ] || [ -z "${SIGNAL_GROUP_ID:-}" ]; }; then
    echo "::notice::No Signal announcement: set the secrets SIGNAL_API_URL, SIGNAL_NUMBER and SIGNAL_GROUP_ID (docs/loop.md)."
    exit 0
fi

# The commits main gained, oldest first; first parent only, so a merge commit stands for its PR.
if [ -z "$before" ] || [[ "$before" =~ ^0+$ ]] || ! git cat-file -e "$before^{commit}" 2>/dev/null; then
    commits="$AFTER"
else
    commits="$(git rev-list --first-parent --reverse "$before..$AFTER")"
fi

lines=()
for sha in $commits; do
    pr="$(gh api "repos/$GITHUB_REPOSITORY/commits/$sha/pulls" \
        -q '[.[] | select(.merged_at != null and .base.ref == "main")][0] // empty | [.title, .html_url] | @tsv' 2>/dev/null || true)"
    if [ -n "$pr" ]; then
        title="${pr%%$'\t'*}"
        url="${pr#*$'\t'}"
    else
        title="$(git log -1 --format=%s "$sha")"
        url="https://github.com/$GITHUB_REPOSITORY/commit/$sha"
    fi
    # Bead PR titles start with the bead id ("archicraft-48fz: "); the group reads the summary.
    title="$(sed -E 's/^[a-z0-9]+-[a-z0-9]+(\.[0-9]+)*: //; s/ \(#[0-9]+\)$//' <<<"$title")"
    lines+=("$title"$'\n'"$url")
done
[ "${#lines[@]}" -gt 0 ] || { echo "Nothing new on main."; exit 0; }

repo_name="${GITHUB_REPOSITORY#*/}"
if [ "${#lines[@]}" -eq 1 ]; then
    message="New in $repo_name: ${lines[0]}"
else
    message="New in $repo_name (${#lines[@]} changes):"
    for l in "${lines[@]}"; do message+=$'\n\n'"• $l"; done
fi

if $dry_run; then
    printf '%s\n' "$message"
    exit 0
fi

payload="$(jq -n --arg m "$message" --arg n "$SIGNAL_NUMBER" --arg g "$SIGNAL_GROUP_ID" \
    '{message: $m, number: $n, recipients: [$g]}')"
auth=()
[ -n "${SIGNAL_API_AUTHORIZATION:-}" ] && auth=(-H "Authorization: $SIGNAL_API_AUTHORIZATION")
curl --fail-with-body --silent --show-error --max-time 30 --retry 2 \
    -X POST "${SIGNAL_API_URL%/}/v2/send" -H 'Content-Type: application/json' ${auth[@]+"${auth[@]}"} \
    --data "$payload"
echo
echo "Announced ${#lines[@]} change(s) in the Signal group."
