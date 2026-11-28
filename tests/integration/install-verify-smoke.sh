#!/usr/bin/env bash
# Fail-closed installer gate: every malicious or broken input must abort before
# the existing app is touched, and the happy path must be atomic with rollback.
# Runs offline against local fixtures (BRIDGEVM_INSTALL_ASSET_DIR).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if ! command -v codesign >/dev/null 2>&1; then
  echo "install verify: SKIP (codesign unavailable; macOS only)"
  exit 0
fi

WORK=$(mktemp -d "/tmp/bridgevm-install.XXXXXX")
FAKE_APP_PID=""
trap '[[ -z $FAKE_APP_PID ]] || kill "$FAKE_APP_PID" 2>/dev/null; rm -rf "$WORK"' EXIT
VERSION="v9.9.9"
TARBALL="BridgeVM-$VERSION.tar.gz"
RELEASE_MANIFEST="BridgeVM-release.json"
pass=0 fail=0
# shellcheck source=tests/integration/install-release-fixtures.sh
source tests/integration/install-release-fixtures.sh

check() { # $1: name, $2: expected exit (ok|fail), then command...
  local name="$1" expect="$2"; shift 2
  local status=0
  "$@" >"$WORK/last.out" 2>&1 || status=$?
  if { [[ "$expect" == ok && $status -eq 0 ]] ||
       { [[ "$expect" == fail && $status -ne 0 ]]; }; }; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    echo "FAIL: $name (exit=$status, expected $expect)" >&2
    sed 's/^/  | /' "$WORK/last.out" >&2
  fi
}
said() { # $1: text the last check must have printed
  grep -Fq -- "$1" "$WORK/last.out" ||
    { fail=$((fail + 1)); echo "FAIL: the last check did not print: $1" >&2; }
}

# --- good fixture ------------------------------------------------------------
GOOD="$WORK/good-content"; mkdir -p "$GOOD"
make_bundle "$GOOD" "dev.bridgevm.control"
GOOD_ASSETS="$WORK/good-assets"; make_assets "$GOOD_ASSETS" "$GOOD"

DEST="$WORK/dest"; mkdir -p "$DEST"
check "dry run verifies without installing" ok \
  run_installer "$GOOD_ASSETS" "$DEST" --dry-run
[[ ! -e "$DEST/BridgeVM.app" ]] || { fail=$((fail+1)); echo "FAIL: dry run installed" >&2; }

check "fresh install" ok run_installer "$GOOD_ASSETS" "$DEST"
[[ -d "$DEST/BridgeVM.app" ]] || { fail=$((fail+1)); echo "FAIL: app missing after install" >&2; }

# Update over an existing install, keeping the backup.
check "update with --keep-backup" ok run_installer "$GOOD_ASSETS" "$DEST" --keep-backup
ls "$DEST"/BridgeVM.app.backup-* >/dev/null 2>&1 ||
  { fail=$((fail+1)); echo "FAIL: --keep-backup kept no backup" >&2; }
rm -rf "$DEST"/BridgeVM.app.backup-*

# A second installer must leave the first installer's destination alone.
exec 8<"$DEST/.BridgeVM.app.install.lock"
/usr/bin/python3 -c 'import fcntl; fcntl.flock(8, fcntl.LOCK_EX)'
check "concurrent update refuses before replacement" fail run_installer "$GOOD_ASSETS" "$DEST"
if ! grep -Fq 'Another BridgeVM installer is updating this destination.' "$WORK/last.out"; then
  fail=$((fail+1)); echo "FAIL: concurrent update did not report the active installer" >&2
fi
exec 8<&-
codesign --verify --deep --strict "$DEST/BridgeVM.app"

# Interrupt a real update after the previous app has been moved to its backup.
INTERRUPT_BIN="$WORK/interrupt-bin"; mkdir "$INTERRUPT_BIN"
cat > "$INTERRUPT_BIN/mv" <<'EOF'
#!/bin/bash
set -eu
/bin/mv "$@"
if [[ $1 == "$INSTALL_TEST_DEST/BridgeVM.app" && $2 == "$INSTALL_TEST_DEST/BridgeVM.app.backup-"* ]]; then
  echo "interrupted after moving previous app" >&2
  kill -TERM "$PPID"
fi
EOF
chmod +x "$INTERRUPT_BIN/mv"
old_inode=$(stat -f %i "$DEST/BridgeVM.app")
PATH="$INTERRUPT_BIN:$PATH" INSTALL_TEST_DEST="$DEST" \
  check "interrupted update exits unsuccessfully" fail run_installer "$GOOD_ASSETS" "$DEST"
if ! grep -Fq 'interrupted after moving previous app' "$WORK/last.out" ||
   [[ "$(stat -f %i "$DEST/BridgeVM.app")" != "$old_inode" ]] ||
   compgen -G "$DEST/.BridgeVM.app.install-*" >/dev/null; then
  fail=$((fail+1)); echo "FAIL: interrupted update did not restore and clean up" >&2
fi

# --- checksum failures -------------------------------------------------------
BAD_SUM="$WORK/bad-sum"; cp -R "$GOOD_ASSETS" "$BAD_SUM"
printf '%064d  %s\n' 0 "$TARBALL" > "$BAD_SUM/SHA256SUMS"
(cd "$BAD_SUM" && shasum -a 256 "$RELEASE_MANIFEST" >> SHA256SUMS)
check "checksum mismatch refuses" fail run_installer "$BAD_SUM" "$DEST"
said "checksum mismatch for $TARBALL"

NO_ENTRY="$WORK/no-entry"; cp -R "$GOOD_ASSETS" "$NO_ENTRY"
: > "$NO_ENTRY/SHA256SUMS"
check "missing checksum entry refuses" fail run_installer "$NO_ENTRY" "$DEST"

LEGACY="$WORK/legacy-assets"; cp -R "$GOOD_ASSETS" "$LEGACY"
rm "$LEGACY/$RELEASE_MANIFEST"
check "legacy release without channel contract refuses" fail run_installer "$LEGACY" "$DEST"

WRONG_CHANNEL="$WORK/wrong-channel"; cp -R "$GOOD_ASSETS" "$WRONG_CHANNEL"
python3 - "$WRONG_CHANNEL/$RELEASE_MANIFEST" <<'EOF'
import json, sys
path = sys.argv[1]; value = json.load(open(path)); value["channel"] = "graphics-lab"
open(path, "w").write(json.dumps(value))
EOF
write_sums "$WRONG_CHANNEL"
check "experimental channel contract refuses" fail run_installer "$WRONG_CHANNEL" "$DEST"
# shellcheck source=tests/integration/install-release-provenance-cases.sh
source tests/integration/install-release-provenance-cases.sh
DRIVER_INCLUDED="$WORK/driver-included"; cp -R "$GOOD_ASSETS" "$DRIVER_INCLUDED"
python3 - "$DRIVER_INCLUDED/$RELEASE_MANIFEST" <<'EOF'
import json, sys
path = sys.argv[1]; value = json.load(open(path))
value["windows_graphics"]["kernel_driver_included"] = True
open(path, "w").write(json.dumps(value))
EOF
write_sums "$DRIVER_INCLUDED"
check "general preview carrying a kernel driver refuses" fail \
  run_installer "$DRIVER_INCLUDED" "$DEST"

# --- bundle identity and signature failures ----------------------------------
WRONG_ID="$WORK/wrong-id-content"; mkdir -p "$WRONG_ID"
make_bundle "$WRONG_ID" "dev.evil.impostor"
WRONG_ID_ASSETS="$WORK/wrong-id-assets"; make_assets "$WRONG_ID_ASSETS" "$WRONG_ID"
check "wrong bundle identifier refuses" fail run_installer "$WRONG_ID_ASSETS" "$DEST"

TAMPER="$WORK/tamper-content"; mkdir -p "$TAMPER"
make_bundle "$TAMPER" "dev.bridgevm.control"
echo tampered >> "$TAMPER/BridgeVM.app/Contents/MacOS/bridgevm"
TAMPER_ASSETS="$WORK/tamper-assets"; make_assets "$TAMPER_ASSETS" "$TAMPER"
check "broken code signature refuses" fail run_installer "$TAMPER_ASSETS" "$DEST"

SCRIPT_EXE="$WORK/script-exe-content"; mkdir -p "$SCRIPT_EXE"
make_bundle "$SCRIPT_EXE" "dev.bridgevm.control"
printf '#!/bin/sh\nexit 0\n' > "$SCRIPT_EXE/BridgeVM.app/Contents/MacOS/bridgevm"
codesign --force --sign - "$SCRIPT_EXE/BridgeVM.app" >/dev/null 2>&1
SCRIPT_EXE_ASSETS="$WORK/script-exe-assets"; make_assets "$SCRIPT_EXE_ASSETS" "$SCRIPT_EXE"
check "non-native bundle executable refuses" fail run_installer "$SCRIPT_EXE_ASSETS" "$DEST"
said "bundle executable is not Apple Silicon native"

# --- archive attacks ---------------------------------------------------------
TRAV="$WORK/trav-assets"; mkdir -p "$TRAV"
python3 - "$TRAV/$TARBALL" <<'EOF'
import sys, tarfile, io
path = sys.argv[1]
with tarfile.open(path, "w:gz") as tf:
    info = tarfile.TarInfo("BridgeVM.app/ok"); data = b"x"
    info.size = len(data); tf.addfile(info, io.BytesIO(data))
    info = tarfile.TarInfo("../escape"); info.size = len(data)
    tf.addfile(info, io.BytesIO(data))
EOF
make_manifest "$TRAV"; write_sums "$TRAV"; make_metadata "$TRAV"
check "path traversal refuses" fail run_installer "$TRAV" "$DEST"
said "archive contains a parent-directory path"

ABS="$WORK/abs-assets"; mkdir -p "$ABS"
python3 - "$ABS/$TARBALL" <<'EOF'
import sys, tarfile, io
with tarfile.open(sys.argv[1], "w:gz") as tf:
    info = tarfile.TarInfo("/tmp/absolute-escape"); data = b"x"
    info.size = len(data); tf.addfile(info, io.BytesIO(data))
EOF
make_manifest "$ABS"; write_sums "$ABS"; make_metadata "$ABS"
check "absolute path refuses" fail run_installer "$ABS" "$DEST"
said "archive contains an absolute path"

SYM="$WORK/sym-content"; mkdir -p "$SYM"
make_bundle "$SYM" "dev.bridgevm.control"
ln -s /etc/passwd "$SYM/BridgeVM.app/Contents/escape-link"
SYM_ASSETS="$WORK/sym-assets"; make_assets "$SYM_ASSETS" "$SYM"
check "escaping symlink refuses" fail run_installer "$SYM_ASSETS" "$DEST"
said "archive contains a symlink escaping the bundle"

INNER="$WORK/inner-link-content"; mkdir -p "$INNER"
make_bundle "$INNER" "dev.bridgevm.control"
mkdir "$INNER/BridgeVM.app/Contents/Resources"
ln -s ../MacOS/bridgevm "$INNER/BridgeVM.app/Contents/Resources/inner-link"
codesign --force --sign - "$INNER/BridgeVM.app" >/dev/null 2>&1
INNER_ASSETS="$WORK/inner-link-assets"; make_assets "$INNER_ASSETS" "$INNER"
check "symlink inside the bundle is accepted" ok run_installer "$INNER_ASSETS" "$DEST" --dry-run

TWO="$WORK/two-content"; mkdir -p "$TWO/extra"
make_bundle "$TWO" "dev.bridgevm.control"
echo x > "$TWO/extra-file"
TWO_ASSETS="$WORK/two-assets"
mkdir -p "$TWO_ASSETS"
tar -czf "$TWO_ASSETS/$TARBALL" -C "$TWO" "BridgeVM.app" "extra-file"
make_manifest "$TWO_ASSETS"; write_sums "$TWO_ASSETS"; make_metadata "$TWO_ASSETS"
check "extra archive entries refuse" fail run_installer "$TWO_ASSETS" "$DEST"
said "archive contains an entry outside BridgeVM.app"

# --- destination and rollback ------------------------------------------------
RO_DEST="$WORK/ro-dest"; mkdir -p "$RO_DEST"; chmod 555 "$RO_DEST"
check "unwritable destination refuses clearly" fail run_installer "$GOOD_ASSETS" "$RO_DEST"
chmod 755 "$RO_DEST"

# Rollback: fake codesign passes staging but fails the final in-place verify,
# so the previously installed app must be restored.
FAKEBIN="$WORK/fakebin"; mkdir -p "$FAKEBIN"
cat > "$FAKEBIN/codesign" <<EOF
#!/usr/bin/env bash
for arg in "\$@"; do case "\$arg" in "$DEST/BridgeVM.app") exit 1 ;; esac; done
exec /usr/bin/codesign "\$@"
EOF
chmod +x "$FAKEBIN/codesign"
marker=$(date +%s)
echo "$marker" > "$DEST/BridgeVM.app/Contents/marker"
check "post-install verify failure rolls back" fail \
  env PATH="$FAKEBIN:$PATH" bash -c \
  'BRIDGEVM_INSTALL_ASSET_DIR="$0" BRIDGEVM_INSTALL_VERSION="'"$VERSION"'" \
     bash scripts/install-bridgevm.sh --dest "$1"' "$GOOD_ASSETS" "$DEST"
[[ "$(cat "$DEST/BridgeVM.app/Contents/marker" 2>/dev/null)" == "$marker" ]] ||
  { fail=$((fail+1)); echo "FAIL: rollback did not restore the previous app" >&2; }

# --- bootstrap pin -----------------------------------------------------------
check "bootstrap pin is current" ok bash scripts/check-install-bootstrap.sh
BOOT="$WORK/boot"; mkdir -p "$BOOT/scripts"
cp install.sh "$BOOT/"
cp scripts/install-bridgevm.sh "$BOOT/scripts/"
echo "# drift" >> "$BOOT/scripts/install-bridgevm.sh"
check "bootstrap refuses a drifted installer" fail bash "$BOOT/install.sh" --dry-run
check "bootstrap runs the pinned installer" ok \
  env BRIDGEVM_INSTALL_ASSET_DIR="$GOOD_ASSETS" BRIDGEVM_INSTALL_VERSION="$VERSION" \
  bash install.sh --dest "$DEST" --dry-run

# --- a fresh Mac -------------------------------------------------------------
# Without the developer tools, python3, lipo and friends in /usr/bin are stubs
# that open an install dialog and exit 1, so neither script may call them.
if grep -inwE 'python[0-9.]*|lipo|otool|xcrun' install.sh scripts/install-bridgevm.sh; then
  fail=$((fail+1)); echo "FAIL: the installer calls a developer-tools stub (lines above)" >&2
fi
NO_DEV="$WORK/no-developer-tools"; mkdir "$NO_DEV"
for tool in python3 python lipo otool xcrun; do
  printf '#!/bin/sh\necho "%s: no developer tools (test stub)" >&2\nexit 1\n' "$tool" > "$NO_DEV/$tool"
  chmod +x "$NO_DEV/$tool"
done
NO_DEV_DEST="$WORK/no-dev-dest"; mkdir "$NO_DEV_DEST"
PATH="$NO_DEV:$PATH" check "install works with developer-tools stubs first on PATH" ok \
  run_installer "$GOOD_ASSETS" "$NO_DEV_DEST"

# --- GitHub paths, served from fixtures by a fake curl ------------------------
FAKE_NET="$WORK/fake-net"; mkdir "$FAKE_NET"
cat > "$FAKE_NET/curl" <<'EOF'
#!/bin/bash
# A missing fixture is an HTTP error (curl -f exits 22), like a rate limit.
out="" url=""
while [[ $# -gt 0 ]]; do
  case "$1" in -o) out="$2"; shift 2 ;; -*) shift ;; *) url="$1"; shift ;; esac
done
case "$url" in
  */releases\?per_page=*) name=releases.json ;;
  */releases/tags/*) name=github-release.json ;;
  */commits/*) name=github-commit.json ;;
  *) name="${url##*/}" ;;
esac
[[ -f "$FAKE_GITHUB_DIR/$name" ]] || exit 22
if [[ -n "$out" ]]; then cp "$FAKE_GITHUB_DIR/$name" "$out"; else cat "$FAKE_GITHUB_DIR/$name"; fi
EOF
chmod +x "$FAKE_NET/curl"
NET="$WORK/github"; cp -R "$GOOD_ASSETS" "$NET"
cp scripts/install-bridgevm.sh "$NET/"
cat > "$NET/releases.json" <<EOF
[{"tag_name": "v9.9.13", "draft": true, "prerelease": false, "assets": [{"name": "$RELEASE_MANIFEST"}]},
 {"tag_name": "v9.9.12", "draft": false, "prerelease": true, "assets": [{"name": "$RELEASE_MANIFEST"}]},
 {"tag_name": "v9.9.11", "draft": false, "prerelease": false, "body": null, "assets": [{"name": "$TARBALL"}]},
 {"tag_name": "$VERSION", "draft": false, "prerelease": false, "assets": [{"name": "$RELEASE_MANIFEST"}]}]
EOF
online() { PATH="$FAKE_NET:$PATH" FAKE_GITHUB_DIR="$1" "${@:2}"; }

check "newest release skips drafts, prereleases and legacy releases" ok \
  online "$NET" bash scripts/install-bridgevm.sh --dest "$DEST" --dry-run
said "dry run: $TARBALL verified"

OFFLINE="$WORK/offline"; mkdir "$OFFLINE"
check "unreachable GitHub API refuses" fail \
  online "$OFFLINE" bash scripts/install-bridgevm.sh --dest "$DEST" --dry-run
said "Couldn't reach GitHub to find the newest release."

# Rosetta (an Intel Homebrew bash or Terminal) makes uname -m say x86_64.
ROSETTA="$WORK/rosetta-bin"; mkdir "$ROSETTA"
printf '#!/bin/sh\n[ "$1" = -m ] && echo x86_64 && exit 0\nexec /usr/bin/uname "$@"\n' > "$ROSETTA/uname"
chmod +x "$ROSETTA/uname"
check "a Rosetta shell passes the Apple Silicon check" ok \
  online "$NET" env PATH="$ROSETTA:$FAKE_NET:$PATH" bash install.sh --dest "$DEST" --dry-run

# curl | bash has no script file, so an older checkout in the current directory
# must not stand in for the downloaded installer.
check "curl | bash ignores a checkout in the current directory" ok \
  online "$NET" env BRIDGEVM_INSTALL_ASSET_DIR="$GOOD_ASSETS" BRIDGEVM_INSTALL_VERSION="$VERSION" \
  bash -c 'cd "$0" && bash -s -- --dest "$1" --dry-run < "$2"' "$BOOT" "$DEST" "$ROOT/install.sh"
STALE="$WORK/stale-github"; mkdir "$STALE"; cp "$BOOT/scripts/install-bridgevm.sh" "$STALE/"
check "curl | bash refuses a stale download" fail \
  online "$STALE" bash -c 'cd "$0" && bash -s -- --dry-run < "$1"' "$WORK" "$ROOT/install.sh"
said "The downloaded installer didn't match"

# --- a running BridgeVM -------------------------------------------------------
# Copied system binaries are killed by launch constraints, so rename sleep.
RUN_DEST="$WORK/running-dest"; mkdir "$RUN_DEST"
check "install before the running-app test" ok run_installer "$GOOD_ASSETS" "$RUN_DEST"
( exec -a "$RUN_DEST/BridgeVM.app/Contents/MacOS/BridgeVMControl" sleep 600 ) &
FAKE_APP_PID=$!
for _ in {1..50}; do
  [[ "$(ps -p "$FAKE_APP_PID" -o command= 2>/dev/null)" == "$RUN_DEST/"* ]] && break
  sleep 0.1
done
old_inode=$(stat -f %i "$RUN_DEST/BridgeVM.app")
# No assets at all: the refusal must come before anything is fetched.
check "update refuses while BridgeVM is running" fail \
  run_installer "$WORK/no-such-assets" "$RUN_DEST"
said "BridgeVM is running. Shut down your VMs, quit BridgeVM, then run the installer again."
[[ "$(stat -f %i "$RUN_DEST/BridgeVM.app")" == "$old_inode" ]] ||
  { fail=$((fail+1)); echo "FAIL: the update replaced a running BridgeVM" >&2; }
check "dry run still verifies while BridgeVM is running" ok \
  run_installer "$GOOD_ASSETS" "$RUN_DEST" --dry-run
kill "$FAKE_APP_PID"; wait "$FAKE_APP_PID" 2>/dev/null || true; FAKE_APP_PID=""
check "update proceeds after BridgeVM quits" ok run_installer "$GOOD_ASSETS" "$RUN_DEST"

echo
if [[ $fail -ne 0 ]]; then
  echo "install verify: FAIL ($fail failed, $pass passed)" >&2
  exit 1
fi
echo "install verify: PASS ($pass checks)"
