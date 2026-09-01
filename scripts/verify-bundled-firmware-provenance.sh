#!/usr/bin/env bash
# Verify that the bundled patched EDK2 image carries its exact receipt/notices.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP="${1:-}"
[[ -d "$APP/Contents" ]] || {
  echo "usage: scripts/verify-bundled-firmware-provenance.sh APP" >&2
  exit 2
}

firmware_source="$ROOT/crates/bridgevm-hvf/firmware"
firmware_bundle="$APP/Contents/Resources/firmware"
receipt="edk2-aarch64-secure-code.fd.build.json"
patch_file="crates/bridgevm-hvf/firmware/patches/0001-armvirt-process-tpm-ppi.patch"
patch_sha="$(awk -F '\t' -v wanted="$patch_file" '$1 == wanted { print $9 }' "$ROOT/THIRD-PARTY-PATCHES.tsv")"

cmp -s "$ROOT/THIRD-PARTY-PATCHES.tsv" "$APP/Contents/Resources/THIRD-PARTY-PATCHES.tsv" ||
  { echo "bundled patch registry differs from the repository source" >&2; exit 1; }
cmp -s "$firmware_source/$receipt" "$firmware_bundle/$receipt" || {
  echo "bundled EDK2 build/patch receipt differs from the repository source" >&2; exit 1;
}
cmp -s "$firmware_source/edk2-licenses.txt" "$firmware_bundle/licenses.txt" || {
  echo "bundled EDK2 licence notices differ from the repository source" >&2; exit 1;
}
grep -qF "\"path\": \"$patch_file\"" "$firmware_bundle/$receipt" || {
  echo "bundled EDK2 receipt does not identify the product patch" >&2
  exit 1
}
grep -qF "\"sha256\": \"$patch_sha\"" "$firmware_bundle/$receipt" || {
  echo "bundled EDK2 receipt does not carry the registered patch digest" >&2
  exit 1
}

echo "bundled_firmware_provenance=pass patch_sha256=$patch_sha"
