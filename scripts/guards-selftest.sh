#!/bin/sh
# Negative self-test of the repository guards (docs/testing.md): each guard must fail on a
# canned violation, so a guard that silently stops checking turns CI red.
# - scripts/check-layering.sh on canned `cargo tree` output (NFR-MNT-002, NFR-MNT-004)
# - scripts/check-legal.sh in a scratch repository with a trademark line and tracked
#   library-part, image and SVG files outside the allowlists (NFR-LEG-002)
# Usage: scripts/guards-selftest.sh
set -eu
root="$(git rev-parse --show-toplevel)"
data="$root/scripts/testdata"
failures=0

# expect <pass|fail> <description> <command...>
expect() {
    want="$1" what="$2"
    shift 2
    if "$@" >/dev/null 2>&1; then got=pass; else got=fail; fi
    if [ "$got" = "$want" ]; then
        echo "ok    $what ($got)"
    else
        echo "WRONG $what: expected $want, got $got"
        failures=$((failures + 1))
    fi
}

layering="$root/scripts/check-layering.sh"
expect pass "layering: clean geometry tree" "$layering" --from-file "$data/layering-clean.txt"
expect fail "layering: egui under archicraft-model" "$layering" --from-file "$data/layering-model-egui.txt"
expect fail "layering: archicraft-ifcx under archicraft-geometry" \
    "$layering" --from-file "$data/layering-geometry-ifcx.txt"

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
git init -q "$scratch"
mkdir -p "$scratch/scripts" "$scratch/docs"
cp "$root/scripts/check-legal.sh" "$root/scripts/legal-allowlist.txt" \
    "$root/scripts/asset-allowlist.txt" "$scratch/scripts/"
echo "Exports IFCX for any viewer." >"$scratch/docs/guide.md"
git -C "$scratch" add -A
legal() { (cd "$scratch" && sh scripts/check-legal.sh); }
expect pass "legal: scratch repository without violations" legal

# The trademark is assembled at run time so this file does not trip the check itself.
mark="Archi""cad"
echo "Opens files like $mark does." >>"$scratch/docs/guide.md"
git -C "$scratch" add -A
expect fail "legal: trademark line outside the allowlist" legal
echo "Exports IFCX for any viewer." >"$scratch/docs/guide.md"
git -C "$scratch" add -A
expect pass "legal: trademark line removed again" legal

for asset in library/door.gsm assets/screenshot.png assets/icons/copied.svg; do
    mkdir -p "$scratch/$(dirname "$asset")"
    : >"$scratch/$asset"
    git -C "$scratch" add -A
    expect fail "legal: tracked $asset outside the asset allowlist" legal
    git -C "$scratch" rm -q --cached "$asset"
    rm "$scratch/$asset"
done

if [ "$failures" -ne 0 ]; then
    echo "$failures guard self-test(s) failed"
    exit 1
fi
echo "guard self-tests passed"
