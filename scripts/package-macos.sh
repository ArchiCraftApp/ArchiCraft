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
    echo "package-macos.sh needs macOS (hdiutil, iconutil, codesign)" >&2
    exit 1
fi

cargo build --release --locked -p archicraft-app --bin archicraft

bin=target/release/archicraft
icon=ArchiCraft_Logo.png
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

rm -rf "$stage" "$out/AppIcon.iconset" "$rw" "$dmg"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$out/AppIcon.iconset"
cp "$bin" "$app/Contents/MacOS/archicraft"
cp LICENSE "$app/Contents/Resources/LICENSE"

# Every size iconutil expects, scaled down from the logo in the repository root
# (rendered from ArchiCraft_Logo.svg at 1024 px, the largest size, so none is upscaled).
logo_size=$(sips -g pixelWidth -g pixelHeight "$icon" | awk '/pixel(Width|Height)/ { print $2 }' | sort -n | head -n 1)
if [ "$logo_size" -lt 1024 ]; then
    echo "$icon is smaller than 1024 px; render it from ArchiCraft_Logo.svg (see README)" >&2
    exit 1
fi
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$icon" --out "$out/AppIcon.iconset/icon_${size}x${size}.png" >/dev/null
    sips -z $((size * 2)) $((size * 2)) "$icon" --out "$out/AppIcon.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$out/AppIcon.iconset" -o "$app/Contents/Resources/AppIcon.icns"
rm -rf "$out/AppIcon.iconset"

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
    <key>CFBundleIdentifier</key><string>com.graphicraft.archicraft</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleName</key><string>ArchiCraft</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.graphics-design</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
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
