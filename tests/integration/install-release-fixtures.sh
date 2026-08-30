#!/usr/bin/env bash
# Shared fixture builders for the fail-closed release installer smoke.

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

make_manifest() { # $1: asset dir
  python3 scripts/generate-general-preview-manifest.py \
    --version "$VERSION" --commit "$(printf 'a%.0s' {1..40})" \
    --output "$1/$RELEASE_MANIFEST" >/dev/null
}

write_sums() { # $1: asset dir
  (cd "$1" && shasum -a 256 "$TARBALL" "$RELEASE_MANIFEST" > SHA256SUMS)
}

make_assets() { # $1: asset dir, $2: dir containing BridgeVM.app
  mkdir -p "$1"
  tar -czf "$1/$TARBALL" -C "$2" "BridgeVM.app"
  make_manifest "$1"
  write_sums "$1"
}

run_installer() { # $1: asset dir, $2: destination, then installer options
  local assets="$1" dest="$2"
  shift 2
  BRIDGEVM_INSTALL_ASSET_DIR="$assets" BRIDGEVM_INSTALL_VERSION="$VERSION" \
    bash scripts/install-bridgevm.sh --dest "$dest" "$@"
}
