#!/usr/bin/env bash
# The beads /bead-loop may start together (docs/loop.md, "Several beads"): the candidates, in the
# order the orchestrator would take them, each taken unless it shares a crate label or a
# requirement row with a bead taken before it or with a bead in progress (archicraft-i2yfz).
# - Crate labels: archicraft-model, archicraft-geometry, archicraft-ifcx, archicraft-app.
# - Requirement rows: the IDs of a bead's SRS: notes line that have a row in
#   docs/status/requirements.json. land.sh lists them on the PR's SRS: line and merge-check makes
#   the PR change each of those rows, so two beads that share one conflict in that row.
# - In progress: a bead stays in progress until sweep.sh sees its PR merge, so an open PR blocks
#   too (user decision of 2026-11-10). Split parents (label loop-split) and epics hold no change
#   of their own and block nothing. The candidates themselves do not count as in progress.
# --first <id> takes that bead before the others whatever it shares: a red-main bead starts
# first in its batch, and the beads after it keep clear of its crates and rows.
# Usage: scripts/loop/batch.sh [--limit <n>] [--first <id>] <id>...
# Prints, in order, "take <id>" for each bead taken, at most <n> of them, and
# "skip <id>: shares <crate|row> <name> with <bead>[ (in progress)][, …]" for each bead left out,
# until the <n>th take.
set -uo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
cd "$(loop_root)" || exit 2

usage() { echo "Usage: scripts/loop/batch.sh [--limit <n>] [--first <id>] <id>..." >&2; exit 2; }
limit=0 first=""
while [ "$#" -gt 0 ]; do
    case "$1" in
    --limit) [[ "${2:-}" =~ ^[0-9]+$ ]] || usage; limit="$2"; shift 2 ;;
    --first) [ -n "${2:-}" ] || usage; first="$2"; shift 2 ;;
    --*) usage ;;
    *) break ;;
    esac
done
ids=()
[ -n "$first" ] && ids+=("$first")
for id in "$@"; do [ "$id" = "$first" ] || ids+=("$id"); done
[ "${#ids[@]}" -gt 0 ] || usage

rows="$(mktemp "${TMPDIR:-/tmp}/loop-rows.XXXXXX")"
trap 'rm -f "$rows"' EXIT
jq -c '[.requirements[]?.id]' docs/status/requirements.json >"$rows" 2>/dev/null || echo '[]' >"$rows"

# A bead's claims (the bead as JSON on stdin) as a JSON array of "crate <label>" and "row <ID>".
claims() {
    local bead srs
    bead="$(cat)"
    srs="$(jq -r '.notes // ""' <<<"$bead" | loop_srs_ids)"
    jq -c --arg srs "$srs" --slurpfile rows "$rows" '
        [((.labels // [])[] | select(test("^archicraft-(model|geometry|ifcx|app)$")) | "crate " + .),
         ($srs | split(", ")[] | select(. != "" and IN($rows[0][])) | "row " + .)]' <<<"$bead"
}

held="$(bd list --status in_progress --limit 0 --json 2>/dev/null)" && jq -e 'type == "array"' <<<"$held" >/dev/null ||
    { echo "batch: bd list --status in_progress failed" >&2; exit 2; }
wanted="$(IFS=,; echo "${ids[*]}")"
candidates="$(bd list --id "$wanted" --limit 0 --json 2>/dev/null)" && jq -e 'type == "array"' <<<"$candidates" >/dev/null ||
    { echo "batch: bd list --id $wanted failed" >&2; exit 2; }

{
    jq -c --arg wanted "$wanted" '.[] | select(.issue_type != "epic" and ((.labels // []) | index("loop-split") | not)
        and (.id | IN($wanted | split(",")[]) | not))' <<<"$held" |
        while IFS= read -r bead; do
            jq -c --argjson c "$(claims <<<"$bead")" '{id, claims: $c, held: true}' <<<"$bead"
        done
    for id in "${ids[@]}"; do
        bead="$(jq -c --arg id "$id" 'first(.[] | select(.id == $id)) // empty' <<<"$candidates")"
        if [ -z "$bead" ]; then
            jq -nc --arg id "$id" '{id: $id, missing: true}'
        else
            jq -c --argjson c "$(claims <<<"$bead")" --arg first "$first" '{id, claims: $c, first: (.id == $first)}' <<<"$bead"
        fi
    done
} | jq -rs --argjson limit "$limit" '
    def own($b; $who): .owner = (($b.claims | map({key: ., value: $who}) | from_entries) + .owner);
    reduce .[] as $b ({owner: {}, out: [], taken: 0};
        if $limit > 0 and .taken >= $limit then .
        elif $b.held then own($b; "\($b.id) (in progress)")
        elif $b.missing then .out += ["skip \($b.id): no open bead with this id"]
        else
            [$b.claims[] as $c | select(.owner[$c]) | "\($c) with \(.owner[$c])"] as $shared
            | if ($shared | length) > 0 and ($b.first | not) then .out += ["skip \($b.id): shares \($shared | join(", "))"]
              else own($b; $b.id) | .out += ["take \($b.id)"] | .taken += 1
              end
        end)
    | .out[]'
