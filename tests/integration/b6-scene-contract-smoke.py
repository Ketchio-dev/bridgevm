#!/usr/bin/env python3
"""Reject wrong-DPI evidence, stale windows and unconfirmed scene teardown."""
import base64
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
CONTRACT = ROOT / "scripts/b6-scene-contract.sh"


def run(body, *, env=None):
    return subprocess.run(
        ["bash", "-c", 'set -euo pipefail; source "$CONTRACT"; ' + body],
        env={**os.environ, "CONTRACT": str(CONTRACT), **(env or {})},
        text=True, capture_output=True, timeout=5)


def check_dpi(line, hwnd, dpi, expected):
    result = run('b6_dpi_matches "$LINE" "$HWND" "$DPI"',
                 env={"LINE": line, "HWND": str(hwnd), "DPI": str(dpi)})
    assert (result.returncode == 0) == expected, (line, result.stderr)


def window(hwnd, title="Untitled - Notepad"):
    encoded = base64.b64encode(title.encode()).decode()
    return f"BVAGENT WINLIST WIN {hwnd} 7 50 60 700 500 {encoded}\n"


def main():
    for dpi, scale in ((96, 100), (120, 125), (144, 150)):
        good = f"BVEFFECTIVEDPI hwnd=123 dpi={dpi} awareness=2 monitor_scale={scale}"
        check_dpi(good, 123, dpi, True)
        check_dpi(good + "\r", 123, dpi, True)
        check_dpi(good, 124, dpi, False)
        check_dpi(good.replace(f"dpi={dpi}", "dpi=192"), 123, dpi, False)
        check_dpi(good.replace(f"monitor_scale={scale}", "monitor_scale=200"), 123, dpi, False)
        check_dpi(good.replace("awareness=2", "awareness=-1"), 123, dpi, False)
        check_dpi(good + "\nforged extra observation", 123, dpi, False)
    check_dpi("BVEFFECTIVEDPI hwnd=123 dpi=192 awareness=2 monitor_scale=200", 123, 192, False)
    check_dpi("", 123, 96, False)

    with tempfile.TemporaryDirectory() as work:
        root = Path(work)
        log, reply = root / "run.log", root / "reply.log"
        env = {"RUN_LOG": str(log), "REPLY": str(reply), "OUT": str(root)}
        sender = 'send() { cat "$REPLY" >> "$RUN_LOG"; }; sleep() { :; }; '
        log.write_text(window(123) + "BVAGENT WINLIST WINEND\n")
        reply.write_text("BVAGENT WINLIST WINEND\n")
        result = run(sender + "wait_window_gone 123", env=env)
        assert result.returncode == 0, result.stderr
        result = run(sender + "find_hwnd Notepad", env=env)
        assert result.returncode == 0 and result.stdout.strip() == "", result

        reply.write_text(window(456) + "BVAGENT WINLIST WINEND\n")
        result = run(sender + "find_hwnd Notepad", env=env)
        assert result.returncode == 0 and result.stdout.strip() == "456", result
        reply.write_text(window(1234) + "BVAGENT WINLIST WINEND\n")
        result = run(sender + "wait_window_gone 123", env=env)
        assert result.returncode == 0, result.stderr
        reply.write_text(window(123) + "BVAGENT WINLIST WINEND\n")
        result = run(sender + "wait_window_gone 123", env=env)
        assert result.returncode != 0 and "still listed" in result.stderr, result
        reply.write_text(window(123) + window(456) * 2000 + "BVAGENT WINLIST WINEND\n")
        result = run(sender + "wait_window_gone 123", env=env)
        assert result.returncode != 0, "a large reply must not turn a grep SIGPIPE into absence"
        result = run("send() { return 1; }; wait_window_gone 123", env=env)
        assert result.returncode != 0, "a failed listing cannot prove absence"
        result = run('send() { rm "$RUN_LOG"; }; wait_window_gone 123', env=env)
        assert result.returncode != 0, "an unreadable reply cannot prove absence"

        result = run("b6_scene_fail 2 packaged reset-failed; touch \"$OUT/continued\"", env=env)
        assert result.returncode == 1 and not (root / "continued").exists(), result
        assert (root / "scene-failure.env").read_text() == (
            "run=2\nscene=packaged\nfailure_code=reset-failed\n")
    subprocess.run(["python3", str(ROOT / "tests/integration/b6-scene-observation-contract.py")], check=True); print("PASS: B6 requested DPI, fresh window identity and fail-closed scene boundaries")


if __name__ == "__main__":
    main()
