#!/usr/bin/env bash
# The gate's layering step: headless crates (model, geometry, IFCX) have no UI or windowing
# dependencies, and model and geometry do not depend on the IFCX crate. The check itself is
# the repository guard scripts/check-layering.sh (docs/testing.md).
# Usage: scripts/loop/check-layering.sh
set -euo pipefail
exec "$(dirname "${BASH_SOURCE[0]}")/../check-layering.sh"
