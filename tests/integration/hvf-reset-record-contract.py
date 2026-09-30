#!/usr/bin/env python3
"""Guest reset detectors must key on the records the HVF runtime prints.

probe_runtime.rs prints `PSCI SYSTEM_RESET: reboot {n}/{max}` for an in-process
reboot and formats the exit-for-recreation stop reason; reboot_watchdog.rs
formats the reboot-limit stop; final_report.rs prints either as `stop: {}`.
Product runs set BRIDGEVM_EXIT_ON_RESET=1 and append every generation to one
run.log. The records are rebuilt from that source and compared with the one
Swift and the one Python definition. No runtime path prints PSCI_SYSTEM_RESET,
and the `max reboots` banner starts every generation, the first included.
"""

from __future__ import annotations

import importlib
from pathlib import Path
import re
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
PROBE = ROOT / "crates/bridgevm-hvf/examples/hvf_gic_boot_probe"
SOURCES = ROOT / "apps/macos/Sources"
ENGINE = SOURCES / "BridgeVMControl/HvfEngine"
LIVE_GATES = ROOT / "scripts/live-gates"
sys.path.insert(0, str(LIVE_GATES))
DEFINITIONS = (ENGINE / "HvfGuestResetRecord.swift", LIVE_GATES / "hvf_reset_record.py")
DETECTORS = ("HvfUnicodeInputRequest", "HvfInputCapabilitiesRequest", "HvfAcknowledgedInputStream",
             "HvfNegotiatedInputStream", "HvfSessionInputRouter", "HvfWindowInventoryRequest",
             "HvfWindowInventoryController", "HvfClipboardPaste")
RETIRED = ("PSCI_SYSTEM_RESET", '"PSCI SYSTEM_RESET')
NAMES = ("processRecreation", "inProcessRebootPrefix", "rebootLimitPrefix", "rebootLimitSuffix")
COMMAND = "WINLIST 277bdd87-9955-4af4-bc5c-f4fe4305c559"


def text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def one(pattern: str, path: Path) -> str:
    found = re.findall(pattern, text(path), re.M)
    assert len(found) == 1, (pattern, path.name, found)
    return found[0]


def runtime() -> dict[str, str]:
    reset = format(int(one(r"^pub\(crate\) const PSCI_SYSTEM_RESET: u64 = (0x[0-9a-f_]+);$",
                           PROBE / "hvf_abi.rs").replace("_", ""), 16), "#x")
    code = one(r"^pub\(crate\) const RESET_EXIT_CODE: u8 = ([0-9]+);$", PROBE / "reboot_watchdog.rs")
    assert text(PROBE / "final_report.rs").count('println!("stop: {}", $stop_reason);') == 1
    recreate = one(r'"(PSCI \{PSCI_SYSTEM_RESET:#x\} exiting for process recreation \(exit \{RESET_EXIT_CODE\}\))"',
                   PROBE / "probe_runtime.rs")
    limit = one(r'"(PSCI \{PSCI_SYSTEM_RESET:#x\} max reboot count \{\} reached)"', PROBE / "reboot_watchdog.rs")
    reboot = one(r'"(PSCI SYSTEM_RESET: reboot )\{reboot_count\}/\{\}"', PROBE / "probe_runtime.rs")
    banner = one(r'"(PSCI SYSTEM_RESET max reboots: )\{\}"', PROBE / "reboot_watchdog/boot_progress.rs")
    limit_prefix, limit_suffix = ("stop: " + limit.replace("{PSCI_SYSTEM_RESET:#x}", reset)).split("{}")
    recreate = recreate.replace("{PSCI_SYSTEM_RESET:#x}", reset).replace("{RESET_EXIT_CODE}", code)
    return {"processRecreation": "stop: " + recreate, "inProcessRebootPrefix": reboot,
            "rebootLimitPrefix": limit_prefix, "rebootLimitSuffix": limit_suffix, "banner": banner + "8"}


class HvfResetRecordContract(unittest.TestCase):
    def setUp(self):
        self.records = records = runtime()
        recreation, reboot = records["processRecreation"], records["inProcessRebootPrefix"]
        limit = (records["rebootLimitPrefix"], records["rebootLimitSuffix"])
        self.resets = (recreation, reboot + "1/8", limit[0] + "8" + limit[1])
        self.inert = ("PSCI_SYSTEM_RESET", "PSCI SYSTEM_RESET: requested", records["banner"],
                      reboot + "1/8 extra", reboot + "1/", reboot + "x/8", "guest " + recreation,
                      recreation + " extra", limit[0] + " " + limit[1], "stop: PSCI 0x84000008 (system off)")

    def test_product_runtime_prints_the_recreation_record_into_one_run_log(self):
        process = text(ROOT / "crates/bridgevm-hvf-runtime/src/vm_process.rs")
        self.assertIn('("BRIDGEVM_EXIT_ON_RESET", "1".to_string())', process)
        self.assertIn('.open(surfaces.evidence_dir.join("run.log"))', process)
        self.assertIn("return SystemResetDecision::ExitForRecreate;", text(PROBE / "reboot_watchdog.rs"))

    def test_swift_definition_equals_runtime_records(self):
        source = text(DEFINITIONS[0])
        for name in NAMES:
            self.assertEqual(re.findall(rf'static let {name} = "([^"\\]*)"', source), [self.records[name]], name)
        stop = one(r'static let systemResetPrefix = "([^"\\]*)"', SOURCES / "BridgeVMProductE2E/HvfStopLine.swift")
        self.assertTrue(self.records["processRecreation"].startswith(stop))
        self.assertTrue(self.records["rebootLimitPrefix"].startswith(stop))

    def test_python_definition_equals_runtime_records_and_matches_whole_lines(self):
        module = importlib.import_module("hvf_reset_record")
        self.assertEqual((module.PROCESS_RECREATION, module.IN_PROCESS_REBOOT_PREFIX, module.REBOOT_LIMIT_PREFIX,
                          module.REBOOT_LIMIT_SUFFIX), tuple(self.records[name] for name in NAMES))
        for line in self.resets:
            self.assertTrue(module.guest_reset(line) and module.guest_reset(line + "\r"), line)
        for line in self.inert:
            self.assertFalse(module.guest_reset(line), line)

    def test_detectors_use_the_shared_definition(self):
        for path in [*SOURCES.rglob("*.swift"), *LIVE_GATES.glob("*.py")]:
            if path not in DEFINITIONS:
                for literal in RETIRED:
                    self.assertNotIn(literal, text(path), path.relative_to(ROOT))
        for name in DETECTORS:
            self.assertIn("HvfGuestResetRecord.matches(", text(ENGINE / (name + ".swift")), name)
        self.assertIn("from hvf_reset_record import", text(LIVE_GATES / "coherence_inventory_observation.py"))

    def test_standalone_compiles_include_the_definition(self):
        paths = [*(ROOT / "scripts").rglob("*.sh"), *(ROOT / "tests").rglob("*.sh"),
                 *(ROOT / ".github/workflows").glob("*.yml")]
        compiled = [path for path in paths
                    if "swiftc" in text(path) and any(name in text(path) for name in DETECTORS)]
        self.assertGreaterEqual(len(compiled), 4, compiled)
        for path in compiled:
            self.assertIn("HvfGuestResetRecord", text(path), path.relative_to(ROOT))

    def test_coherence_inventory_cancels_only_on_runtime_records(self):
        inventory = importlib.import_module("coherence_inventory_observation").inventory
        rows = ["BVAGENT " + COMMAND + " WIN 100 123 20 60 300 180 QQ==", "BVAGENT " + COMMAND + " WINEND"]
        for reset in self.resets:
            for position in range(3):
                lines = list(rows); lines.insert(position, reset)
                with self.assertRaises(ValueError, msg=(reset, position)):
                    inventory(lines, COMMAND)
        for line in self.inert:
            self.assertEqual(len(inventory(rows + [line], COMMAND)["rows"]), 1, line)


if __name__ == "__main__":
    unittest.main(verbosity=2)
