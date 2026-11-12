# Sourced by scripts/package-macos.sh and scripts/tests/test_macos_actool.sh: the choice of
# Xcode's actool for the app icon.
#
# The app icon is the Icon Composer document packaging/macos/AppIcon.icon (the gradient and
# the mark of ArchiCraft_Logo.svg), which only actool from Xcode 26 or newer compiles: the
# selected Xcode's (xcrun --find actool), otherwise the newest Xcode*.app in XCODE_APPS_DIR
# (default /Applications; GitHub's macOS 15 runners keep Xcode 26 next to an older default).

# actool_version <actool>: prints the version of actool --version as one number, 26.0.1 as
# 260001, or nothing when it prints no short-bundle-version.
actool_version() {
    "$1" --version 2>/dev/null | awk '/short-bundle-version/ {
        getline; gsub(/[^0-9.]/, ""); split($0, v, "."); print v[1] * 10000 + v[2] * 100 + v[3]; exit
    }'
}

# pick_actool: prints the path of the actool to use, or prints an error naming Xcode 26 to
# stderr and returns 1 when no actool is 26 or newer.
pick_actool() {
    selected=$(xcrun --find actool 2>/dev/null || true)
    actool=
    best=0
    for tool in "$selected" "${XCODE_APPS_DIR:-/Applications}"/Xcode*.app/Contents/Developer/usr/bin/actool; do
        [ -x "$tool" ] || continue
        version=$(actool_version "$tool")
        [ "${version:-0}" -ge 260000 ] || continue
        if [ "$tool" = "$selected" ]; then
            actool=$tool
            break
        fi
        if [ "$version" -gt "$best" ]; then
            actool=$tool
            best=$version
        fi
    done
    if [ -z "$actool" ]; then
        echo "package-macos.sh needs Xcode 26 or newer: its actool compiles packaging/macos/AppIcon.icon" >&2
        return 1
    fi
    echo "$actool"
}
