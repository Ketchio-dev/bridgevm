#!/usr/bin/env bash
# Shell adapter for the fail-closed PPSSPP payload validator.
set -euo pipefail

A3_PAYLOAD_VALIDATOR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/a3-title-payload.py"

a3_ppsspp_executable_sha256() {
    python3 "$A3_PAYLOAD_VALIDATOR" "$1"
}

a3_validate_ppsspp_payload() {
    local executable_hash
    executable_hash="$(a3_ppsspp_executable_sha256 "$1")" || {
        echo "A3 PPSSPP payload archive is unsafe or incomplete" >&2
        return 1
    }
    printf '%s\n' "$executable_hash"
}

if [[ "${1:-}" == --self-test ]]; then
    python3 "$A3_PAYLOAD_VALIDATOR" --self-test
fi
