#!/usr/bin/env bash
# BridgeVM fail-closed installer.
# Every step verifies before it destroys: the release asset is pinned by name,
# checked against SHA256SUMS, extracted only after an archive-safety audit,
# codesign-verified, and only then swapped into place atomically with the old
# app preserved as a backup until the new one verifies in place. Your VMs
# (~/Library/Application Support/BridgeVM/vms or the storage folder you chose),
# their shared folders and app settings are never touched.
# Trust root, stated honestly: HTTPS to github.com plus the SHA256SUMS file
# published in the same GitHub release. The checksum proves the bytes you got
# are the bytes the release carries; it does not prove who built them. The
# ad-hoc code signature proves the bundle is intact, not who signed it.
set -euo pipefail
REPO="${BRIDGEVM_INSTALL_REPO:-Ketchio-dev/bridgevm}"
EXPECTED_APP="BridgeVM.app"
EXPECTED_BUNDLE_ID="dev.bridgevm.control"
RELEASE_MANIFEST="BridgeVM-release.json" DEST="/Applications"
VERSION=""
DRY_RUN=0
KEEP_BACKUP=0
LAUNCH=0
VERBOSE=0
usage() {
  cat >&2 <<'EOF'
usage: install-bridgevm.sh [options]
  --version <tag>   install an exact release tag (e.g. v0.1.0)
  --latest          install the latest verified General Preview (default)
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
# Use only tools every macOS ships with. On a Mac without the developer tools,
# several commands in /usr/bin are stubs that open an install dialog and fail.
if [[ -z "${BRIDGEVM_INSTALL_ASSET_DIR:-}" ]]; then
  # Unlike uname -m, this still reports Apple Silicon from a Rosetta shell.
  [[ "$(/usr/sbin/sysctl -n hw.optional.arm64 2>/dev/null)" == 1 ]] ||
    fail "BridgeVM runs Windows 11 ARM on Apple Silicon Macs only"
fi
work=$(mktemp -d); backup=""; install_stage=""
final_app="$DEST/$EXPECTED_APP"
cleanup() {
  # On any failure after the old app was moved aside, put it back.
  if [[ -n "$backup" && -e "$backup" && ! -e "$final_app" ]]; then
    mv "$backup" "$final_app" 2>/dev/null &&
      echo "rolled back: previous app restored at $final_app" >&2
  fi
  if [[ -n "$install_stage" ]]; then
    rm -rf -- "$install_stage" || true
  fi
  rm -rf "$work"
}
trap cleanup EXIT
# JSON is read with JavaScript for Automation. Any unreadable or unexpected
# JSON prints nothing, which every caller treats as a refusal.
cat > "$work/json.js" <<'JS'
ObjC.import("Foundation")
const read = (path) => JSON.parse(
  $.NSString.stringWithContentsOfFileEncodingError(path, $.NSUTF8StringEncoding, null).js)
const assetNames = (release) => (release.assets || []).map((asset) => asset.name)
const queries = {
  // The newest non-draft, non-prerelease release carrying the contract.
  newest(releases) {
    for (const release of read(releases)) {
      if (!release.draft && !release.prerelease &&
          assetNames(release).includes("BridgeVM-release.json")) return release.tag_name
    }
    return ""
  },
  contract(releasePath, commitPath, manifestPath, version, tarball) {
    const release = read(releasePath), commit = read(commitPath), manifest = read(manifestPath)
    const assets = assetNames(release), graphics = manifest.windows_graphics || {}
    const ok = release.tag_name === version && !release.draft && !release.prerelease &&
      ["BridgeVM-release.json", "SHA256SUMS", tarball].every((name) => assets.includes(name)) &&
      typeof commit.sha === "string" && /^[0-9a-f]{40}$/.test(commit.sha) &&
      manifest.schema_version === 1 && manifest.project === "BridgeVM" &&
      manifest.version === version && manifest.channel === "general-preview" &&
      manifest.product_state === "ENGINEERING_PREVIEW" &&
      manifest.source_commit === commit.sha &&
      graphics.kernel_driver_included === false && graphics.test_signing_required === false &&
      graphics.product_injection_available === false &&
      graphics.install_mode === "3d-off"
    return ok ? "ok" : ""
  },
}
function run([query, ...args]) {
  try { return queries[query](...args) } catch (error) { return "" }
}
JS
json() { /usr/bin/osascript -l JavaScript "$work/json.js" "$@"; }
# Replacing the bundle under a running app or VM would leave it running the
# new bundle's helpers. Match any process started from inside the bundle.
refuse_if_running() {
  local physical processes
  physical=$(cd "$DEST" 2>/dev/null && pwd -P) || return 0
  processes=$(ps -axww -o command=) || fail "could not list running processes"
  if grep -Fq -e "$final_app/Contents/" -e "$physical/$EXPECTED_APP/Contents/" <<<"$processes"; then
    fail "BridgeVM is running. Shut down your VMs, quit BridgeVM, then run the installer again."
  fi
}
[[ $DRY_RUN -eq 1 ]] || refuse_if_running
# Resolve only a non-prerelease carrying the General Preview contract.
if [[ -z "$VERSION" ]]; then
  if [[ -n "${BRIDGEVM_INSTALL_ASSET_DIR:-}" ]]; then
    VERSION="${BRIDGEVM_INSTALL_VERSION:?test fixture must pin a version}"
  else
    curl -fsSL "https://api.github.com/repos/$REPO/releases?per_page=20" -o "$work/github-releases.json" ||
      fail "Couldn't reach GitHub to find the newest release. Check your connection, wait a few minutes, or pass --version vX.Y.Z."
    VERSION=$(json newest "$work/github-releases.json") || VERSION=""
    [[ -n "$VERSION" ]] || fail "no verified general-preview release is published"
  fi
fi
[[ "$VERSION" =~ ^v[0-9]+(\.[0-9]+){2}([.-][0-9A-Za-z]+)*$ ]] ||
  fail "unexpected release tag shape: $VERSION"
TARBALL="BridgeVM-$VERSION.tar.gz"; log "release $VERSION, asset $TARBALL"
release_json="$work/github-release.json" commit_json="$work/github-commit.json"
if [[ -n "${BRIDGEVM_INSTALL_ASSET_DIR:-}" ]]; then
  cp "$BRIDGEVM_INSTALL_ASSET_DIR/github-release.json" "$release_json" &&
    cp "$BRIDGEVM_INSTALL_ASSET_DIR/github-commit.json" "$commit_json" || fail "test fixture has no release provenance metadata"
else
  curl -fsSL "https://api.github.com/repos/$REPO/releases/tags/$VERSION" -o "$release_json" || fail "could not resolve GitHub release metadata for $VERSION"
  curl -fsSL "https://api.github.com/repos/$REPO/commits/$VERSION" -o "$commit_json" || fail "could not resolve the commit for release tag $VERSION"
fi
fetch() {
  if [[ -n "${BRIDGEVM_INSTALL_ASSET_DIR:-}" ]]; then
    cp "$BRIDGEVM_INSTALL_ASSET_DIR/$1" "$2"
  else
    curl -fL --progress-bar \
      "https://github.com/$REPO/releases/download/$VERSION/$1" -o "$2"
  fi
}
fetch "SHA256SUMS" "$work/SHA256SUMS" || fail "could not fetch SHA256SUMS for $VERSION"
fetch "$RELEASE_MANIFEST" "$work/$RELEASE_MANIFEST" ||
  fail "$VERSION has no general-preview release contract; refusing the legacy release"
fetch "$TARBALL" "$work/$TARBALL" || fail "could not fetch $TARBALL"
verify_checksum() {
  local name="$1" expected actual
  expected=$(awk -v name="$name" '$2 == name || $2 == "./" name { print $1 }' "$work/SHA256SUMS")
  [[ -n "$expected" ]] || fail "SHA256SUMS has no entry for $name"
  actual=$(shasum -a 256 "$work/$name" | awk '{print $1}')
  [[ "$actual" == "$expected" ]] || fail "checksum mismatch for $name"
  log "checksum verified: $name $actual"
}
verify_checksum "$RELEASE_MANIFEST"; verify_checksum "$TARBALL"
[[ "$(json contract "$release_json" "$commit_json" "$work/$RELEASE_MANIFEST" \
  "$VERSION" "$TARBALL")" == ok ]] ||
  fail "$VERSION is not a published, commit-bound General Preview"
while IFS= read -r entry; do
  case "$entry" in
    /*) fail "archive contains an absolute path: $entry" ;;
    *../*|../*|*/..|..) fail "archive contains a parent-directory path: $entry" ;;
    "$EXPECTED_APP"|"$EXPECTED_APP"/*) ;;
    *) fail "archive contains an entry outside $EXPECTED_APP: $entry" ;;
  esac
done < <(tar -tzf "$work/$TARBALL")
stage="$work/stage"; mkdir "$stage"
tar -xzf "$work/$TARBALL" -C "$stage"
entries=$(find "$stage" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')
[[ "$entries" == "1" && -d "$stage/$EXPECTED_APP" ]] ||
  fail "archive must contain exactly one $EXPECTED_APP bundle"
# Symlinks may point only inside the bundle: an escaping link would turn the
# post-install copy or a later signature check into a read/write elsewhere.
# A link that does not resolve is refused too.
stage_real=$(/bin/realpath "$stage")
while IFS= read -r -d '' link; do
  target=$(/bin/realpath "$link" 2>/dev/null) && [[ "$target/" == "$stage_real/"* ]] ||
    fail "archive contains a symlink escaping the bundle"
done < <(find "$stage" -type l -print0)
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
architectures=$(/usr/bin/file -b "$app/Contents/MacOS/$executable") &&
  grep -Eqw 'arm64e?' <<<"$architectures" || fail "bundle executable is not Apple Silicon native"
log "bundle verified: $bundle_id / $executable"
if [[ $DRY_RUN -eq 1 ]]; then
  echo "dry run: $TARBALL verified (checksum, archive safety, signature," \
    "identifier); would install to $final_app"
  exit 0
fi
[[ -d "$DEST" ]] || fail "destination does not exist: $DEST"
[[ -w "$DEST" ]] ||
  fail "no write permission for $DEST; re-run with a writable --dest or fix permissions (the installer does not invoke sudo for you)"

# Keep one lock inode per destination. The inherited descriptor holds it
# through publication or rollback, and the empty file stays for later installs.
lock_path="$DEST/.$EXPECTED_APP.install.lock"
if [[ ! -e "$lock_path" && ! -L "$lock_path" ]]; then
  (umask 022; set -o noclobber; : > "$lock_path") 2>/dev/null || true
fi
[[ -f "$lock_path" && ! -L "$lock_path" ]] || fail "could not create the installer lock file"
exec 9<"$lock_path" || fail "could not open the installer lock file"
# flock(2) the descriptor this shell holds open (perl already backs shasum).
/usr/bin/perl -MFcntl=:flock -e '
  my $lock;
  open($lock, "<&=", 9) && flock($lock, LOCK_EX | LOCK_NB) and exit 0;
  die($!{EWOULDBLOCK}
    ? "Another BridgeVM installer is updating this destination. Try again after it finishes.\n"
    : "Could not lock the installation destination ($!).\n");' || exit 1
refuse_if_running
# Stage in an owned directory on the destination filesystem so the last move is a rename.
install_stage=$(mktemp -d -- "$DEST/.$EXPECTED_APP.install-XXXXXX") ||
  fail "could not stage the app in $DEST"
staged="$install_stage/$EXPECTED_APP"
cp -R "$app" "$staged" || fail "could not stage the app in $DEST"
codesign --verify --deep --strict "$staged" ||
  fail "staged copy failed signature verification"
if [[ -e "$final_app" ]]; then
  backup_path="$DEST/$EXPECTED_APP.backup-$(date +%Y%m%d-%H%M%S)-$$"
  [[ ! -e "$backup_path" && ! -L "$backup_path" ]] ||
    fail "the installer backup path already exists"
  backup="$backup_path"
  mv "$final_app" "$backup" || fail "could not preserve the existing app"
fi
mv "$staged" "$final_app" || fail "could not move the new app into place"
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
exec 9<&-
echo "Installed $final_app ($VERSION)"
echo "Your VMs and their data were not touched."
[[ $LAUNCH -eq 1 ]] && open "$final_app"
echo "First run: open BridgeVM from Applications or:  open '$final_app'"
