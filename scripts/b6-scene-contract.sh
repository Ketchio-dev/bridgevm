#!/usr/bin/env bash
# Scene evidence must describe the requested scale and the current WINLIST.
# The caller owns RUN_LOG, OUT and the agent-channel send() function.

b6_dpi_matches() {
  local line="${1%$'\r'}" hwnd="$2" dpi="$3" scale
  case "$dpi" in 96) scale=100 ;; 120) scale=125 ;; 144) scale=150 ;; *) return 1 ;; esac
  [[ "$hwnd" =~ ^[1-9][0-9]*$ && "$line" != *$'\n'* ]] || return 1
  [[ "$line" =~ ^BVEFFECTIVEDPI\ hwnd=$hwnd\ dpi=$dpi\ awareness=[012]\ monitor_scale=$scale$ ]]
}

b6_scene_fail() {
  printf 'run=%s\nscene=%s\nfailure_code=%s\n' "$1" "$2" "$3" > "${OUT:?}/scene-failure.env"
  echo "FAIL: run=$1 scene=$2 $3; no subsequent scene may count" >&2; python3 "$(dirname "${BASH_SOURCE[0]}")/b6-observe-scene-failure.py" --out "$OUT" >&2 || true
  exit 1
}

wait_window_gone() {
  local hwnd="$1" attempt before reply log="${RUN_LOG:?}"
  for attempt in 1 2 3 4 5 6 7 8 9 10; do
    before=$(wc -l < "$log")
    send 'WINLIST' '^BVAGENT WINLIST WINEND$' || return 1
    reply=$(tail -n "+$((before + 1))" "$log") || return 1
    if ! grep "^BVAGENT WINLIST WIN $hwnd " <<<"$reply" >/dev/null; then
      return 0
    fi
    sleep 2
  done
  echo "FAIL: hwnd=$hwnd still listed after teardown" >&2
  return 1
}

find_hwnd() {
  local title_substr="$1" win_line before log="${RUN_LOG:?}"
  # Historical listings can name a closed or different scene. Only the reply
  # to this WINLIST request is eligible, even when it contains no match.
  before=$(wc -l < "$log")
  send 'WINLIST' '^BVAGENT WINLIST WINEND$' || return 1
  win_line=$(tail -n "+$((before + 1))" "$log" | grep '^BVAGENT WINLIST WIN ' | while IFS= read -r line; do
    local title_b64 title
    title_b64=$(awk '{print $10}' <<<"$line")
    title=$(printf '%s' "$title_b64" | base64 -d 2>/dev/null || true)
    [[ "$title" == *"$title_substr"* ]] && { printf '%s\n' "$line"; }
  done | tail -1 || true)
  awk '{print $4}' <<<"${win_line:-}"
}
