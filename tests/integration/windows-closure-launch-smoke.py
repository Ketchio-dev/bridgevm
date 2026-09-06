#!/usr/bin/env python3
"""Static guest launch protocol mutations; not Windows execution evidence."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
guest = (ROOT / "scripts/win-assets/bv-windows-closure-launch.ps1").read_bytes()
host = (ROOT / "scripts/windows-1.0-closure-interact.sh").read_text()


def valid(payload: bytes, caller: str) -> bool:
    required = (
        b"Invoke-CimMethod -ClassName Win32_Process -MethodName Create",
        b"$result.ReturnValue -ne 0 -or $result.ProcessId -le 0",
        b"[IO.File]::WriteAllText($marker",
        b"C:\\BridgeVMClosure\\bv-notepad-started.log",
    )
    launch = next((line for line in caller.splitlines() if line.startswith("LAUNCH_CMD=")), "")
    return (
        payload.count(b"\n") > 0 and payload.count(b"\n") == payload.count(b"\r\n")
        and all(part in payload for part in required)
        and b"Start-Process" not in payload
        and "-File C:\\BridgeVMClosure\\bv-windows-closure-launch.ps1" in launch
        and "wait_for '^BVAGENT SHARE guest->host bv-notepad-started.log bytes=' 1 30" in caller
        and "echo 'FAIL: Notepad launch completion missing' >&2; exit 1;" in caller
    )


assert valid(guest, host)
for original in (
    b"Invoke-CimMethod", b"$result.ReturnValue -ne 0", b"$result.ProcessId -le 0",
    b"[IO.File]::WriteAllText", b"bv-notepad-started.log", b"\r\n",
):
    assert not valid(guest.replace(original, b"invalid"), host), original
for original in ("-File C:", "guest->host bv-notepad-started.log", "exit 1;"):
    assert not valid(guest, host.replace(original, "invalid")), original
print("PASS: closure launch protocol and nine static rejection mutations")
