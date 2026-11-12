#!/usr/bin/env sh
# Builds ArchiCraft for macOS and packs it into a DMG with ArchiCraft.app and
# a link to /Applications (no installer, SRS D-12).
# Usage: scripts/package-macos.sh
# Output: target/dmg/ArchiCraft-<version>-macos-<arch>.dmg
#
# The app is signed ad hoc unless CODESIGN_IDENTITY names a Developer ID; an
# ad hoc signed app downloaded on another Mac opens only after the user allows
# it in System Settings > Privacy & Security.
set -eu
cd "$(dirname "$0")/.."

if [ "$(uname -s)" != Darwin ]; then
    echo "package-macos.sh needs macOS (hdiutil, codesign, Xcode's actool)" >&2
    exit 1
fi

# The app icon needs actool from Xcode 26 or newer (scripts/macos-actool.sh, tested by
# scripts/tests/test_macos_actool.sh).
. scripts/macos-actool.sh
actool=$(pick_actool) || exit 1

cargo build --release --locked -p archicraft-app --bin archicraft

bin=target/release/archicraft
macos_min=11.0
version=$(cargo pkgid -p archicraft-app | sed 's/.*[#@]//')
commit=$(git describe --always --dirty)
arch=$(lipo -archs "$bin")
out=target/dmg
stage=$out/stage
app=$stage/ArchiCraft.app
rw=$out/ArchiCraft-rw.dmg
dmg=$out/ArchiCraft-$version-macos-$arch.dmg
mnt=$(mktemp -d)
trap 'hdiutil detach -quiet "$mnt" 2>/dev/null || true; rmdir "$mnt" 2>/dev/null || true' EXIT

rm -rf "$stage" "$rw" "$dmg"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin" "$app/Contents/MacOS/archicraft"
cp LICENSE "$app/Contents/Resources/LICENSE"
cp THIRD-PARTY.md "$app/Contents/Resources/THIRD-PARTY.md"

# Assets.car holds the icon that macOS 26 and newer draw in every appearance (light, dark,
# clear, tinted); an .icns alone they only mask, or shrink onto a grey plate when its shape is
# not theirs. AppIcon.icns has every size, masked and with Apple's margins, for macOS 11 to 15.
# actool wants an absolute path.
DEVELOPER_DIR=${actool%/usr/bin/actool} "$actool" "$PWD/packaging/macos/AppIcon.icon" \
    --compile "$app/Contents/Resources" --app-icon AppIcon --standalone-icon-behavior all \
    --platform macosx --target-device mac --minimum-deployment-target "$macos_min" \
    --enable-on-demand-resources NO --development-region en \
    --output-partial-info-plist "$out/AppIcon.plist" --output-format human-readable-text \
    --errors --warnings >&2
rm -f "$out/AppIcon.plist"
for file in Assets.car AppIcon.icns; do
    if [ ! -f "$app/Contents/Resources/$file" ]; then
        echo "actool did not write $file from packaging/macos/AppIcon.icon" >&2
        exit 1
    fi
done

# No CFBundleDocumentTypes: the .ifcx association is made with the user's
# consent at first start (SRS SW-001), not by installing the bundle.
cat >"$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>ArchiCraftCommit</key><string>$commit</string>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleDisplayName</key><string>ArchiCraft</string>
    <key>CFBundleExecutable</key><string>archicraft</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundleIconName</key><string>AppIcon</string>
    <key>CFBundleIdentifier</key><string>com.graphicraft.archicraft</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleName</key><string>ArchiCraft</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.graphics-design</string>
    <key>LSMinimumSystemVersion</key><string>$macos_min</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSPrincipalClass</key><string>NSApplication</string>
    <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict>
</plist>
EOF
plutil -lint -s "$app/Contents/Info.plist"

if [ -n "${CODESIGN_IDENTITY:-}" ]; then
    codesign --force --options runtime --timestamp --sign "$CODESIGN_IDENTITY" "$app"
else
    codesign --force --sign - "$app"
fi
codesign --verify --strict "$app"

# The volume shows the app icon too: .VolumeIcon.icns plus the Finder flag
# kHasCustomIcon on the volume root, so the files go into a blank writable
# image first. (hdiutil create -srcfolder mounts a temporary volume under
# /Volumes, which Spotlight can keep busy so that it fails to unmount.)
cp "$app/Contents/Resources/AppIcon.icns" "$stage/.VolumeIcon.icns"
ln -s /Applications "$stage/Applications"
size=$(du -sm "$stage" | cut -f1)
hdiutil create -quiet -size $((size + 16))m -fs HFS+ -volname "ArchiCraft $version" "$rw"
hdiutil attach -quiet -nobrowse -noautoopen -mountpoint "$mnt" "$rw"
ditto "$stage" "$mnt"
xattr -wx com.apple.FinderInfo 0000000000000000040000000000000000000000000000000000000000000000 "$mnt"
hdiutil detach -quiet "$mnt" || hdiutil detach -quiet -force "$mnt"
hdiutil convert -quiet "$rw" -format UDZO -imagekey zlib-level=9 -o "$dmg"
rm -f "$rw"
hdiutil verify -quiet "$dmg"

echo "$dmg"
