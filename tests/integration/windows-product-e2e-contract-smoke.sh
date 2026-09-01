#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-product-e2e.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT
FAKE="$TMP/bin"
mkdir -p "$FAKE" "$TMP/assets"
CATALOG_VERIFIER="$("$ROOT/tests/integration/build-windows-catalog-verifier-test-helper.sh" "$TMP")"
cp "$ROOT/scripts/win-assets/"{winpeshl.ini,bvinstall.cmd,bvdiskpart.txt,unattend.xml,bvagent.ps1,bvagent-firstboot.ps1} "$TMP/assets/"
printf 'fake ISO\n' > "$TMP/windows.iso"
python3 "$ROOT/tests/fixtures/make-synthetic-windows-guest-payload.py" "$TMP/payload" "$TMP/payload.tsv"
cat > "$FAKE/hdiutil" <<'MOCK'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$BRIDGEVM_HDIUTIL_LOG"
case "$1" in
  attach)
    mountpoint=""; readonly=0
    while (( $# )); do
      [[ "$1" == -readonly ]] && readonly=1
      if [[ "$1" == -mountpoint ]]; then mountpoint="$2"; shift; fi
      shift
    done
    if [[ -n "$mountpoint" && "$readonly" == 1 ]]; then
      mkdir -p "$mountpoint/sources" "$mountpoint/efi/boot"
      : > "$mountpoint/sources/install.wim"
      : > "$mountpoint/sources/boot.wim"
      : > "$mountpoint/efi/boot/bootaa64.efi"
      printf '/dev/disk-iso\t%s\n' "$mountpoint"
    elif [[ -n "$mountpoint" ]]; then
      mkdir -p "$mountpoint"
      printf '/dev/disk-destination\t%s\n' "$mountpoint"
    else
      printf '/dev/disk-destination\n'
    fi
    ;;
  detach|info) ;;
  *) exit 2 ;;
esac
MOCK
cat > "$FAKE/diskutil" <<'MOCK'
#!/usr/bin/env bash
exit 0
MOCK
cat > "$FAKE/mkfile" <<'MOCK'
#!/usr/bin/env bash
for argument in "$@"; do output="$argument"; done
: > "$output"
MOCK
cat > "$FAKE/rsync" <<'MOCK'
#!/usr/bin/env bash
source_path="${@: -2:1}"; destination="${@: -1}"
cp -R "$source_path/." "$destination/"
rm -f "$destination/sources/install.wim"
MOCK
cat > "$FAKE/wimlib-imagex" <<'MOCK'
#!/usr/bin/env bash
case "$1" in
  split) : > "$3" ;;
  update) cat >/dev/null ;;
  dir) printf '%s\n' winpeshl.ini bvinstall.cmd bvdiskpart.txt ;;
  *) exit 2 ;;
esac
MOCK
chmod 755 "$FAKE/"*
BRIDGEVM_HDIUTIL_LOG="$TMP/hdiutil.log" \
ISO="$TMP/windows.iso" ASSETS="$TMP/assets" OUT="$TMP/source.raw" \
WIMLIB="$FAKE/wimlib-imagex" \
WINDOWS_GUEST_PAYLOAD_DIR="$TMP/payload" WINDOWS_GUEST_PAYLOAD_MANIFEST="$TMP/payload.tsv" WINDOWS_GUEST_PAYLOAD_CATALOG_VERIFIER="$CATALOG_VERIFIER" \
TMPDIR="$TMP" PATH="$FAKE:/usr/bin:/bin:/usr/sbin:/sbin" \
  "$ROOT/scripts/build-hvf-windows-scripted-source.sh" >/dev/null
[[ "$(grep -c '^attach ' "$TMP/hdiutil.log")" == 3 ]]
[[ "$(grep -c -- '-readonly' "$TMP/hdiutil.log")" == 1 ]]
[[ "$(grep -c "$TMP/windows.iso" "$TMP/hdiutil.log")" == 1 ]]
[[ "$(grep -c '^detach ' "$TMP/hdiutil.log")" == 3 ]]
grep -q -- '-mountpoint .*/bridgevm-win-source\..*/iso' "$TMP/hdiutil.log"
grep -q -- '-mountpoint .*/bridgevm-win-source\..*/dst' "$TMP/hdiutil.log"
python3 "$ROOT/tests/integration/windows-guest-payload-verifier-smoke.py"; python3 "$ROOT/tests/integration/windows-install-security-contract-smoke.py"
python3 "$ROOT/scripts/verify-windows-product-e2e-receipt.py" --self-test; "$ROOT/tests/integration/windows-product-e2e-live-tier-smoke.sh"
echo "PASS: Windows product E2E deterministic contracts"
