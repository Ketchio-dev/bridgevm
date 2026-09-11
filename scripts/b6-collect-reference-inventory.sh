# Sourced after measured closure interactions, before final guest shutdown.
b6_reference_name=''
b6_collect_reference_inventory() {
  local nonce bytes output
  nonce=$(uuidgen) || return 1
  [[ "$nonce" =~ ^[A-Fa-f0-9-]{36}$ ]] || return 1
  b6_reference_name="bv-b6-reference-$nonce"
  cp "$REPO/scripts/win-assets/bv-b6-reference-inventory.ps1" "$OUT/share/$b6_reference_name.ps1" || return 1
  bytes=$(wc -c < "$OUT/share/$b6_reference_name.ps1") || return 1
  bytes=${bytes//[[:space:]]/}
  wait_for "^BVAGENT SHARE host->guest ${b6_reference_name}\.ps1 bytes=$bytes " 1 180 || return 1
  send_ok "powershell -NoProfile -ExecutionPolicy Bypass -File C:\\BridgeVMClosure\\$b6_reference_name.ps1 -OutputPath C:\\BridgeVMClosure\\$b6_reference_name.json" || return 1
  wait_for "^BVAGENT SHARE guest->host ${b6_reference_name}\.json bytes=" 1 60 || return 1
  output="$OUT/share/$b6_reference_name.json"
  [[ -f "$output" && ! -L "$output" ]] || return 1
  bytes=$(wc -c < "$output") || return 1
  (( bytes > 0 && bytes <= 65536 ))
}
if b6_collect_reference_inventory; then
  b6_reference_status=collected
else
  b6_reference_status=unavailable
fi
printf '%s\n' "reference_inventory=$b6_reference_status" "reference_inventory_file=$b6_reference_name.json" \
  'observation_only=true' 'reference_accepted=false' 'criterion_pass=false' > "$OUT/reference-inventory-status.txt"
