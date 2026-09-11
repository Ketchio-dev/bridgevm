# UIA non-discovery is recorded, not advertised as visual proof of no overlay.
b6_tip_hid_point() {
  local hwnd="$1" before line x y
  [[ "$hwnd" =~ ^[1-9][0-9]*$ && "$WIDTH" =~ ^[0-9]+$ && "$HEIGHT" =~ ^[0-9]+$ ]] || return 1
  [[ "$WIDTH" -gt 1 && "$HEIGHT" -gt 1 ]] || return 1
  before=$(wc -l < "$RUN_LOG")
  send_ok "powershell -Mta -NoProfile -ExecutionPolicy Bypass -File C:\\BridgeVMClosure\\bv-b6-native-tip-point.ps1 -Hwnd $hwnd -Width $WIDTH -Height $HEIGHT" || return 1
  line=$(tail -n "+$((before + 1))" "$RUN_LOG" | tr '\r' '\n' | grep '^BVTIPPOINT ' || true)
  if [[ "$line" == "BVTIPPOINT hwnd=$hwnd state=not-found" ]]; then printf 'not-found'; return 0; fi
  [[ "$line" =~ ^BVTIPPOINT\ hwnd=([0-9]+)\ state=present\ x=([0-9]+)\ y=([0-9]+)\ owner=([0-9]+)$ ]] || return 1
  [[ "${BASH_REMATCH[1]}" == "$hwnd" && "${BASH_REMATCH[4]}" == "$hwnd" ]] || return 1
  x="${BASH_REMATCH[2]}"; y="${BASH_REMATCH[3]}"
  [[ "$x" -lt "$WIDTH" && "$y" -lt "$HEIGHT" ]] || return 1
  printf '%sx%s' "$((x * 32767 / (WIDTH - 1)))" "$((y * 32767 / (HEIGHT - 1)))"
}

b6_dismiss_packaged_tip() {
  local hwnd="$1" point baseline attempt
  point=$(b6_tip_hid_point "$hwnd") || return 1
  if [[ "$point" == not-found ]]; then echo "TIP: hwnd=$hwnd UIA button not found; visual review still required" >&2; return 0; fi
  baseline=$(wait_baseline 'live input accepted: command=Pointer\(')
  echo "TIP: hwnd=$hwnd clicking owned UIA button at $point" >&2
  printf 'POINTER click:%s\n' "$point" >> "$INPUT"
  wait_after 'live input accepted: command=Pointer\(' "$baseline" 15 || return 1
  for attempt in 1 2 3; do
    sleep 1
    point=$(b6_tip_hid_point "$hwnd") || return 1
    if [[ "$point" == not-found ]]; then echo "TIP: hwnd=$hwnd button absent from UIA after click" >&2; return 0; fi
  done
  echo "FAIL: hwnd=$hwnd Got it button remains after owned click" >&2
  return 1
}
