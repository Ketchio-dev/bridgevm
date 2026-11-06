#!/usr/bin/env python3
"""vm_bench.py without a VM: a cancelled campaign, a shutdown that never
completes, a relative --out, and the probe line that times the guest's power-off.

    python3 tests/integration/vm-bench-contract.py
"""

from __future__ import annotations

import contextlib
import io
import os
import signal
import sys
import tempfile
import types
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/perf"))
import vm_bench  # noqa: E402

GUEST_OFF = "guest powered off; host teardown follows"
BOOT_ARGS = dict(ready_timeout=600, settle_s=0, idle_s=0, pointer=False, desktop_mode=None, display=False,
                 shutdown_diag=False, stall_idle=False, stall=False, net=False, only=None, skip_suite=True,
                 audio=False, hw_cursor=False, overlap_guard=False)


class Clock:
    """time.monotonic, advanced only by time.sleep."""

    def __init__(self):
        self.now = 1000.0

    def monotonic(self):
        return self.now

    def sleep(self, seconds):
        self.now += seconds


class NeverOff(vm_bench.Engine):
    """A VM that is ready at once and never powers off; `fail` interrupts the boot wait."""

    vmm_pid = None

    def __init__(self, fail=None):
        self.fail = fail
        self.proc = mock.Mock(pid=4242, **{"poll.return_value": None})

    def start(self, boot_dir):
        pass

    def wait_ready(self, deadline):
        if self.fail is not None:
            raise self.fail
        return {"ready_wall_ms": 1}

    def request_shutdown(self):
        pass

    def vmm_alive(self):
        return True

    def stop(self, deadline):
        return False


class VmBenchContract(unittest.TestCase):
    def boot(self, engine):
        clock = Clock()
        with mock.patch.object(vm_bench.time, "monotonic", clock.monotonic), \
                mock.patch.object(vm_bench.time, "sleep", clock.sleep), \
                mock.patch.object(vm_bench.os, "killpg") as self.killpg:
            return vm_bench.one_boot(engine, Path("boot-2"), types.SimpleNamespace(**BOOT_ARGS))

    def test_a_shutdown_that_never_completes_is_an_error_and_no_shutdown_sample(self):
        record = self.boot(NeverOff())
        self.assertNotIn("shutdown_ms", record)
        self.assertEqual(record["errors"], ["shutdown: the VMM still ran 240 s after the request"])
        self.assertFalse(record["clean_exit"])

    def test_the_overlap_guard_marks_a_boot_whose_process_outlives_its_window_invalid(self):
        engine = NeverOff()
        answers = iter(["BVGUARD pids=7", "BVGUARD pids=7,9"])  # after READY, then as the idle window opens
        engine.run = lambda command, deadline: (0, next(answers))
        with mock.patch.dict(BOOT_ARGS, overlap_guard=True):
            record = self.boot(engine)
        self.assertEqual(record["invalid"], "a bench process was still running when idle opened")
        self.assertIn("overlap guard before idle: processes [9] still running", record["errors"])

    def test_a_guard_that_times_out_is_an_error_and_the_boot_goes_on_to_its_shutdown(self):
        for answers in ([TimeoutError("guard")], ["BVGUARD pids=7", TimeoutError("guard")]):
            with self.subTest(timeouts_after=len(answers) - 1):
                engine, replies = NeverOff(), iter(answers)
                engine.request_shutdown = mock.Mock()

                def run(command, deadline):
                    reply = next(replies)
                    if isinstance(reply, Exception):
                        raise reply
                    return 0, reply
                engine.run = run
                with mock.patch.dict(BOOT_ARGS, overlap_guard=True):
                    record = self.boot(engine)
                engine.request_shutdown.assert_called_once()
                self.assertNotIn("invalid", record)
                self.assertEqual(record["errors"][0], "overlap guard after READY: no answer; no guard this boot"
                                 if len(answers) == 1 else "overlap guard before idle: no answer")

    def test_sigterm_or_ctrl_c_during_a_boot_kills_the_vm_and_ends_the_campaign(self):
        for error in (SystemExit(143), KeyboardInterrupt()):
            with self.subTest(error=type(error).__name__), self.assertRaises(type(error)):
                self.boot(NeverOff(fail=error))
            self.killpg.assert_called_once_with(4242, signal.SIGKILL)

    def test_a_relative_out_reaches_the_runner_as_absolute_paths(self):
        launched = {}

        def one_boot(engine, boot_dir, _args):
            with mock.patch.object(vm_bench.subprocess, "Popen") as popen:
                engine.start(boot_dir)
            launched.update(popen.call_args.kwargs, cmd=popen.call_args.args[0])
            return {"boot": 1}

        args = types.SimpleNamespace(engine="bridgevm", out="campaign", probe="probe", disk="disk.raw",
                                     vars="vars.fd", boots=1, net=False, keep_media=False, cpus=4, ram_mib=6144,
                                     boot_timer_ms=60000, no_xhci=False, display=False, audio=False,
                                     guest_date="2026-08-01T00:00:00", bvm_env=[])
        caller = os.getcwd()
        with tempfile.TemporaryDirectory() as scratch, contextlib.redirect_stdout(io.StringIO()):
            os.chdir(scratch)
            try:
                Path("probe").write_bytes(b"probe")
                with mock.patch.object(vm_bench, "identity", return_value={}), \
                        mock.patch.object(vm_bench, "clone"), \
                        mock.patch.object(vm_bench.subprocess, "run"), \
                        mock.patch.object(vm_bench, "one_boot", one_boot):
                    vm_bench.run(args)
            finally:
                os.chdir(caller)
        self.assertEqual(launched["cwd"], vm_bench.ROOT)
        cmd = launched["cmd"]
        paths = [cmd[cmd.index(flag) + 1] for flag in ("--target", "--vars", "--evidence-dir",
                                                       "--agent-service-control", "--input-control")]
        for path in paths + [launched["env"]["BRIDGEVM_PREBUILT_PROBE"]]:
            self.assertTrue(Path(path).is_absolute(), path)

    def test_an_unknown_only_test_stops_before_any_vm_starts(self):
        argv = ["vm_bench.py", "run", "--engine", "qemu", "--disk", "d", "--vars", "v", "--out", "o",
                "--only", "cpu-sha256-mt,mem-first-tuch"]
        with mock.patch.object(sys, "argv", argv), mock.patch.object(vm_bench, "run") as run, \
                contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            vm_bench.main()
        run.assert_not_called()
        argv[-1] = "cpu-sha256-1t,cpu-sha256-mt,mem-first-touch,mem-copy"
        with mock.patch.object(sys, "argv", argv), mock.patch.object(vm_bench, "run") as run, \
                mock.patch.object(vm_bench.signal, "signal"):
            vm_bench.main()
        self.assertEqual(run.call_args.args[0].only, argv[-1])

    def test_guest_off_is_the_power_off_line_the_probe_prints_before_persisting_media(self):
        runtime = (ROOT / "crates/bridgevm-hvf/examples/hvf_gic_boot_probe/probe_runtime.rs").read_text()
        printed = runtime.find(f'println!("{GUEST_OFF}");')
        self.assertGreater(printed, 0)
        self.assertLess(printed, runtime.index("persist_and_report_stop!("))
        with tempfile.TemporaryDirectory() as scratch:
            engine = vm_bench.BridgeVM(None, Path("disk.raw"), Path("vars.fd"))
            engine.log, engine.offset, engine.pending = Path(scratch) / "run.log", 0, ""
            engine.proc = mock.Mock(**{"poll.return_value": None})
            engine.log.write_text(f"BVAGENT SERVICE alive t=1\n{GUEST_OFF}\n")
            self.assertTrue(engine.guest_off_seen())


if __name__ == "__main__":
    unittest.main()
