#!/bin/sh
# Runs one cargo-fuzz target for a while on its corpus (NFR-REL-007, NFR-SEC-003, SRS
# Appendix D.9; docs/testing.md, Fuzzing), with the flags the nightly job
# (.github/workflows/fuzz.yml) uses: a slow input (over 10 s), a memory-hungry one (over
# 2 GB) and a crash are all findings, since a crafted file that stalls the reader or exhausts
# memory is a denial of service. libFuzzer exits non-zero on a finding and writes the input
# to fuzz/artifacts/<target>/. The IFCX targets also get the IFCX dictionary.
# Seed the corpus first with scripts/fuzz-corpus.sh.
# Usage: scripts/fuzz-run.sh <target> [minutes, default 10]
set -eu
cd "$(git rev-parse --show-toplevel)"

[ $# -ge 1 ] || { echo "usage: $0 <target> [minutes]" >&2; exit 2; }
target="$1"
minutes="${2:-10}"
# libFuzzer reads -max_total_time=0 as no limit, so 0 is refused.
case "$minutes" in
    '' | *[!0-9]*) minutes=-1 ;;
esac
[ "$minutes" -ge 1 ] || { echo "minutes must be a whole number above 0, got '${2:-}'" >&2; exit 2; }

set -- -rss_limit_mb=2048 -timeout=10 -max_len=4194304 -print_final_stats=1 \
    -max_total_time=$((minutes * 60))
case "$target" in
    ifcx_*) set -- "$@" -dict=fuzz/dict/ifcx.dict ;;
esac
exec cargo +nightly fuzz run "$target" -- "$@"
