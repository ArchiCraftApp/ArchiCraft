#!/usr/bin/env sh
# Runs the IFX-006 conformance check on IFCX files, in three steps per file:
#   (a)  buildingSMART's reference implementation loads it with schema validation (check.ts);
#   (b1) the reference viewer's composition (compose3) gives the expected spatial tree, in Node;
#   (b2) the reference viewer itself, in headless Chromium, shows that tree, renders the
#        geometry and every material colour (viewer.spec.ts; screenshots in test-results/).
# Prints PASS or FAIL per file and step; exits non-zero if any step failed. The lines name each
# file by its repository path with forward slashes, so runs on Linux, macOS and Windows (Git
# Bash) print the same lines for the same inputs (compare-runs.sh).
# Usage: conformance/run.sh <file.ifcx>...
set -eu
# The absolute path of a directory. Git Bash's pwd prints /c/...; Node on Windows needs C:/...,
# which pwd -W prints there (elsewhere -W is not an option and pwd prints the path as it is).
absdir() {
    (cd "$1" && { pwd -W 2>/dev/null || pwd; })
}
here="$(absdir "$(dirname "$0")")"
# Make the file arguments absolute before changing directory.
files=""
for f in "$@"; do
    files="$files $(absdir "$(dirname "$f")")/$(basename "$f")"
done
# buildingSMART IFC5-development at the commit SRS R2 pins.
IFC5_COMMIT=1a63082ada967c683cfacee2005f8f749c8e1b79
# Without end-of-line conversion, so a Windows checkout serves the viewer's files as they are.
if [ ! -d "$here/.ifc5/.git" ]; then
    git clone --quiet --config core.autocrlf=false --filter=blob:none --no-checkout \
        https://github.com/buildingSMART/IFC5-development.git "$here/.ifc5"
fi
git -C "$here/.ifc5" checkout --quiet "$IFC5_COMMIT"
cd "$here"
[ -d node_modules ] || npm ci --no-audit --no-fund --silent
npm run --silent build
# A no-op when Chromium is installed already (Linux CI installs it with install-chromium.sh).
npx playwright install chromium >/dev/null

status=0
# shellcheck disable=SC2086 # paths of generated test files contain no spaces
node .build/check.mjs "$here/../crates/archicraft-ifcx/schemas" $files || status=1
CONFORMANCE_FILES="$(printf '%s\n' $files)" npx playwright test || status=1
exit $status
