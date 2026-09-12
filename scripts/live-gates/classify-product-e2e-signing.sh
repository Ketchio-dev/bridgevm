#!/usr/bin/env bash
# Classify signature evidence; Gatekeeper predicate matches the existing release gate.
set -euo pipefail
[[ $# == 1 ]] || exit 2
codesign --verify --deep --strict "$1" >/dev/null 2>&1 || exit 1
metadata=$(codesign -dv --verbose=4 "$1" 2>&1) || exit 1
if printf '%s\n' "$metadata" | grep -q '^Signature=adhoc$'; then
  printf 'development-ad-hoc\n'
elif ! printf '%s\n' "$metadata" | grep -q '^Authority='; then
  exit 1
elif printf '%s\n' "$metadata" | grep -q '^Authority=Developer ID Application' &&
     spctl --assess --type execute "$1" >/dev/null 2>&1; then
  printf 'developer-id-notarized\n'
else
  printf 'development-signed\n'
fi
