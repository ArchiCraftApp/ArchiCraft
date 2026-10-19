#!/bin/sh
# Layering guard (docs/testing.md). Fails when a headless crate depends, directly or
# transitively, on a UI or windowing crate (NFR-MNT-002: model, geometry evaluation and IFCX
# serialization run without the UI), or when the model or geometry crate depends on the IFCX
# crate (NFR-MNT-004: a new IFCX version needs no change to the model or geometry crates).
# Normal and build dependencies count; dev-dependencies (egui_kittest in tests) do not.
#
# Usage: scripts/check-layering.sh
#        scripts/check-layering.sh --from-file <cargo-tree-output>...
# --from-file checks canned `cargo tree --prefix none` output instead of the workspace; the
# crate is the root package on the file's first line (negative self-test in CI).
set -eu

ui='^(archicraft-app|eframe|egui|egui-wgpu|egui-winit|egui_extras|egui_kittest|egui_tiles|epaint|wgpu|winit|rfd)$'

# check <crate> <file with one package per line>: prints the violations, fails on any.
check() {
    crate="$1"
    forbidden="$ui"
    case "$crate" in
        archicraft-model | archicraft-geometry) forbidden="$ui|^archicraft-ifcx\$" ;;
    esac
    hits="$(awk 'NR > 1 { print $1 }' "$2" | sort -u | grep -E "$forbidden" || true)"
    if [ -n "$hits" ]; then
        echo "$crate depends on forbidden crates: $(echo "$hits" | tr '\n' ' ')"
        return 1
    fi
    echo "$crate: ok"
}

status=0
if [ "${1:-}" = "--from-file" ]; then
    shift
    [ "$#" -gt 0 ] || { echo "usage: $0 --from-file <cargo-tree-output>..." >&2; exit 2; }
    for file in "$@"; do
        crate="$(awk 'NR == 1 { print $1 }' "$file")"
        check "$crate" "$file" || status=1
    done
    exit "$status"
fi

cd "$(git rev-parse --show-toplevel)"
tree="$(mktemp)"
trap 'rm -f "$tree"' EXIT
for crate in archicraft-model archicraft-geometry archicraft-ifcx; do
    cargo tree --locked -p "$crate" -e normal,build --prefix none --format '{p}' >"$tree"
    check "$crate" "$tree" || status=1
done
exit "$status"
