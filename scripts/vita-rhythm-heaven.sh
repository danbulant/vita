#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "$0")/.." && pwd)
command_name="${1:-}"
vita_ip="${VITA_IP:-}"
vita_port="${VITA_PORT:-1337}"
stage_dir="$repo_root/dist/rhythm-heaven-vita"
artifact_dir="$repo_root/artifacts/rhythm-heaven-vita"

if [[ -z "$vita_ip" ]]; then
    echo "VITA_IP is not set" >&2
    exit 1
fi

ftp_root="ftp://$vita_ip:$vita_port"

case "$command_name" in
    deploy)
        vpk_path="${2:-$stage_dir/DSVita-debug.vpk}"
        rom_path="$stage_dir/ux0/data/dsvita/Rhythm Heaven.nds"
        if [[ ! -f "$vpk_path" || ! -f "$rom_path" ]]; then
            echo "Staged VPK or ROM is missing; run build-rhythm-heaven-vita.sh first" >&2
            exit 1
        fi

        echo "Uploading $(basename "$vpk_path") to ux0:data" >&2
        curl --fail --ftp-create-dirs --silent --show-error \
            --upload-file "$vpk_path" "$ftp_root/ux0:/data/$(basename "$vpk_path")"
        echo "Uploading Rhythm Heaven ROM to ux0:data/dsvita" >&2
        curl --fail --ftp-create-dirs --silent --show-error \
            --upload-file "$rom_path" "$ftp_root/ux0:/data/dsvita/Rhythm%20Heaven.nds"
        echo "Upload complete; install the VPK from ux0:data in VitaShell" >&2
        ;;
    logs)
        mkdir -p "$artifact_dir"
        echo "Fetching DSVita log" >&2
        curl --fail --silent --show-error \
            "$ftp_root/ux0:/data/dsvita/log/log.txt" \
            --output "$artifact_dir/log.txt"
        echo "Saved log to $artifact_dir/log.txt" >&2
        ;;
    prerequisites)
        kubridge_path="$stage_dir/prerequisites/kubridge-v0.3.1-hotfix.skprx"
        if [[ ! -f "$kubridge_path" ]]; then
            echo "Staged kubridge is missing; run build-rhythm-heaven-vita.sh first" >&2
            exit 1
        fi
        echo "Uploading kubridge to a safe staging path" >&2
        curl --fail --ftp-create-dirs --silent --show-error \
            --upload-file "$kubridge_path" \
            "$ftp_root/ux0:/data/rhythm-heaven-prerequisites/kubridge-v0.3.1-hotfix.skprx"
        echo "Plugin staged only; install it manually and do not edit taiHEN config blindly" >&2
        ;;
    *)
        echo "Usage: $0 deploy [vpk-path] | logs | prerequisites" >&2
        exit 1
        ;;
esac
