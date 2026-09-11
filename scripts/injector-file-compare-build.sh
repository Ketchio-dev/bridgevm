cp "$ASSETS/bv-firstboot-policy.cmd" "$DST_VOL/bv-firstboot-policy.cmd"
if [[ "$NEEDS_GPU_FIRSTBOOT" == "1" ]]; then
  [[ -f "$ASSETS/bv-file-compare.c" ]] || {
    echo "FAIL: binary comparator source missing" >&2; exit 1; }
  log "building ARM64 binary comparator \\bv-file-compare.exe"
  zig cc -target aarch64-windows-gnu -Os -s -std=c11 -Wall -Wextra -Werror \
    "$ASSETS/bv-file-compare.c" -o "$DST_VOL/bv-file-compare.exe"
fi
