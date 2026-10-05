#!/usr/bin/env python3
"""Mocked phase transitions exercise real D11 orchestration, never a live guest."""
from contextlib import ExitStack
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

from d11_fixture_test_support import COMMIT, HASH
from d11_fixture_runtime import execute
from d11_fixture_commands import boot_command


class Runtime(unittest.TestCase):
    def invoke(self, shutdown_failure=False, detach_failure=False, staged_mutation=False):
        temp = tempfile.TemporaryDirectory(); self.addCleanup(temp.cleanup)
        root = Path(temp.name).resolve(); output = root / "output"; output.mkdir()
        work = output / "mount"
        container = Mock(mount=work, backing=output / "fixture.sparseimage")
        def create():
            work.mkdir(); (work / "source.raw").write_bytes(b"synthetic")
            container.backing.write_bytes(b"synthetic backing")
        container.create.side_effect = create
        if detach_failure: container.cleanup.side_effect = ValueError("synthetic detach refusal")
        processes = Mock(records=[], cleanup_complete=True)
        processes.wait.return_value = 0
        inputs = Mock(data=b"synthetic manifest", rows={"container_gib": ["24"], "source_commit": [COMMIT], "binary": ["unused", HASH]})
        inputs.path.return_value = root
        controller = Mock(); controller.run.return_value = HASH
        probe_seal = Mock()
        def check_probe():
            if staged_mutation and processes.run.call_count >= 1:
                raise ValueError("staged probe changed after install")
        probe_seal.check.side_effect = check_probe
        calls = []
        def shutdown(*args):
            calls.append("shutdown")
            if shutdown_failure and len(calls) == 2: raise ValueError("watchdog is not SYSTEM_OFF")
            return HASH
        with ExitStack() as stack:
            for name, value in (("Processes", processes), ("Container", container), ("FixtureController", controller)):
                stack.enter_context(patch("d11_fixture_runtime." + name, return_value=value))
            stack.enter_context(patch("d11_fixture_runtime.FileSeal", return_value=probe_seal, create=True))
            for name in ("quiet_host", "resource_identity", "stage_probe", "admit"):
                stack.enter_context(patch("d11_fixture_runtime." + name))
            stack.enter_context(patch("d11_fixture_runtime.prepare_commands", return_value=[]))
            stack.enter_context(patch("d11_fixture_runtime.install_command", return_value=["synthetic"]))
            stack.enter_context(patch("d11_fixture_runtime.boot_command", return_value=(["synthetic"], {})))
            stack.enter_context(patch("d11_fixture_runtime.stage_share", return_value=HASH))
            stack.enter_context(patch("d11_fixture_runtime.digest", return_value=HASH))
            stack.enter_context(patch("d11_fixture_runtime.os.ftruncate"))  # no media geometry allocated by this test
            sealing = stack.enter_context(patch("d11_fixture_runtime.seal_pair", return_value={"disk_sha256": HASH, "vars_sha256": HASH}))
            stack.enter_context(patch("d11_fixture_runtime.shutdown_observed", side_effect=shutdown))
            result = execute(root, root, inputs, output, root / "probe")
        return result, processes, controller, sealing

    def test_success_seals_only_after_ready_natural_shutdown_and_cleanup(self):
        result, processes, controller, sealing = self.invoke()
        self.assertTrue(result["installed"]); self.assertTrue(result["ready_stopped"])
        self.assertTrue(result["sealed_fixture"]); self.assertFalse(result["t15_ready"])
        controller.write_command.assert_called_once_with("shutdown.exe /s /t 0")
        sealing.assert_called_once(); self.assertGreaterEqual(processes.finish.call_count, 2)

    def test_watchdog_or_missing_system_off_leaves_partial_fixture(self):
        result, _, _, sealing = self.invoke(shutdown_failure=True)
        self.assertTrue(result["installed"]); self.assertFalse(result["ready_stopped"])
        self.assertFalse(result["sealed_fixture"]); self.assertTrue(result["cleanup_verified"])
        sealing.assert_not_called()

    def test_detach_refusal_keeps_ready_but_never_sealed_or_clean(self):
        result, _, _, _ = self.invoke(detach_failure=True)
        self.assertTrue(result["ready_stopped"])
        self.assertFalse(result["sealed_fixture"]); self.assertFalse(result["cleanup_verified"])
        self.assertEqual(result["failure"], "cleanup-unproved")

    def test_staged_probe_mutation_after_install_prevents_boot_and_sealing(self):
        result, processes, _, sealing = self.invoke(staged_mutation=True)
        self.assertFalse(result["sealed_fixture"])
        processes.launch.assert_not_called()
        sealing.assert_not_called()

    def test_fresh_first_boot_keeps_bounded_setup_reboots(self):
        inputs = Mock(); inputs.path.return_value = Path("/synthetic/firmware")
        argv, _ = boot_command(Path("/synthetic/repo"), inputs, Path("/synthetic/work"), Path("/synthetic/probe"))
        self.assertEqual(argv[argv.index("--max-reboots") + 1], "8")


if __name__ == "__main__": unittest.main()
