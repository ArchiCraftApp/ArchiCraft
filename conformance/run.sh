#!/usr/bin/env sh
# Runs the IFX-006 conformance check on IFCX files.
# Usage: conformance/run.sh <file.ifcx>...
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
# Make the file arguments absolute before changing directory.
files=""
for f in "$@"; do
    files="$files $(cd "$(dirname "$f")" && pwd)/$(basename "$f")"
done
# buildingSMART IFC5-development at the commit SRS R2 pins.
IFC5_COMMIT=1a63082ada967c683cfacee2005f8f749c8e1b79
if [ ! -d "$here/.ifc5/.git" ]; then
    git clone --quiet --filter=blob:none --no-checkout https://github.com/buildingSMART/IFC5-development.git "$here/.ifc5"
fi
git -C "$here/.ifc5" checkout --quiet "$IFC5_COMMIT"
cd "$here"
[ -d node_modules ] || npm ci --no-audit --no-fund --silent
npm run --silent build
# shellcheck disable=SC2086 # paths of generated test files contain no spaces
node .build/check.mjs "$here/../crates/archicraft-ifcx/schemas" $files
