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
trap 'rm -rf "$WORK"' EXIT
VERSION="v9.9.9"
TARBALL="BridgeVM-$VERSION.tar.gz"
pass=0 fail=0

make_bundle() { # $1: dir to create BridgeVM.app in, $2: bundle id
  local app="$1/BridgeVM.app"
  mkdir -p "$app/Contents/MacOS"
  cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>$2</string>
  <key>CFBundleExecutable</key><string>bridgevm</string>
  <key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>
EOF
  cp /bin/ls "$app/Contents/MacOS/bridgevm"
  codesign --force --sign - "$app" >/dev/null 2>&1
}

make_assets() { # $1: asset dir, $2: dir containing BridgeVM.app (release shape)
  mkdir -p "$1"
  tar -czf "$1/$TARBALL" -C "$2" "BridgeVM.app"
  (cd "$1" && shasum -a 256 "$TARBALL" > SHA256SUMS)
}

run_installer() { # $1: asset dir, $2: dest, then extra args
  local assets="$1" dest="$2"; shift 2
  BRIDGEVM_INSTALL_ASSET_DIR="$assets" BRIDGEVM_INSTALL_VERSION="$VERSION" \
    bash scripts/install-bridgevm.sh --dest "$dest" "$@"
}

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

# --- checksum failures -------------------------------------------------------
BAD_SUM="$WORK/bad-sum"; cp -R "$GOOD_ASSETS" "$BAD_SUM"
printf '%064d  %s\n' 0 "$TARBALL" > "$BAD_SUM/SHA256SUMS"
check "checksum mismatch refuses" fail run_installer "$BAD_SUM" "$DEST"

NO_ENTRY="$WORK/no-entry"; cp -R "$GOOD_ASSETS" "$NO_ENTRY"
: > "$NO_ENTRY/SHA256SUMS"
check "missing checksum entry refuses" fail run_installer "$NO_ENTRY" "$DEST"

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
(cd "$TRAV" && shasum -a 256 "$TARBALL" > SHA256SUMS)
check "path traversal refuses" fail run_installer "$TRAV" "$DEST"

ABS="$WORK/abs-assets"; mkdir -p "$ABS"
python3 - "$ABS/$TARBALL" <<'EOF'
import sys, tarfile, io
with tarfile.open(sys.argv[1], "w:gz") as tf:
    info = tarfile.TarInfo("/tmp/absolute-escape"); data = b"x"
    info.size = len(data); tf.addfile(info, io.BytesIO(data))
EOF
(cd "$ABS" && shasum -a 256 "$TARBALL" > SHA256SUMS)
check "absolute path refuses" fail run_installer "$ABS" "$DEST"

SYM="$WORK/sym-content"; mkdir -p "$SYM"
make_bundle "$SYM" "dev.bridgevm.control"
ln -s /etc/passwd "$SYM/BridgeVM.app/Contents/escape-link"
SYM_ASSETS="$WORK/sym-assets"; make_assets "$SYM_ASSETS" "$SYM"
check "escaping symlink refuses" fail run_installer "$SYM_ASSETS" "$DEST"

TWO="$WORK/two-content"; mkdir -p "$TWO/extra"
make_bundle "$TWO" "dev.bridgevm.control"
echo x > "$TWO/extra-file"
TWO_ASSETS="$WORK/two-assets"
mkdir -p "$TWO_ASSETS"
tar -czf "$TWO_ASSETS/$TARBALL" -C "$TWO" "BridgeVM.app" "extra-file"
(cd "$TWO_ASSETS" && shasum -a 256 "$TARBALL" > SHA256SUMS)
check "extra archive entries refuse" fail run_installer "$TWO_ASSETS" "$DEST"

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

echo
if [[ $fail -ne 0 ]]; then
  echo "install verify: FAIL ($fail failed, $pass passed)" >&2
  exit 1
fi
echo "install verify: PASS ($pass checks)"
