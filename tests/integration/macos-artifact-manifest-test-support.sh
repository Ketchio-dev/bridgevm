#!/usr/bin/env bash
# Shared assertions for the unsigned artifact-manifest recording fixtures.

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

assert_contains_file() {
  local file="$1"
  local needle="$2"
  local label="$3"
  grep -Fq "$needle" "$file" || fail "$label missing expected text: $needle"
}

assert_not_contains_file() {
  local file="$1"
  local needle="$2"
  local label="$3"
  if grep -Fq "$needle" "$file"; then
    fail "$label included unexpected text: $needle"
  fi
}

assert_policy_record() {
  local manifest="$1" digest row
  digest="$(shasum -a 256 "$HVF_SECURE_BOOT_POLICY" | awk '{ print $1 }')"
  for row in "path=$HVF_SECURE_BOOT_POLICY" "present=true" "sha256=$digest"; do
    grep -Fxq "windows_hvf_secure_boot_policy.$row" "$manifest" \
      || fail "Secure Boot policy record differs: $row"
  done
}

check_policy_and_required_input_boundaries() {
  local environment_manifest="$WORKDIR/environment-app-artifacts.txt"
  local absent_manifest="$WORKDIR/absent-policy-artifacts.txt"
  local refused_manifest="$WORKDIR/refused-artifacts.txt"
  local failure_log="$WORKDIR/refused-input.log"

  # Exercise the writer's environment path independently of positional inputs.
  env BRIDGEVM_MACOS_APP="$APP" BRIDGEVM_MACOS_ARTIFACT_MANIFEST="$environment_manifest" \
    "$SCRIPT" --app-only >/dev/null
  assert_policy_record "$environment_manifest"

  # This is a recorder: optional missing policy remains explicit, without a hash.
  rm "$HVF_SECURE_BOOT_POLICY"
  "$SCRIPT" --app-only "$APP" "$absent_manifest" >/dev/null
  assert_contains_file "$absent_manifest" "windows_hvf_secure_boot_policy.path=$HVF_SECURE_BOOT_POLICY" "absent policy identity"
  assert_contains_file "$absent_manifest" "windows_hvf_secure_boot_policy.present=false" "absent policy status"
  assert_not_contains_file "$absent_manifest" "windows_hvf_secure_boot_policy.sha256=" "absent policy digest"

  if "$SCRIPT" --app-only "$WORKDIR/missing.app" "$refused_manifest" >"$failure_log" 2>&1; then
    fail "missing required app was accepted"
  fi
  assert_contains_file "$failure_log" "BridgeVM app bundle is missing:" "required app refusal"
  [[ ! -e "$refused_manifest" ]] || fail "missing app published a manifest"
  if "$SCRIPT" "$APP" "$WORKDIR/missing.dmg" "$refused_manifest" >"$failure_log" 2>&1; then
    fail "missing required DMG was accepted"
  fi
  assert_contains_file "$failure_log" "BridgeVM DMG is missing:" "required DMG refusal"
  [[ ! -e "$refused_manifest" ]] || fail "missing DMG published a manifest"
}
