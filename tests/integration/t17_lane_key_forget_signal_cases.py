"""A stop signal during the T17 lane key forget still removes the recovery package and code.

A queue cancel sends SIGTERM to the tier's process group. Each case blocks the
packaged app in forget-import, after its export wrote the package and code,
signals the tool alone and checks that the tool removed both, killed the app
and exited with 128 plus the signal number.
"""
from __future__ import annotations

import importlib.util
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time

FIXTURE = Path(__file__).with_name("t17-lane-key-forget-test.py")
SPEC = importlib.util.spec_from_file_location("t17_lane_key_forget_fixture", FIXTURE)
assert SPEC and SPEC.loader
base = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(base)


def kill_if_alive(pid: int) -> bool:
    """SIGKILL PID if it still exists, so a failing case leaves no process; return whether it existed."""
    try:
        os.kill(pid, signal.SIGKILL)
    except ProcessLookupError:
        return False
    return True


class LaneKeyForgetSignalCases(base.LaneKeyForgetCase):
    def blocked_app(self, tool: subprocess.Popen, pid_file: Path) -> int:
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline and tool.poll() is None:
            text = pid_file.read_text() if pid_file.exists() else ""
            if text.endswith("\n"):
                return int(text)
            time.sleep(0.02)
        self.fail(f"the app never blocked in forget-import (tool status {tool.poll()})")

    def test_a_stop_signal_during_forget_removes_the_material_and_the_app(self) -> None:
        pid_file = self.temporary / "forget-import.pid"
        exec_fake_app = base.STUB.split("\n", 1)[1]
        self.executable.write_text(f'#!/bin/sh\n[ "$2" != forget-import ] || {{ echo $$ > "{pid_file}"; exec sleep 600; }}\n'
                                   + exec_fake_app)
        self.seal(self.mode)
        for signum in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP):
            with self.subTest(signal=signum.name):
                shutil.rmtree(self.work / "lane-1", ignore_errors=True)
                (self.keychain / "calls.jsonl").unlink(missing_ok=True)
                pid_file.unlink(missing_ok=True)
                request = self.lane(1)
                lane = Path(request["lane_root"])
                tool = subprocess.Popen([sys.executable, str(base.TOOL), "--work", str(self.work), "--verified",
                                         str(self.verified)], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
                self.addCleanup(tool.kill)
                pid = self.blocked_app(tool, pid_file)
                self.assertEqual(sorted(path.name for path in lane.glob("vtpm-key-forget.*/*")),
                                 ["vtpm-recovery-code.txt", "vtpm-recovery.json"])
                tool.send_signal(signum)
                _, stderr = tool.communicate(timeout=30)
                app_left = kill_if_alive(pid)
                material = sorted(str(path.relative_to(lane)) for path in lane.rglob("*")
                                  if path.name.startswith(("vtpm-recovery", "vtpm-key-forget.")))
                self.assertEqual((tool.returncode, material, app_left), (128 + signum, [], False), stderr)
                self.assertEqual([call[1] for call in self.calls()], ["export"])
                self.assertTrue(self.key(request).exists(), "an interrupted forget removed the key")
                self.assertNotIn(str(self.temporary), stderr)

    def test_the_first_stop_signal_ignores_repeats_so_its_cleanup_finishes(self) -> None:
        sys.path.insert(0, str(base.TOOL.parent))
        import stop_signals
        saved = {signum: signal.getsignal(signum) for signum in stop_signals.STOP_SIGNALS}
        self.addCleanup(lambda: [signal.signal(signum, handler) for signum, handler in saved.items()])
        self.assertEqual(stop_signals.stoppable(lambda: 7), 7)
        self.assertEqual({signal.getsignal(signum) for signum in saved}, {stop_signals.stop})
        with self.assertRaises(SystemExit) as stopped:
            stop_signals.stop(signal.SIGHUP, None)
        self.assertEqual(stopped.exception.code, 128 + signal.SIGHUP)
        self.assertEqual({signal.getsignal(signum) for signum in saved}, {signal.SIG_IGN})
