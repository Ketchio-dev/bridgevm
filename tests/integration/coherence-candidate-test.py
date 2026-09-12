#!/usr/bin/env python3
"""Synthetic candidate identity and fixture staging contracts."""
import copy
from pathlib import Path
import sys
import tempfile
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts/live-gates"))
from coherence_candidate_observation import fixture, compare
from guest_input_fixture_staging import stage
from guest_input_profiles import COHERENCE_PROFILE, PROFILE

NONCE = "5038d4b9-9678-45c0-bf99-64e939e5ee52"
VALUE = {"schema": "bridgevm.coherence-fixture.v1", "nonce": NONCE, "pid": 12,
    "windows": [{"id": str(i), "title": "BridgeVM Coherence " + NONCE + " " + str(i-1)} for i in (1, 2)],
    "candidate": {"completed": True, "handles": ["1", "2"], "failure": "none"},
    "powershell_version": "5.1.26100.1", "process_architecture": "ARM64"}


class Candidate(unittest.TestCase):
    def test_baseline_is_not_replaced(self):
        result = compare(fixture(copy.deepcopy(VALUE), NONCE), {"rows": []})
        self.assertEqual(result["observed_windows"], 0)
        self.assertFalse(result["all_fixture_windows_observed"])
        self.assertEqual(result["candidate"]["observed_windows"], 2)
        self.assertTrue(result["candidate"]["all_fixture_windows_observed"])
        self.assertTrue(result["candidate"]["target_runtime"])
        self.assertFalse(result["candidate"]["claim_eligible"])

    def test_failed_or_partial_candidate(self):
        for completed, handles in ((False, []), (True, []), (True, ["1"])):
            value = copy.deepcopy(VALUE)
            value["candidate"] = {"completed": completed, "handles": handles, "failure": "none" if completed else "EnumerationFailed"}
            result = compare(fixture(value, NONCE), {"rows": []})["candidate"]
            self.assertFalse(result["all_fixture_windows_observed"])
            self.assertEqual(result["collection_complete"], completed)
        for architecture in ("AMD64", "x86"):
            value = copy.deepcopy(VALUE); value["process_architecture"] = architecture
            self.assertFalse(compare(fixture(value, NONCE), {"rows": []})["candidate"]["target_runtime"])

    def test_invalid_candidate(self):
        for key, data in (("handles", ["1", "1"]), ("handles", ["3"]), ("handles", [True]),
                          ("completed", 1), ("failure", "wrong")):
            value = copy.deepcopy(VALUE); value["candidate"][key] = data
            with self.assertRaises(ValueError): fixture(value, NONCE)
        for key, data in (("powershell_version", "wrong"), ("process_architecture", "unknown"), ("candidate", {})):
            value = copy.deepcopy(VALUE); value[key] = data
            with self.assertRaises(ValueError): fixture(value, NONCE)

    def test_stages_all_dependencies_without_overwrite(self):
        root = Path(__file__).resolve().parents[2]
        for profile, count in ((COHERENCE_PROFILE, 3), (PROFILE, 1)):
            with tempfile.TemporaryDirectory() as tmp:
                target = Path(tmp)
                primary, hashes = stage(root, target, profile)
                self.assertEqual(len(hashes), count)
                self.assertIn(primary.name, hashes)
                self.assertEqual(set(hashes), {p.name for p in target.iterdir()})
                with self.assertRaises(FileExistsError): stage(root, target, profile)


if __name__ == "__main__":
    unittest.main()
