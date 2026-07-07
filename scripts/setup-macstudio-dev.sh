#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

log() {
  printf 'setup-macstudio-dev: %s\n' "$*"
}

require_command() {
  local cmd="$1"
  local hint="$2"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n%s\n' "$cmd" "$hint" >&2
    exit 2
  fi
}

if [[ "$(uname -s)" != "Darwin" ]]; then
  printf 'This setup script is for macOS hosts.\n' >&2
  exit 2
fi

if ! xcode-select -p >/dev/null 2>&1; then
  printf 'Xcode command line tools are missing. Run: xcode-select --install\n' >&2
  exit 2
fi

require_command brew "Install Homebrew from https://brew.sh, then re-run this script."
require_command cargo "Install Rust with rustup or Homebrew, then re-run this script."
require_command swift "Install Xcode, then re-run this script."

brew_deps=(
  qemu
  meson
  ninja
  pkgconf
  libepoxy
  molten-vk
  vulkan-loader
  vulkan-headers
)

missing=()
for dep in "${brew_deps[@]}"; do
  if ! brew list --versions "$dep" >/dev/null 2>&1; then
    missing+=("$dep")
  fi
done

if ((${#missing[@]})); then
  log "installing Homebrew dependencies: ${missing[*]}"
  brew install "${missing[@]}"
else
  log "Homebrew dependencies already installed"
fi

if ! python3 -c 'import yaml' >/dev/null 2>&1; then
  log "installing PyYAML for virglrenderer Meson helpers"
  python3 -m pip install --break-system-packages pyyaml
fi

log "checking Rust workspace"
cargo check --workspace

log "checking macOS Swift package"
swift test --package-path "$ROOT/apps/macos"

log "building BridgeVM Venus host dependencies"
"$ROOT/scripts/build-venus-host-deps.sh"

cat <<EOF

Mac Studio development setup is ready.

Useful environment for Venus runs:
  export BRIDGEVM_VENUS_PREFIX="\$HOME/BridgeVM/3d/prefix"
  export BRIDGEVM_VULKAN_LIB="/opt/homebrew/lib/libMoltenVK.dylib"

Quick checks:
  cargo run -p bridgevm-cli -- doctor
  scripts/run-venus-host-probe.sh
EOF
