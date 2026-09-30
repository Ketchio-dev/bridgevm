#!/usr/bin/env python3
"""Packaged Windows install media stay in the VM bundle's private staging directory.

HvfWindowsInstallStaging.swift names that bundle-relative directory. The T17
product sampler is built in a target that cannot import it, so its copy of the
name is compared here. The fixed shared-/tmp names an earlier build used may
survive only where journals sealed by that build are admitted.
"""

from __future__ import annotations

from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
SOURCES = ROOT / "apps/macos/Sources"
STAGING = SOURCES / "BridgeVMControl/HvfEngine/HvfWindowsInstallStaging.swift"
E2E = SOURCES / "BridgeVMProductE2E"
LEGACY = "/tmp/bridgevm-appinstall-"


class HvfInstallStagingContract(unittest.TestCase):
    def staging_name(self) -> str:
        self.assertTrue(STAGING.is_file(), f"missing {STAGING.relative_to(ROOT)}")
        names = re.findall(r'static let directoryName = "([a-z0-9-]+)"',
                           STAGING.read_text(encoding="utf-8"))
        self.assertEqual(len(names), 1, names)
        return names[0]

    def test_shared_tmp_names_remain_only_for_legacy_journal_admission(self):
        holders = sorted(path.relative_to(ROOT).as_posix() for path in SOURCES.rglob("*.swift")
                         if LEGACY in path.read_text(encoding="utf-8"))
        self.assertEqual(holders, [STAGING.relative_to(ROOT).as_posix()])
        lines = [line for line in STAGING.read_text(encoding="utf-8").splitlines() if LEGACY in line]
        self.assertEqual(len(lines), 2, lines)
        self.assertTrue(all("-evidence" not in line for line in lines), lines)

    def test_t17_sampler_reads_evidence_from_the_bundle_staging_directory(self):
        name = self.staging_name()
        diagnostic = (E2E / "T17InstallTimeoutDiagnostic.swift").read_text(encoding="utf-8")
        self.assertIn(f'"/metadata/{name}/evidence"', diagnostic)
        runner = (E2E / "T17ProductRunner.swift").read_text(encoding="utf-8")
        self.assertEqual(runner.count("T17InstallEnvironmentSampler(bundlePath: request.bundlePath)"), 1)


if __name__ == "__main__":
    unittest.main()
