#!/usr/bin/env bash
# Ordinary CLI and owned subprocess transport checks; no app window or VM.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"
[[ $# == 1 ]] || { echo "usage: check-native-runtime.sh ABSOLUTE_NATIVE_BINARY" >&2; exit 2; }
python3 -B tests/integration/native-app-cli-contract.py "$1"
RUNTIME_CONTRACT_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-runtime-contracts.XXXXXX")
RUNTIME_CONTRACT_ROOT=$(cd "$RUNTIME_CONTRACT_ROOT" && pwd -P)
trap 'rm -rf "$RUNTIME_CONTRACT_ROOT"' EXIT
python3 -B tests/integration/native-runtime-transport-contract.py --output "$RUNTIME_CONTRACT_ROOT/output"
