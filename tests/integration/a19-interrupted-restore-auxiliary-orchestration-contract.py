#!/usr/bin/env python3
"""Helper-only auxiliary orchestration without guest or private media execution."""
from pathlib import Path
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import a19_interrupt_auxiliary as auxiliary
import a19_interrupt_auxiliary_io as io
import a19_interrupt_restore_child as observer
import a19_interrupt_stop_points as points
from a19_seed_source_binding_cases import AuxiliarySeedBinding


class AuxiliaryWiring(unittest.TestCase):
    def test_auxiliary_dispatch_follows_the_unchanged_four_guest_boots(self):
        source = (ROOT / "scripts/verify-native-snapshot-interrupted-restore.sh").read_text()
        self.assertEqual(source.count("boot_and_mark "), 4)
        self.assertIn("a19_interrupt_auxiliary.py", source)
        self.assertGreater(source.index("a19_interrupt_auxiliary.py"), source.index('> "$OUT/final-vars.sha256"'))
        self.assertIn('"$LOGICAL_DISK" "$LOGICAL_VARS"', source)


def fixture(root: Path, fault: str = ""):
    output = root / "out"
    (output / "live").mkdir(parents=True)
    disk, variables, snapshot = root / "phase3.raw", root / "phase3.fd", root / "original.snapshot"
    disk.write_bytes(b"clobbered guest marker")
    variables.write_bytes(b"clobbered vars")
    snapshot.mkdir()
    (snapshot / "disk.raw").write_bytes(b"original guest marker")
    (snapshot / "vars.fd").write_bytes(b"original vars")
    value = {"format_version": 1, "vm_id": "a19-native-cli-live", **io.pair(snapshot / "disk.raw", snapshot / "vars.fd")}
    (snapshot / "manifest.json").write_text(json.dumps(value))
    helper = root / "helper.py"
    fixture_path = ROOT / "tests/fixtures/a19_auxiliary_helper.py"
    helper.write_text(f"#!/usr/bin/env python3\nimport os,runpy\nos.environ['A19_FIXTURE_CONFIG']={str(helper.with_suffix('.json'))!r}\nrunpy.run_path({str(fixture_path)!r},run_name='__main__')\n")
    helper.chmod(0o700)
    helper.with_suffix(".json").write_text(json.dumps({"calls": str(root / "calls.json"), "fault": fault}))
    return helper, snapshot, disk, variables, output


class AuxiliaryExecution(AuxiliarySeedBinding, unittest.TestCase):
    def setUp(self):
        if not Path(observer.LSOF).is_file():
            if sys.platform == "darwin":
                self.fail("macOS auxiliary test requires owner-readable lsof")
            self.skipTest("staged-FD orchestration is exercised on hosted macOS")

    def run_case(self, fault=""):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        args = fixture(Path(temporary.name), fault)
        before = [path.read_bytes() for path in (*args[1:4],) if path.is_file()]
        return args, before

    def test_both_cases_stop_retry_authenticate_and_cleanup_isolated_clones(self):
        args, before = self.run_case()
        auxiliary.run(*args, deadline=5, budget=60, clone=shutil.copyfile)
        helper, snapshot, disk, variables, output = args
        self.assertEqual([disk.read_bytes(), variables.read_bytes()], before)
        self.assertFalse((output / "live/auxiliary").exists())
        logs = []
        for name, point in (("swap", points.SWAP_RESTORE), ("create", points.CREATE_EXPORT)):
            retained = output / f"aux-{name}"
            proof = json.loads((retained / "interrupt-observation.json").read_text())
            context = json.loads((retained / "interrupt-helper-context.private.json").read_text())
            raw = (retained / "interrupt-helper-fd.private.log").read_text()
            self.assertEqual(proof["interruption_stage"], point.name)
            self.assertTrue(observer.read_fd_observed(raw, context["helper_pid"], Path(context["staged_disk_path"])))
            logs.append(proof["stop_fd_log_sha256"])
            labels = ("preinterrupt", "postkill", "postretry") if name == "swap" else ("source", "source-postkill", "source-postretry", "postretry")
            for label in labels:
                self.assertEqual((retained / f"{label}-digest.txt").read_bytes(), (retained / f"{label}-host-digest.txt").read_bytes())
            self.assertEqual((retained / "interrupt-helper-context.private.json").stat().st_mode & 0o777, 0o600)
        self.assertEqual(len(set(logs)), 2)
        calls = json.loads((helper.parent / "calls.json").read_text())
        self.assertEqual(sum(call[0] == "restore" for call in calls), 3)
        self.assertEqual(sum(call[0] == "create" for call in calls), 3)
        self.assertEqual(io.manifest(snapshot)["disk_sha256"], hashlib.sha256(b"original guest marker").hexdigest())

    def test_wrong_selection_noop_retry_and_changed_create_source_refuse_and_cleanup(self):
        for fault in ("digest-lie", "swap-noop-retry", "create-source-mutated", "retry-failed", "create-published"):
            with self.subTest(fault=fault):
                args, before = self.run_case(fault)
                with self.assertRaises((ValueError, RuntimeError)):
                    auxiliary.run(*args, deadline=1, budget=30, clone=shutil.copyfile)
                self.assertFalse((args[-1] / "live/auxiliary").exists())
                self.assertEqual([args[2].read_bytes(), args[3].read_bytes()], before)
                pid_path = args[0].parent / "held.pid"
                if pid_path.exists():
                    with self.assertRaises(ProcessLookupError):
                        os.kill(int(pid_path.read_text()), 0)


class TerminalOwnership(unittest.TestCase):
    def test_terminal_wait_status_never_signals_reaped_pid(self):
        stage = Path("/private/owned/staging")
        for status, expected in ((0, 0), (9, -9)):
            with self.subTest(status=status):
                child = mock.Mock(pid=12345, returncode=None)
                child.poll.return_value = None
                observed = mock.Mock(returncode=0, stdout=f"p12345\nf3\nar\nn{stage}/disk.raw\n")
                with mock.patch.object(observer, "staged_ready", return_value=True), \
                        mock.patch.object(observer.subprocess, "run", return_value=observed), \
                        mock.patch.object(observer.os, "waitpid", return_value=(12345, status)), \
                        mock.patch.object(observer.os, "kill") as killed:
                    with self.assertRaisesRegex(RuntimeError, "stop was not observed"):
                        observer.stop_at_verified_stage(child, points.SWAP_RESTORE, stage, lambda: True,
                                                       observer.time.monotonic() + 1, Path("/unused"))
                self.assertEqual(child.returncode, expected)
                self.assertEqual([call.args[1] for call in killed.call_args_list], [observer.signal.SIGSTOP])

    def test_lost_wait_ownership_never_signals_or_invents_exit_status(self):
        stage = Path("/private/owned/staging")
        child = mock.Mock(pid=12345, returncode=None, _a19_reaped=False)
        child.poll.return_value = None
        observed = mock.Mock(returncode=0, stdout=f"p12345\nf3\nar\nn{stage}/disk.raw\n")
        with mock.patch.object(observer, "staged_ready", return_value=True), \
                mock.patch.object(observer.subprocess, "run", return_value=observed), \
                mock.patch.object(observer.os, "waitpid", side_effect=ChildProcessError), \
                mock.patch.object(observer.os, "kill") as killed:
            with self.assertRaisesRegex(RuntimeError, "ownership was lost"):
                observer.stop_at_verified_stage(child, points.SWAP_RESTORE, stage, lambda: True,
                                               observer.time.monotonic() + 1, Path("/unused"))
        self.assertIsNone(child.returncode)
        self.assertEqual([call.args[1] for call in killed.call_args_list], [observer.signal.SIGSTOP])


class AuxiliarySafety(unittest.TestCase):
    def test_reaped_session_leader_never_receives_group_signals(self):
        child = mock.Mock(pid=12345, returncode=0)
        signals = []
        def kill_group(pid, sig):
            if sig == 0:
                raise ProcessLookupError
            signals.append((pid, sig))
        with mock.patch.object(io.os, "killpg", side_effect=kill_group):
            io.stop_group(child)
        self.assertEqual(signals, [])
        child.wait.assert_not_called()

    def test_reaped_leader_residual_or_denied_group_fences_without_signaling(self):
        for denied in (False, True):
            with self.subTest(denied=denied):
                child = mock.Mock(pid=12345, returncode=0)
                calls = []
                def kill_group(pid, sig):
                    calls.append(sig)
                    if denied:
                        raise PermissionError
                with mock.patch.object(io.os, "killpg", side_effect=kill_group):
                    with self.assertRaises(io.CleanupUncertain):
                        io.stop_group(child)
                self.assertEqual(calls, [0])
                child.wait.assert_not_called()

    def test_unreaped_denied_group_is_reaped_and_requires_verified_absence(self):
        child = mock.Mock(pid=12345, returncode=None)
        order = []
        def kill_group(pid, sig):
            order.append(("signal", sig, child.returncode))
            if child.returncode is None:
                raise PermissionError
            self.assertEqual(sig, 0)
            raise ProcessLookupError
        def reap(**_kwargs):
            order.append(("reap",))
            child.returncode = 0
            return 0
        child.wait.side_effect = reap
        with mock.patch.object(io.os, "killpg", side_effect=kill_group):
            io.stop_group(child)
        self.assertEqual(order[-1], ("signal", 0, 0))
        self.assertEqual([item[1] for item in order if item[0] == "signal" and item[1] != 0],
                         [observer.signal.SIGCONT, observer.signal.SIGTERM, observer.signal.SIGKILL])
        self.assertTrue(all(item[2] is None for item in order if item[0] == "signal" and item[1] != 0))

    def test_missing_or_tampered_stop_context_cannot_admit_retry(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            staged = root / "owned/.export.snapshot.staging/disk.raw"
            raw = f"p12345\nf3\nar\nn{staged}\n".encode()
            (root / "interrupt-helper-fd.private.log").write_bytes(raw)
            proof = {"interruption_stage": points.CREATE_EXPORT.name, "helper_stop_verified": True,
                     "staged_file_sync_order_verified": True, "old_selection_before_kill": True,
                     "helper_killed_and_reaped": True, "stop_fd_log_sha256": hashlib.sha256(raw).hexdigest()}
            (root / "interrupt-observation.json").write_text(json.dumps(proof))
            with self.assertRaises(FileNotFoundError):
                io.stop_proof(root, points.CREATE_EXPORT, staged)
            context = {"helper_pid": 12345, "staged_disk_path": str(staged),
                       "selection_before": "absent", "selection_after": "absent"}
            path = root / "interrupt-helper-context.private.json"
            path.write_text(json.dumps(context))
            io.stop_proof(root, points.CREATE_EXPORT, staged)
            for change in ({"helper_pid": True}, {"helper_pid": 12346}, {"staged_disk_path": "/wrong"},
                           {"selection_after": "published"}, {"selection_before": "existing", "selection_after": "existing"}):
                with self.subTest(change=change):
                    path.write_text(json.dumps({**context, **change}))
                    with self.assertRaises(ValueError):
                        io.stop_proof(root, points.CREATE_EXPORT, staged)

    def test_cleanup_uncertainty_preserves_owned_media_for_the_live_fence(self):
        with tempfile.TemporaryDirectory() as temporary:
            args = fixture(Path(temporary))
            with mock.patch.object(io, "stop_group", side_effect=io.CleanupUncertain("injected cleanup uncertainty")):
                with self.assertRaises(io.CleanupUncertain):
                    auxiliary.run(*args, deadline=1, budget=30, clone=shutil.copyfile)
            self.assertTrue((args[-1] / "live/auxiliary/swap/disk.raw").is_file())
            self.assertTrue((args[-1] / "live/auxiliary/cleanup-unproven").is_file())
            source = (ROOT / "scripts/verify-native-snapshot-interrupted-restore.sh").read_text()
            self.assertIn('[[ ! -e "$WORK/auxiliary" && ! -L "$WORK/auxiliary" ]] || return 1', source)

    def test_constructor_interruption_preserves_media_without_guessing_child_ownership(self):
        with tempfile.TemporaryDirectory() as temporary:
            args = fixture(Path(temporary))
            tracked, owned = [], []
            original_popen = subprocess.Popen
            class Tracked(io.Commands):
                def __init__(self, *values, **options):
                    super().__init__(*values, **options)
                    tracked.append(self)
            def interrupted_constructor(*_values, **_options):
                owned.append(original_popen(["/bin/sleep", "30"], start_new_session=True))
                raise InterruptedError("fixture interruption after spawn before ownership assignment")
            try:
                with mock.patch.object(auxiliary, "Commands", Tracked), \
                        mock.patch.object(io.subprocess, "Popen", side_effect=interrupted_constructor), \
                        mock.patch.object(io, "stop_group", wraps=io.stop_group) as cleanup:
                    with self.assertRaises(InterruptedError):
                        auxiliary.run(*args, deadline=1, budget=30, clone=shutil.copyfile)
                    cleanup.assert_not_called()
                self.assertFalse(tracked[0].cleanup_verified)
                self.assertTrue((args[-1] / "live/auxiliary/swap/disk.raw").is_file())
                self.assertTrue((args[-1] / "live/auxiliary/cleanup-unproven").is_file())
                self.assertIsNone(owned[0].returncode)
            finally:
                # Only this fixture retains the constructor's original child handle.
                for child in owned:
                    io.stop_group(child)

    def test_timeout_kills_and_reaps_an_owned_command_tree(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            script = root / "tree.py"
            pid_path = root / "descendant.pid"
            script.write_text("import pathlib,signal,subprocess,sys,time\n"
                              "child=subprocess.Popen(['/bin/sleep','30'])\n"
                              f"pathlib.Path({str(pid_path)!r}).write_text(str(child.pid))\n"
                              "def stop(signum,frame):\n"
                              "    child.terminate();child.wait(timeout=1);raise SystemExit(0)\n"
                              "signal.signal(signal.SIGTERM,stop)\n"
                              "time.sleep(30)\n")
            commands = io.Commands(5, command_timeout=0.5)
            with self.assertRaises(subprocess.TimeoutExpired):
                commands.run([sys.executable, str(script)], root / "tree.stdout")
            self.assertTrue(commands.cleanup_verified)
            with self.assertRaises(ProcessLookupError):
                os.kill(int(pid_path.read_text()), 0)

    def test_term_unwinds_and_cleans_the_active_owned_command_tree(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            tree, driver = root / "tree.py", root / "driver.py"
            descendant, group, result = root / "descendant.pid", root / "group.pid", root / "cleanup.json"
            tree.write_text("import os,pathlib,signal,subprocess,time\n"
                            "child=subprocess.Popen(['/bin/sleep','30'])\n"
                            f"pathlib.Path({str(descendant)!r}).write_text(str(child.pid))\n"
                            f"pathlib.Path({str(group)!r}).write_text(str(os.getpgrp()))\n"
                            "def stop(signum,frame):\n"
                            "    child.terminate();child.wait(timeout=1);raise SystemExit(0)\n"
                            "signal.signal(signal.SIGTERM,stop)\n"
                            "time.sleep(30)\n")
            driver.write_text("import json,pathlib,signal,sys\n"
                              f"sys.path.insert(0,{str(ROOT / 'scripts/live-gates')!r})\n"
                              "from a19_interrupt_auxiliary import interrupted\n"
                              "from a19_interrupt_auxiliary_io import Commands\n"
                              "signal.signal(signal.SIGTERM,interrupted)\n"
                              "commands=Commands(60)\n"
                              "try:\n"
                              f"    commands.run([sys.executable,{str(tree)!r}],pathlib.Path({str(root / 'tree.stdout')!r}))\n"
                              "except InterruptedError:\n"
                              f"    pathlib.Path({str(result)!r}).write_text(json.dumps(commands.cleanup_verified))\n")
            child = subprocess.Popen([sys.executable, str(driver)])
            try:
                until = observer.time.monotonic() + 3
                while not group.exists() and observer.time.monotonic() < until:
                    observer.time.sleep(0.02)
                self.assertTrue(group.exists(), "fixture command tree never started")
                child.terminate()
                self.assertEqual(child.wait(timeout=5), 0)
                self.assertIs(json.loads(result.read_text()), True)
                with self.assertRaises(ProcessLookupError):
                    os.kill(int(descendant.read_text()), 0)
            finally:
                if child.poll() is None:
                    child.kill()
                    child.wait(timeout=2)
                if group.exists():
                    try:
                        os.killpg(int(group.read_text()), observer.signal.SIGKILL)
                    except ProcessLookupError:
                        pass


if __name__ == "__main__":
    unittest.main()
