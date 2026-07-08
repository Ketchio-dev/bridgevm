#!/usr/bin/env bash
# Validate a Windows ARM64 viogpu3d package before offline injection.
set -euo pipefail

VIOGPU3D_DIR="${VIOGPU3D_DIR:-}"
VIOGPU3D_PROTOCOL="${VIOGPU3D_PROTOCOL:-auto}"
VIOGPU3D_MANIFEST="${VIOGPU3D_MANIFEST:-}"
VIOGPU3D_PCI_DEVICE_ID="${VIOGPU3D_PCI_DEVICE_ID:-}"
VIOGPU3D_PROVENANCE="${VIOGPU3D_PROVENANCE:-}"
VIOGPU3D_SOURCE_REPO="${VIOGPU3D_SOURCE_REPO:-<unknown>}"
VIOGPU3D_SOURCE_REF="${VIOGPU3D_SOURCE_REF:-<unknown>}"
VIOGPU3D_BUILD_ID="${VIOGPU3D_BUILD_ID:-<unknown>}"
VIOGPU3D_SIGNING_CERT="${VIOGPU3D_SIGNING_CERT:-<unknown>}"
VIOGPU3D_PROVENANCE_EXPLICIT=0
[[ -z "$VIOGPU3D_PROVENANCE" ]] || VIOGPU3D_PROVENANCE_EXPLICIT=1
loaded_provenance="<none>"
provenance_protocol_loaded=0

usage() {
  cat >&2 <<'EOF'
usage: VIOGPU3D_DIR=/path/to/viogpu3d-package scripts/check-hvf-windows-viogpu3d-package.sh
       scripts/check-hvf-windows-viogpu3d-package.sh [--manifest PATH] [--provenance PATH] [--pci-device-id 1050|10f7] /path/to/viogpu3d-package

Environment:
  VIOGPU3D_DIR       Directory containing viogpu3d .inf/.sys/.cat files. May
                     also be passed as the first positional argument.
  VIOGPU3D_PROTOCOL  auto, venus, or virgl. Default: auto. Use venus/virgl only
                     after auditing the package/source when auto is ambiguous.
  VIOGPU3D_MANIFEST  Optional path for a text manifest containing package file
                     hashes, sizes, PE machine fields, and source metadata.
  VIOGPU3D_PROVENANCE
                     Optional provenance env file. When unset, the checker
                     auto-loads bridgevm-package-provenance.env from the package
                     directory if present. Explicit --provenance or env path
                     must exist.
  VIOGPU3D_PCI_DEVICE_ID
                     Optional expected virtio-gpu PCI device id, 1050 or 10f7.
                     When unset, either id is accepted and recorded.
  VIOGPU3D_SOURCE_REPO / VIOGPU3D_SOURCE_REF / VIOGPU3D_BUILD_ID /
  VIOGPU3D_SIGNING_CERT
                     Optional provenance fields copied into the manifest.
EOF
}

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

read_bytes_dec() {
  local path="$1"
  local offset="$2"
  local count="$3"
  LC_ALL=C od -An -tu1 -j "$offset" -N "$count" "$path" 2>/dev/null
}

pe_machine_hex() {
  local path="$1"
  local label="$2"
  local -a bytes
  local -a sig
  local -a machine
  local pe_offset

  read -r -a bytes <<<"$(read_bytes_dec "$path" 0 2)"
  (( ${#bytes[@]} == 2 )) || fail "$label is too small to be a PE image: $path"
  [[ "${bytes[0]}" == "77" && "${bytes[1]}" == "90" ]] || {
    fail "$label is not a PE/MZ image: $path"
  }

  read -r -a bytes <<<"$(read_bytes_dec "$path" 60 4)"
  (( ${#bytes[@]} == 4 )) || fail "$label missing PE header offset: $path"
  pe_offset=$(( bytes[0] + bytes[1] * 256 + bytes[2] * 65536 + bytes[3] * 16777216 ))

  read -r -a sig <<<"$(read_bytes_dec "$path" "$pe_offset" 4)"
  (( ${#sig[@]} == 4 )) || fail "$label missing PE signature: $path"
  [[ "${sig[0]}" == "80" && "${sig[1]}" == "69" && "${sig[2]}" == "0" && "${sig[3]}" == "0" ]] || {
    fail "$label has an invalid PE signature: $path"
  }

  read -r -a machine <<<"$(read_bytes_dec "$path" "$((pe_offset + 4))" 2)"
  (( ${#machine[@]} == 2 )) || fail "$label missing PE machine field: $path"
  printf '0x%02x%02x\n' "${machine[1]}" "${machine[0]}"
}

pe_arm64_machine_gate() {
  local path="$1"
  local label="$2"
  local machine_hex

  machine_hex="$(pe_machine_hex "$path" "$label")"
  [[ "$machine_hex" == "0xaa64" ]] || {
    fail "$label is not ARM64 PE (machine=$machine_hex): $path"
  }
}

reject_whitespace_path() {
  case "$2" in
    *[[:space:]]*) fail "$1 path contains whitespace, unsupported by DRIVER_DIRS: $2" ;;
  esac
}

apply_provenance_default() {
  local key="$1"
  local value="$2"
  case "$key" in
    VIOGPU3D_SOURCE_REPO)
      [[ "$VIOGPU3D_SOURCE_REPO" != "<unknown>" ]] || VIOGPU3D_SOURCE_REPO="$value"
      ;;
    VIOGPU3D_SOURCE_REF)
      [[ "$VIOGPU3D_SOURCE_REF" != "<unknown>" ]] || VIOGPU3D_SOURCE_REF="$value"
      ;;
    VIOGPU3D_BUILD_ID)
      [[ "$VIOGPU3D_BUILD_ID" != "<unknown>" ]] || VIOGPU3D_BUILD_ID="$value"
      ;;
    VIOGPU3D_SIGNING_CERT)
      [[ "$VIOGPU3D_SIGNING_CERT" != "<unknown>" ]] || VIOGPU3D_SIGNING_CERT="$value"
      ;;
    VIOGPU3D_PROTOCOL)
      if [[ "$VIOGPU3D_PROTOCOL" == "auto" && -n "$value" ]]; then
        VIOGPU3D_PROTOCOL="$value"
        provenance_protocol_loaded=1
      fi
      ;;
    VIOGPU3D_PCI_DEVICE_ID)
      [[ -n "$VIOGPU3D_PCI_DEVICE_ID" || -z "$value" ]] || VIOGPU3D_PCI_DEVICE_ID="$value"
      ;;
    *)
      fail "unsupported provenance key $key in $loaded_provenance"
      ;;
  esac
}

load_provenance_defaults() {
  local path="$1"
  local explicit="$2"
  local line
  local key
  local value

  if [[ ! -f "$path" ]]; then
    [[ "$explicit" == "0" ]] || fail "provenance file not found: $path"
    return 0
  fi

  loaded_provenance="$path"
  while IFS= read -r line || [[ -n "$line" ]]; do
    line="${line%$'\r'}"
    [[ -n "$line" ]] || continue
    [[ "$line" != \#* ]] || continue
    [[ "$line" == *=* ]] || fail "invalid provenance line in $path: $line"
    key="${line%%=*}"
    value="${line#*=}"
    [[ "$key" =~ ^[A-Z0-9_]+$ ]] || fail "invalid provenance key in $path: $key"
    apply_provenance_default "$key" "$value"
  done < "$path"
}

normalize_pci_device_id() {
  local value="$1"
  local upper
  upper="$(printf '%s\n' "$value" | tr '[:lower:]' '[:upper:]')"
  upper="${upper#0X}"
  case "$upper" in
    1050|10F7) printf '%s\n' "$upper" ;;
    *) fail "VIOGPU3D_PCI_DEVICE_ID must be 1050 or 10f7, got: $value" ;;
  esac
}

append_unique_hwid() {
  local id="$1"
  local existing
  for existing in ${viogpu3d_hwids:-}; do
    [[ "$existing" == "$id" ]] && return 0
  done
  viogpu3d_hwids="${viogpu3d_hwids:+$viogpu3d_hwids }$id"
}

detect_hwid_device_ids() {
  local inf
  local match
  local id
  viogpu3d_hwids=""
  for inf in "${viogpu3d_infs[@]}"; do
    while IFS= read -r match; do
      [[ -n "$match" ]] || continue
      id="$(printf '%s\n' "$match" | grep -Eio 'DEV_[0-9A-F]{4}' | head -n 1 | cut -d_ -f2 | tr '[:lower:]' '[:upper:]')"
      case "$id" in
        1050|10F7) append_unique_hwid "$id" ;;
      esac
    done < <(
      LC_ALL=C grep -Eio 'VEN_1AF4[^[:space:],;]*DEV_(1050|10F7)|DEV_(1050|10F7)[^[:space:],;]*VEN_1AF4' "$inf" 2>/dev/null || true
    )
  done
}

join_hwids() {
  local sep=""
  local id
  for id in ${viogpu3d_hwids:-}; do
    printf '%sPCI\\VEN_1AF4&DEV_%s' "$sep" "$id"
    sep=","
  done
  printf '\n'
}

hwid_contains() {
  local expected="$1"
  local id
  for id in ${viogpu3d_hwids:-}; do
    [[ "$id" == "$expected" ]] && return 0
  done
  return 1
}

detect_protocol() {
  local venus_hit=0
  local virgl_hit=0

  if grep -Eaiq 'venus|vulkan|capset[^[:alnum:]]*4' "${scan_files[@]}" 2>/dev/null; then
    venus_hit=1
  fi
  if grep -Eaiq 'virgl|d3d10|d3d10umd|wgl|opengl|gallium' "${scan_files[@]}" 2>/dev/null; then
    virgl_hit=1
  fi

  if (( venus_hit == 1 && virgl_hit == 0 )); then
    printf 'venus\n'
  elif (( virgl_hit == 1 && venus_hit == 0 )); then
    printf 'virgl\n'
  elif (( venus_hit == 1 && virgl_hit == 1 )); then
    printf 'mixed\n'
  else
    printf 'unknown\n'
  fi
}

sha256_for_file() {
  shasum -a 256 "$1" | awk '{ print $1 }'
}

size_for_file() {
  wc -c < "$1" | tr -d '[:space:]'
}

write_manifest_entry() {
  local role="$1"
  local path="$2"
  local machine="$3"
  printf 'file=%s\tsha256=%s\tsize=%s\tpe_machine=%s\tpath=%s\n' \
    "$role" "$(sha256_for_file "$path")" "$(size_for_file "$path")" "$machine" "$path"
}

write_manifest() {
  local manifest="$1"
  local tmp
  local file
  tmp="$manifest.tmp.$$"
  {
    printf 'BridgeVM viogpu3d package manifest\n'
    printf 'generated_utc=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    printf 'dir=%s\n' "$VIOGPU3D_DIR"
    printf 'provenance=%s\n' "$loaded_provenance"
    printf 'source_repo=%s\n' "$VIOGPU3D_SOURCE_REPO"
    printf 'source_ref=%s\n' "$VIOGPU3D_SOURCE_REF"
    printf 'build_id=%s\n' "$VIOGPU3D_BUILD_ID"
    printf 'signing_cert=%s\n' "$VIOGPU3D_SIGNING_CERT"
    printf 'hwids=%s\n' "$(join_hwids)"
    printf 'expected_hwid=%s\n' "${VIOGPU3D_PCI_DEVICE_ID:+PCI\\VEN_1AF4&DEV_$VIOGPU3D_PCI_DEVICE_ID}"
    printf 'protocol=%s\n' "$detected_protocol"
    printf 'protocol_source=%s\n' "$protocol_source"
    printf 'auto_protocol=%s\n' "$auto_protocol"
    printf 'inf_count=%s\n' "${#viogpu3d_infs[@]}"
    printf 'sys_count=%s\n' "${#viogpu3d_sys[@]}"
    printf 'cat_count=%s\n' "${#viogpu3d_cats[@]}"
    printf 'dll_count=%s\n' "${#viogpu3d_dlls[@]}"
    for file in "${viogpu3d_infs[@]}"; do
      write_manifest_entry inf "$file" n/a
    done
    for file in "${viogpu3d_sys[@]}"; do
      write_manifest_entry sys "$file" "$(pe_machine_hex "$file" "viogpu3d SYS")"
    done
    for file in "${viogpu3d_cats[@]}"; do
      write_manifest_entry cat "$file" n/a
    done
    if (( ${#viogpu3d_dlls[@]} > 0 )); then
      for file in "${viogpu3d_dlls[@]}"; do
        write_manifest_entry dll "$file" "$(pe_machine_hex "$file" "viogpu3d DLL")"
      done
    fi
  } > "$tmp"
  mv "$tmp" "$manifest"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --manifest)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      VIOGPU3D_MANIFEST="$2"
      shift 2
      ;;
    --provenance)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      VIOGPU3D_PROVENANCE="$2"
      VIOGPU3D_PROVENANCE_EXPLICIT=1
      shift 2
      ;;
    --pci-device-id)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      VIOGPU3D_PCI_DEVICE_ID="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    -*)
      usage
      exit 2
      ;;
    *)
      [[ -z "$VIOGPU3D_DIR" || "$VIOGPU3D_DIR" == "$1" ]] || fail "multiple viogpu3d package directories specified"
      VIOGPU3D_DIR="$1"
      shift
      ;;
  esac
done

[[ -n "$VIOGPU3D_DIR" ]] || { usage; fail "VIOGPU3D_DIR is required"; }
[[ -d "$VIOGPU3D_DIR" ]] || fail "viogpu3d driver directory not found: $VIOGPU3D_DIR"
if [[ -z "$VIOGPU3D_PROVENANCE" ]]; then
  VIOGPU3D_PROVENANCE="$VIOGPU3D_DIR/bridgevm-package-provenance.env"
fi
load_provenance_defaults "$VIOGPU3D_PROVENANCE" "$VIOGPU3D_PROVENANCE_EXPLICIT"
case "$VIOGPU3D_PROTOCOL" in
  auto|venus|virgl) ;;
  *) fail "VIOGPU3D_PROTOCOL must be auto, venus, or virgl" ;;
esac
if [[ -n "$VIOGPU3D_PCI_DEVICE_ID" ]]; then
  VIOGPU3D_PCI_DEVICE_ID="$(normalize_pci_device_id "$VIOGPU3D_PCI_DEVICE_ID")"
fi

reject_whitespace_path VIOGPU3D_DIR "$VIOGPU3D_DIR"

viogpu3d_infs=()
viogpu3d_sys=()
viogpu3d_cats=()
viogpu3d_dlls=()
viogpu3d_hwids=""
shopt -s nullglob
for file in "$VIOGPU3D_DIR"/*.inf "$VIOGPU3D_DIR"/*.INF; do viogpu3d_infs+=("$file"); done
for file in "$VIOGPU3D_DIR"/*.sys "$VIOGPU3D_DIR"/*.SYS; do viogpu3d_sys+=("$file"); done
for file in "$VIOGPU3D_DIR"/*.cat "$VIOGPU3D_DIR"/*.CAT; do viogpu3d_cats+=("$file"); done
for file in "$VIOGPU3D_DIR"/*.dll "$VIOGPU3D_DIR"/*.DLL; do viogpu3d_dlls+=("$file"); done
shopt -u nullglob

(( ${#viogpu3d_infs[@]} > 0 )) || fail "no .inf found in $VIOGPU3D_DIR"
(( ${#viogpu3d_sys[@]} > 0 )) || fail "no .sys found in $VIOGPU3D_DIR"
(( ${#viogpu3d_cats[@]} > 0 )) || fail "no .cat catalog found in $VIOGPU3D_DIR"
detect_hwid_device_ids
[[ -n "$viogpu3d_hwids" ]] ||
  fail "viogpu3d INF/INX does not advertise a supported VirtIO GPU HWID: PCI\\VEN_1AF4&DEV_1050 or PCI\\VEN_1AF4&DEV_10F7"
if [[ -n "$VIOGPU3D_PCI_DEVICE_ID" ]]; then
  hwid_contains "$VIOGPU3D_PCI_DEVICE_ID" ||
    fail "viogpu3d INF/INX does not advertise expected PCI\\VEN_1AF4&DEV_$VIOGPU3D_PCI_DEVICE_ID; package hwids=$(join_hwids)"
fi

for sys in "${viogpu3d_sys[@]}"; do
  pe_arm64_machine_gate "$sys" "viogpu3d SYS"
done
if (( ${#viogpu3d_dlls[@]} > 0 )); then
  for dll in "${viogpu3d_dlls[@]}"; do
    pe_arm64_machine_gate "$dll" "viogpu3d DLL"
  done
fi

scan_files=("${viogpu3d_infs[@]}" "${viogpu3d_sys[@]}")
if (( ${#viogpu3d_dlls[@]} > 0 )); then
  scan_files+=("${viogpu3d_dlls[@]}")
fi
auto_protocol="$(detect_protocol)"
detected_protocol="$auto_protocol"
protocol_source="auto"
if [[ "$VIOGPU3D_PROTOCOL" != "auto" ]]; then
  if [[ "$auto_protocol" == "venus" || "$auto_protocol" == "virgl" ]]; then
    [[ "$auto_protocol" == "$VIOGPU3D_PROTOCOL" ]] || {
      fail "VIOGPU3D_PROTOCOL=$VIOGPU3D_PROTOCOL conflicts with package scan protocol=$auto_protocol"
    }
  fi
  detected_protocol="$VIOGPU3D_PROTOCOL"
  if [[ "$provenance_protocol_loaded" == "1" ]]; then
    protocol_source="provenance"
  else
    protocol_source="env"
  fi
fi
case "$detected_protocol" in
  venus|virgl) ;;
  mixed) fail "viogpu3d protocol is ambiguous; set VIOGPU3D_PROTOCOL=venus or virgl after package/source audit" ;;
  *) fail "could not identify viogpu3d protocol; set VIOGPU3D_PROTOCOL=venus or virgl after package/source audit" ;;
esac

printf 'BridgeVM viogpu3d package check\n'
printf 'dir=%s\n' "$VIOGPU3D_DIR"
printf 'inf_count=%s\n' "${#viogpu3d_infs[@]}"
printf 'sys_count=%s\n' "${#viogpu3d_sys[@]}"
printf 'cat_count=%s\n' "${#viogpu3d_cats[@]}"
printf 'dll_count=%s\n' "${#viogpu3d_dlls[@]}"
printf 'provenance=%s\n' "$loaded_provenance"
printf 'source_repo=%s\n' "$VIOGPU3D_SOURCE_REPO"
printf 'source_ref=%s\n' "$VIOGPU3D_SOURCE_REF"
printf 'build_id=%s\n' "$VIOGPU3D_BUILD_ID"
printf 'signing_cert=%s\n' "$VIOGPU3D_SIGNING_CERT"
printf 'hwids=%s\n' "$(join_hwids)"
printf 'expected_hwid=%s\n' "${VIOGPU3D_PCI_DEVICE_ID:+PCI\\VEN_1AF4&DEV_$VIOGPU3D_PCI_DEVICE_ID}"
printf 'protocol=%s\n' "$detected_protocol"
printf 'protocol_source=%s\n' "$protocol_source"
if [[ -n "$VIOGPU3D_MANIFEST" ]]; then
  write_manifest "$VIOGPU3D_MANIFEST"
  printf 'manifest=%s\n' "$VIOGPU3D_MANIFEST"
fi
if (( ${#viogpu3d_dlls[@]} == 0 )); then
  printf 'warning=no viogpu3d .dll files found; package appears KMD-only\n'
fi
printf 'PASS: viogpu3d package is injection-ready\n'
