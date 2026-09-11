# Construct only fixed read-only diagnostic commands; inputs validated by caller.
b6_uia_probe_command() {
  if [[ "$1" == Native ]]; then
    printf 'powershell -Mta -NoProfile -NonInteractive -ExecutionPolicy Bypass -File C:\\BridgeVMClosure\\bv-b6-native-uia.ps1 -Hwnd %s' "$2"
  else
    printf 'powershell -%s -NoProfile -NonInteractive -ExecutionPolicy Bypass -File C:\\BridgeVMClosure\\bv-b6-uia-diagnostic.ps1 -Hwnd %s -Width %s -Height %s' "$1" "$2" "$3" "$4"
  fi
}
