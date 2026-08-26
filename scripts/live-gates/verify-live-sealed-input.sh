#!/usr/bin/env bash
# Verify immutable queue inputs before an exact-code tier can consume them.
set -euo pipefail
TIER="$1"; DIR="$2"; REPO="$3"; MANIFEST="$DIR/input-manifest.tsv"
[[ -f "$MANIFEST" && ! -L "$MANIFEST" ]] || exit 1
expected="$(awk -F= '$1=="input_manifest_sha256"{print $2}' "$DIR/job.env")"
actual="$(shasum -a 256 "$MANIFEST" 2>/dev/null | cut -d' ' -f1 || true)"
[[ -n "$expected" && "$actual" == "$expected" ]] || exit 1
case "$TIER" in
  t6-a3-title|t7-windows-closure)
    [[ -f "$DIR/hvf_gic_boot_probe" && ! -L "$DIR/hvf_gic_boot_probe" ]] || exit 1
    expected="$(awk -F= '$1=="sealed_binary_sha256"{print $2}' "$DIR/job.env")"
    actual="$(shasum -a 256 "$DIR/hvf_gic_boot_probe" 2>/dev/null | cut -d' ' -f1 || true)"
    [[ -n "$expected" && "$actual" == "$expected" ]] ;;
  t8-pointer-reliability|t12-b4-umd-diagnostic)
    expected="$(awk -F= '$1=="sealed_package_sha256"{print $2}' "$DIR/job.env")"
    actual="$(python3 "$REPO/scripts/live-gates/b4-diagnostic-package.py" verify --manifest "$MANIFEST" --dir "$DIR/sealed-package" 2>/dev/null || true)"
    injector_expected="$(awk -F= '$1=="sealed_injector_vars_sha256"{print $2}' "$DIR/job.env")"; injector_actual="$(shasum -a 256 "$DIR/sealed-injector-vars.fd" 2>/dev/null | cut -d' ' -f1 || true)"
    iso_expected="$(awk -F= '$1=="sealed_injector_iso_sha256"{print $2}' "$DIR/job.env")"; iso_path="$(awk -F= '$1=="sealed_injector_iso_path"{print substr($0,index($0,"=")+1)}' "$DIR/job.env")"; iso_actual="$(openssl dgst -sha256 -r "$iso_path" 2>/dev/null | cut -d' ' -f1 || true)"
    [[ -n "$expected" && "$actual" == "$expected" && -n "$injector_expected" && "$injector_actual" == "$injector_expected" && -f "$iso_path" && ! -L "$iso_path" && ! -w "$iso_path" && -n "$iso_expected" && "$iso_actual" == "$iso_expected" ]] ;;
  *) [[ "$TIER" == t13-compatibility-observation ]] && exec "$REPO/scripts/live-gates/verify-compatibility-queue-input.sh" "$DIR" "$REPO"; exit 2 ;;
esac
