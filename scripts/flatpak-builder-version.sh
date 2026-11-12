# Sourced by scripts/package-flatpak.sh and scripts/tests/test_flatpak_builder_version.sh: the
# check that flatpak-builder is 1.4 or newer.
#
# flatpak-builder 1.2 (Ubuntu 22.04) runs appstream-compose inside the SDK, which SDK 26.08 no
# longer has, and fails only after the build. `flatpak-builder --version` prints
# "flatpak-builder-1.4.2" (Ubuntu 24.04's 1.4.2) or "flatpak-builder 1.4.4".

# check_flatpak_builder <version output>: succeeds when the output of
# `flatpak-builder --version` names 1.4 or newer, otherwise prints an error naming the output
# and fails.
check_flatpak_builder() {
    builder_version=$(printf '%s\n' "$1" |
        sed -n 's/^flatpak-builder[- ]\([0-9][0-9]*\.[0-9][0-9]*\).*/\1/p' | head -n 1)
    builder_major=${builder_version%%.*}
    builder_minor=${builder_version#*.}
    if [ -z "$builder_version" ] || [ "$builder_major" -lt 1 ] ||
        { [ "$builder_major" -eq 1 ] && [ "$builder_minor" -lt 4 ]; }; then
        echo "package-flatpak.sh needs flatpak-builder 1.4 or newer, found: $1 (on Debian 13 or Ubuntu 24.04 or later: apt install flatpak-builder)" >&2
        return 1
    fi
}
