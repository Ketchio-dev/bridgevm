#!/usr/bin/env bash
# Stage one sealed PPSSPP ZIP as bounded share files, then install it in-guest.
set -euo pipefail

A3_PAYLOAD_CHUNK_BYTES=$((7 * 1024 * 1024))
A3_PAYLOAD_MAX_PARTS=64

prepare_a3_payload_share() {
    local payload="$1" share="$2" payload_sha="$3" part size
    A3_PAYLOAD_PREFIX="ppsspp-${payload_sha}.zip.part-"
    rm -f "$share"/ppsspp-*.zip.part-*
    split -d -b "$A3_PAYLOAD_CHUNK_BYTES" -a 3 "$payload" \
        "$share/$A3_PAYLOAD_PREFIX"
    A3_PAYLOAD_PART_COUNT=0
    for part in "$share/$A3_PAYLOAD_PREFIX"*; do
        [[ -f "$part" && ! -L "$part" ]] || return 1
        size=$(stat -f %z "$part")
        (( size > 0 && size <= A3_PAYLOAD_CHUNK_BYTES )) || return 1
        A3_PAYLOAD_PART_COUNT=$((A3_PAYLOAD_PART_COUNT + 1))
    done
    (( A3_PAYLOAD_PART_COUNT > 0 && A3_PAYLOAD_PART_COUNT <= A3_PAYLOAD_MAX_PARTS ))
}

wait_for_a3_payload_share() {
    local share="$1" part bytes
    for part in "$share/$A3_PAYLOAD_PREFIX"*; do
        bytes=$(stat -f %z "$part")
        wait_for "^BVAGENT SHARE host->guest $(basename "$part") bytes=$bytes " 1 600 \
            || fail "PPSSPP payload chunk sync timeout: $(basename "$part")"
    done
}

install_a3_payload_guest() {
    local payload_sha="$1" executable_sha="$2" command
    command='powershell -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMShare\bvgpu-stage-ppsspp.ps1'
    command+=" -ArchivePrefix $A3_PAYLOAD_PREFIX -ChunkCount $A3_PAYLOAD_PART_COUNT"
    command+=" -ExpectedPayloadSha256 $payload_sha -ExpectedExecutableSha256 $executable_sha"
    run_guest "$command" 300
    tr -d '\r' < "$RUN_LOG" | grep -q '^prep=PPSSPPPAYLOADOK$' \
        || fail "sealed PPSSPP payload installation failed"
}

if [[ "${1:-}" == --self-test ]]; then
    work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
    truncate -s $((A3_PAYLOAD_CHUNK_BYTES * 2 + 17)) "$work/payload.zip"
    prepare_a3_payload_share "$work/payload.zip" "$work" "$(printf 'a%.0s' {1..64})"
    [[ "$A3_PAYLOAD_PART_COUNT" -eq 3 ]]
    for part in "$work/$A3_PAYLOAD_PREFIX"*; do
        [[ $(stat -f %z "$part") -le $A3_PAYLOAD_CHUNK_BYTES ]]
    done
    echo "PASS: bounded A3 payload staging"
fi
