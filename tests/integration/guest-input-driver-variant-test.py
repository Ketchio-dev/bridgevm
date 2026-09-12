#!/usr/bin/env python3
"""Sealing and dispatch contracts; subprocess results are synthetic here."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts/live-gates"))
import guest_input_driver_variant as variant
from guest_input_live_inputs import digest
from guest_input_profiles import asset_names, PROFILE, PRODUCTION_PROFILE


class Variant(unittest.TestCase):
    def test_seal_bind_and_tamper(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp).resolve()
            binary, staging, manifest = root / "driver", root / "queue", root / "manifest.json"
            binary.write_bytes(b"sealed fixture")
            staging.mkdir()
            value = {"profile": PRODUCTION_PROFILE, "assets": {name: {} for name in asset_names(PRODUCTION_PROFILE)}}
            value["assets"]["driver"] = {"path": str(binary), "sha256": digest(binary)}
            manifest.write_text(json.dumps(value))
            variant.seal(manifest, staging)
            variant.bind(staging, value)
            copied = staging / "production-input-driver"
            self.assertEqual(value["assets"]["driver"]["path"], str(copied))
            self.assertEqual(copied.read_bytes(), binary.read_bytes())
            self.assertEqual(copied.stat().st_mode & 0o777, 0o500)
            copied.chmod(0o600)
            copied.write_bytes(b"tampered")
            with self.assertRaises(ValueError):
                variant.bind(staging, value)

    def test_manual_profile_refuses_driver(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            value = {"profile": PROFILE, "assets": {name: {} for name in asset_names(PRODUCTION_PROFILE)}}
            path = root / "manifest.json"
            path.write_text(json.dumps(value))
            with self.assertRaises(ValueError):
                variant.seal(path, root)
            value["profile"] = PRODUCTION_PROFILE
            del value["assets"]["driver"]
            with self.assertRaises(ValueError):
                variant.bind(root, value)
            with self.assertRaises(ValueError):
                asset_names("unknown")

    def test_report_strictness_and_dispatch(self):
        good = dict(schema="bridgevm.production-input-driver.v1", claim_eligible=False,
                    production_ui_proven=False, guest_application_proven=False,
                    driver_receipts_observed=True, sent=4, inserted=4, failure="none")
        variant.checked_report(good)
        for key, value in (("sent", True), ("inserted", 3), ("production_ui_proven", True),
                           ("guest_application_proven", True), ("extra", "private")):
            with self.assertRaises(ValueError):
                variant.checked_report(dict(good, **{key: value}))
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            driver = variant.make_controller(root / "ctl", root / "log", root / "share", {"driver": root / "driver"})
            response = subprocess.CompletedProcess([], 0, json.dumps(good).encode(), b"")
            with patch.object(variant.subprocess, "run", return_value=response) as run:
                driver.dispatch_inputs(123, 32767)
                self.assertEqual(run.call_args.args[0], [str(root / "driver"), str(root / "ctl"), str(root / "log"), "123", "32767"])
            self.assertTrue(driver.driver_observed)
            self.assertEqual(json.loads((root / "production-driver.json").read_text()), good)

    def test_manual_dispatch_stays_explicit(self):
        driver = variant.make_controller("ctl", "log", "share", {})
        calls = []
        driver.input = lambda *args: calls.append(args)
        driver.dispatch_inputs(123, 32767)
        self.assertEqual(len(calls), 4)
        self.assertEqual(calls[-1], ("POINTERINPUT", "click:123x32767", 2))
        self.assertNotIsInstance(driver, variant.ProductionController)


if __name__ == "__main__":
    unittest.main()
