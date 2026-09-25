#!/usr/bin/env bash
# T22 marker workload travels through the guest share, never as agent payload.

t22_marker_log_has() {
  [[ -f "$1" ]] && tr '\r' '\n' < "$1" | grep -E "$2" > /dev/null
}

t22_random_marker() { # fixed action label, host CSPRNG identity
  [[ "$1" == ORIGINAL || "$1" == CLOBBERED || "$1" == POSTKILL || "$1" == FINAL ]] || return 1
  local nonce
  nonce=$(/usr/bin/openssl rand -hex 16) || return 1
  [[ "$nonce" =~ ^[0-9a-f]{32}$ ]] || return 1
  printf 'BV-%s-%s\n' "$1" "$nonce"
}

t22_marker_wait_log() { # log, exact-pattern, seconds
  local log=$1 pattern=$2 deadline=$((SECONDS + $3))
  while (( SECONDS < deadline )); do
    t22_marker_log_has "$log" "$pattern" && return 0
    kill -0 "$SNAPSHOT_LAUNCHER" 2>/dev/null || return 1
    sleep 1
  done
  return 1
}

t22_marker_wait_result() { # share, nonce, action, expected marker, output
  local share=$1 nonce=$2 action=$3 expected=$4 output=$5
  local result=$share/t22-$nonce-$action.txt done_file=$share/t22-$nonce-$action.done
  local deadline=$((SECONDS + STEP_TIMEOUT)) size digest actual value
  while (( SECONDS < deadline )); do
    if [[ -e "$done_file" || -L "$done_file" ]]; then
      [[ -f "$done_file" && ! -L "$done_file" ]] || return 1
      size=$(stat -f %z "$done_file") || return 1
      (( size == 64 )) || return 1
      digest=$(cat "$done_file") || return 1
      [[ "$digest" =~ ^[0-9a-f]{64}$ ]] || return 1
      if [[ -e "$result" || -L "$result" ]]; then
        [[ -f "$result" && ! -L "$result" ]] || return 1
        size=$(stat -f %z "$result") || return 1
        (( size > 0 && size <= 128 )) || return 1
        actual=$(shasum -a 256 "$result") || return 1
        if [[ "${actual%% *}" == "$digest" ]]; then
          value=$(cat "$result") || return 1
          if [[ "$action" == Write ]]; then
            [[ "$value" == "$expected" ]] || return 1
          else
            [[ "$value" == BV-NO-MARKER || "$value" =~ ^BV-(ORIGINAL|CLOBBERED|POSTKILL|FINAL)-[0-9a-f]{32}$ ]] || return 1
          fi
          printf '%s\n' "$value" > "$output" || return 1
          return 0
        fi
      fi
    fi
    kill -0 "$SNAPSHOT_LAUNCHER" 2>/dev/null || return 1
    sleep 1
  done
  return 1
}

t22_marker_action() { # share, phase directory, control, log, action, marker, output
  local share=$1 pdir=$2 ctl=$3 log=$4 action=$5 marker=$6 output=$7
  local nonce command receipt result done_file pending asset_sha
  nonce=$(/usr/bin/openssl rand -hex 16) || return 1
  [[ "$nonce" =~ ^[0-9a-f]{32}$ ]] || return 1
  [[ "$action" == Read || "$action" == Write ]] || return 1
  result=$share/t22-$nonce-$action.txt
  done_file=$share/t22-$nonce-$action.done
  pending=$share/t22-$nonce-$action.pending
  [[ ! -e "$result" && ! -L "$result" && ! -e "$done_file" && ! -L "$done_file" && ! -e "$pending" && ! -L "$pending" ]] || return 1
  if [[ "$action" == Write ]]; then
    [[ "$marker" =~ ^BV-(ORIGINAL|CLOBBERED|POSTKILL|FINAL)-[0-9a-f]{32}$ ]] || return 1
  fi
  asset_sha=$(shasum -a 256 "$share/bv-a19-t22-marker.ps1") || return 1
  asset_sha=${asset_sha%% *}
  [[ "$asset_sha" =~ ^[0-9a-f]{64}$ ]] || return 1
  command="powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\\bridgevm-share\\bv-a19-t22-marker.ps1 -Action Launch -WorkAction $action -Nonce $nonce -ExpectedSha256 $asset_sha"
  [[ "$action" != Write ]] || command+=" -Marker $marker"
  receipt=$pdir/$action.launch.txt
  python3 "$REPO/scripts/snapshot-restore-channel.py" "$ctl" "$log" "$command" "$STEP_TIMEOUT" "$receipt" || return 1
  [[ "$(tr '\r' '\n' < "$receipt" | sed '/^$/d')" == "T22-MARKER-LAUNCHED-$nonce" ]] || return 1
  t22_marker_wait_result "$share" "$nonce" "$action" "$marker" "$output"
}

t22_boot_and_mark() { # new marker, phase name
  local marker=$1 phase=$2
  [[ "$phase" == phase1-original || "$phase" == phase3-clobber || "$phase" == phase5-postkill || "$phase" == phase7-restored ]] || return 1
  [[ "$marker" =~ ^BV-(ORIGINAL|CLOBBERED|POSTKILL|FINAL)-[0-9a-f]{32}$ ]] || return 1
  local pdir=$OUT/$phase
  local ctl=$pdir/agent.ctl log=$pdir/run.log share=$pdir/share launcher
  local probe_args=(); [[ -z "${BRIDGEVM_PREBUILT_PROBE:-}" ]] || probe_args+=(--release --skip-build)
  mkdir -m 700 "$pdir" || return 1
  mkdir -m 700 "$share" || return 1
  install -m 600 "$REPO/scripts/win-assets/bv-a19-t22-marker.ps1" "$share/bv-a19-t22-marker.ps1" || return 1
  : > "$ctl" || return 1
  chmod 600 "$ctl" || return 1
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$WORK_DISK" --vars "$WORK_VARS" \
    --evidence-dir "$pdir" --watchdog-ms $((BOOT_TIMEOUT * 1000)) \
    --ram-mib 6144 --smp-cpus 4 --agent-service-control "$ctl" \
    --agent-share-host "$share" --agent-share-guest 'C:\bridgevm-share' \
    --agent-share-ms 1000 --agent-share-max-kb 1024 \
    ${probe_args[@]+"${probe_args[@]}"} > "$pdir/launcher.out" 2>&1 &
  launcher=$!
  SNAPSHOT_LAUNCHER=$launcher
  if ! t22_marker_wait_log "$log" '^BVAGENT SERVICE start' "$BOOT_TIMEOUT"; then
    if t22_marker_log_has "$log" 'ramfb checkpoint'; then
      echo 'guest framebuffer appeared without the required agent service' >&2
    fi
    return 1
  fi
  t22_marker_wait_log "$log" '^BVAGENT SHARE host->guest bv-a19-t22-marker\.ps1 bytes=' "$STEP_TIMEOUT" || return 1
  t22_marker_action "$share" "$pdir" "$ctl" "$log" Read "" "$pdir/marker-before.txt" || return 1
  t22_marker_action "$share" "$pdir" "$ctl" "$log" Write "$marker" "$pdir/marker-after.txt" || return 1
  [[ "$(cat "$pdir/marker-after.txt")" == "$marker" ]] || return 1
  snapshot_shutdown "$ctl" "$log"
}

boot_and_mark() {
  t22_boot_and_mark "$@" || { snapshot_stop_launcher; return 1; }
}
