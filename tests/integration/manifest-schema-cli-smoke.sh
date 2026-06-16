#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-manifest-schema.XXXXXX")"
PRESERVE_STORE=1

bridgevm() {
  cargo run --quiet -p bridgevm-cli -- --store "$STORE" "$@"
}

fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
  exit 1
}

cleanup() {
  if [[ "$PRESERVE_STORE" == "0" ]]; then
    rm -rf "$STORE"
  fi
}

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  case "$haystack" in
    *"$needle"*) ;;
    *) fail "$label missing '$needle'; got: $haystack" ;;
  esac
}

assert_file_contains() {
  local path="$1"
  local needle="$2"
  local label="$3"
  rg -F -q "$needle" "$path" || fail "$label missing '$needle' in $path"
}

assert_fails_contains() {
  local label="$1"
  local needle="$2"
  shift 2

  local output
  local status
  set +e
  output="$("$@" 2>&1)"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    fail "$label unexpectedly succeeded; got: $output"
  fi
  assert_contains "$output" "$needle" "$label"
}

write_manifest() {
  local path="$1"
  local schema_version="$2"
  local name="$3"

  cat >"$path" <<EOF
schemaVersion: $schema_version
name: "$name"
mode: compatibility
guest:
  os: ubuntu
  arch: x86_64
backend:
  engine: qemu
  fallback: tcg
  accelerator: hvf
resources:
  profile: automatic
  memory: auto
  cpu: auto
display:
  renderer: spice
  framePolicy: adaptive
  retina: true
storage:
  primary:
    path: disks/root.qcow2
    size: 80GiB
    format: qcow2
    discard: false
boot:
  mode: existing-disk
network:
  mode: nat
  hostname: $name.bridgevm.local
  forwards: []
integration:
  tools: optional
  clipboard: true
  dragDrop: false
  dynamicResolution: true
  sharedFolders: true
  applications: true
  windows: true
security:
  sharedFolderApproval: required
  guestCommandExecution: false
  signedAgentUpdates: true
sharedFolders: []
EOF
}

trap cleanup EXIT

schema_output="$(bridgevm metadata manifest-schema)"
schema_path="$STORE/manifest-schema.json"
printf '%s\n' "$schema_output" >"$schema_path"

assert_contains "$schema_output" "bridgevm.io/v1" "manifest schema output"
assert_contains "$schema_output" "schemaVersion" "manifest schema output"
assert_contains "$schema_output" "name" "manifest schema output"
assert_contains "$schema_output" "mode" "manifest schema output"
assert_file_contains "$schema_path" "bridgevm.io/v1" "manifest schema JSON"
assert_file_contains "$schema_path" "schemaVersion" "manifest schema JSON"
assert_file_contains "$schema_path" "name" "manifest schema JSON"
assert_file_contains "$schema_path" "mode" "manifest schema JSON"

valid_manifest="$STORE/valid-manifest.yaml"
future_manifest="$STORE/future-manifest.yaml"
empty_name_manifest="$STORE/empty-name-manifest.yaml"
malformed_manifest="$STORE/malformed-manifest.yaml"

write_manifest "$valid_manifest" "bridgevm.io/v1" "schema-smoke"
write_manifest "$future_manifest" "bridgevm.io/v99" "schema-smoke-future"
write_manifest "$empty_name_manifest" "bridgevm.io/v1" ""
printf 'schemaVersion: bridgevm.io/v1\nname: [not-valid-yaml\n' >"$malformed_manifest"

valid_output="$(bridgevm metadata validate-manifest "$valid_manifest")"
assert_contains "$valid_output" "Manifest valid" "valid manifest validation"
assert_contains "$valid_output" "$valid_manifest" "valid manifest validation"

assert_fails_contains \
  "future manifest validation" \
  "manifest schema version must be bridgevm.io/v1" \
  bridgevm metadata validate-manifest "$future_manifest"

assert_fails_contains \
  "empty manifest name validation" \
  "manifest name cannot be empty" \
  bridgevm metadata validate-manifest "$empty_name_manifest"

assert_fails_contains \
  "malformed manifest validation" \
  "YAML error" \
  bridgevm metadata validate-manifest "$malformed_manifest"

PRESERVE_STORE=0
echo "PASS: manifest schema CLI integration smoke"
