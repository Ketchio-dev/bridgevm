#!/usr/bin/env bash
# Copy a fixed, bounded Windows diagnostic allowlist from an already mounted,
# read-only guest volume. This helper never accepts a guest-relative path from
# the caller and never writes through the mounted volume.
set -euo pipefail

usage() {
  echo "usage: $0 --volume PATH --evidence-dir PATH --phase pre-run|post-run" >&2
  exit 2
}

VOLUME=""
EVIDENCE_DIR=""
PHASE=""
while (( $# > 0 )); do
  case "$1" in
    --volume) [[ $# -ge 2 ]] || usage; VOLUME="$2"; shift 2 ;;
    --evidence-dir) [[ $# -ge 2 ]] || usage; EVIDENCE_DIR="$2"; shift 2 ;;
    --phase) [[ $# -ge 2 ]] || usage; PHASE="$2"; shift 2 ;;
    *) usage ;;
  esac
done
[[ -d "$VOLUME" && -n "$EVIDENCE_DIR" ]] || usage
[[ "$PHASE" == "pre-run" || "$PHASE" == "post-run" ]] || usage

VOLUME="$(python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$VOLUME")"
EVIDENCE_DIR="$(python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$EVIDENCE_DIR")"
case "$EVIDENCE_DIR" in
  "$VOLUME"|"$VOLUME"/*)
    echo "REFUSED: evidence directory resolves inside the guest volume" >&2
    exit 1
    ;;
esac

MAX_FILE_BYTES=33554432
MANIFEST="$EVIDENCE_DIR/windows-postmortem-$PHASE.tsv"
COPY_DIR="$EVIDENCE_DIR/windows-postmortem"
if [[ -L "$MANIFEST" || -L "$COPY_DIR" ]]; then
  echo "REFUSED: post-mortem evidence output may not be a symlink" >&2
  exit 1
fi
mkdir -p "$EVIDENCE_DIR"

info="$(diskutil info -plist "$VOLUME" 2>/dev/null || true)"
writable_media="$(printf '%s' "$info" | plutil -extract WritableMedia raw -o - - 2>/dev/null || true)"
writable_volume="$(printf '%s' "$info" | plutil -extract WritableVolume raw -o - - 2>/dev/null || true)"
media_read_only="unknown"
volume_read_only="unknown"
[[ "$writable_media" == "false" ]] && media_read_only="true"
[[ "$writable_volume" == "false" ]] && volume_read_only="true"
{
  printf 'schema\tbridgevm-windows-postmortem-v1\n'
  printf 'phase\t%s\n' "$PHASE"
  printf 'media_read_only\t%s\n' "${media_read_only:-unknown}"
  printf 'volume_read_only\t%s\n' "${volume_read_only:-unknown}"
  printf 'max_file_bytes\t%s\n' "$MAX_FILE_BYTES"
  printf 'artifact_id\tguest_path\tstatus\tsize_bytes\tmtime_epoch\tsha256\tevidence_path\n'
} > "$MANIFEST"

if [[ "$media_read_only" != "true" || "$volume_read_only" != "true" ]]; then
  echo "REFUSED: Windows post-mortem harvest requires read-only media and volume" >&2
  exit 1
fi
[[ "$PHASE" == "pre-run" ]] || mkdir -p "$COPY_DIR"

record() {
  local id="$1" guest_path="$2" source="$3" output_name="$4"
  local status="missing" size="-" mtime="-" sha="-" evidence="-"
  if [[ -L "$source" ]]; then
    status="refused-symlink"
  elif [[ -f "$source" ]]; then
    size="$(stat -f '%z' "$source")"
    mtime="$(stat -f '%m' "$source")"
    if (( size > MAX_FILE_BYTES )); then
      status="refused-oversize"
    else
      sha="$(shasum -a 256 "$source" | awk '{print $1}')"
      status="metadata-only"
      if [[ "$PHASE" == "post-run" ]]; then
        local destination="$COPY_DIR/$output_name" temporary copied_sha
        if [[ -d "$destination" ]]; then
          status="refused-output-directory"
        else
          temporary="$(mktemp "$COPY_DIR/.harvest.XXXXXX")"
          if cp -p "$source" "$temporary"; then
            copied_sha="$(shasum -a 256 "$temporary" | awk '{print $1}')"
            if [[ "$copied_sha" == "$sha" ]]; then
              mv -f "$temporary" "$destination"
              status="copied"
              evidence="windows-postmortem/$output_name"
            else
              status="copy-hash-mismatch"
              rm -f "$temporary"
            fi
          else
            status="copy-failed"
            rm -f "$temporary"
          fi
        fi
      fi
    fi
  fi
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$id" "$guest_path" "$status" "$size" "$mtime" "$sha" "$evidence" >> "$MANIFEST"
}

record agent_log 'C:\bvagent.log' \
  "$VOLUME/bvagent.log" bvagent.log
record agent_task 'C:\Windows\System32\Tasks\BridgeVM Guest Agent' \
  "$VOLUME/Windows/System32/Tasks/BridgeVM Guest Agent" bridgevm-guest-agent-task.xml
record winlogon_evtx 'C:\Windows\System32\winevt\Logs\Microsoft-Windows-Winlogon%4Operational.evtx' \
  "$VOLUME/Windows/System32/winevt/Logs/Microsoft-Windows-Winlogon%4Operational.evtx" winlogon-operational.evtx
record task_scheduler_evtx 'C:\Windows\System32\winevt\Logs\Microsoft-Windows-TaskScheduler%4Operational.evtx' \
  "$VOLUME/Windows/System32/winevt/Logs/Microsoft-Windows-TaskScheduler%4Operational.evtx" task-scheduler-operational.evtx
record security_evtx 'C:\Windows\System32\winevt\Logs\Security.evtx' \
  "$VOLUME/Windows/System32/winevt/Logs/Security.evtx" security.evtx
record system_evtx 'C:\Windows\System32\winevt\Logs\System.evtx' \
  "$VOLUME/Windows/System32/winevt/Logs/System.evtx" system.evtx
