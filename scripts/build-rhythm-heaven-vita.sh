#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "$0")/.." && pwd)
dsvita_dir="$repo_root/third_party/DSVita"
rom_path="${1:-$repo_root/roms/3588 - Rhythm Heaven (US)(XenoPhobia).nds}"
stage_dir="$repo_root/dist/rhythm-heaven-vita"
profile="${DSVITA_PROFILE:-release}"
vita3k="${DSVITA_VITA3K:-0}"

case "$profile" in
    release)
        cargo_profile_args=(--release)
        vpk_name=DSVita.vpk
        ;;
    release-debug)
        cargo_profile_args=(--profile release-debug)
        vpk_name=DSVita-debug.vpk
        ;;
    *)
        echo "Unsupported DSVITA_PROFILE: $profile" >&2
        echo "Expected release or release-debug" >&2
        exit 1
        ;;
esac

case "$vita3k" in
    0)
        cargo_feature_args=()
        ;;
    1)
        cargo_feature_args=(--features vita3k)
        vpk_name="${vpk_name%.vpk}-vita3k.vpk"
        ;;
    *)
        echo "Unsupported DSVITA_VITA3K: $vita3k" >&2
        echo "Expected 0 or 1" >&2
        exit 1
        ;;
esac

if [[ ! -f "$rom_path" ]]; then
    echo "Missing Rhythm Heaven ROM: $rom_path" >&2
    exit 1
fi

header=$(dd if="$rom_path" bs=1 count=16 status=none)
if [[ "$header" != RHYTHMHEAVENYLZE ]]; then
    echo "Unexpected ROM header; expected RHYTHMHEAVEN/YLZE" >&2
    exit 1
fi

echo "Building DSVita $profile VPK" >&2
nix develop "$repo_root" --command bash -lc \
    "cd '$dsvita_dir' && cargo vita build vpk ${cargo_profile_args[*]} ${cargo_feature_args[*]}"

mkdir -p "$stage_dir/ux0/data/dsvita"
cp "$dsvita_dir/target/armv7-sony-vita-newlibeabihf/$profile/dsvita.vpk" \
    "$stage_dir/$vpk_name"
cp "$rom_path" "$stage_dir/ux0/data/dsvita/Rhythm Heaven.nds"

kubridge_path=$(find \
    "$dsvita_dir/target/armv7-sony-vita-newlibeabihf/$profile/build" \
    -path '*/out/kubridge/kubridge.skprx' -print -quit)
if [[ -z "$kubridge_path" ]]; then
    echo "Built kubridge.skprx was not found" >&2
    exit 1
fi
mkdir -p "$stage_dir/prerequisites"
cp "$kubridge_path" "$stage_dir/prerequisites/kubridge-v0.3.1-hotfix.skprx"

vpk_listing=$(python3 -c \
    'import sys, zipfile; print("\n".join(zipfile.ZipFile(sys.argv[1]).namelist()))' \
    "$stage_dir/$vpk_name")
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

echo "Staged $vpk_name and Vita data files in $stage_dir" >&2
echo "Staged kubridge separately for manual installation if required" >&2
echo "Install $vpk_name and copy the ux0 tree to the Vita." >&2
