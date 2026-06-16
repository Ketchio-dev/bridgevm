#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "SKIP: macOS settings defaults smoke requires Darwin"
  exit 0
fi

cd "$ROOT"

swift test \
  --package-path "$ROOT/apps/macos" \
  --filter AppSettingsTests \
  --jobs 1

echo "PASS: macOS settings defaults drive bundled daemon mode"
