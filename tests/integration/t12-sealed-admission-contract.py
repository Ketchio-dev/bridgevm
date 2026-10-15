#!/usr/bin/env python3
"""Exercise actual T12 admission with synthetic inputs; never launch a guest."""
import subprocess
import unittest
from t12_sealed_test_support import Fixture, TIER, digest

class Admission(unittest.TestCase):
    def setUp(self): self.f = Fixture()
    def tearDown(self): self.f.close()

    def test_missing_manifest_is_refused_before_queue_publication(self):
        result = self.f.command("submit", TIER, "--sha", self.f.commit, "--job-id", "missing")
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertFalse(self.f.queue.exists())

    def test_valid_manifest_seals_both_input_copies(self):
        result = self.f.submit("sealed")
        self.assertEqual(result.returncode, 0, result.stderr)
        job = self.f.queue / "queued/sealed"
        self.assertEqual(digest(job / "hvf_gic_boot_probe"), self.f.rows["binary"][1])
        self.assertEqual(digest(job / "t12-firmware.fd"), self.f.rows["firmware"][1])
        self.assertEqual((job / "input-manifest.tsv").read_bytes(), self.f.manifest.read_bytes())
        self.assertFalse(self.f.marker.exists())

    def test_unsealed_tier_refuses_before_any_build(self):
        result = subprocess.run(["/bin/bash", str(self.f.scripts / "run-bridgevm-pc-nvme-block-tier.sh"),
            "--out", str(self.f.base / "out"), "--job-id", "unsealed"], env=self.f.env,
            capture_output=True, text=True, timeout=20)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.f.marker.exists(), "unsealed tier reached the forbidden builder")

if __name__ == "__main__": unittest.main()
