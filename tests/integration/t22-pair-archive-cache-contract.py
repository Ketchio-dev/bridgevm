#!/usr/bin/env python3
"""The supported archived-receipt CLI must not create future rejected caches."""
import json
import platform
import shutil
import subprocess
import unittest

from t22_pair_worker_fixtures import WorkerFixture, LIVE


@unittest.skipUnless(platform.system() == "Darwin", "owned worker uses fixed Darwin taskpolicy/caffeinate")
class ArchiveCache(unittest.TestCase):
    def test_actual_receipt_cli_preserves_cache_free_admitted_source(self):
        fixture = WorkerFixture(self)
        cli = fixture.repo / "scripts/live-gates/receipt-cli"
        shutil.copy2(LIVE / "bridgevm-live", cli)
        fixture.git("add", "scripts/live-gates/receipt-cli")
        fixture.git("-c", "user.name=Owned Fixture", "-c", "user.email=fixture@example.invalid",
                    "commit", "-qm", "owned actual receipt reader")
        fixture.commit = fixture.git("rev-parse", "HEAD").stdout.strip()
        fixture.sealed_job(mode="success")
        code, output = fixture.run()
        self.assertEqual(code, 0, output)
        admission = ["/bin/bash", "--noprofile", "--norc", "-p",
            str(fixture.repo / "scripts/live-gates/t22-pair-source-admission.sh"),
            str(fixture.repo), fixture.commit]
        before = subprocess.run(admission, env=fixture.env(), capture_output=True, text=True, timeout=35)
        self.assertEqual(before.returncode, 0, before.stderr)
        result = subprocess.run([str(cli), "receipt", "a-fixture"], env=fixture.env(),
                                capture_output=True, text=True, timeout=35)
        self.assertEqual(result.returncode, 0, result.stderr)
        public = json.loads(result.stdout)
        self.assertEqual(public, json.loads((fixture.queue / "done/a-fixture/receipt.public.json").read_bytes()))
        for field in ("pass", "claim_eligible", "criterion_pass"): self.assertFalse(public[field])
        after = subprocess.run(admission, env=fixture.env(), capture_output=True, text=True, timeout=35)
        self.assertEqual(after.returncode, 0, after.stderr)
        self.assertEqual(fixture.git("status", "--porcelain", "--untracked-files=all").stdout, "")
        for root in ("scripts", "scripts/live-gates"):
            self.assertFalse((fixture.repo / root / "__pycache__").exists())


if __name__ == "__main__": unittest.main()
