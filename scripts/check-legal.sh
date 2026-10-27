#!/bin/sh
# Legal guard (NFR-LEG-002, docs/testing.md): the mechanical half of "no content of the
# reference product's maker and no use of its trademarks except nominative references".
# Fails when
# - a tracked file names the maker or the product outside scripts/legal-allowlist.txt
#   (docs/SRS.md, docs/technology-choices.md and docs/implementation-status.md cite the
#   reference product nominatively throughout and are exempt; so are
#   docs/status/requirements.json, from which docs/implementation-status.md is generated, and
#   the beads export .beads/, whose task texts quote those documents);
# - a tracked image (SVG, ICO, ICNS, WebP and TIFF included), library-part or template file is
#   not listed with its origin and licence in scripts/asset-allowlist.txt.
# Usage: scripts/check-legal.sh
set -eu
cd "$(git rev-parse --show-toplevel)"
legal_allow=scripts/legal-allowlist.txt
asset_allow=scripts/asset-allowlist.txt
status=0

# Assembled at run time so this script does not match itself.
pattern="graphi""soft|archi""cad"
rc=0
hits="$(git grep -n -I -i -E "$pattern" -- . ':!docs/SRS.md' ':!docs/technology-choices.md' \
    ':!docs/implementation-status.md' ':!docs/status/requirements.json' ':!.beads/')" || rc=$?
[ "$rc" -le 1 ] || { echo "git grep failed ($rc)" >&2; exit 2; }

if [ -n "$hits" ]; then
    # An allowlist entry "<path>: <text> # <reason>" covers the hits in <path> whose line
    # contains <text>. Entries without a reason are rejected.
    unlisted="$(printf '%s\n' "$hits" | awk -v allow="$legal_allow" '
        function trim(s) { gsub(/^[ \t]+|[ \t]+$/, "", s); return s }
        BEGIN {
            while ((getline line < allow) > 0) {
                if (line ~ /^[ \t]*(#|$)/) continue
                colon = index(line, ": "); hash = index(line, " # ")
                if (colon == 0 || hash < colon || trim(substr(line, hash + 3)) == "") {
                    print allow ": malformed entry (needs <path>: <text> # <reason>): " line
                    continue
                }
                n++
                path[n] = trim(substr(line, 1, colon - 1))
                text[n] = trim(substr(line, colon + 2, hash - colon - 2))
            }
        }
        {
            p = index($0, ":"); file = substr($0, 1, p - 1)
            rest = substr($0, p + 1); content = substr(rest, index(rest, ":") + 1)
            for (i = 1; i <= n; i++) if (path[i] == file && index(content, text[i]) > 0) next
            print
        }')"
    if [ -n "$unlisted" ]; then
        echo "Trademark or maker named outside $legal_allow (NFR-LEG-002):"
        printf '%s\n' "$unlisted" | sed 's/^/  /'
        echo "Remove it, or add a reviewed nominative entry to $legal_allow."
        status=1
    fi
fi

assets="$(git ls-files -- ':(icase,glob)**/*.png' ':(icase,glob)**/*.jpg' \
    ':(icase,glob)**/*.jpeg' ':(icase,glob)**/*.gif' ':(icase,glob)**/*.bmp' \
    ':(icase,glob)**/*.svg' ':(icase,glob)**/*.ico' ':(icase,glob)**/*.icns' \
    ':(icase,glob)**/*.webp' ':(icase,glob)**/*.tif' ':(icase,glob)**/*.tiff' \
    ':(icase,glob)**/*.gsm' ':(icase,glob)**/*.lcf' ':(icase,glob)**/*.pln' \
    ':(icase,glob)**/*.pla' ':(icase,glob)**/*.tpl' ':(icase,glob)**/*.aat')"
if [ -n "$assets" ]; then
    # An entry "<path> | <origin> | <licence>" with all three fields lists one file.
    unlisted="$(printf '%s\n' "$assets" | awk -v allow="$asset_allow" '
        function trim(s) { gsub(/^[ \t]+|[ \t]+$/, "", s); return s }
        BEGIN {
            while ((getline line < allow) > 0) {
                if (line ~ /^[ \t]*(#|$)/) continue
                if (split(line, f, "|") != 3 || trim(f[2]) == "" || trim(f[3]) == "") {
                    print allow ": malformed entry (needs <path> | <origin> | <licence>): " line
                    continue
                }
                listed[trim(f[1])] = 1
            }
        }
        !($0 in listed)')"
    if [ -n "$unlisted" ]; then
        echo "Image, library-part or template files without origin and licence in $asset_allow (NFR-LEG-002):"
        printf '%s\n' "$unlisted" | sed 's/^/  /'
        status=1
    fi
fi

[ "$status" -eq 0 ] && echo "no unreviewed trademarks or third-party assets"
exit "$status"
