#!/usr/bin/env bash
# BridgeVM fail-closed installer.
#
# Every step verifies before it destroys: the release asset is pinned by name,
# checked against SHA256SUMS, extracted only after an archive-safety audit,
# codesign-verified, and only then swapped into place atomically with the old
# app preserved as a backup until the new one verifies in place. VM data under
# ~/BridgeVM and app settings are never touched.
#
# Trust root, stated honestly: HTTPS to github.com plus the SHA256SUMS file
# published in the same GitHub release. The checksum proves the bytes you got
# are the bytes the release carries; it does not prove who built them. The
# ad-hoc code signature proves the bundle is intact, not who signed it.
set -euo pipefail

REPO="${BRIDGEVM_INSTALL_REPO:-Ketchio-dev/bridgevm}"
EXPECTED_APP="BridgeVM.app"
EXPECTED_BUNDLE_ID="dev.bridgevm.control"

DEST="/Applications"
VERSION=""
DRY_RUN=0
KEEP_BACKUP=0
LAUNCH=0
VERBOSE=0

usage() {
  cat >&2 <<'EOF'
usage: install-bridgevm.sh [options]
  --version <tag>   install an exact release tag (e.g. v1.0.0)
  --latest          install the latest non-draft release (default)
  --dest <dir>      install directory, defaults to /Applications
  --dry-run         verify everything, change nothing
  --keep-backup     keep the previous app as a .backup bundle
  --launch          open the app after a successful install
  --verbose         print each verification step
EOF
  exit 2
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version) VERSION="${2:?--version needs a tag}"; shift 2 ;;
    --latest) VERSION=""; shift ;;
    --dest) DEST="${2:?--dest needs a directory}"; shift 2 ;;
    --dry-run) DRY_RUN=1; shift ;;
    --keep-backup) KEEP_BACKUP=1; shift ;;
    --launch) LAUNCH=1; shift ;;
    --no-launch) LAUNCH=0; shift ;;
    --verbose) VERBOSE=1; shift ;;
    *) usage ;;
  esac
done

log() { [[ $VERBOSE -eq 1 ]] && echo "install: $*" >&2 || true; }
fail() { echo "FAIL: $*" >&2; exit 1; }

if [[ -z "${BRIDGEVM_INSTALL_ASSET_DIR:-}" ]]; then
  [[ "$(uname -s)/$(uname -m)" == "Darwin/arm64" ]] ||
    fail "BridgeVM runs Windows 11 ARM on Apple Silicon Macs only"
fi

work=$(mktemp -d)
backup=""
final_app="$DEST/$EXPECTED_APP"
cleanup() {
  # On any failure after the old app was moved aside, put it back.
  if [[ -n "$backup" && -e "$backup" && ! -e "$final_app" ]]; then
    mv "$backup" "$final_app" 2>/dev/null &&
      echo "rolled back: previous app restored at $final_app" >&2
  fi
  rm -rf "$work"
}
trap cleanup EXIT

# --- resolve the release and its exact asset names ---------------------------
if [[ -z "$VERSION" ]]; then
  if [[ -n "${BRIDGEVM_INSTALL_ASSET_DIR:-}" ]]; then
    VERSION="${BRIDGEVM_INSTALL_VERSION:?test fixture must pin a version}"
  else
    VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" |
      /usr/bin/python3 -c 'import json,sys
release = json.load(sys.stdin)
if release.get("draft") or release.get("prerelease"):
    raise SystemExit("latest release is draft/prerelease; pass --version")
print(release["tag_name"])')
  fi
fi
[[ "$VERSION" =~ ^v[0-9]+(\.[0-9]+){2}([.-][0-9A-Za-z]+)*$ ]] ||
  fail "unexpected release tag shape: $VERSION"
TARBALL="BridgeVM-$VERSION.tar.gz"
log "release $VERSION, asset $TARBALL"

# --- download the asset and its checksum manifest ----------------------------
fetch() {
  # $1 asset name, $2 destination. The fixture dir override exists for the
  # deterministic installer tests; it replaces only the transport.
  if [[ -n "${BRIDGEVM_INSTALL_ASSET_DIR:-}" ]]; then
    cp "$BRIDGEVM_INSTALL_ASSET_DIR/$1" "$2"
  else
    curl -fL --progress-bar \
      "https://github.com/$REPO/releases/download/$VERSION/$1" -o "$2"
  fi
}
fetch "SHA256SUMS" "$work/SHA256SUMS" || fail "could not fetch SHA256SUMS for $VERSION"
fetch "$TARBALL" "$work/$TARBALL" || fail "could not fetch $TARBALL"

# --- verify the checksum before opening the archive --------------------------
expected=$(awk -v name="$TARBALL" \
  '$2 == name || $2 == "./" name { print $1 }' "$work/SHA256SUMS")
[[ -n "$expected" ]] || fail "SHA256SUMS has no entry for $TARBALL"
actual=$(shasum -a 256 "$work/$TARBALL" | awk '{print $1}')
[[ "$actual" == "$expected" ]] ||
  fail "checksum mismatch for $TARBALL (expected $expected, got $actual)"
log "checksum verified: $actual"

# --- audit the archive before extracting it ----------------------------------
while IFS= read -r entry; do
  case "$entry" in
    /*) fail "archive contains an absolute path: $entry" ;;
    *../*|../*|*/..|..) fail "archive contains a parent-directory path: $entry" ;;
    "$EXPECTED_APP"|"$EXPECTED_APP"/*) ;;
    *) fail "archive contains an entry outside $EXPECTED_APP: $entry" ;;
  esac
done < <(tar -tzf "$work/$TARBALL")

stage="$work/stage"
mkdir "$stage"
tar -xzf "$work/$TARBALL" -C "$stage"
entries=$(find "$stage" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')
[[ "$entries" == "1" && -d "$stage/$EXPECTED_APP" ]] ||
  fail "archive must contain exactly one $EXPECTED_APP bundle"

# Symlinks may point only inside the bundle: an escaping link would turn the
# post-install copy or a later signature check into a read/write elsewhere.
/usr/bin/python3 - "$stage" <<'EOF' || fail "archive contains a symlink escaping the bundle"
import os, sys
stage = os.path.realpath(sys.argv[1])
for root, dirs, files in os.walk(stage):
    for name in dirs + files:
        path = os.path.join(root, name)
        if os.path.islink(path):
            target = os.path.realpath(path)
            if not (target + os.sep).startswith(stage + os.sep):
                raise SystemExit(f"escaping symlink: {path} -> {target}")
EOF

# --- verify the bundle before touching the destination -----------------------
app="$stage/$EXPECTED_APP"
codesign --verify --deep --strict "$app" ||
  fail "code signature verification failed for $EXPECTED_APP"
bundle_id=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' \
  "$app/Contents/Info.plist" 2>/dev/null) || fail "bundle has no Info.plist identifier"
[[ "$bundle_id" == "$EXPECTED_BUNDLE_ID" ]] ||
  fail "unexpected bundle identifier: $bundle_id (expected $EXPECTED_BUNDLE_ID)"
executable=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' \
  "$app/Contents/Info.plist" 2>/dev/null) || fail "bundle declares no executable"
[[ -x "$app/Contents/MacOS/$executable" ]] ||
  fail "bundle executable is missing: $executable"
if command -v lipo >/dev/null 2>&1; then
  lipo -archs "$app/Contents/MacOS/$executable" 2>/dev/null |
    grep -Eqw 'arm64e?' || fail "bundle executable is not Apple Silicon native"
fi
log "bundle verified: $bundle_id / $executable"

if [[ $DRY_RUN -eq 1 ]]; then
  echo "dry run: $TARBALL verified (checksum, archive safety, signature," \
    "identifier); would install to $final_app"
  exit 0
fi

# --- install atomically with rollback ----------------------------------------
[[ -d "$DEST" ]] || fail "destination does not exist: $DEST"
[[ -w "$DEST" ]] ||
  fail "no write permission for $DEST; re-run with a writable --dest or fix permissions (the installer does not invoke sudo for you)"

staged="$DEST/.$EXPECTED_APP.install-$$"
rm -rf "$staged"
# Copy into the destination filesystem first so the final step is a rename.
cp -R "$app" "$staged" || { rm -rf "$staged"; fail "could not stage the app in $DEST"; }
codesign --verify --deep --strict "$staged" ||
  { rm -rf "$staged"; fail "staged copy failed signature verification"; }

if [[ -e "$final_app" ]]; then
  backup="$DEST/$EXPECTED_APP.backup-$(date +%Y%m%d-%H%M%S)"
  mv "$final_app" "$backup" || { rm -rf "$staged"; fail "could not preserve the existing app"; }
fi
if ! mv "$staged" "$final_app"; then
  rm -rf "$staged"
  fail "could not move the new app into place"
fi

# Post-install verification; the trap restores the backup if this fails.
codesign --verify --deep --strict "$final_app" || {
  rm -rf "$final_app"
  fail "installed app failed post-install verification"
}

if [[ -n "$backup" && $KEEP_BACKUP -eq 0 ]]; then
  rm -rf "$backup"; backup=""
elif [[ -n "$backup" ]]; then
  echo "previous app kept at: $backup"
  backup=""  # do not let the rollback trap touch a deliberately kept backup
fi

echo "Installed $final_app ($VERSION)"
echo "VM data under ~/BridgeVM was not touched."
[[ $LAUNCH -eq 1 ]] && open "$final_app"
echo "First run: open it from Launchpad or:  open '$final_app'"
