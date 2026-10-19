#!/usr/bin/env bash
# Fails when a headless crate (model, geometry, IFCX) depends on UI or windowing crates or on
# the app crate, directly or transitively (CLAUDE.md conventions, technology-choices.md).
# Usage: scripts/loop/check-layering.sh
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

forbidden='^(archicraft-app|eframe|egui|egui-wgpu|egui-winit|egui_extras|egui_kittest|epaint|wgpu|winit|rfd)$'
status=0
for crate in archicraft-model archicraft-geometry archicraft-ifcx; do
    hits="$(cargo tree --locked -p "$crate" -e normal,build --prefix none --format '{p}' |
        awk '{ print $1 }' | sort -u | grep -E "$forbidden" || true)"
    if [ -n "$hits" ]; then
        echo "$crate depends on UI crates: $(echo "$hits" | tr '\n' ' ')"
        status=1
    fi
done
[ "$status" -eq 0 ] && echo "headless crates have no UI dependencies"
exit "$status"
