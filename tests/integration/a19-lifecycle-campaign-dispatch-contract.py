#!/usr/bin/env python3
"""A19 T23 queue routes retain strict receipt, seal and delegation behavior."""
import json
import os
from pathlib import Path
import tempfile
import unittest

import a19_lifecycle_campaign_fixtures as F
import a19_lifecycle_campaign_read as read
import native_snapshot_restore_inputs as inputs

ROOT, receipt = F.ROOT, F.receipt


class CampaignQueueDispatch(unittest.TestCase):
    def test_missing_receipt_is_written_but_withheld_and_fenced(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            job = F.queue_job(root, state="running")
            result = F.run([str(ROOT / "scripts/live-gates/write-missing-receipt.sh"), F.TIER, str(job), str(ROOT),
                            job.name, F.COMMIT])
            self.assertEqual(result.returncode, 0, result.stderr)
            value = receipt.validate(json.loads((job / "receipt.json").read_text()), F.COMMIT)
            self.assertEqual((value["outcome"], value["failure_code"], value["pass"]),
                             ("failed-before-receipt", "missing-tier-receipt", False))
            self.assertNotEqual(F.publish(job).returncode, 0)
            self.assertEqual(F.fence(job).returncode, 126)

    def test_stale_recovery_without_worktree_withholds(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            job = F.queue_job(root, state="running")
            F.write_receipt(job, F.campaign(job))
            (job / "result.env").write_text("result=interrupted-worker-exit\n")
            result = F.run([str(ROOT / "scripts/live-gates/recover-stale-receipt.sh"), str(ROOT),
                            str(root / "work"), str(job), job.name, F.TIER, F.COMMIT])
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("receipt=withheld-no-sealed-worktree", (job / "result.env").read_text())
            self.assertFalse((job / "receipt.public.json").exists())

    def test_run_tier_routes_to_the_campaign_runner_and_fails_closed_unsealed(self):
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary) / "unsealed-job"
            result = F.run([str(ROOT / "scripts/live-gates/run-tier.sh"), F.TIER, "--out", str(out), "--job-id",
                            "unsealed-job", "--input-manifest", str(out / "missing.tsv"),
                            "--sealed-binary", str(out / "missing-probe")])
            self.assertNotEqual(result.returncode, 0)
            value = receipt.validate(json.loads((out / "receipt.json").read_text()))
            self.assertEqual((value["tier"], value["outcome"], value["pass"], value["run_count"]),
                             (F.TIER, "preflight-blocked", False, 0))

    def test_submission_validates_and_seals_the_manifest_and_probe(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            app = root / "BridgeVM.app"
            artifacts = {"app_bundle": app, "image": root / "image.raw", "vars": root / "vars.fd"}
            for key, relation in inputs.RELATIONS.items():
                artifacts[key] = app / relation
            for key, path in artifacts.items():
                if key != "app_bundle":
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(key.encode())
                    path.chmod(0o700)
            rows = [f"{key}\t{path}\t{inputs.tree_hash(path) if key == 'app_bundle' else inputs.digest(path)}"
                    for key, path in artifacts.items()]
            rows += [f"source_commit\t{F.COMMIT}", "app_profile\trelease", "binary_profile\trelease",
                     "binary_features\tvenus", "rust_toolchain\t1.97.0"]
            manifest = root / "manifest.tsv"
            manifest.write_text("\n".join(rows) + "\n")
            env = dict(os.environ, BRIDGEVM_LIVE_ROOT=str(root / "queue"))
            submitted = F.run(["bash", str(ROOT / "scripts/live-gates/bridgevm-live"), "submit", F.TIER,
                               "--sha", F.COMMIT, "--input-manifest", str(manifest), "--job-id", "t23-submit"], env=env)
            self.assertEqual(submitted.returncode, 0, submitted.stderr)
            job = root / "queue/queued/t23-submit"
            sealed = read.sealed_hashes(job, "t23-submit", F.COMMIT)
            self.assertEqual(sealed, {"input_manifest_sha256": inputs.digest(job / "input-manifest.tsv"),
                                      "binary_hash": inputs.digest(job / "hvf_gic_boot_probe")})
            refused = F.run(["bash", str(ROOT / "scripts/live-gates/bridgevm-live"), "submit", F.TIER,
                             "--sha", F.COMMIT, "--job-id", "t23-no-manifest"], env=env)
            self.assertNotEqual(refused.returncode, 0)

    def test_queue_dispatch_implementations_name_the_campaign_tier(self):
        live = ROOT / "scripts/live-gates"
        for name in ("bridgevm-live", "run-tier.sh", "run-a19-snapshot-tiers.sh", "app-ui-manifest-dispatch.sh",
                     "verify-live-receipt-legacy.sh", "write-live-missing-receipt.sh", "write-missing-receipt.sh",
                     "recover-stale-receipt.sh", "t17-worker-cleanup-fence.sh", "a19-worker-cleanup-dispatch.sh"):
            with self.subTest(name=name):
                self.assertIn(F.TIER, (live / name).read_text())
        self.assertIn("run-a19-lifecycle-campaign-tier.py", (live / "run-a19-snapshot-tiers.sh").read_text())
        self.assertIn("bridgevm_t23_guard_or_fence", (live / "a19-worker-cleanup-dispatch.sh").read_text())

    def test_verifier_wrapper_forwards_arguments_and_failure_status(self):
        with tempfile.TemporaryDirectory() as temporary:
            worktree = Path(temporary) / "source with spaces"
            legacy = worktree / "scripts/live-gates/verify-live-receipt-legacy.sh"
            legacy.parent.mkdir(parents=True)
            legacy.write_text("#!/usr/bin/env bash\nprintf '%s\\0' \"$@\"\nexit 47\n")
            arguments = [F.TIER, str(worktree / "receipt with spaces.json"), str(worktree), F.COMMIT]
            result = F.run(["bash", str(ROOT / "scripts/live-gates/verify-live-receipt.sh"), *arguments])
            self.assertEqual(result.returncode, 47, result.stderr)
            self.assertEqual(result.stdout.split("\0"), [*arguments, ""])


if __name__ == "__main__":
    unittest.main()
