#!/usr/bin/env bash
# The Windows Coherence protocol lives in two languages that cannot import each
# other: the PowerShell agent answers WINLIST/WINBOUNDS/WINFOCUS/WINCLOSE and
# the Swift host parses and emits the same grammar. Nothing but this check
# notices when one side drifts.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.." || exit 1

AGENT=scripts/win-assets/bvagent.ps1
HOST=apps/macos/Sources/BridgeVMApp/Services/HvfGuestWindowProtocol.swift
CHANNEL=crates/bridgevm-hvf/examples/hvf_gic_boot_probe/agent_console
TESTS=apps/macos/Tests/BridgeVMAppTests/HvfGuestWindowProtocolTests.swift
status=0

fail() { echo "FAIL: $1" >&2; status=1; }

# Every verb must exist on both sides.
for verb in WINLIST WINBOUNDS WINFOCUS WINCLOSE; do
  grep -q "'$verb'" "$AGENT" || fail "$verb missing from the agent ($AGENT)"
done
for verb in WINBOUNDS WINFOCUS WINCLOSE; do
  grep -q "\"$verb " "$HOST" || fail "$verb missing from the host emitter ($HOST)"
done
for verb in WINLIST WINBOUNDS WINFOCUS WINCLOSE; do
  grep -q "\"$verb\"" "$CHANNEL/protocol.rs" || fail "$verb is not raw on the agent channel"
done

# The list grammar: agent emits WIN lines and a WINEND terminator; the host
# parses exactly that shape and the tests pin the field order.
grep -q 'WIN \$w \$wpid' "$AGENT" || fail "agent WIN line lost its hwnd/pid field order"
grep -q "Write-Line \$h 'WINEND' 'WINEND'" "$AGENT" || fail "agent no longer terminates with WINEND"
grep -q '"WINEND"' "$HOST" || fail "host parser no longer stops at WINEND"
grep -q 'maxSplits: 7' "$HOST" || fail "host parser field split no longer matches the 8-field WIN line"
grep -q 'testOutboundCommandsMatchTheAgentGrammar' "$TESTS" || fail "grammar pin test is gone"
grep -q 'line == "WINEND"' "$CHANNEL/window_protocol.rs" || fail "channel no longer frames WINLIST"

# The agent must post WM_CLOSE, never send it: a hung window must not hang the
# agent loop that every other verb shares.
grep -q 'PostMessage(\$hw, 0x10' "$AGENT" || fail "WINCLOSE no longer posts WM_CLOSE"

[[ $status -eq 0 ]] && echo "hvf windows coherence protocol: PASS"
exit $status
