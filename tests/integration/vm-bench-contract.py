#!/usr/bin/env python3
"""vm_bench.py without a VM: a cancelled campaign, a shutdown that never
completes, a relative --out, and the probe line that times the guest's power-off.

    python3 tests/integration/vm-bench-contract.py
"""

from __future__ import annotations

import base64
import contextlib
import io
import os
import shutil
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
                 audio=False, hw_cursor=False, overlap_guard=False, guest_dns_block=False)


def decoded(command):
    return base64.b64decode(command.rsplit(" ", 1)[1]).decode("utf-16-le")


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

    def test_guest_exit_status_controls_which_metrics_reach_the_summary(self):
        engine = NeverOff()
        # A command can print a valid metric before a later guest operation fails.
        engine.run = mock.Mock(side_effect=[
            (9, "BVBENCH disk-small-files-flushed 125.0 files/s"),
            (0, "BVBENCH process-spawn 42.0 proc/s"),
        ])
        with mock.patch.dict(BOOT_ARGS, skip_suite=False, only="disk-small-files-flushed,process-spawn"):
            record = self.boot(engine)
        self.assertEqual(record["bench"], {"process-spawn": 42.0})
        self.assertTrue(any("disk-small-files-flushed: exit=9" in error for error in record["errors"]))
        self.assertEqual(record["bench_exit_codes"], {"disk-small-files-flushed": 9, "process-spawn": 0})

        # Older results stored successful multi-metric replies as "no value" errors.
        # Other error output, including from a failing guest command, cannot supply scores.
        record["errors"] += [
            "old: exit=0 no value: BVBENCH recovered 3.0 Hz",
            "old: exit=9 no value: BVBENCH rejected 123.0 Hz",
            "diagnostic failed\nBVBENCH unrelated 1.0 Hz",
        ]
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "results.json"
            path.write_text(vm_bench.json.dumps({"identity": {"engine": "mock"}, "boots": [record]}))
            scores = vm_bench.summary(str(path))["median"]
        self.assertEqual(scores["process-spawn"], 42.0)
        self.assertEqual(scores["recovered"], 3.0)
        self.assertNotIn("disk-small-files-flushed", scores)
        self.assertNotIn("rejected", scores)
        self.assertNotIn("unrelated", scores)

    def test_failed_display_workload_does_not_sample_unrelated_host_frames(self):
        # The guest may fail before the three-second delay to start frame sampling.
        engine = mock.Mock(spec=vm_bench.BridgeVM)
        engine.run.return_value = (2, "BVBENCH display-present 60.0 Hz")
        with tempfile.TemporaryDirectory() as tmp:
            engine.display_fb = Path(tmp) / "display.fb"
            engine.display_fb.touch()
            record = {"errors": [], "bench": {}}
            with mock.patch.object(vm_bench, "consume_display") as consume:
                vm_bench.display_test(engine, record)
        consume.assert_not_called()
        self.assertEqual(record["bench"], {})
        self.assertIn("display: exit=2", record["errors"][0])

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

    def test_a_guard_with_no_answer_ends_the_boot_invalid_through_its_shutdown(self):
        # A late guard reply would stay in QEMU's agent stream and become the next command's output.
        for answers, window in (([TimeoutError("guard")], "the settle window"),
                                (["BVGUARD pids=7", TimeoutError("guard")], "idle")):
            with self.subTest(window=window):
                engine, replies, sent = NeverOff(), iter(answers), []
                engine.request_shutdown = mock.Mock()

                def run(command, deadline):
                    sent.append(command)
                    reply = next(replies)
                    if isinstance(reply, Exception):
                        raise reply
                    return 0, reply
                engine.run = run
                with mock.patch.dict(BOOT_ARGS, overlap_guard=True, skip_suite=False, only="process-spawn"):
                    record = self.boot(engine)
                self.assertEqual(len(sent), len(answers), "a guest command followed the unanswered guard")
                engine.request_shutdown.assert_called_once()
                self.assertEqual(record["invalid"], f"the overlap guard gave no answer before {window}")
                self.assertEqual(record["errors"][0], f"overlap guard before {window}: no answer")
                self.assertNotIn("boot: ", " ".join(record["errors"]))

    def test_the_guest_dns_block_marks_a_boot_whose_guest_keeps_a_dns_server_invalid(self):
        net = "BVNET rx=10 tx=5"
        for dns, invalid in (("BVDNS adapters=1 5/2=127.0.0.1 5/23=::1", None),
                             ("BVDNS adapters=1 5/2=127.0.0.1 5/23=fec0:0:0:ffff::1", "the guest DNS block did not apply")):
            with self.subTest(dns=dns), mock.patch.dict(BOOT_ARGS, guest_dns_block=True):
                engine = NeverOff()
                engine.run = lambda command, deadline, dns=dns: (0, dns if "BVDNS" in decoded(command) else net)
                record = self.boot(engine)
                self.assertEqual(record.get("invalid"), invalid)
                self.assertEqual(record["guest_net_bytes"], {w: {"rx": 10, "tx": 5} for w in ("ready", "before", "after")})
        self.assertEqual(vm_bench.nat_stats(b"x\nvirtio-net NAT stats: guest_frames=7 dns_queries=0\n"),
                         {"guest_frames": "7", "dns_queries": "0"})

    def test_a_dns_block_with_no_answer_ends_the_boot_invalid_through_its_shutdown(self):
        engine, sent = NeverOff(), []
        engine.request_shutdown = mock.Mock()

        def run(command, deadline):
            sent.append(command)
            raise TimeoutError("dns")
        engine.run = run
        with mock.patch.dict(BOOT_ARGS, guest_dns_block=True, skip_suite=False, only="process-spawn"):
            record = self.boot(engine)
        self.assertEqual(len(sent), 1, "a guest command followed the unanswered DNS block")
        engine.request_shutdown.assert_called_once()
        self.assertEqual(record["invalid"], "the guest DNS block gave no answer")
        self.assertNotIn("boot: ", " ".join(record["errors"]))

    def test_a_guard_invalid_boot_is_left_out_of_the_comparison_and_named(self):
        def results(boots):
            path = Path(self.tmp.name) / f"r{len(os.listdir(self.tmp.name))}.json"
            path.write_text(vm_bench.json.dumps({"identity": {"engine": "bridgevm"}, "boots": boots}))
            return str(path)
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        boot = lambda n, cpu, **extra: dict(boot=n, errors=[], bench={}, idle_cpu_pct=cpu, **extra)  # noqa: E731
        mixed = vm_bench.summary(results([boot(1, 1.0), boot(2, 99.0, invalid="a bench process was still running "
                                                                "when idle opened"), boot(3, 5.0)]))
        self.assertEqual(mixed["median"]["idle_cpu_pct"], 5.0)
        self.assertEqual(mixed["invalid"], ["boot 2: a bench process was still running when idle opened"])
        only_invalid = results([boot(1, 1.0), boot(2, 99.0, invalid="x")])
        self.assertEqual(vm_bench.summary(only_invalid)["median"], {})
        printed = io.StringIO()
        with contextlib.redirect_stdout(printed):
            vm_bench.compare(only_invalid, results([boot(1, 1.0), boot(2, 4.0)]))
        self.assertIn("bridgevm: left out, boot 2: x", printed.getvalue())
        self.assertIn("bridgevm: no valid measured boot", printed.getvalue())
        self.assertNotIn("99.0", printed.getvalue())

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

    def test_a_packaged_probe_runs_from_a_private_copy_of_its_bundle(self):
        with tempfile.TemporaryDirectory() as scratch, contextlib.redirect_stdout(io.StringIO()):
            root = Path(scratch).resolve()
            app = root / "BridgeVM.app"
            probe_path = Path("Contents/Resources/target/release/examples/hvf_gic_boot_probe")
            library_path = Path("Contents/Frameworks/libvirglrenderer.1.dylib")
            probe = app / probe_path
            probe.parent.mkdir(parents=True)
            probe.write_bytes(b"signed probe")
            (app / "Contents/Info.plist").write_text("example app")
            (app / library_path).parent.mkdir()
            (app / library_path).write_bytes(b"signed library")
            out = root / "run"
            staged_app = out / "bin/BridgeVM.app"
            args = types.SimpleNamespace(engine="bridgevm", out=out, probe=probe, disk=root / "disk.raw",
                                         vars=root / "vars.fd", boots=1, net=False, keep_media=False)

            def copy_bundle(command, **_):
                self.assertEqual(command, ["/usr/bin/ditto", str(app), str(staged_app)])
                shutil.copytree(command[1], command[2], symlinks=True)

            with mock.patch.object(vm_bench, "identity", return_value={}), \
                    mock.patch.object(vm_bench, "clone"), \
                    mock.patch.object(vm_bench.subprocess, "run", side_effect=copy_bundle) as copy_command, \
                    mock.patch.object(vm_bench, "one_boot", return_value={"boot": 1}):
                vm_bench.run(args)
            copy_command.assert_called_once()
            self.assertEqual(Path(args.probe), staged_app / probe_path)
            self.assertEqual(Path(args.probe).read_bytes(), probe.read_bytes())
            self.assertEqual((staged_app / library_path).read_bytes(), (app / library_path).read_bytes())
            self.assertEqual((out / "bin").stat().st_mode & 0o777, 0o700)

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

    def test_guest_lines_read_without_the_probes_prefix_and_a_printed_end_ends_nothing(self):
        with tempfile.TemporaryDirectory() as scratch:
            engine = vm_bench.BridgeVM(None, Path("disk.raw"), Path("vars.fd"))
            engine.log, engine.offset, engine.pending = Path(scratch) / "run.log", 0, ""
            engine.ctl, engine.input_ctl = Path(scratch) / "agent.ctl", Path(scratch) / "input.ctl"
            engine.proc = mock.Mock(**{"poll.return_value": None})
            engine.log.write_text("BVAGENT CMD probe exit=0\n| BVBENCH x 1 MB/s\r\n| BVAGENT END probe\n| \nBVAGENT END probe\n")
            deadline = vm_bench.time.monotonic() + 5
            self.assertEqual(engine.run("probe", deadline), (0, "BVBENCH x 1 MB/s\r\nBVAGENT END probe\n"))
            with engine.log.open("a") as log:
                log.write("BVAGENT CMD DISPLAYMODE 1920 1080 exit=0\n| BVDISPLAY target=1920x1080 result=0\r\n| \n"
                          "BVAGENT END DISPLAYMODE 1920 1080\n")
            self.assertEqual(engine.resize("1920x1080", deadline), {"display": "BVDISPLAY target=1920x1080 result=0"})


if __name__ == "__main__":
    unittest.main()
