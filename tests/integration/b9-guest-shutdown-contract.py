#!/usr/bin/env python3
"""B9 guest_shutdown_observed needs exit 0 and SYSTEM_OFF as the final report's stop.

The runner's receipt field must be the shared helper applied to the runner's own
run.log path; lookalike, retired, unframed and guest-tail records are rejected.
"""

from __future__ import annotations

import ast
import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
RUNNER_PATH = ROOT / "scripts/live-gates/run-b9-real-workload-pilot.py"
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
from hvf_guest_shutdown import guest_shutdown_observed  # noqa: E402
from hvf_stop_line import SYSTEM_OFF  # noqa: E402

SPEC = importlib.util.spec_from_file_location("b9_shutdown_runner", RUNNER_PATH)
assert SPEC is not None and SPEC.loader is not None
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)
LOOKALIKES = ("stop: PSCI SYSTEM_OFF", "stop: PSCI SYSTEM_OFF (system off)", "guest " + SYSTEM_OFF, SYSTEM_OFF + " extra",
              SYSTEM_OFF + " ", "(system off)", "stop: PSCI 0x84000009 exiting for process recreation (exit 42)")


class B9GuestShutdownContract(unittest.TestCase):
    def observed(self, exit_code: object, stop: str | None, serial: str = "boot\r\n") -> bool:
        text = serial if stop is None else (
            f"REGS: pc=0x0\n=== EDK2 boot probe (with Apple hv_gic) ===\n{stop}\nexits: 1\nserial raw bytes: "
            f"{len(serial)} output bytes: {len(serial)}\n--- serial (tail) ---\n{serial}\n--- end ---\n")
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "run.log"
            path.write_bytes(text.encode())
            return guest_shutdown_observed(exit_code, path)

    def test_only_zero_exit_with_the_final_report_system_off_is_observed(self):
        for serial in ("boot\r\n", f"serial\r\n{SYSTEM_OFF}\r\n"):
            self.assertTrue(self.observed(0, SYSTEM_OFF, serial), repr(serial))
        for stop, serial in ((None, f"boot\n{SYSTEM_OFF}\n"), ("stop: host diagnostic stop requested", f"{SYSTEM_OFF}\r\n"),
                             *((lookalike, "boot\r\n") for lookalike in LOOKALIKES)):
            self.assertFalse(self.observed(0, stop, serial), repr((stop, serial)))
        for exit_code in (1, -15, 42, None, False, 0.0, "0"):
            self.assertFalse(self.observed(exit_code, SYSTEM_OFF), repr(exit_code))
        self.assertFalse(guest_shutdown_observed(0, ROOT / "missing-run.log"))

    def test_runner_receipt_field_is_the_helper_over_the_run_log(self):
        self.assertIs(RUNNER.guest_shutdown_observed, guest_shutdown_observed)
        tree = ast.parse(RUNNER_PATH.read_text(encoding="utf-8"))
        writes = [ast.unparse(node) for node in ast.walk(tree) if isinstance(node, ast.Assign)
                  and any(isinstance(target, ast.Subscript) and isinstance(target.slice, ast.Constant)
                          and target.slice.value == "guest_shutdown_observed"
                          for target in node.targets)]
        self.assertEqual(writes, ["receipt['guest_shutdown_observed'] = guest_shutdown_observed("
                                  "receipt['guest_shutdown_exit'], boot / 'run.log')"])
        keys = [node for node in ast.walk(tree)
                if isinstance(node, ast.Constant) and node.value == "guest_shutdown_observed"]
        guards = [node for node in ast.walk(tree) if isinstance(node, ast.If)
                  and ast.unparse(node.test) == "not receipt['guest_shutdown_observed']"
                  and isinstance(node.body[0], ast.Raise)]
        self.assertEqual((len(keys), len(guards)), (2, 1))
        rebinds = [node for node in ast.walk(tree)
                   if isinstance(node, ast.Name) and node.id == "guest_shutdown_observed"
                   and not isinstance(node.ctx, ast.Load)
                   or isinstance(node, ast.FunctionDef) and node.name == "guest_shutdown_observed"
                   or isinstance(node, ast.arg) and node.arg == "guest_shutdown_observed"]
        self.assertEqual(rebinds, [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
