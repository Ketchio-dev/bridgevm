#!/usr/bin/env bash
# Stage the pinned arehnman/akre ARM64 VirGL payload as an unsigned,
# UMD-registered package. Windows WDK catalog generation and signing are a
# separate, mandatory finalization step; stale signed metadata is never reused.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE="arehnman-arm64-minimal"
INPUT_DIR="${INPUT_DIR:-}"
SOURCE_INX="${SOURCE_INX:-$HOME/BridgeVM/viogpu3d-arehnman/viogpu/viogpu3d/viogpu3d_arm64.inx}"
OUT_DIR="${OUT_DIR:-}"
EXPECTED_INPUT_MANIFEST="${EXPECTED_INPUT_MANIFEST:-$ROOT/scripts/viogpu3d-arehnman-arm64-minimal-input.sha256}"
EXPECTED_SOURCE_HEAD="${EXPECTED_SOURCE_HEAD:-4c27e477e6560cea724d848b98149f03cb1f2083}"
EXPECTED_SOURCE_INX_SHA256="${EXPECTED_SOURCE_INX_SHA256:-8cce50b61cf258dfe183e48ac64a0d4c5fb96009c7c9cd093c06576fd9086159}"
CANONICAL_INF="$ROOT/scripts/win-assets/viogpu3d-arehnman-arm64-minimal.inf"
EXPECTED_CANONICAL_INF_SHA256="f8bc2e3bb097d1d8f9d461745dc6665b65bddf53cbb986dc57df1059f374b5e9"
SOURCE_HEAD_OVERRIDE="${SOURCE_HEAD_OVERRIDE:-}"
SOURCE_REPO="${SOURCE_REPO:-arehnman/kvm-guest-drivers-windows}"

usage() {
  cat >&2 <<'EOF'
usage: scripts/stage-hvf-windows-viogpu3d-render-package.sh \
         --input-dir DIR --out-dir DIR [options]

Required:
  --input-dir DIR       Pinned CI package containing viogpu3d.sys, the five
                        audited ARM64 Mesa DLLs, and its original viogpu3d.inf.
  --out-dir DIR         New output kit directory. It must not already exist.

Options:
  --source-inx FILE     Audited viogpu3d_arm64.inx. Default:
                        $HOME/BridgeVM/viogpu3d-arehnman/viogpu/viogpu3d/
                        viogpu3d_arm64.inx
  --expected-input-manifest FILE
                        SHA-256 allowlist for the seven input files.
  --expected-source-head SHA
                        Required source Git HEAD. Default is pinned akre HEAD
                        4c27e477e6560cea724d848b98149f03cb1f2083.
  --expected-source-inx-sha256 SHA
                        Required source INX hash.
  --source-head SHA     Explicit source identity when FILE is not in a Git
                        checkout (primarily for hermetic fixtures).
  --source-repo TEXT    Provenance label. Default:
                        arehnman/kvm-guest-drivers-windows.

The result is deliberately not injection-ready: package/ contains no CAT or
certificate. Copy the complete kit to Windows and run
finalize-viogpu3d-test-package.ps1 on an elevated disposable SDK/WDK machine, or
finalize-viogpu3d-package.ps1 with a separately managed code-signing PFX.
EOF
}

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

sha256_file() {
  shasum -a 256 "$1" | awk '{print tolower($1)}'
}

read_bytes_dec() {
  local path="$1"
  local offset="$2"
  local count="$3"
  LC_ALL=C od -An -tu1 -j "$offset" -N "$count" "$path" 2>/dev/null
}

pe_arm64_gate() {
  local path="$1"
  local label="$2"
  local -a bytes
  local -a sig
  local -a machine
  local pe_offset
  local machine_hex

  read -r -a bytes <<<"$(read_bytes_dec "$path" 0 2)"
  (( ${#bytes[@]} == 2 )) || fail "$label is too small to be a PE image: $path"
  [[ "${bytes[0]}" == "77" && "${bytes[1]}" == "90" ]] ||
    fail "$label is not a PE/MZ image: $path"

  read -r -a bytes <<<"$(read_bytes_dec "$path" 60 4)"
  (( ${#bytes[@]} == 4 )) || fail "$label is missing its PE header offset: $path"
  pe_offset=$(( bytes[0] + bytes[1] * 256 + bytes[2] * 65536 + bytes[3] * 16777216 ))

  read -r -a sig <<<"$(read_bytes_dec "$path" "$pe_offset" 4)"
  (( ${#sig[@]} == 4 )) || fail "$label is missing its PE signature: $path"
  [[ "${sig[0]}" == "80" && "${sig[1]}" == "69" &&
     "${sig[2]}" == "0" && "${sig[3]}" == "0" ]] ||
    fail "$label has an invalid PE signature: $path"

  read -r -a machine <<<"$(read_bytes_dec "$path" "$((pe_offset + 4))" 2)"
  (( ${#machine[@]} == 2 )) || fail "$label is missing its PE machine field: $path"
  printf -v machine_hex '0x%02x%02x' "${machine[1]}" "${machine[0]}"
  [[ "$machine_hex" == "0xaa64" ]] ||
    fail "$label is not ARM64 PE (machine=$machine_hex): $path"
}

require_source_contract() {
  local needle
  for needle in \
    'PCI\VEN_1AF4&DEV_1050' \
    'UserModeDriverName' \
    'OpenGLDriverName' \
    'OpenGLFlags' \
    'OpenGLVersion' \
    'InstalledDisplayDrivers' \
    'viogpu_d3d10_arm64.dll' \
    'viogpu_wgl_arm64.dll' \
    'opengl32_arm64.dll' \
    'libEGL_arm64.dll' \
    'libGLESv2_arm64.dll'
  do
    grep -Fqi -- "$needle" "$SOURCE_INX" ||
      fail "source INX is missing required contract '$needle': $SOURCE_INX"
  done
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --input-dir)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      INPUT_DIR="$2"
      shift 2
      ;;
    --out-dir)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      OUT_DIR="$2"
      shift 2
      ;;
    --source-inx)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      SOURCE_INX="$2"
      shift 2
      ;;
    --expected-input-manifest)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      EXPECTED_INPUT_MANIFEST="$2"
      shift 2
      ;;
    --expected-source-head)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      EXPECTED_SOURCE_HEAD="$2"
      shift 2
      ;;
    --expected-source-inx-sha256)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      EXPECTED_SOURCE_INX_SHA256="$(printf '%s' "$2" | tr '[:upper:]' '[:lower:]')"
      shift 2
      ;;
    --source-head)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      SOURCE_HEAD_OVERRIDE="$2"
      shift 2
      ;;
    --source-repo)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      SOURCE_REPO="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

[[ -n "$INPUT_DIR" ]] || { usage; exit 2; }
[[ -n "$OUT_DIR" ]] || { usage; exit 2; }
[[ -n "$SOURCE_REPO" && "$SOURCE_REPO" != *$'\n'* && "$SOURCE_REPO" != *$'\r'* ]] ||
  fail "source repo provenance label must be one non-empty line"
[[ -d "$INPUT_DIR" ]] || fail "input directory does not exist: $INPUT_DIR"
[[ -f "$SOURCE_INX" && ! -L "$SOURCE_INX" ]] || fail "source INX is missing or is a symlink: $SOURCE_INX"
[[ -f "$EXPECTED_INPUT_MANIFEST" && ! -L "$EXPECTED_INPUT_MANIFEST" ]] ||
  fail "expected input manifest is missing or is a symlink: $EXPECTED_INPUT_MANIFEST"
[[ -f "$CANONICAL_INF" && ! -L "$CANONICAL_INF" ]] ||
  fail "canonical minimal INF is missing or is a symlink: $CANONICAL_INF"
[[ ! -e "$OUT_DIR" ]] || fail "output path already exists: $OUT_DIR"

INPUT_DIR="$(cd "$INPUT_DIR" && pwd -P)"
SOURCE_INX="$(cd "$(dirname "$SOURCE_INX")" && pwd -P)/$(basename "$SOURCE_INX")"
EXPECTED_INPUT_MANIFEST="$(cd "$(dirname "$EXPECTED_INPUT_MANIFEST")" && pwd -P)/$(basename "$EXPECTED_INPUT_MANIFEST")"
canonical_inf_sha256="$(sha256_file "$CANONICAL_INF")"
[[ "$canonical_inf_sha256" == "$EXPECTED_CANONICAL_INF_SHA256" ]] ||
  fail "canonical minimal INF hash mismatch: expected $EXPECTED_CANONICAL_INF_SHA256, got $canonical_inf_sha256"

source_head="$SOURCE_HEAD_OVERRIDE"
source_root=""
if source_root="$(git -C "$(dirname "$SOURCE_INX")" rev-parse --show-toplevel 2>/dev/null)"; then
  git -C "$source_root" diff --quiet --ignore-submodules -- ||
    fail "source checkout has tracked changes: $source_root"
  git -C "$source_root" diff --cached --quiet --ignore-submodules -- ||
    fail "source checkout has staged changes: $source_root"
  source_head="$(git -C "$source_root" rev-parse HEAD)"
fi
[[ -n "$source_head" ]] || fail "source INX is not in Git; pass --source-head explicitly"
[[ "$source_head" == "$EXPECTED_SOURCE_HEAD" ]] ||
  fail "source HEAD mismatch: expected $EXPECTED_SOURCE_HEAD, got $source_head"

source_inx_sha256="$(sha256_file "$SOURCE_INX")"
[[ "$source_inx_sha256" == "$EXPECTED_SOURCE_INX_SHA256" ]] ||
  fail "source INX hash mismatch: expected $EXPECTED_SOURCE_INX_SHA256, got $source_inx_sha256"
require_source_contract

required_files=(
  viogpu3d.inf
  viogpu3d.sys
  viogpu_d3d10_arm64.dll
  viogpu_wgl_arm64.dll
  opengl32_arm64.dll
  libEGL_arm64.dll
  libGLESv2_arm64.dll
)

manifest_lines="$(awk 'NF { count += 1 } END { print count + 0 }' "$EXPECTED_INPUT_MANIFEST")"
[[ "$manifest_lines" == "${#required_files[@]}" ]] ||
  fail "input manifest must contain exactly ${#required_files[@]} non-empty entries: $EXPECTED_INPUT_MANIFEST"

for name in "${required_files[@]}"; do
  [[ -f "$INPUT_DIR/$name" && ! -L "$INPUT_DIR/$name" ]] ||
    fail "required input is missing or is a symlink: $INPUT_DIR/$name"
  LC_ALL=C grep -Eq "^[0-9A-Fa-f]{64}  ${name//./\.}$" "$EXPECTED_INPUT_MANIFEST" ||
    fail "input manifest has no unique pinned entry for $name"
done

while read -r expected_hash name extra; do
  [[ -n "${expected_hash:-}" ]] || continue
  [[ -z "${extra:-}" ]] || fail "invalid input manifest line for $name"
  case " $name " in
    *" viogpu3d.inf "*|*" viogpu3d.sys "*|*" viogpu_d3d10_arm64.dll "*|*" viogpu_wgl_arm64.dll "*|*" opengl32_arm64.dll "*|*" libEGL_arm64.dll "*|*" libGLESv2_arm64.dll "*) ;;
    *) fail "unexpected filename in input manifest: $name" ;;
  esac
  expected_hash="$(printf '%s' "$expected_hash" | tr '[:upper:]' '[:lower:]')"
  actual_hash="$(sha256_file "$INPUT_DIR/$name")"
  [[ "$actual_hash" == "$expected_hash" ]] ||
    fail "input hash mismatch for $name: expected $expected_hash, got $actual_hash"
done < "$EXPECTED_INPUT_MANIFEST"

top_level_binary_count="$(find "$INPUT_DIR" -maxdepth 1 -type f \( -iname '*.sys' -o -iname '*.dll' \) | wc -l | tr -d '[:space:]')"
[[ "$top_level_binary_count" == "6" ]] ||
  fail "profile requires exactly one SYS and five DLL inputs; found $top_level_binary_count"
nested_binary_count="$(find "$INPUT_DIR" -mindepth 2 -type f \( -iname '*.sys' -o -iname '*.dll' -o -iname '*.inf' \) | wc -l | tr -d '[:space:]')"
[[ "$nested_binary_count" == "0" ]] || fail "nested INF/SYS/DLL inputs are not allowed"

for name in "${required_files[@]:1}"; do
  pe_arm64_gate "$INPUT_DIR/$name" "$name"
done

out_parent="$(dirname "$OUT_DIR")"
out_name="$(basename "$OUT_DIR")"
mkdir -p "$out_parent"
out_parent="$(cd "$out_parent" && pwd -P)"
[[ "$out_name" != "." && "$out_name" != ".." && -n "$out_name" ]] ||
  fail "invalid output directory name: $out_name"
OUT_DIR="$out_parent/$out_name"
tmp_dir="$(mktemp -d "$out_parent/.${out_name}.staging.XXXXXX")"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

package_dir="$tmp_dir/package"
mkdir -p "$package_dir"
for name in "${required_files[@]:1}"; do
  cp "$INPUT_DIR/$name" "$package_dir/$name"
done

cp "$CANONICAL_INF" "$package_dir/viogpu3d.inf"

input_manifest_sha256="$(sha256_file "$EXPECTED_INPUT_MANIFEST")"
stage_id="arehnman-arm64-minimal-${input_manifest_sha256:0:16}"
cat > "$package_dir/bridgevm-package-provenance.env" <<EOF
VIOGPU3D_SOURCE_REPO=$SOURCE_REPO
VIOGPU3D_SOURCE_REF=akre@$source_head source-inx-sha256=$source_inx_sha256
VIOGPU3D_BUILD_ID=$stage_id
VIOGPU3D_SIGNING_CERT=<pending-windows-wdk-finalization>
VIOGPU3D_PROTOCOL=virgl
VIOGPU3D_PCI_DEVICE_ID=1050
EOF

cp "$ROOT/scripts/finalize-hvf-windows-viogpu3d-package.ps1" \
  "$tmp_dir/finalize-viogpu3d-package.ps1"
cp "$ROOT/scripts/finalize-hvf-windows-viogpu3d-test-package.ps1" \
  "$tmp_dir/finalize-viogpu3d-test-package.ps1"

{
  printf 'BridgeVM viogpu3d render-package stage\n'
  printf 'profile=%s\n' "$PROFILE"
  printf 'source_repo=%s\n' "$SOURCE_REPO"
  printf 'source_head=%s\n' "$source_head"
  printf 'source_inx=%s\n' "$SOURCE_INX"
  printf 'source_inx_sha256=%s\n' "$source_inx_sha256"
  printf 'canonical_inf_sha256=%s\n' "$canonical_inf_sha256"
  printf 'input_dir=%s\n' "$INPUT_DIR"
  printf 'input_manifest=%s\n' "$EXPECTED_INPUT_MANIFEST"
  printf 'input_manifest_sha256=%s\n' "$input_manifest_sha256"
  printf 'stage_id=%s\n' "$stage_id"
  printf 'package_dir=package\n'
  printf 'pre_finalization_manifest=pre-finalization-sha256.txt\n'
  printf 'default_finalized_dir=package-finalized\n'
  printf 'package_capability=unsigned-umd-registered-stage\n'
  printf 'catalog_present=false\n'
  printf 'signing_certificate_present=false\n'
  printf 'finalization_required=true\n'
  printf 'render_candidate_verified=false\n'
} > "$tmp_dir/stage-report.txt"

(
  cd "$package_dir"
  for name in *; do
    [[ -f "$name" ]] || continue
    shasum -a 256 "$name"
  done
) > "$tmp_dir/pre-finalization-sha256.txt"

cat > "$tmp_dir/README.txt" <<'EOF'
BridgeVM viogpu3d ARM64 render package finalization kit

package/ is an unsigned, UMD-registered staging directory. It deliberately has
no viogpu3d.cat and no certificate; do not inject it yet.

On an elevated disposable Windows machine with matching Windows SDK and WDK,
the shortest test-only path is:

  powershell -ExecutionPolicy Bypass -File .\finalize-viogpu3d-test-package.ps1

That wrapper creates and trusts an ephemeral Code Signing certificate, keeps
its password inside the PowerShell process, deletes the private PFX, and leaves
public certificate trust in place for a live install in the same test VM. Its
report marks test_signing_required=true; enable Windows TESTSIGNING and reboot
before installing. A self-signed test root is not a Microsoft kernel-policy root.

To use a separately managed code-signing PFX instead, run:

  powershell -ExecutionPolicy Bypass -File .\finalize-viogpu3d-package.ps1 `
    -PackageDir .\package `
    -PreFinalizationManifest .\pre-finalization-sha256.txt `
    -CertificatePfx C:\path\BridgeVM-Test.pfx

Set VIOGPU3D_CERTIFICATE_PASSWORD instead of placing a PFX password in shell
history. Install both kits: the WDK supplies InfVerif and Inf2Cat, and the
Windows SDK supplies SignTool. The finalizer discovers their Windows Kits
bin/Tools locations even when the current process PATH predates installation. It
runs InfVerif, signs viogpu3d.sys and all five UMD DLLs,
regenerates the catalog with Inf2Cat, signs and verifies the artifacts, exports
the public CER, and writes the result transactionally to package-finalized/. By
default a managed PFX must already chain to a trusted kernel-policy root so
SignTool /kp can verify it. Pass -TestSigning only for an explicitly test-mode
package; that path verifies Authenticode and reports /kp as skipped, never passed.
The unsigned package/ input is never modified.

Copy package-finalized/ back to the Mac, then require the repository gate:

  scripts/check-hvf-windows-viogpu3d-package.sh \
    --pci-device-id 1050 --require-render-candidate /path/to/package-finalized

Passing that gate proves package structure, not live Windows bind or rendering.
EOF

mv "$tmp_dir" "$OUT_DIR"
trap - EXIT
printf 'BridgeVM viogpu3d render-package stage\n'
printf 'profile=%s\n' "$PROFILE"
printf 'out_dir=%s\n' "$OUT_DIR"
printf 'package_dir=%s/package\n' "$OUT_DIR"
printf 'finalization_required=true\n'
printf 'render_candidate_verified=false\n'
