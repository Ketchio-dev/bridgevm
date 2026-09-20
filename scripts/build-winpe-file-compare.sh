#!/usr/bin/env bash
set -euo pipefail
[[ $# == 1 && "$1" == /* && ! -e "$1" ]] || {
  echo "usage: scripts/build-winpe-file-compare.sh /absolute/new/output.exe" >&2
  exit 2
}
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT="$1"
[[ -d "$(dirname "$OUTPUT")" ]] || { echo "output parent is missing" >&2; exit 1; }
command -v zig >/dev/null 2>&1 || { echo "zig is required to build the WinPE comparator" >&2; exit 1; }
zig cc -target aarch64-windows-gnu -Os -s -std=c11 -Wall -Wextra -Werror \
  "$ROOT/scripts/win-assets/bv-file-compare.c" -o "$OUTPUT"
description="$(file -b "$OUTPUT")"
[[ "$description" == *PE32+* && "$description" == *Aarch64* ]] || {
  echo "WinPE comparator is not an ARM64 PE executable: $description" >&2
  exit 1
}
