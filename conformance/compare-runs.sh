#!/usr/bin/env sh
# Compares the output of conformance/run.sh on several platforms (IFX-006 on the platforms of
# NFR-PORT-001..003, .github/workflows/conformance-matrix.yml). The same inputs must give the
# same PASS lines everywhere. Lines are compared as sorted sets: Playwright prints them in the
# order its workers finish, which differs from run to run.
# Fails when a log has no result lines, when a log has a FAIL line or when two logs differ; the
# differences are shown against the first log.
# Usage: conformance/compare-runs.sh <run.sh output> <run.sh output>...
set -eu
[ $# -ge 2 ] || { echo "usage: compare-runs.sh <run.sh output> <run.sh output>..." >&2; exit 2; }
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
# The PASS and FAIL lines of one run, without carriage returns, sorted byte-wise.
results() {
    tr -d '\r' <"$1" | grep -E '^(PASS|FAIL) ' | LC_ALL=C sort || true
}
status=0
results "$1" >"$tmp/first"
for log in "$@"; do
    results "$log" >"$tmp/this"
    if [ ! -s "$tmp/this" ]; then
        echo "$log: no PASS or FAIL lines"
        status=1
        continue
    fi
    if grep '^FAIL ' "$tmp/this" >"$tmp/failed"; then
        echo "$log: $(wc -l <"$tmp/failed" | tr -d ' ') failed steps"
        sed 's/^/  /' "$tmp/failed"
        status=1
    fi
    if ! diff -u --label "$1" --label "$log" "$tmp/first" "$tmp/this"; then
        status=1
    fi
done
[ "$status" -eq 0 ] && echo "$# runs print the same $(wc -l <"$tmp/first" | tr -d ' ') PASS lines"
exit "$status"
