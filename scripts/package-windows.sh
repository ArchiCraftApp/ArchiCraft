#!/usr/bin/env bash
# Builds ArchiCraft for Windows with the app icon in the executable and packs it into a zip with
# the licence and the third-party notices (no installer, SRS D-12). Runs in Git Bash with the
# MSVC toolchain, the Windows SDK (rc.exe) and Python with Pillow (python -m pip install pillow).
# Usage: scripts/package-windows.sh
# Output: target/windows/ArchiCraft-<version>-windows-<arch>.zip
#
# The icon is the logo in the repository root (ArchiCraft_Logo.png), converted to an .ico and
# compiled into a resource file that only the final link of the executable gets. The executable
# is not signed.
set -euo pipefail
cd "$(dirname "$0")/.."

case "$(uname -s)" in
MINGW* | MSYS* | CYGWIN*) ;;
*)
    echo "package-windows.sh needs Windows (Git Bash)" >&2
    exit 1
    ;;
esac

out=target/windows
stage=$out/stage/ArchiCraft
rm -rf "$out"
mkdir -p "$stage"

python - ArchiCraft_Logo.png "$out/archicraft.ico" <<'PY'
import sys
from PIL import Image

sizes = [(s, s) for s in (16, 20, 24, 32, 40, 48, 64, 128, 256)]
Image.open(sys.argv[1]).convert("RGBA").save(sys.argv[2], format="ICO", sizes=sizes)
PY

# The newest rc.exe of the installed Windows SDKs.
rc=$(printf "%s\n" "/c/Program Files (x86)/Windows Kits/10/bin/"*/x64/rc.exe | sort -V | tail -n 1)
printf '1 ICON "archicraft.ico"\n' >"$out/archicraft.rc"
(cd "$out" && "$rc" -nologo -fo archicraft.res archicraft.rc)

cargo rustc --release --locked -p archicraft-app --bin archicraft -- \
    -C "link-arg=$(cygpath -w "$PWD/$out/archicraft.res")"

version=$(cargo pkgid -p archicraft-app | sed 's/.*[#@]//')
zip=$out/ArchiCraft-$version-windows-x86_64.zip
cp target/release/archicraft.exe "$stage/archicraft.exe"
cp LICENSE "$stage/LICENSE.txt"
cp THIRD-PARTY.md "$stage/THIRD-PARTY.txt"
(cd "$out/stage" && python -m zipfile -c "../$(basename "$zip")" ArchiCraft)

echo "$zip"
