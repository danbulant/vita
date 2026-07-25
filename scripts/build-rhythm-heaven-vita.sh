#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "$0")/.." && pwd)
dsvita_dir="$repo_root/third_party/DSVita"
rom_path="${1:-$repo_root/roms/3588 - Rhythm Heaven (US)(XenoPhobia).nds}"
stage_dir="$repo_root/dist/rhythm-heaven-vita"

if [[ ! -f "$rom_path" ]]; then
    echo "Missing Rhythm Heaven ROM: $rom_path" >&2
    exit 1
fi

header=$(dd if="$rom_path" bs=1 count=16 status=none)
if [[ "$header" != RHYTHMHEAVENYLZE ]]; then
    echo "Unexpected ROM header; expected RHYTHMHEAVEN/YLZE" >&2
    exit 1
fi

echo "Building DSVita release VPK" >&2
nix develop "$repo_root" --command bash -lc \
    "cd '$dsvita_dir' && cargo vita build vpk --release"

mkdir -p "$stage_dir/ux0/data/dsvita"
cp "$dsvita_dir/target/armv7-sony-vita-newlibeabihf/release/dsvita.vpk" \
    "$stage_dir/DSVita.vpk"
cp "$rom_path" "$stage_dir/ux0/data/dsvita/Rhythm Heaven.nds"

vpk_listing=$(python3 -c \
    'import sys, zipfile; print("\n".join(zipfile.ZipFile(sys.argv[1]).namelist()))' \
    "$stage_dir/DSVita.vpk")
for required_entry in eboot.bin sce_sys/param.sfo sce_sys/icon0.png; do
    if ! grep -Fxq "$required_entry" <<<"$vpk_listing"; then
        echo "Invalid VPK: missing $required_entry" >&2
        exit 1
    fi
done
if grep -Eiq '\.(nds|cia|3ds)$' <<<"$vpk_listing"; then
    echo "Invalid VPK: game image was embedded in the application package" >&2
    exit 1
fi

echo "Staged Vita install files in $stage_dir" >&2
echo "Install DSVita.vpk and copy the ux0 tree to the Vita." >&2
