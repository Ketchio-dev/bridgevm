#!/usr/bin/env bash
# Validate and stage the user-supplied signed ARM64 virtio packages plus the
# BridgeVM-owned guest agent. This proves payload identity and CMS integrity;
# Windows trust and live device binding remain guest/live acceptance criteria.
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: stage-hvf-windows-guest-payload.sh --payload-dir DIR --manifest TSV --assets DIR --output DIR --catalog-verifier PATH

The manifest must use bridgevm-windows-guest-payload-v1 and list exactly the
storage, serial and network roles. The output path must not already exist.
EOF
}
fail() {
  printf 'BLOCKER[%s]: %s\n' "$1" "$2" >&2
  exit 1
}
PAYLOAD_DIR=""
MANIFEST=""
ASSETS=""
OUTPUT=""
CATALOG_VERIFIER=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --payload-dir) [[ $# -ge 2 ]] || { usage; exit 2; }; PAYLOAD_DIR="$2"; shift 2 ;;
    --manifest) [[ $# -ge 2 ]] || { usage; exit 2; }; MANIFEST="$2"; shift 2 ;;
    --assets) [[ $# -ge 2 ]] || { usage; exit 2; }; ASSETS="$2"; shift 2 ;;
    --output) [[ $# -ge 2 ]] || { usage; exit 2; }; OUTPUT="$2"; shift 2 ;;
    --catalog-verifier) [[ $# -ge 2 ]] || { usage; exit 2; }; CATALOG_VERIFIER="$2"; shift 2 ;;
    --help|-h) usage; exit 0 ;;
    *) usage; exit 2 ;;
  esac
done
[[ "$PAYLOAD_DIR" == /* && -d "$PAYLOAD_DIR" && ! -L "$PAYLOAD_DIR" ]] || \
  fail guest-payload-missing "payload directory must be an absolute, non-symlink directory"
[[ "$MANIFEST" == /* && -f "$MANIFEST" && ! -L "$MANIFEST" ]] || \
  fail guest-payload-manifest-missing "manifest must be an absolute, non-symlink file"
[[ "$ASSETS" == /* && -d "$ASSETS" && ! -L "$ASSETS" ]] || \
  fail guest-tools-missing "BridgeVM asset directory is unavailable"
[[ "$OUTPUT" == /* && ! -e "$OUTPUT" ]] || \
  fail guest-payload-output "output must be an absent absolute path"
[[ "$CATALOG_VERIFIER" == /* && -f "$CATALOG_VERIFIER" && -x "$CATALOG_VERIFIER" && ! -L "$CATALOG_VERIFIER" ]] || \
  fail guest-payload-signature-tool "catalog verifier must be an absolute regular executable"

sha256_file() {
  /usr/bin/shasum -a 256 "$1" | /usr/bin/awk '{print $1}'
}

payload_real="$(cd "$PAYLOAD_DIR" && pwd -P)"
manifest_real="$(cd "$(dirname "$MANIFEST")" && pwd -P)/$(basename "$MANIFEST")"
case "$manifest_real" in
  "$payload_real"/*) fail guest-payload-manifest-location "manifest must be outside the payload directory" ;;
esac
if find "$PAYLOAD_DIR" -type l -print -quit | grep -q .; then
  fail guest-payload-symlink "payload tree contains a symlink"
fi
if find "$PAYLOAD_DIR" ! -type f ! -type d -print -quit | grep -q .; then fail guest-payload-file-set "payload tree contains a non-regular entry"; fi
work="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-guest-payload.XXXXXX")"
trap 'rm -rf "$work"' EXIT
manifest_hash="$(sha256_file "$MANIFEST")"
files="$work/files.tsv"
drivers="$work/drivers.tsv"
listed="$work/listed.txt"
actual="$work/actual.txt"
binaries="$work/binaries.txt"
: > "$files"; : > "$drivers"; : > "$binaries"
schema_count=0
architecture_count=0

valid_path() {
  local path="$1"
  [[ "$path" =~ ^[A-Za-z0-9][A-Za-z0-9._/-]*$ ]] || return 1
  [[ "$path" != */../* && "$path" != ../* && "$path" != */.. && "$path" != */./* ]]
}

while IFS=$'\t' read -r kind a b c d extra || [[ -n "${kind}${a}${b}${c}${d}${extra}" ]]; do
  [[ -z "$kind" || "$kind" == \#* ]] && continue
  case "$kind" in
    schema)
      [[ "$a" == bridgevm-windows-guest-payload-v1 && -z "${b}${c}${d}${extra}" ]] || \
        fail guest-payload-schema "unsupported or malformed schema record"
      schema_count=$((schema_count + 1))
      ;;
    architecture)
      [[ "$a" == arm64 && -z "${b}${c}${d}${extra}" ]] || \
        fail guest-payload-architecture "payload architecture must be arm64"
      architecture_count=$((architecture_count + 1))
      ;;
    driver)
      [[ "$a" == storage || "$a" == serial || "$a" == network ]] || \
        fail guest-payload-roles "unknown driver role: $a"
      valid_path "$b" && valid_path "$c" || \
        fail guest-payload-path "driver paths must be safe relative paths"
      [[ "$b" == *.inf && "$c" == *.cat && -n "$d" && -z "$extra" ]] || \
        fail guest-payload-schema "driver record requires role, INF, CAT and binary list"
      printf '%s\t%s\t%s\t%s\n' "$a" "$b" "$c" "$d" >> "$drivers"
      IFS=',' read -r -a binary_items <<< "$d"
      for binary in "${binary_items[@]}"; do
        valid_path "$binary" || fail guest-payload-path "unsafe binary path: $binary"
        case "$binary" in *.sys|*.dll|*.exe) ;; *) fail guest-payload-schema "unsupported driver binary: $binary" ;; esac
        printf '%s\n' "$binary" >> "$binaries"
      done
      ;;
    file)
      valid_path "$a" || fail guest-payload-path "unsafe file path: $a"
      [[ "$b" =~ ^[0-9a-f]{64}$ && -z "${c}${d}${extra}" ]] || \
        fail guest-payload-hash "file record requires a lowercase SHA-256: $a"
      printf '%s\t%s\n' "$a" "$b" >> "$files"
      ;;
    *) fail guest-payload-schema "unknown manifest record: $kind" ;;
  esac
done < <(tr -d '\r' < "$MANIFEST")

[[ "$schema_count" == 1 && "$architecture_count" == 1 ]] || \
  fail guest-payload-schema "manifest needs exactly one schema and architecture record"
[[ "$(cut -f1 "$drivers" | LC_ALL=C sort | tr '\n' ',')" == "network,serial,storage," ]] || \
  fail guest-payload-roles "exactly one storage, serial and network driver is required"
[[ -s "$files" ]] || fail guest-payload-schema "manifest has no files"
if cut -f1 "$files" | tr '[:upper:]' '[:lower:]' | LC_ALL=C sort | uniq -d | grep -q .; then
  fail guest-payload-path "manifest contains duplicate case-insensitive file paths"
fi
if LC_ALL=C sort "$binaries" | uniq -d | grep -q .; then
  fail guest-payload-schema "a binary is assigned to more than one driver"
fi

(cd "$PAYLOAD_DIR" && find . -type f -print | sed 's#^\./##' | LC_ALL=C sort) > "$actual"
cut -f1 "$files" | LC_ALL=C sort > "$listed"
while IFS= read -r path; do
  valid_path "$path" || fail guest-payload-path "payload contains an unsafe path: $path"
done < "$actual"
cmp -s "$listed" "$actual" || fail guest-payload-file-set "payload has missing or unmanifested files"
(( $(wc -l < "$listed") <= 4096 )) || fail guest-payload-file-set "payload contains more than 4096 files"

total_bytes=0
while IFS=$'\t' read -r path expected; do
  source_path="$PAYLOAD_DIR/$path"
  [[ -f "$source_path" && ! -L "$source_path" ]] || fail guest-payload-file-set "missing regular file: $path"
  bytes="$(wc -c < "$source_path" | tr -d ' ')"
  (( bytes <= 268435456 )) || fail guest-payload-file-size "payload file exceeds 256 MiB: $path"
  total_bytes=$((total_bytes + bytes))
  (( total_bytes <= 1073741824 )) || fail guest-payload-file-size "payload exceeds 1 GiB"
  actual_hash="$(sha256_file "$source_path")"
  [[ "$actual_hash" == "$expected" ]] || fail guest-payload-hash "SHA-256 mismatch: $path"
done < "$files"

while IFS=$'\t' read -r role inf catalog binary_list; do
  grep -Fqx "$inf" "$listed" && grep -Fqx "$catalog" "$listed" || \
    fail guest-payload-schema "$role driver references an unlisted INF or CAT"
  LC_ALL=C grep -Eiq 'NTARM64' "$PAYLOAD_DIR/$inf" || \
    fail guest-payload-architecture "$role INF has no NTARM64 decoration"
  "$CATALOG_VERIFIER" "$PAYLOAD_DIR/$catalog" >/dev/null 2>&1 || \
    fail guest-payload-catalog-signature "$role catalog has no valid PKCS#7 signature and content digest"
  IFS=',' read -r -a binary_items <<< "$binary_list"
  has_sys=0
  for binary in "${binary_items[@]}"; do
    grep -Fqx "$binary" "$listed" || fail guest-payload-schema "$role driver references unlisted binary: $binary"
    [[ "$binary" == *.sys ]] && has_sys=1
    binary_path="$PAYLOAD_DIR/$binary"
    [[ "$(od -An -tx1 -N2 "$binary_path" | tr -d ' \n')" == 4d5a ]] || \
      fail guest-payload-pe "driver binary is not PE: $binary"
    pe_offset="$(od -An -tu4 -j60 -N4 "$binary_path" | tr -d ' ')"
    [[ "$pe_offset" =~ ^[0-9]+$ ]] || fail guest-payload-pe "invalid PE offset: $binary"
    [[ "$(od -An -tx1 -j "$pe_offset" -N4 "$binary_path" | tr -d ' \n')" == 50450000 ]] || \
      fail guest-payload-pe "missing PE signature: $binary"
    [[ "$(od -An -tx1 -j $((pe_offset + 4)) -N2 "$binary_path" | tr -d ' \n')" == 64aa ]] || \
      fail guest-payload-architecture "driver binary is not ARM64: $binary"
  done
  [[ "$has_sys" == 1 ]] || fail guest-payload-schema "$role driver has no SYS binary"
done < "$drivers"

while IFS= read -r binary; do
  grep -Fqx "$binary" "$binaries" || fail guest-payload-schema "unassigned executable payload: $binary"
done < <(grep -Ei '\.(sys|dll|exe)$' "$listed" || true)

for asset in bvagent.ps1 bvagent-firstboot.ps1; do
  [[ -f "$ASSETS/$asset" && ! -L "$ASSETS/$asset" ]] || fail guest-tools-missing "missing BridgeVM asset: $asset"
  LC_ALL=C awk 'substr($0, length($0), 1) != "\r" { exit 1 }' "$ASSETS/$asset" || \
    fail guest-tools-line-endings "$asset must use CRLF line endings"
done

stage="$work/staging"
mkdir -p "$stage/drivers" "$stage/agent"
while IFS=$'\t' read -r path expected; do
  destination="$stage/drivers/$path"
  mkdir -p "$(dirname "$destination")"
  cp "$PAYLOAD_DIR/$path" "$destination"
  [[ "$(sha256_file "$destination")" == "$expected" ]] || \
    fail guest-payload-staging "staged hash mismatch: $path"
done < "$files"
cp "$MANIFEST" "$stage/payload-manifest.tsv"
[[ "$(sha256_file "$stage/payload-manifest.tsv")" == "$manifest_hash" ]] || fail guest-payload-staging "manifest changed during staging"
for asset in bvagent.ps1 bvagent-firstboot.ps1; do cp "$ASSETS/$asset" "$stage/agent/$asset"; done

{
  printf 'schema\tbridgevm-windows-guest-payload-receipt-v1\n'
  printf 'architecture\tarm64\n'
  printf 'manifest_sha256\t%s\n' "$manifest_hash"
  printf 'catalog_signature_policy\tpkcs7-signature-and-digest-only-windows-trust-and-live-bind-still-required\n'
  while IFS=$'\t' read -r role inf catalog binary_list; do
    printf 'driver\t%s\t%s\t%s\t%s\n' "$role" "$inf" "$catalog" "$binary_list"
  done < "$drivers"
  while IFS=$'\t' read -r path expected; do printf 'file\tdrivers/%s\t%s\n' "$path" "$expected"; done < "$files"
  for asset in bvagent.ps1 bvagent-firstboot.ps1; do
    printf 'guest_tool\tagent/%s\t%s\n' "$asset" "$(sha256_file "$stage/agent/$asset")"
  done
} > "$stage/payload-receipt.tsv"

mkdir -p "$(dirname "$OUTPUT")"
mv "$stage" "$OUTPUT"
printf 'guest payload staged: roles=storage,serial,network output=%s\n' "$OUTPUT"
