#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-media-fake-curl.XXXXXX")"
VM="media-fake-curl"
BUNDLE="$STORE/vms/$VM.vmbridge"
DESTINATION="$BUNDLE/media/ubuntu.iso"
FIXTURE="$STORE/source-installer.iso"
FAKE_BIN="$STORE/fake-bin"
FAKE_CURL_LOG="$STORE/fake-curl.log"
DOWNLOAD_URL="https://example.invalid/bridgevm/source-installer.iso"
PRESERVE_STORE=1

printf "bridgevm boot media fake curl fixture\n" >"$FIXTURE"
EXPECTED_SHA256="$(shasum -a 256 "$FIXTURE" | awk '{print $1}')"
FIXTURE_BYTES="$(wc -c <"$FIXTURE" | tr -d ' ')"

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
  local file="$1"
  local needle="$2"
  local label="$3"
  [[ -f "$file" ]] || fail "$label missing file $file"
  grep -Fq "$needle" "$file" || fail "$label missing '$needle' in $file"
}

trap cleanup EXIT

mkdir -p "$FAKE_BIN"
cat >"$FAKE_BIN/curl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

: "${BRIDGEVM_FAKE_CURL_FIXTURE:?}"
: "${BRIDGEVM_FAKE_CURL_LOG:?}"
: "${BRIDGEVM_FAKE_CURL_URL:?}"

{
  printf 'argc=%s\n' "$#"
  for arg in "$@"; do
    printf 'arg=%q\n' "$arg"
  done
} >>"$BRIDGEVM_FAKE_CURL_LOG"

if [[ "$#" -ne 7 ]]; then
  echo "fake curl expected 7 args, got $#" >&2
  exit 64
fi
if [[ "$1" != "--location" || "$2" != "--fail" || "$3" != "--silent" || "$4" != "--show-error" || "$5" != "--output" ]]; then
  echo "fake curl received unexpected options" >&2
  exit 64
fi
if [[ "$7" != "$BRIDGEVM_FAKE_CURL_URL" ]]; then
  echo "fake curl refused unexpected URL: $7" >&2
  exit 65
fi

case "$7" in
  http://127.0.0.1:*|http://localhost:*|https://127.0.0.1:*|https://localhost:*)
    echo "fake curl refused loopback URL in metadata-safe smoke: $7" >&2
    exit 66
    ;;
esac

mkdir -p "$(dirname "$6")"
cp "$BRIDGEVM_FAKE_CURL_FIXTURE" "$6"
SH
chmod +x "$FAKE_BIN/curl"

export BRIDGEVM_FAKE_CURL_FIXTURE="$FIXTURE"
export BRIDGEVM_FAKE_CURL_LOG="$FAKE_CURL_LOG"
export BRIDGEVM_FAKE_CURL_URL="$DOWNLOAD_URL"
export PATH="$FAKE_BIN:$PATH"

bridgevm create "$VM" \
  --os ubuntu \
  --arch arm64 \
  --mode fast \
  --boot-mode linux-installer \
  --installer-image media/ubuntu.iso >/dev/null

plan_output="$(bridgevm media download-plan "$VM" --url "$DOWNLOAD_URL" --sha256 "$EXPECTED_SHA256")"
assert_contains "$plan_output" "Planned boot media download for $VM" "download plan output"
assert_contains "$plan_output" "URL: $DOWNLOAD_URL" "download plan output"
assert_contains "$plan_output" "Destination: $DESTINATION" "download plan output"
assert_contains "$plan_output" "Destination exists: false" "download plan output"
assert_contains "$plan_output" "Expected SHA-256: $EXPECTED_SHA256" "download plan output"

PLAN_METADATA="$BUNDLE/metadata/boot-media/installer-image-download.json"
RESULT_METADATA="$BUNDLE/metadata/boot-media/installer-image-download-result.json"
if ! download_output="$(bridgevm media download "$VM" 2>&1)"; then
  echo "$download_output" >&2
  if [[ -f "$RESULT_METADATA" ]]; then
    echo "Download result metadata:" >&2
    sed -n '1,220p' "$RESULT_METADATA" >&2
  fi
  fail "media download failed"
fi
status_output="$(bridgevm media status "$VM")"

assert_contains "$download_output" "Downloaded boot media for $VM" "download output"
assert_contains "$download_output" "Boot media kind: installer-image" "download output"
assert_contains "$download_output" "URL: $DOWNLOAD_URL" "download output"
assert_contains "$download_output" "Destination: $DESTINATION" "download output"
assert_contains "$download_output" "Downloaded: true" "download output"
assert_contains "$download_output" "Replaced existing media: false" "download output"
assert_contains "$download_output" "Bytes: $FIXTURE_BYTES" "download output"
assert_contains "$download_output" "Expected SHA-256: $EXPECTED_SHA256" "download output"
assert_contains "$download_output" "Actual SHA-256: $EXPECTED_SHA256" "download output"
assert_contains "$download_output" "Verified: true" "download output"

cmp "$FIXTURE" "$DESTINATION" >/dev/null || fail "downloaded destination did not match fixture"

assert_file_contains "$PLAN_METADATA" "\"url\": \"$DOWNLOAD_URL\"" "download plan metadata"
assert_file_contains "$PLAN_METADATA" "\"expected_sha256\": \"$EXPECTED_SHA256\"" "download plan metadata"
assert_file_contains "$RESULT_METADATA" '"downloaded": true' "download result metadata"
assert_file_contains "$RESULT_METADATA" '"verified": true' "download result metadata"
assert_file_contains "$RESULT_METADATA" "\"bytes\": $FIXTURE_BYTES" "download result metadata"
assert_file_contains "$RESULT_METADATA" "\"actual_sha256\": \"$EXPECTED_SHA256\"" "download result metadata"
assert_file_contains "$RESULT_METADATA" '"curl"' "download result metadata"
[[ ! -e "$BUNDLE/media/.ubuntu.iso.download" ]] \
  || fail "temporary download file was left behind"

assert_file_contains "$FAKE_CURL_LOG" "argc=7" "fake curl log"
assert_file_contains "$FAKE_CURL_LOG" "arg=--location" "fake curl log"
assert_file_contains "$FAKE_CURL_LOG" "arg=--fail" "fake curl log"
assert_file_contains "$FAKE_CURL_LOG" "arg=--silent" "fake curl log"
assert_file_contains "$FAKE_CURL_LOG" "arg=--show-error" "fake curl log"
assert_file_contains "$FAKE_CURL_LOG" "arg=--output" "fake curl log"
assert_file_contains "$FAKE_CURL_LOG" "arg=$DOWNLOAD_URL" "fake curl log"

assert_contains "$status_output" "VM: $VM" "status output"
assert_contains "$status_output" "Path: $DESTINATION" "status output"
assert_contains "$status_output" "Exists: true" "status output"
assert_contains "$status_output" "Last download URL: $DOWNLOAD_URL" "status output"
assert_contains "$status_output" "Last download expected SHA-256: $EXPECTED_SHA256" "status output"
assert_contains "$status_output" "Last download completed: true" "status output"
assert_contains "$status_output" "Last download bytes: $FIXTURE_BYTES" "status output"

PRESERVE_STORE=0
echo "PASS: boot media download fake curl metadata-safe smoke"
