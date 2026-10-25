#!/usr/bin/env python3
"""Without the sealed row the default T17/T19 input path is byte-for-byte what it was, and the row is
refused everywhere but a T19 pilot manifest (docs/history/windows-hvf/bitlocker-restart-check-20261023.md).
The helper's arguments, lane result and receipt are covered by t19-bitlocker-switch-test.py,
t19-bitlocker-private-lane-test.py and T19BitLockerLaneTests.swift."""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
GATES = ROOT / "scripts/live-gates"
ROW = "bitlocker_restart_check\ttpm-only"


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
    return module


SWITCH_TEST = load("t19_default_switch_fixture", ROOT / "tests/integration/t19-bitlocker-switch-test.py")
T19 = SWITCH_TEST.MANIFEST
T17 = load("t17_default_manifest", GATES / "windows-product-e2e-manifest.py")


class DefaultUnchangedTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory()
        cls.root = Path(cls.temporary.name)
        cls.assets = SWITCH_TEST.sealed_inputs(cls.root)

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def written(self, name: str, *rows: str, mode: str = "pilot") -> tuple[int, bytes]:
        manifest = self.root / f"{name}.tsv"
        manifest.write_text("".join(f"{row}\n" for row in (f"campaign_mode\t{mode}", *rows)), encoding="utf-8")
        out = self.root / f"{name}.json"
        completed = subprocess.run([sys.executable, str(GATES / "windows-import-product-e2e-manifest.py"), "--manifest", str(manifest),
                                    "--out", str(out)], capture_output=True, text=True, check=False)
        return completed.returncode, out.read_bytes()

    def test_default_verified_inputs_are_the_bytes_the_base_verifier_wrote(self):
        for mode in ("pilot", "release"):
            status, data = self.written(f"default-{mode}", *self.assets, mode=mode)
            self.assertEqual(status, 0)
            assets = {}
            for row in self.assets:
                key, path, digest = row.split("\t")
                assets[key] = {"path": path, "sha256": digest}
            # The base verifier: json.dump(result, indent=2, sort_keys=True) and one newline, six keys.
            expected = {"valid": True, "verified": True, "campaign_mode": mode, "failure_code": "none", "detail": "none", "assets": assets}
            self.assertEqual(data, (json.dumps(expected, indent=2, sort_keys=True) + "\n").encode())

    def test_the_row_adds_one_key_and_nothing_else(self):
        _, plain = self.written("plain", *self.assets)
        status, checked = self.written("checked", ROW, *self.assets)
        self.assertEqual(status, 0)
        value = json.loads(checked)
        self.assertEqual(value.pop("bitlocker_restart_check"), "tpm-only")
        self.assertEqual(value, json.loads(plain))

    def test_refused_rows_write_the_base_refusal_shape(self):
        for name, rows, mode in (("release", (ROW, *self.assets), "release"), ("duplicate", (ROW, ROW, *self.assets), "pilot"),
                                 ("other value", ("bitlocker_restart_check\ttpm-pin", *self.assets), "pilot")):
            with self.subTest(name):
                status, data = self.written(name.replace(" ", "-"), *rows, mode=mode)
                self.assertEqual(status, 1)
                self.assertEqual(set(json.loads(data)), {"valid", "verified", "campaign_mode", "failure_code", "detail", "assets"})

    def test_the_t17_verifier_refuses_the_row_itself(self):
        manifest = self.root / "t17.tsv"
        manifest.write_text(f"campaign_mode\tpilot\n{ROW}\n", encoding="utf-8")
        with self.assertRaisesRegex(T17.ManifestError, r"unknown manifest row 2$"):
            T17.parse(manifest)

    def test_no_default_path_reads_the_switch(self):
        for relative in ("scripts/live-gates/make-windows-import-product-e2e-request.py", "scripts/live-gates/windows-product-e2e-manifest.py",
                         "scripts/live-gates/run-windows-product-e2e-tier.sh", "scripts/live-gates/run-windows-import-product-e2e-tier.sh",
                         "scripts/live-gates/write-windows-product-e2e-receipt.py", "scripts/live-gates/product_e2e_private_lane.py",
                         "apps/macos/Sources/BridgeVMWindowProtocol/ProductE2EWorkBoundary.swift",
                         "apps/macos/Sources/BridgeVMProductE2E/T17ProductRunner.swift",
                         "apps/macos/Sources/BridgeVMProductE2E/T17GuestJourney.swift"):
            with self.subTest(relative):
                self.assertNotIn("bitlocker", (ROOT / relative).read_text(encoding="utf-8").lower())


if __name__ == "__main__":
    result = unittest.main(exit=False).result
    raise SystemExit(0 if result.wasSuccessful() else 1)
