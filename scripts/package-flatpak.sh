#!/usr/bin/env sh
# Builds ArchiCraft for Linux and packs it into a single-file Flatpak bundle with the desktop
# entry, AppStream metainfo and icons, so GNOME Software, KDE Discover or
# `flatpak install --user` install and uninstall it.
# Usage: scripts/package-flatpak.sh
# Output: target/flatpak/ArchiCraft-<version>-linux-<arch>.flatpak
#
# Needs Linux with flatpak, flatpak-builder 1.4 or newer (Ubuntu 24.04 or later: 1.2 runs
# appstream-compose inside the SDK, which SDK 26.08 no longer has) and Python with Pillow
# (python3-pil), which scales ArchiCraft_Logo.png to the icon sizes (Flatpak refuses exported
# icons above 512 px). The binary is built outside the Flatpak build sandbox (that has no
# network for crates) and copied in. flatpak-builder installs the freedesktop runtime and SDK
# from Flathub into the user installation when they are missing; the bundle names Flathub as
# the runtime's source, so installing it fetches the runtime too. The bundle has no update
# remote and nothing is set up at first start beyond what the app itself does (SRS D-12).
set -eu
cd "$(dirname "$0")/.."

app_id=com.graphicraft.archicraft
# The current org.freedesktop.Platform branch on Flathub.
runtime_version=26.08
flathub=https://dl.flathub.org/repo/flathub.flatpakrepo

if [ "$(uname -s)" != Linux ]; then
    echo "package-flatpak.sh needs Linux" >&2
    exit 1
fi
for tool in flatpak flatpak-builder python3; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "package-flatpak.sh needs $tool (on Debian 13 or Ubuntu 24.04 or later: apt install flatpak flatpak-builder python3-pil)" >&2
        exit 1
    fi
done
# flatpak-builder 1.2 cannot build with SDK 26.08 (scripts/flatpak-builder-version.sh, tested by
# scripts/tests/test_flatpak_builder_version.sh).
. scripts/flatpak-builder-version.sh
check_flatpak_builder "$(flatpak-builder --version)" || exit 1
if ! python3 -c 'import PIL' 2>/dev/null; then
    echo "package-flatpak.sh needs Pillow for python3 (on Debian and Ubuntu: apt install python3-pil)" >&2
    exit 1
fi

cargo build --release --locked -p archicraft-app --bin archicraft

version=$(cargo pkgid -p archicraft-app | sed 's/.*[#@]//')
arch=$(uname -m)
out=$(pwd)/target/flatpak
src=$out/src
bundle=$out/ArchiCraft-$version-linux-$arch.flatpak

rm -rf "$src" "$out/build" "$out/repo" "$bundle"
mkdir -p "$src/icons"
cp target/release/archicraft "$src/archicraft"
cp LICENSE "$src/LICENSE"
cp THIRD-PARTY.md "$src/THIRD-PARTY.md"

python3 - ArchiCraft_Logo.png "$src/icons" <<'PY'
import sys
from PIL import Image

logo = Image.open(sys.argv[1]).convert("RGBA")
for size in (512, 256, 128, 64, 48, 32):
    logo.resize((size, size), Image.LANCZOS).save(f"{sys.argv[2]}/{size}.png")
PY

# Exported files carry the app id in their names. Flatpak rewrites Exec into
# `flatpak run ... @@ %f @@`, which forwards the opened file into the sandbox. StartupWMClass
# is the app id the window sets (main.rs). No MimeType: the .ifcx association is made with the
# user's consent at first start (SRS SW-001), not by installing the package.
cat >"$src/$app_id.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=ArchiCraft
Comment=Building modeller that saves IFCX (IFC5)
Exec=archicraft %f
Icon=$app_id
Terminal=false
Categories=Graphics;Engineering;
StartupWMClass=archicraft
DESKTOP

cat >"$src/$app_id.metainfo.xml" <<METAINFO
<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>$app_id</id>
  <metadata_license>CC0-1.0</metadata_license>
  <project_license>CC-BY-4.0</project_license>
  <name>ArchiCraft</name>
  <summary>Model buildings and save them as IFCX</summary>
  <description>
    <p>
      ArchiCraft is a desktop modeller for buildings. Walls and slabs are drawn in plan and
      shown in 3D, and projects are saved as IFCX (IFC5), an open format that other BIM tools
      read.
    </p>
    <p>ArchiCraft works offline and sends no data anywhere.</p>
  </description>
  <developer id="app.archicraft">
    <name>Jakob Brandstötter and Alexander Partsch</name>
  </developer>
  <url type="homepage">https://archicraft.app</url>
  <url type="vcs-browser">https://github.com/ArchiCraftApp/ArchiCraft</url>
  <launchable type="desktop-id">$app_id.desktop</launchable>
  <provides>
    <binary>archicraft</binary>
  </provides>
  <content_rating type="oars-1.1"/>
  <releases>
    <release version="$version" date="$(git log -1 --format=%cs)"/>
  </releases>
</component>
METAINFO

# Permissions (finish-args):
# - Wayland, with X11 only where there is no Wayland, and IPC for X11's shared memory;
# - the GPU (dri) for wgpu's Vulkan or OpenGL;
# - the home folder and the mount points of removable drives and other disks (/media,
#   /run/media, /mnt): a save writes a temporary file next to the project and keeps backups
#   beside it (NFR-REL-001, NFR-REL-003), and the recent projects reopen by path (SW-001). The
#   document portal grants a single file and neither, so a project on a USB drive reached only
#   through it would lose its backups; with these paths granted, the file dialogs (XDG portals)
#   hand the real path to the app instead. Files elsewhere still open and save through the
#   file dialogs, without backups.
# No network: ArchiCraft makes no network requests.
cat >"$out/$app_id.json" <<MANIFEST
{
  "id": "$app_id",
  "runtime": "org.freedesktop.Platform",
  "runtime-version": "$runtime_version",
  "sdk": "org.freedesktop.Sdk",
  "command": "archicraft",
  "finish-args": [
    "--socket=wayland",
    "--socket=fallback-x11",
    "--share=ipc",
    "--device=dri",
    "--filesystem=home",
    "--filesystem=/media",
    "--filesystem=/run/media",
    "--filesystem=/mnt"
  ],
  "modules": [
    {
      "name": "archicraft",
      "buildsystem": "simple",
      "build-commands": [
        "install -Dm755 archicraft /app/bin/archicraft",
        "install -Dm644 $app_id.desktop /app/share/applications/$app_id.desktop",
        "install -Dm644 $app_id.metainfo.xml /app/share/metainfo/$app_id.metainfo.xml",
        "for size in 512 256 128 64 48 32; do install -Dm644 icons/\$size.png /app/share/icons/hicolor/\${size}x\${size}/apps/$app_id.png; done",
        "install -Dm644 LICENSE /app/share/licenses/$app_id/LICENSE",
        "install -Dm644 THIRD-PARTY.md /app/share/licenses/$app_id/THIRD-PARTY.md"
      ],
      "sources": [
        { "type": "dir", "path": "$src" }
      ]
    }
  ]
}
MANIFEST

flatpak remote-add --user --if-not-exists flathub "$flathub"
# rofiles-fuse only speeds up large builds and is missing in containers without FUSE.
flatpak-builder --user --install-deps-from=flathub --disable-rofiles-fuse --force-clean \
    --state-dir="$out/state" --repo="$out/repo" "$out/build" "$out/$app_id.json"
flatpak build-bundle --runtime-repo="$flathub" "$out/repo" "$bundle" "$app_id"

echo "$bundle"
