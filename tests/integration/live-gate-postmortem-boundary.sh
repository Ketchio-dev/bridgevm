# Read-only Windows post-mortem boundary checks for the live-gate policy
# smoke. Sourced by live-gate-policy-smoke.sh; it uses that script's
# `check` helper, $WORK, $POSTMORTEM_HARVEST and $POSTMORTEM_MOUNT.
postmortem_src="$WORK/postmortem-source"
postmortem_image="$WORK/postmortem.dmg"
postmortem_evidence="$WORK/postmortem-evidence"
postmortem_logs="$postmortem_src/Windows/System32/winevt/Logs"
mkdir -p "$postmortem_src/Windows/System32/Tasks" "$postmortem_logs" "$WORK/postmortem-mount"
printf 'agent-current\n' > "$postmortem_src/bvagent.log"
printf '<Task>fixed</Task>\n' > "$postmortem_src/Windows/System32/Tasks/BridgeVM Guest Agent"
printf 'winlogon\n' > "$postmortem_logs/Microsoft-Windows-Winlogon%4Operational.evtx"
printf 'system\n' > "$postmortem_logs/System.evtx"
ln -s System.evtx "$postmortem_logs/Microsoft-Windows-TaskScheduler%4Operational.evtx"
mkfile -n 33554433 "$postmortem_logs/Security.evtx"
printf 'not-allowlisted\n' > "$postmortem_logs/Unexpected.evtx"

check "the post-mortem harvester refuses a writable volume" \
    '! "$POSTMORTEM_HARVEST" --volume "$postmortem_src" --evidence-dir "$WORK/refused" --phase pre-run >/dev/null 2>&1'
hdiutil create -quiet -size 64m -fs HFS+ -srcfolder "$postmortem_src" \
    -format UDRW "$postmortem_image"
postmortem_image_before="$(shasum -a 256 "$postmortem_image" | awk '{print $1}')"
POSTMORTEM_MOUNT="$WORK/postmortem-mount"
hdiutil attach -readonly -nobrowse -mountpoint "$POSTMORTEM_MOUNT" "$postmortem_image" >/dev/null || { echo "FAIL: hdiutil attach" >&2; exit 1; }

"$POSTMORTEM_HARVEST" --volume "$POSTMORTEM_MOUNT" \
    --evidence-dir "$postmortem_evidence" --phase pre-run
check "pre-run post-mortem state is metadata only" \
    'grep -q $'"'"'^agent_log\\t.*\\tmetadata-only\\t'"'"' "$postmortem_evidence/windows-postmortem-pre-run.tsv"'
check "the post-mortem manifest proves a read-only mount" \
    'grep -q $'"'"'^media_read_only\\ttrue$'"'"' "$postmortem_evidence/windows-postmortem-pre-run.tsv" && grep -q $'"'"'^volume_read_only\\ttrue$'"'"' "$postmortem_evidence/windows-postmortem-pre-run.tsv"'
check "pre-run post-mortem capture copies no guest file" \
    '[ ! -e "$postmortem_evidence/windows-postmortem" ]'

"$POSTMORTEM_HARVEST" --volume "$POSTMORTEM_MOUNT" \
    --evidence-dir "$postmortem_evidence" --phase post-run
postmortem_manifest="$postmortem_evidence/windows-postmortem-post-run.tsv"
check "the post-mortem manifest has exactly six fixed artifact ids" \
    '[ "$(awk -F '\''\\t'\'' '\''$1 ~ /^(agent_log|agent_task|winlogon_evtx|task_scheduler_evtx|security_evtx|system_evtx)$/ {n++} END {print n+0}'\'' "$postmortem_manifest")" -eq 6 ]'
check "the post-mortem harvester refuses symlink inputs" \
    'grep -q $'"'"'^task_scheduler_evtx\\t.*\\trefused-symlink\\t'"'"' "$postmortem_manifest"'
check "the post-mortem harvester refuses oversize inputs" \
    'grep -q $'"'"'^security_evtx\\t.*\\trefused-oversize\\t33554433\\t'"'"' "$postmortem_manifest"'
check "the post-mortem harvester copies only bounded allowlisted files" \
    '[ "$(find "$postmortem_evidence/windows-postmortem" -type f | wc -l | tr -d " ")" -eq 4 ]'
check "an unlisted guest event log is never copied" \
    '[ ! -e "$postmortem_evidence/windows-postmortem/Unexpected.evtx" ] && ! grep -q "Unexpected.evtx" "$postmortem_manifest"'
check "post-mortem evidence paths are relative and private-path free" \
    '! grep -E '\''/Users/|/tmp/|BridgeVM/work'\'' "$postmortem_manifest"'

hdiutil detach "$POSTMORTEM_MOUNT" -quiet
POSTMORTEM_MOUNT=""
postmortem_image_after="$(shasum -a 256 "$postmortem_image" | awk '{print $1}')"
check "read-only post-mortem harvest leaves the guest image byte-identical" \
    '[ "$postmortem_image_before" = "$postmortem_image_after" ]'

