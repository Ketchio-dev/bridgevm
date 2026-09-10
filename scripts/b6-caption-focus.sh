# Resolve a fresh, physically hit-tested caption point before any HID click.
b6_caption_hid_point() {
  local hwnd="$1" before line x y
  [[ "$hwnd" =~ ^[1-9][0-9]*$ && "$WIDTH" =~ ^[0-9]+$ && "$HEIGHT" =~ ^[0-9]+$ ]] || return 1
  [[ "$WIDTH" -gt 1 && "$HEIGHT" -gt 1 ]] || return 1
  before=$(wc -l < "$RUN_LOG")
  send_ok "powershell -NoProfile -ExecutionPolicy Bypass -File C:\\BridgeVMClosure\\bv-b6-caption-point.ps1 -Hwnd $hwnd -Width $WIDTH -Height $HEIGHT" || return 1
  line=$(tail -n "+$((before + 1))" "$RUN_LOG" | tr '\r' '\n' | grep '^BVCAPTIONPOINT ' || true)
  [[ "$line" =~ ^BVCAPTIONPOINT\ hwnd=([0-9]+)\ x=([0-9]+)\ y=([0-9]+)\ dpi=([0-9]+)\ hit=2\ owner=([0-9]+)$ ]] || return 1
  [[ "${BASH_REMATCH[1]}" == "$hwnd" && "${BASH_REMATCH[5]}" == "$hwnd" ]] || return 1
  x="${BASH_REMATCH[2]}"; y="${BASH_REMATCH[3]}"
  [[ "$x" -lt "$WIDTH" && "$y" -lt "$HEIGHT" ]] || return 1
  printf '%sx%s' "$((x * 32767 / (WIDTH - 1)))" "$((y * 32767 / (HEIGHT - 1)))"
}
