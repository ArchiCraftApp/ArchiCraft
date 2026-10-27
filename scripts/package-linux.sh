#!/usr/bin/env sh
# Builds ArchiCraft for Linux and packs it into a tarball with the executable, the app icon, a
# desktop entry, the licence and the third-party notices (no installer, SRS D-12).
# Usage: scripts/package-linux.sh
# Output: target/linux/ArchiCraft-<version>-linux-<arch>.tar.gz
#
# The icon is the logo in the repository root (ArchiCraft_Logo.png). The binary links against
# the glibc of the build machine: build on the oldest distribution it has to run on.
set -eu
cd "$(dirname "$0")/.."

if [ "$(uname -s)" != Linux ]; then
    echo "package-linux.sh needs Linux" >&2
    exit 1
fi

cargo build --release --locked -p archicraft-app --bin archicraft

version=$(cargo pkgid -p archicraft-app | sed 's/.*[#@]//')
arch=$(uname -m)
out=target/linux
stage=$out/ArchiCraft
tarball=$out/ArchiCraft-$version-linux-$arch.tar.gz

rm -rf "$stage" "$tarball"
mkdir -p "$stage"
cp target/release/archicraft "$stage/archicraft"
cp ArchiCraft_Logo.png "$stage/archicraft.png"
cp LICENSE "$stage/LICENSE"
cp THIRD-PARTY.md "$stage/THIRD-PARTY.md"

# StartupWMClass is the app id the window sets (main.rs). No MimeType: the .ifcx association is
# made with the user's consent at first start (SRS SW-001), not by installing the package.
cat >"$stage/archicraft.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=ArchiCraft
Comment=Building modeller that saves IFCX (IFC5)
Exec=archicraft %f
Icon=archicraft
Terminal=false
Categories=Graphics;Engineering;
StartupWMClass=archicraft
DESKTOP

tar -C "$out" -czf "$tarball" ArchiCraft

echo "$tarball"
