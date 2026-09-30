#!/usr/bin/env python3
"""A19's T20 and T22 tiers count a natural shutdown only from the final host report.

natural_shutdown_count, and boots_passed with it, counts a phase only when its
whole run.log binds the final report's SYSTEM_OFF stop. A PSCI SYSTEM_RESET
stop is not a shutdown, and the forged logs of hvf-stop-readers-contract.py
carry the record as guest text. Each tier runs to its receipt with a failing
lifecycle and its other inputs stubbed, so the counts come from its own call.
"""

from __future__ import annotations

import contextlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
LIVE_GATES = ROOT / "scripts/live-gates"
sys.path.insert(0, str(LIVE_GATES))
COMMIT = subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True).strip()
SEALED = {"input_manifest_sha256": "b" * 64, "binary_hash": "c" * 64}


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


READERS = load("hvf_stop_readers_contract", ROOT / "tests/integration/hvf-stop-readers-contract.py")
GENUINE = READERS.GENUINE.encode()
NOT_SHUTDOWN = {name: text.encode() for name, text in {
    **READERS.FORGED_STOP, "PSCI SYSTEM_RESET stop": READERS.AGENT + READERS.report(
        "stop: PSCI 0x84000009 (system reset)")}.items()}
TIERS = {"T20": ("run-native-snapshot-restore-tier.py", ("phase1-original", "phase3-clobber", "phase5-restored")),
         "T22": ("run-a19-interrupted-restore-tier.py",
                 ("phase1-original", "phase3-clobber", "phase5-postkill", "phase7-restored"))}


class ShutdownCountContract(unittest.TestCase):
    def counts(self, tier: str, logs: list[bytes | str | None]) -> tuple[int, int, int]:
        """(boots_attempted, boots_passed, natural_shutdown_count) the tier's receipt records.

        Each entry is a phase's run.log: bytes, "missing" for a phase directory
        without one, "linked" for a link to a genuine log, None for no phase.
        """
        script, phases = TIERS[tier]
        self.assertEqual(len(logs), len(phases))
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "job"
            output.mkdir()
            (output.parent / "genuine.log").write_bytes(GENUINE)
            for phase, log in zip(phases, logs):
                if log is not None:
                    (output / phase).mkdir()
                if log == "linked":
                    (output / phase / "run.log").symlink_to(output.parent / "genuine.log")
                elif isinstance(log, bytes):
                    (output / phase / "run.log").write_bytes(log)
            runner = load(f"shutdown_count_{tier}", LIVE_GATES / script)

            def prepare(*_args):
                (output / "prepared-inputs").mkdir()
                for name in ("disk.raw", "vars.fd"):
                    (output / "prepared-inputs" / name).write_bytes(b"x")
                return dict(SEALED), {"binary": "binary", "app_cli": "cli", "sealed_app": temporary}

            def identity(command, **_kwargs):
                return COMMIT if command[0] == "git" else "Mac17,9"
            with (mock.patch.object(runner.sys, "argv", ["tier", str(output), "job", "manifest", "binary"]),
                  mock.patch.object(runner.subprocess, "check_output", side_effect=identity),
                  mock.patch.object(runner.subprocess, "run", return_value=SimpleNamespace(returncode=1)),
                  mock.patch.object(runner.platform, "mac_ver", return_value=("26.0", (), "")),
                  mock.patch.object(runner, "sealed_hashes", return_value=dict(SEALED)),
                  mock.patch.object(runner, "prepare", side_effect=prepare),
                  mock.patch.object(runner, "digest", return_value="d" * 64),
                  mock.patch.object(runner, "reauthenticate"),
                  mock.patch.object(runner, "regular", create=True),
                  mock.patch.object(runner, "run_lifecycle", create=True, return_value=1),
                  contextlib.redirect_stderr(io.StringIO())):
                self.assertEqual(runner.main(), 1)
            receipt = json.loads((output / "receipt.json").read_text(encoding="utf-8"))
        return receipt["boots_attempted"], receipt["boots_passed"], receipt["natural_shutdown_count"]

    def test_genuine_system_off_counts_every_phase(self):
        for tier, (_script, phases) in TIERS.items():
            with self.subTest(tier):
                self.assertEqual(self.counts(tier, [GENUINE] * len(phases)), (len(phases),) * 3)

    def test_guest_text_and_system_reset_are_not_shutdowns(self):
        for tier, (_script, phases) in TIERS.items():
            for name, log in NOT_SHUTDOWN.items():
                with self.subTest(tier=tier, log=name):
                    self.assertEqual(self.counts(tier, [log] * len(phases)), (len(phases), 0, 0))

    def test_each_phase_counts_on_its_own_log(self):
        reset = NOT_SHUTDOWN["PSCI SYSTEM_RESET stop"]
        for tier, logs, expected in (("T20", [GENUINE, reset, GENUINE], (3, 2, 2)),
                                     ("T22", [GENUINE, GENUINE, GENUINE, reset], (4, 3, 3)),
                                     ("T20", [GENUINE, "missing", None], (2, 1, 1)),
                                     ("T22", [GENUINE, "linked", "missing", None], (3, 1, 1))):
            with self.subTest(tier=tier, expected=expected):
                self.assertEqual(self.counts(tier, logs), expected)


if __name__ == "__main__":
    unittest.main(verbosity=2)
