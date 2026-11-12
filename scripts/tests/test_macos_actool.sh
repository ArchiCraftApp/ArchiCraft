#!/bin/sh
# Self-test of the choice of actool in scripts/package-macos.sh (scripts/macos-actool.sh, which
# that script sources) against fake Xcodes: the selected Xcode's actool when it is 26 or newer,
# otherwise the newest Xcode*.app whose actool is 26 or newer, otherwise an error naming
# Xcode 26. Needs no Xcode and runs on Linux; scripts/guards-selftest.sh calls it in CI.
# Usage: scripts/tests/test_macos_actool.sh
set -eu
root="$(git rev-parse --show-toplevel)"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
failures=0

# fake_xcode <dir> <name> <version>: <dir>/<name>.app with an actool whose --version prints
# the plist of Xcode's actool with short-bundle-version <version>. Prints the actool's path.
fake_xcode() {
    tool="$1/$2.app/Contents/Developer/usr/bin/actool"
    mkdir -p "$(dirname "$tool")"
    cat >"$tool" <<EOF
#!/bin/sh
[ "\${1:-}" = --version ] || exit 64
cat <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>com.apple.actool.version</key>
	<dict>
		<key>bundle-version</key>
		<string>24506</string>
		<key>short-bundle-version</key>
		<string>$3</string>
	</dict>
</dict>
</plist>
PLIST
EOF
    chmod +x "$tool"
    echo "$tool"
}

# A fake xcrun prints the selected actool (FAKE_SELECTED_ACTOOL), or fails as xcrun does
# when no Xcode is selected.
mkdir -p "$scratch/bin"
cat >"$scratch/bin/xcrun" <<'EOF'
#!/bin/sh
if [ "$*" = "--find actool" ] && [ -n "${FAKE_SELECTED_ACTOOL:-}" ]; then
    echo "$FAKE_SELECTED_ACTOOL"
    exit 0
fi
echo "xcrun: error: unable to find utility \"actool\", not a developer tool or in PATH" >&2
exit 72
EOF
chmod +x "$scratch/bin/xcrun"

# pick <apps dir> <selected actool or empty>: runs pick_actool of scripts/macos-actool.sh;
# prints its stdout, or "error: <stderr>" when it fails.
pick() {
    (
        PATH="$scratch/bin:$PATH" XCODE_APPS_DIR="$1" FAKE_SELECTED_ACTOOL="$2"
        export PATH XCODE_APPS_DIR FAKE_SELECTED_ACTOOL
        . "$root/scripts/macos-actool.sh"
        if out=$(pick_actool 2>"$scratch/stderr"); then
            echo "$out"
        else
            echo "error: $(cat "$scratch/stderr")"
        fi
    )
}

# expect <description> <wanted> <got>
expect() {
    if [ "$3" = "$2" ]; then
        echo "ok    $1"
    else
        echo "WRONG $1: expected $2, got $3"
        failures=$((failures + 1))
    fi
}

error="error: package-macos.sh needs Xcode 26 or newer: its actool compiles packaging/macos/AppIcon.icon"

apps="$scratch/case1"
selected=$(fake_xcode "$apps" Xcode_26.2 26.2)
fake_xcode "$apps" Xcode_26.3 26.3 >/dev/null
expect "selected 26.2 next to an installed 26.3: the selected one" "$selected" "$(pick "$apps" "$selected")"

# The names sort so that the newest is neither the first nor the last Xcode 26 the glob finds.
apps="$scratch/case2"
fake_xcode "$apps" Xcode_A 16.0 >/dev/null
fake_xcode "$apps" Xcode_B 26.0.1 >/dev/null
newest=$(fake_xcode "$apps" Xcode_C 26.3)
selected=$(fake_xcode "$apps" Xcode_D 16.4)
fake_xcode "$apps" Xcode_E 26.2 >/dev/null
expect "selected 16.4 with 16.0, 16.4, 26.0.1, 26.2 and 26.3 installed: 26.3" "$newest" \
    "$(pick "$apps" "$selected")"

apps="$scratch/case3"
fake_xcode "$apps" Xcode_16.4 16.4 >/dev/null
newest=$(fake_xcode "$apps" Xcode_26.0.1 26.0.1)
expect "none selected, with 16.4 and 26.0.1 installed: 26.0.1" "$newest" "$(pick "$apps" "")"

apps="$scratch/case4"
fake_xcode "$apps" Xcode_16.0 16.0 >/dev/null
selected=$(fake_xcode "$apps" Xcode_16.4 16.4)
expect "selected 16.4 with only 16.0 and 16.4 installed: the error" "$error" "$(pick "$apps" "$selected")"

# The selected Xcode lies outside the scanned folder, so only the selection can pick it.
apps="$scratch/case5"
fake_xcode "$apps" Xcode_26.3 26.3 >/dev/null
selected=$(fake_xcode "$scratch/elsewhere" Xcode-beta 27.2)
expect "selected 27.2 next to an installed 26.3: the selected one" "$selected" "$(pick "$apps" "$selected")"

if [ "$failures" -ne 0 ]; then
    echo "$failures actool choice test(s) failed"
    exit 1
fi
echo "actool choice tests passed"
