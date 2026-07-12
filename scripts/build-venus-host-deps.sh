#!/usr/bin/env bash
set -euo pipefail

BRIDGEVM_3D_DIR="${BRIDGEVM_3D_DIR:-"$HOME/BridgeVM/3d"}"
VIRGL_COMMIT="${VIRGL_COMMIT:-2a173ee}"
PREFIX="$BRIDGEVM_3D_DIR/prefix"
SRC_DIR="$BRIDGEVM_3D_DIR/virglrenderer"
BUILD_DIR="$SRC_DIR/build-venus"
PATCH_FILE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/patches/virglrenderer-macos-venus.patch"

log() {
  printf 'build-venus-host-deps: %s\n' "$*"
}

missing=()
for dep in meson ninja pkgconf libepoxy molten-vk vulkan-loader vulkan-headers; do
  if ! brew list --versions "$dep" >/dev/null 2>&1; then
    missing+=("$dep")
  fi
done

if ((${#missing[@]})); then
  printf 'Missing Homebrew dependencies:\n' >&2
  printf '  %s\n' "${missing[@]}" >&2
  exit 2
fi

if ! python3 -c 'import yaml' >/dev/null 2>&1; then
  printf 'Python module PyYAML is not importable. Install with:\n' >&2
  printf '  python3 -m pip install --break-system-packages pyyaml\n' >&2
  exit 2
fi

mkdir -p "$BRIDGEVM_3D_DIR"

if [[ ! -d "$SRC_DIR/.git" ]]; then
  git clone https://gitlab.freedesktop.org/virgl/virglrenderer.git "$SRC_DIR"
fi

git -C "$SRC_DIR" fetch origin
git -C "$SRC_DIR" checkout --detach "$VIRGL_COMMIT"
git -C "$SRC_DIR" submodule update --init --recursive

# Apply BridgeVM's local renderer patches before both first setup and reconfigure
# builds. They provide MoltenVK-direct dlopen via BRIDGEVM_VULKAN_LIB,
# host-pointer import for MoltenVK's non-aliasing MTLBuffer path, and the Apple
# core-profile GLSL rule that avoids requiring a UBO extension already in 1.40.
if [[ -f "$PATCH_FILE" ]]; then
  if git -C "$SRC_DIR" apply --check "$PATCH_FILE" 2>/dev/null; then
    git -C "$SRC_DIR" apply "$PATCH_FILE"
    log "applied virglrenderer-macos-venus.patch"
  elif git -C "$SRC_DIR" apply --reverse --check "$PATCH_FILE" 2>/dev/null; then
    log "virglrenderer-macos-venus.patch already applied"
  else
    printf 'Failed to apply %s against %s at %s\n' "$PATCH_FILE" "$SRC_DIR" "$VIRGL_COMMIT" >&2
    exit 1
  fi
fi

meson_args=(
  "$BUILD_DIR"
  "$SRC_DIR"
  -Dvenus=true
  -Dplatforms=
  -Dtests=false
  -Drender-server-mode=process
  --prefix "$PREFIX"
)

if [[ -d "$BUILD_DIR" ]]; then
  meson setup "${meson_args[@]}" --reconfigure
else
  meson setup "${meson_args[@]}"
fi

ninja -C "$BUILD_DIR" install

lib_path="$PREFIX/lib/libvirglrenderer.dylib"
if [[ ! -e "$lib_path" ]]; then
  printf 'Expected %s to exist after install\n' "$lib_path" >&2
  exit 1
fi

server_path=""
while IFS= read -r candidate; do
  server_path="$candidate"
  break
done < <(find "$PREFIX" -name virgl_render_server -type f -perm -111 -print)

if [[ -z "$server_path" ]]; then
  server_path="$BUILD_DIR/server/virgl_render_server"
  printf 'virgl_render_server was not installed under %s; use build-tree path: %s\n' "$PREFIX" "$server_path"
else
  printf 'virgl_render_server installed at: %s\n' "$server_path"
fi

if [[ ! -x "$server_path" ]]; then
  printf 'Render server is not executable: %s\n' "$server_path" >&2
  exit 1
fi

vkr_count="$(nm -U "$server_path" | grep -c 'vkr_' || true)"
if ((vkr_count <= 100)); then
  printf 'Expected >100 vkr_ symbols in %s, found %s\n' "$server_path" "$vkr_count" >&2
  exit 1
fi

printf 'Verified libvirglrenderer: %s\n' "$lib_path"
printf 'Verified render server vkr_ symbols: %s\n' "$vkr_count"
