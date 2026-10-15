#!/usr/bin/env python3
"""Real tier/phase shell paths with synthetic runner/verifier, not hardware proof."""
import json
import unittest
from t12_sealed_test_support import digest, IMAGE, VARS
from t12_sealed_runtime_fixture import Runtime

class RuntimeContract(unittest.TestCase):
    def setUp(self): self.f = Runtime()
    def tearDown(self): self.f.close()

    def test_twenty_lanes_use_queue_copies_without_build_or_sign(self):
        job = self.f.queue_job()
        binary_hash = digest(job / "hvf_gic_boot_probe")
        self.f.binary.write_bytes(b"source changed after submission")
        self.f.firmware.write_bytes(b"source changed after submission")
        result = self.f.run_tier(job)
        self.assertEqual(result.returncode, 0, result.stderr)
        receipt = json.loads((job / "receipt.json").read_text())
        self.assertEqual((receipt["sample_count"], receipt["passes"], receipt["failures"]), (20, 20, 0))
        self.assertEqual((receipt["firmware_boot_count"], receipt["nvme_block_read_count"]), (40, 40))
        self.assertEqual(receipt["binary_hash"], binary_hash)
        self.assertEqual(receipt["expected_initial_image_sha256"], IMAGE)
        self.assertEqual(receipt["expected_initial_vars_sha256"], VARS)
        self.assertEqual(len(self.f.phases()), 60)
        self.assertTrue(all(row[0] == str(job / "t12-firmware.fd") for row in self.f.phases()))
        self.assertEqual(len(list((job / "lanes").glob("*/BridgeVmPcVars.fd"))), 20)
        self.assertNotEqual(receipt["vars_hash"], VARS)
        self.assertFalse(self.f.marker.exists())

    def test_changed_queue_inputs_refuse_before_phase(self):
        for member in ["hvf_gic_boot_probe", "t12-firmware.fd", "input-manifest.tsv", "t12-input-seal.json"]:
            with self.subTest(member=member):
                job = self.f.queue_job("changed-" + member.replace(".", "-"))
                path = job / member; path.chmod(0o600); path.write_bytes(b"changed")
                result = self.f.run_tier(job)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(self.f.phases(), [])

    def test_wrong_queue_or_ledger_refuses_before_phase(self):
        job = self.f.queue_job()
        ledger = self.f.queue / "job-ledger/run/entry.env"
        ledger.chmod(0o600)
        result = self.f.run_tier(job)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.f.phases(), [])
        ledger.chmod(0o400)
        moved = self.f.base / "run"; job.rename(moved)
        result = self.f.run_tier(moved)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.f.phases(), [])

    def test_signature_refusal_does_not_start_phase(self):
        job = self.f.queue_job()
        self.f.signature.write_text("#!/bin/sh\nexit 1\n")
        result = self.f.run_tier(job)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.f.phases(), [])

    def test_wrong_actual_initial_vars_hash_fails_first_lane(self):
        original = self.f.binary.read_text()
        self.f.binary.write_text(original.replace('hashlib.sha256(initial).hexdigest()', '"0" * 64'))
        self.f.rows["binary"][1] = digest(self.f.binary); self.f.write_manifest()
        job = self.f.queue_job()
        result = self.f.run_tier(job)
        self.assertNotEqual(result.returncode, 0)
        receipt = json.loads((job / "receipt.json").read_text())
        self.assertEqual((receipt["sample_count"], receipt["passes"], receipt["failures"]), (1, 0, 1))
        self.assertFalse(receipt["pass"])

if __name__ == "__main__": unittest.main()
