#!/usr/bin/env python3
"""B9 guest_shutdown_observed needs exit 0 and the exact HVF SYSTEM_OFF record.

The runner's receipt field must be the shared helper applied to the runner's own
run.log split; lookalike, retired and non-final records are rejected.
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
LOOKALIKES = ("stop: PSCI SYSTEM_OFF", "stop: PSCI SYSTEM_OFF (system off)",
              "guest " + SYSTEM_OFF, SYSTEM_OFF + " extra", SYSTEM_OFF + " ", "(system off)",
              "stop: PSCI 0x84000009 exiting for process recreation (exit 42)")


class B9GuestShutdownContract(unittest.TestCase):
    def run_log(self, text: str) -> list[str]:
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "run.log"
            path.write_bytes(text.encode())
            return RUNNER.lines(path)

    def test_zero_exit_and_exact_record_is_observed(self):
        for text in (f"boot\n{SYSTEM_OFF}\n", f"boot\r\n{SYSTEM_OFF}\r\nserial tail\r\n", SYSTEM_OFF):
            self.assertTrue(guest_shutdown_observed(0, self.run_log(text)), repr(text))

    def test_lookalike_records_nonzero_exit_and_unsplit_text_are_rejected(self):
        for stop in LOOKALIKES:
            self.assertFalse(guest_shutdown_observed(0, self.run_log(f"boot\r\n{stop}\r\n")), stop)
        for exit_code in (1, -15, 42, None, False, 0.0, "0"):
            self.assertFalse(guest_shutdown_observed(exit_code, [SYSTEM_OFF]), repr(exit_code))
        self.assertFalse(guest_shutdown_observed(0, []))
        self.assertFalse(guest_shutdown_observed(0, f"boot\n{SYSTEM_OFF}\n"))

    def test_runner_receipt_field_is_the_helper_over_the_run_log(self):
        self.assertIs(RUNNER.guest_shutdown_observed, guest_shutdown_observed)
        tree = ast.parse(RUNNER_PATH.read_text(encoding="utf-8"))
        writes = [ast.unparse(node) for node in ast.walk(tree) if isinstance(node, ast.Assign)
                  and any(isinstance(target, ast.Subscript) and isinstance(target.slice, ast.Constant)
                          and target.slice.value == "guest_shutdown_observed"
                          for target in node.targets)]
        self.assertEqual(writes, ["receipt['guest_shutdown_observed'] = guest_shutdown_observed("
                                  "receipt['guest_shutdown_exit'], lines(boot / 'run.log'))"])
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
