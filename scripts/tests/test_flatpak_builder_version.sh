#!/bin/sh
# Self-test of the flatpak-builder version check of scripts/package-flatpak.sh
# (scripts/flatpak-builder-version.sh, which that script sources) against the strings
# `flatpak-builder --version` prints: Ubuntu 24.04's "flatpak-builder-1.4.2" and
# "flatpak-builder 1.4.4" pass, 1.2 fails with the message naming 1.4. Needs no flatpak and
# runs anywhere; scripts/guards-selftest.sh calls it in CI.
# Usage: scripts/tests/test_flatpak_builder_version.sh
set -eu
root="$(git rev-parse --show-toplevel)"
failures=0

# check <expected pass|fail> <version output>: runs check_flatpak_builder in a subshell.
check() {
    if err=$( (. "$root/scripts/flatpak-builder-version.sh" && check_flatpak_builder "$2") 2>&1); then
        got=pass
    else
        got=fail
    fi
    if [ "$got" != "$1" ]; then
        echo "FAIL: '$2': expected $1, got $got ($err)"
        failures=$((failures + 1))
        return
    fi
    if [ "$1" = fail ]; then
        case "$err" in
        *"needs flatpak-builder 1.4 or newer, found: $2"*) ;;
        *)
            echo "FAIL: '$2': rejected without the message naming 1.4: $err"
            failures=$((failures + 1))
            return
            ;;
        esac
    fi
    echo "ok: '$2' -> $1"
}

check pass "flatpak-builder-1.4.2"
check pass "flatpak-builder 1.4.4"
check pass "flatpak-builder 2.0.1"
check fail "flatpak-builder 1.2.2"
check fail "flatpak-builder-1.2.3"
check fail "something else"
check fail ""

if [ "$failures" -ne 0 ]; then
    echo "$failures flatpak-builder version check(s) failed"
    exit 1
fi
echo "flatpak-builder version checks passed"
