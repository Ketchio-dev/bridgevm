"""Recheck retained B9 guest focus and input order from exact log lines."""
from __future__ import annotations
import re
def verify_raw_focus_order(raw: bytes, nonce: str, hwnd: int,
                           control_sha: str, guest_sha: str, umd_sha: str) -> None:
    if (not re.fullmatch(r"[0-9a-f]{32}", nonce)
            or type(hwnd) is not int or not 0 < hwnd < 2**63
            or any(not re.fullmatch(r"[0-9a-f]{64}", item)
                   for item in (control_sha, guest_sha, umd_sha))):
        raise ValueError("B9 raw focus identity is invalid")
    lines = [line for line in raw.decode("utf-8", errors="replace")
             .replace("\r", "\n").splitlines() if line]
    launch = f"B9-WORKLOAD-LAUNCHED-{nonce}"
    focus = f"BVAGENT WINFOCUS {hwnd} -> OK WINFOCUS"
    foreground = f"B9-FOREGROUND-{hwnd}"
    key = "live input accepted: command=Key(<redacted>)"
    events = [(index, line) for index, line in enumerate(lines) if line.startswith((
        "B9-WORKLOAD-LAUNCHED-", "BVAGENT WINFOCUS ",
        "B9-FOREGROUND-", "live input accepted: command=Key(",
        "stop: PSCI SYSTEM_OFF"))]
    if (len(events) != 7 or [line for _, line in events[:6]] != [
            launch, focus, foreground, focus, foreground, key]
            or not events[6][1].startswith("stop: PSCI SYSTEM_OFF")):
        raise ValueError("B9 launch, focus, foreground, input or shutdown order differs")
    base = (r"powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\BridgeVMB9\bv-b9-control.ps1"
            + f" -Action {{}} -ExpectedControlSha256 {control_sha}")
    commands = (base.format("Launch") + f" -Nonce {nonce} -ExpectedGuestScriptSha256 {guest_sha}"
                + f" -ExpectedD3D11UmdSha {umd_sha}",
                base.format("Foreground") + f" -Hwnd {hwnd}")
    for index, command in zip((events[0][0], events[2][0], events[4][0]),
                              (commands[0], commands[1], commands[1])):
        if index == 0 or index + 1 >= len(lines):
            raise ValueError("B9 shared command envelope is incomplete")
        if (lines[index - 1] != "BVAGENT CMD " + command + " exit=0"
                or lines[index + 1] != "BVAGENT END " + command):
            raise ValueError("B9 shared command envelope differs")
