"""Recheck retained B9 guest focus and input order from exact log lines."""

from __future__ import annotations

import re


def verify_raw_focus_order(raw: bytes, nonce: str, hwnd: int) -> None:
    if (not re.fullmatch(r"[0-9a-f]{32}", nonce)
            or type(hwnd) is not int or not 0 < hwnd < 2**63):
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
    for index in (events[2][0], events[4][0]):
        if index == 0 or index + 1 >= len(lines):
            raise ValueError("B9 foreground command envelope is incomplete")
        header = lines[index - 1]
        if not header.startswith("BVAGENT CMD ") or not header.endswith(" exit=0"):
            raise ValueError("B9 foreground command did not complete successfully")
        command = header[len("BVAGENT CMD "):-len(" exit=0")]
        if (not re.fullmatch(r"powershell\.exe -NoProfile -EncodedCommand [A-Za-z0-9+/=]+",
                             command)
                or lines[index + 1] != "BVAGENT END " + command):
            raise ValueError("B9 foreground command envelope differs")
