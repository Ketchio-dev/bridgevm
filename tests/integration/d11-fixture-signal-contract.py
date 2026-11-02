#!/usr/bin/env python3
"""D11 cancellation escapes subprocess wait OSError fallbacks; no VM or disk image."""
from contextlib import ExitStack
import json
import os
from pathlib import Path
import signal
import tempfile
import unittest
from unittest.mock import Mock, patch

from d11_fixture_test_support import COMMIT, HASH, ROOT, binding, rows
import d11_fixture_runtime as runtime
from d11_fixture_files import document
from d11_fixture_queue import run
from signal_driver_support import swallowing_wait, terminate_when_ready

DRIVER = """import importlib.util, pathlib, subprocess, sys
contract, base = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
sys.path.insert(0, str(contract.parent))
spec = importlib.util.spec_from_file_location("d11_signal", contract)
module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
body = "import os,pathlib,sys,time; pathlib.Path(sys.argv[1]).write_text(str(os.getpid())); time.sleep(30)"
module.execute(base, lambda: subprocess.check_output([sys.executable, "-c", body, str(base / "probe.pid")],
                                                     text=True, timeout=10))
"""


def execute(root, quiet):
    """Real D11 orchestration; every host phase except ``quiet`` is a mock."""
    output = root / "output"; output.mkdir()
    work = output / "mount"
    container = Mock(mount=work, backing=output / "fixture.sparseimage")
    def create():
        work.mkdir(); (work / "source.raw").write_bytes(b"synthetic")
        container.backing.write_bytes(b"synthetic backing")
    container.create.side_effect = create
    processes = Mock(records=[], cleanup_complete=True); processes.wait.return_value = 0
    inputs = Mock(data=b"synthetic manifest", rows={"container_gib": ["24"], "source_commit": [COMMIT],
                                                    "binary": ["unused", HASH]})
    inputs.path.return_value = root
    controller = Mock(); controller.run.return_value = HASH
    with ExitStack() as stack:
        for name, value in (("Processes", processes), ("Container", container), ("FixtureController", controller),
                            ("FileSeal", Mock()), ("prepare_commands", []), ("install_command", ["synthetic"]),
                            ("boot_command", (["synthetic"], {})), ("stage_share", HASH), ("digest", HASH),
                            ("shutdown_observed", HASH),
                            ("seal_pair", {"disk_sha256": HASH, "vars_sha256": HASH})):
            stack.enter_context(patch.object(runtime, name, return_value=value))
        for name in ("resource_identity", "stage_probe", "admit"):
            stack.enter_context(patch.object(runtime, name))
        stack.enter_context(patch.object(runtime.os, "ftruncate"))  # no media geometry allocated
        stack.enter_context(patch.object(runtime, "quiet_host", side_effect=quiet))
        return runtime.execute(root, root, inputs, output, root / "probe"), processes, container


class FixtureSignal(unittest.TestCase):
    def test_host_probe_wait_cannot_swallow_cancellation(self):
        with tempfile.TemporaryDirectory() as temporary:
            result, processes, container = execute(Path(temporary).resolve(), swallowing_wait)
        self.assertEqual((result["failure"], result.get("failure_type")), ("canceled", "FixtureInterrupted"))
        self.assertFalse(result["installed"]); self.assertFalse(result["sealed_fixture"])
        processes.finish.assert_called_once(); container.cleanup.assert_called_once()

    def test_attempt_refuses_cancellation_outside_the_owned_try(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve(); job = root / "job"; job.mkdir()
            with patch(run.__module__ + ".bound", return_value=(binding(), rows(root))), \
                    patch(run.__module__ + ".Inputs"), \
                    patch(run.__module__ + ".create_output", return_value=root / "output"), \
                    patch(run.__module__ + ".execute", side_effect=lambda *_: runtime.interrupted(signal.SIGTERM, None)):
                self.assertEqual(run(job, root, COMMIT, job / "input-manifest.tsv", job / "hvf_gic_boot_probe"), 1)
            self.assertEqual(document(job / "d11-refusal.private.json"), {"reason": "admission-or-integrity-refused"})
            self.assertEqual(document(job / "receipt.json")["reason"], "cleanup-unproved")

    def test_real_term_interrupts_the_piped_host_probe(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary).resolve(); ready = base / "probe.pid"
            (base / "driver.py").write_text(DRIVER)
            contract = ROOT / "tests/integration/d11-fixture-signal-contract.py"
            status = terminate_when_ready([base / "driver.py", contract, base], ready)
            self.assertEqual(status, 0, "TERM did not end the fixture probe within five seconds")
            value = json.loads((base / "output/preparation.private.json").read_text())
            self.assertEqual((value["failure"], value["failure_type"]), ("canceled", "FixtureInterrupted"))
            self.assertFalse(value["sealed_fixture"])
            with self.assertRaises(ProcessLookupError): os.kill(int(ready.read_text()), 0)


if __name__ == "__main__": unittest.main()
