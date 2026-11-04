#!/usr/bin/env bash
# The free disk space of a Linux CI job (archicraft-r8sta): prints `df -h`, the sizes of target/
# and ~/.cargo, and a line "<label>: <n> GiB free" in the job summary. Below a margin of
# DISK_MARGIN_GIB (default 5) free on the file system of the checkout it adds a warning to the
# run, so a build that grows toward the runner's limit shows before a job runs out of space.
# Usage: scripts/ci-disk-space.sh <label>   (for example "after the build")
set -euo pipefail

label="${1:?usage: scripts/ci-disk-space.sh <label>}"
margin="${DISK_MARGIN_GIB:-5}"

df -h
du -sh target "$HOME/.cargo" 2>/dev/null || true

free="$(df -BM --output=avail . | tail -n 1 | tr -dc '0-9')"
free_gib="$(awk -v m="$free" 'BEGIN { printf "%.1f", m / 1024 }')"
line="Disk space $label: $free_gib GiB free on $(df --output=target . | tail -n 1)"
echo "$line"
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
    echo "$line" >>"$GITHUB_STEP_SUMMARY"
fi
if [ "$free" -lt "$((margin * 1024))" ]; then
    echo "::warning title=Disk space::Only $free_gib GiB free $label, below the margin of $margin GiB the job keeps (scripts/ci-disk-space.sh)."
fi
