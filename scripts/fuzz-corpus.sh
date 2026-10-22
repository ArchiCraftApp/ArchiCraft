#!/bin/sh
# Seeds the cargo-fuzz corpora (NFR-REL-007, NFR-SEC-003, SRS Appendix D.9; docs/testing.md,
# Fuzzing). Copies into fuzz/corpus/ifcx_load/ and fuzz/corpus/ifcx_validate/:
# - both reference models, written by the reference_models example;
# - the golden files, crates/archicraft-ifcx/tests/golden/*.ifcx;
# - every corrupt fixture, crates/archicraft-ifcx/tests/corrupt/*.ifcx;
# - the hand-crafted seeds (stress inputs, extreme numbers, reader branches the other seeds
#   miss), written by the fuzz_seeds example;
# - the R2 examples (Hello Wall, PCERT sample scene, Tekla House, ACCA building) from
#   buildingSMART IFC5-development at the commit conformance/run.sh pins. They are fetched,
#   never committed (NFR-LEG-003): the blob-filtered clone in conformance/.ifc5 downloads
#   only the blobs read here.
# and a few typed inputs into fuzz/corpus/units_parse/. Existing corpus entries stay, so a
# nightly run keeps what earlier nights found. Everything written is git-ignored.
# Usage: scripts/fuzz-corpus.sh
set -eu
cd "$(git rev-parse --show-toplevel)"

ifc5=conformance/.ifc5
commit="$(sed -n 's/^IFC5_COMMIT=//p' conformance/run.sh)"
[ -n "$commit" ] || { echo "no IFC5_COMMIT in conformance/run.sh" >&2; exit 2; }
# <corpus name>|<path in IFC5-development>
r2_examples='r2-hello-wall.ifcx|examples/Hello Wall/hello-wall.ifcx
r2-pcert-building-architecture.ifcx|examples/PCERT-Sample-Scene/Building-Architecture.ifcx
r2-tekla-house.ifcx|examples/Tekla House/TeklaHouse.ifcx
r2-acca-building.ifcx|examples/ACCA Building/esempio_01 edificius.ifcx'

ifcx_corpora="fuzz/corpus/ifcx_load fuzz/corpus/ifcx_validate"
units_corpus=fuzz/corpus/units_parse
# shellcheck disable=SC2086 # fixed paths without spaces
mkdir -p $ifcx_corpora "$units_corpus"

# seed <name> <source file>: copies one file into both IFCX corpora.
seed() {
    for corpus in $ifcx_corpora; do
        cp "$2" "$corpus/$1"
    done
    count=$((count + 1))
}
count=0

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
cargo run --quiet --locked -p archicraft-ifcx --example reference_models -- "$tmp" >/dev/null
for f in "$tmp"/*.ifcx; do
    seed "reference-$(basename "$f")" "$f"
done
for f in crates/archicraft-ifcx/tests/golden/*.ifcx; do
    seed "golden-$(basename "$f")" "$f"
done
for f in crates/archicraft-ifcx/tests/corrupt/*.ifcx; do
    seed "corrupt-$(basename "$f")" "$f"
done
cargo run --quiet --locked -p archicraft-ifcx --example fuzz_seeds -- "$tmp/seeds" >/dev/null
for f in "$tmp"/seeds/*.ifcx; do
    seed "$(basename "$f")" "$f"
done

if [ ! -d "$ifc5/.git" ]; then
    git clone --quiet --filter=blob:none --no-checkout \
        https://github.com/buildingSMART/IFC5-development.git "$ifc5"
fi
git -C "$ifc5" cat-file -e "$commit^{commit}" 2>/dev/null || git -C "$ifc5" fetch --quiet origin
printf '%s\n' "$r2_examples" | while IFS='|' read -r name path; do
    git -C "$ifc5" show "$commit:$path" > "$tmp/$name"
done
for f in "$tmp"/r2-*.ifcx; do
    seed "$(basename "$f")" "$f"
done

# Typed lengths and angles in every unit and both decimal separators (INP-005, STR-010).
i=0
for input in '2.5' '-0.3' '+1' '2,5' '1.234,5' '1,234.5' '1 234,5' '365mm' '30 cm' '10ft' \
    '6in' "3'-4 1/2\"" "3' 4\"" '1/2"' '3′4″' '45' '45°' "45°30'15\"" '30d' '100' \
    '1.5707963' '1e3' '0.000001' '-'; do
    printf '%s' "$input" > "$units_corpus/typed-$i"
    i=$((i + 1))
done

echo "Seeded $count IFCX files into $ifcx_corpora and $i typed inputs into $units_corpus."
