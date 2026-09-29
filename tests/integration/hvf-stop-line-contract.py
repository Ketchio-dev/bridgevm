#!/usr/bin/env python3
"""Host stop-record definitions must equal the line the HVF runtime prints.

final_report.rs prints `stop: {stop_reason}`; probe_runtime.rs formats the PSCI
terminal reasons from the hvf_abi.rs function IDs. The line is rebuilt from that
source here and compared with the one Swift and the one Python definition.
"""

from __future__ import annotations

import hashlib
import importlib
import importlib.util
from pathlib import Path
import re
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
PROBE = ROOT / "crates/bridgevm-hvf/examples/hvf_gic_boot_probe"
SWIFT = ROOT / "apps/macos/Sources/BridgeVMProductE2E"
LIVE_GATES = ROOT / "scripts/live-gates"
sys.path.insert(0, str(LIVE_GATES))
from b9_real_workload_receipt import validate_private  # noqa: E402

SPEC = importlib.util.spec_from_file_location(
    "b9_pilot_fixture", ROOT / "tests/integration/b9-real-workload-pilot-contract.py")
assert SPEC is not None and SPEC.loader is not None
PILOT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PILOT)
RETIRED = ("stop: PSCI SYSTEM_OFF", "stop: PSCI SYSTEM_OFF (system off)")


def psci(name: str) -> str:
    values = re.findall(rf"^pub\(crate\) const {name}: u64 = (0x[0-9a-f_]+);$",
                        (PROBE / "hvf_abi.rs").read_text(encoding="utf-8"), re.M)
    assert len(values) == 1, name
    return format(int(values[0].replace("_", ""), 16), "#x")


def printed(name: str) -> list[str]:
    report = (PROBE / "final_report.rs").read_text(encoding="utf-8")
    assert report.count('println!("stop: {}", $stop_reason);') == 1
    runtime = (PROBE / "probe_runtime.rs").read_text(encoding="utf-8")
    templates = re.findall(r'"(PSCI \{' + name + r':#x\}[^"]*)"', runtime)
    assert templates, name
    return ["stop: " + text.replace("{" + name + ":#x}", psci(name)) for text in templates]


class HvfStopLineContract(unittest.TestCase):
    def setUp(self):
        off = set(printed("PSCI_SYSTEM_OFF"))
        self.assertEqual(len(off), 1, off)
        self.off = off.pop()
        self.reset_prefix = f"stop: PSCI {psci('PSCI_SYSTEM_RESET')} "
        self.assertTrue(all(line.startswith(self.reset_prefix)
                            for line in printed("PSCI_SYSTEM_RESET")))

    def test_python_definition_equals_runtime_line(self):
        self.assertEqual(importlib.import_module("hvf_stop_line").SYSTEM_OFF, self.off)

    def test_swift_definitions_equal_runtime_lines(self):
        source = (SWIFT / "HvfStopLine.swift").read_text(encoding="utf-8")
        self.assertEqual(re.findall(r'static let systemOff = "([^"\\]*)"', source), [self.off])
        self.assertEqual(re.findall(r'static let systemResetPrefix = "([^"\\]*)"', source),
                         [self.reset_prefix])

    def test_consumers_use_the_shared_definition(self):
        swift = {path.name: path.read_text(encoding="utf-8") for path in SWIFT.glob("*.swift")}
        python = {path.name: path.read_text(encoding="utf-8") for path in LIVE_GATES.glob("*.py")}
        for name, text in {**swift, **python}.items():
            self.assertFalse("PSCI SYSTEM_OFF" in text, name)
        for name, text in swift.items():
            self.assertFalse(name != "HvfStopLine.swift" and '"stop: PSCI' in text, name)
        for name in ("T17GuestProof.swift", "T17GuestJourney.swift", "T17ProductRunner.swift",
                     "A9ImportProductRunner.swift", "T17FirstBootDiagnostic.swift",
                     "T17InstallTimeoutDiagnostic.swift"):
            self.assertIn("HvfStopLine.systemOff", swift[name], name)
        self.assertIn("HvfStopLine.systemResetPrefix", swift["T17FirstBootDiagnostic.swift"])
        for name in ("windows_product_e2e_guest_evidence.py", "b9_raw_focus_order.py",
                     "run-b9-real-workload-pilot.py"):
            self.assertIn("hvf_stop_line", python[name], name)

    def test_b9_raw_order_requires_the_exact_stop_record(self):
        case = PILOT.B9PilotContract("test_raw_pid_capture_receipt_and_public_no_claim")
        case.setUp()
        self.addCleanup(case.doCleanups)
        diagnostic, receipt = case.fixture()
        log = diagnostic / "raw/guest/run.log"
        original = log.read_bytes()
        self.assertIn(f"\r\n{self.off}\r\n".encode(), original)
        validate_private(receipt, case.job, diagnostic)
        for stop in (*RETIRED, self.off + " extra", "guest " + self.off):
            raw = original.replace(self.off.encode(), stop.encode())
            log.write_bytes(raw)
            receipt["private_artifacts"]["run.log"] = {
                "bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}
            with self.assertRaisesRegex(ValueError, "order differs", msg=stop):
                validate_private(receipt, case.job, diagnostic)


if __name__ == "__main__":
    unittest.main(verbosity=2)
