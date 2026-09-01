#!/usr/bin/env bash
# Require an exact, licence-backed distribution classification for every patch.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

registry="${1:-THIRD-PARTY-PATCHES.tsv}"
expected_header=$'path\tcomponent\tupstream_revision\tlicense\tdistribution_scope\tmodified_output\tlicense_text\tlicense_sha256\tpatch_sha256'
errors=0

fail() {
  echo "$1" >&2
  errors=$((errors + 1))
}

[[ -s "$registry" ]] || { echo "third-party patch registry is missing: $registry" >&2; exit 1; }
[[ "$(head -n 1 "$registry")" == "$expected_header" ]] ||
  fail "third-party patch registry header is invalid"

bad_fields="$(awk -F '\t' 'NR > 1 && NF != 9 { print NR ":" NF }' "$registry")"
[[ -z "$bad_fields" ]] || fail "third-party patch registry rows must have nine fields: $bad_fields"

duplicates="$(tail -n +2 "$registry" | cut -f1 | sort | uniq -d)"
[[ -z "$duplicates" ]] || fail "duplicate third-party patch rows: $duplicates"

tracked_patches="$(git ls-files '*.patch' '*.diff' | sort)"
registered_patches="$(tail -n +2 "$registry" | cut -f1 | sort)"
patch_delta="$(comm -3 <(printf '%s\n' "$tracked_patches") <(printf '%s\n' "$registered_patches"))"
[[ -z "$patch_delta" ]] || fail "tracked third-party patches and registry rows differ: $patch_delta"

while IFS=$'\t' read -r patch_file component revision license scope output license_text license_sha patch_sha; do
  [[ -n "$patch_file" && -n "$component" && -n "$license" && -n "$output" ]] || {
    fail "third-party patch registry has an empty required field: $patch_file"
    continue
  }
  [[ "$revision" =~ ^[0-9a-f]{40}$ ]] ||
    fail "third-party patch revision is not an exact 40-hex commit: $patch_file"
  case "$scope" in
    product-bundled|graphics-lab-artifact|internal-validation-only) ;;
    *) fail "third-party patch has an unknown distribution scope: $patch_file ($scope)" ;;
  esac
  case "$patch_file" in
    crates/bridgevm-hvf/firmware/patches/0001-armvirt-process-tpm-ppi.patch)
      expected_revision=b03a21a63e3bd001f52c527e5a57feddb53a690b
      expected_license=BSD-2-Clause-Patent
      expected_scope=product-bundled
      expected_license_text=crates/bridgevm-hvf/firmware/edk2-licenses.txt ;;
    scripts/patches/virglrenderer-macos-venus.patch)
      expected_revision=2a173eef8c044abeddd2a9e842f52659bedd5376
      expected_license=MIT
      expected_scope=product-bundled
      expected_license_text=docs/licenses/virglrenderer-MIT.txt ;;
    scripts/patches/virtio-win-mesa-*.patch)
      expected_revision=cb531c440ff34a9c6334859dda0848132be49ec3
      expected_license=MIT
      expected_scope=graphics-lab-artifact
      expected_license_text=docs/licenses/Mesa-patched-files-MIT.txt ;;
    scripts/patches/dxvk-macos-venus-relax.patch)
      expected_revision=6b20f622a77b87b2921fe5d2c1774d2f2ba3e9b7
      expected_license=Zlib
      expected_scope=internal-validation-only
      expected_license_text=docs/licenses/DXVK-zlib.txt ;;
    scripts/patches/virglrenderer-macos-venus-bv-draw-probes.patch)
      expected_revision=2a173eef8c044abeddd2a9e842f52659bedd5376
      expected_license=MIT
      expected_scope=internal-validation-only
      expected_license_text=docs/licenses/virglrenderer-MIT.txt ;;
    *)
      fail "tracked patch has no independent classification rule: $patch_file"
      continue ;;
  esac
  [[ "$revision" == "$expected_revision" && "$license" == "$expected_license" && \
    "$scope" == "$expected_scope" && "$license_text" == "$expected_license_text" ]] ||
    fail "third-party patch classification disagrees with its pinned build path: $patch_file"
  [[ -f "$patch_file" && ! -L "$patch_file" ]] || {
    fail "registered third-party patch is missing or is a symlink: $patch_file"
    continue
  }
  [[ -s "$license_text" && ! -L "$license_text" ]] || {
    fail "registered third-party patch licence text is missing or is a symlink: $license_text"
    continue
  }
  actual_license_sha="$(shasum -a 256 "$license_text" | awk '{ print tolower($1) }')"
  [[ "$license_sha" =~ ^[0-9a-f]{64}$ && "$actual_license_sha" == "$license_sha" ]] ||
    fail "registered third-party licence SHA-256 is malformed or stale: $license_text"
  [[ "$patch_sha" =~ ^[0-9a-f]{64}$ ]] || {
    fail "registered third-party patch SHA-256 is malformed: $patch_file"
    continue
  }
  actual_sha="$(shasum -a 256 "$patch_file" | awk '{ print tolower($1) }')"
  [[ "$actual_sha" == "$patch_sha" ]] ||
    fail "registered third-party patch SHA-256 is stale: $patch_file"
done < <(tail -n +2 "$registry")

# Scope must agree with the build graph, not merely with prose.
grep -qF 'virglrenderer-macos-venus.patch' scripts/build-venus-host-deps.sh ||
  fail "the product renderer build is not bound to its registered patch"
grep -qF '2a173eef8c044abeddd2a9e842f52659bedd5376' scripts/build-venus-host-deps.sh ||
  fail "the product renderer build is not pinned to the registered revision"
grep -qF '0001-armvirt-process-tpm-ppi.patch' scripts/build-hvf-edk2-secure-firmware.sh ||
  fail "the product firmware build is not bound to its registered patch"
grep -qF 'b03a21a63e3bd001f52c527e5a57feddb53a690b' scripts/build-hvf-edk2-secure-firmware.sh ||
  fail "the product firmware build is not pinned to the registered revision"
grep -qF 'cb531c440ff34a9c6334859dda0848132be49ec3' .github/workflows/windows-umd.yml ||
  fail "the Graphics Lab workflow is not pinned to the registered Mesa revision"
for mesa_patch in \
  scripts/patches/virtio-win-mesa-unbound-clear.patch \
  scripts/patches/virtio-win-mesa-submit-trace.patch; do
  grep -qF "$mesa_patch" .github/workflows/windows-umd.yml ||
    fail "the Graphics Lab workflow is not bound to its registered patch: $mesa_patch"
done
grep -qF 'package-windows-graphics-notices.py assemble' .github/workflows/windows-umd.yml ||
  fail "the Windows UMD workflow does not invoke the notice packager"
for notice_marker in Mesa-license.rst Mesa-upstream-licenses.zip \
  BridgeVM-MODIFICATIONS.txt THIRD-PARTY-NOTICE-SHA256SUMS; do
  grep -qF "$notice_marker" scripts/package-windows-graphics-notices.py ||
    fail "the Windows UMD artifact omits its notice marker: $notice_marker"
done
while IFS=$'\t' read -r patch_file _ _ _ scope _ _ _ _; do
  [[ "$scope" == "internal-validation-only" ]] || continue
  patch_name="$(basename "$patch_file")"
  internal_hit="$(grep -rlF --exclude='check-third-party-patch-registry.sh' \
    --exclude-dir=patches "$patch_name" .github apps packaging scripts 2>/dev/null || true)"
  [[ -z "$internal_hit" ]] ||
    fail "internal-only patch is referenced by a product/CI build path: $patch_file ($internal_hit)"
done < <(tail -n +2 "$registry")

grep -qF "THIRD-PARTY-PATCHES.tsv" THIRD-PARTY-NOTICES.md ||
  fail "THIRD-PARTY-NOTICES.md does not link the patch registry"
grep -qF "THIRD-PARTY-PATCHES.tsv" docs/licensing-and-attribution.md ||
  fail "licensing guidance does not link the patch registry"
python3 scripts/package-windows-graphics-notices.py self-test ||
  fail "Windows graphics notice mutation self-test failed"

if [[ $errors -ne 0 ]]; then
  echo "third-party patch registry: FAIL ($errors error(s))" >&2
  exit 1
fi

echo "third-party patch registry: PASS ($(printf '%s\n' "$registered_patches" | awk 'NF { count++ } END { print count + 0 }') patches)"
