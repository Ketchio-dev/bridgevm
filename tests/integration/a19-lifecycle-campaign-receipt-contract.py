#!/usr/bin/env python3
"""A19 T23 campaign receipt: strict schema, lane evidence, publication, read and fence."""
from __future__ import annotations

import copy
import importlib.util
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
_spec = importlib.util.spec_from_file_location("campaign_fixtures", ROOT / "tests/integration/a19_lifecycle_campaign_fixtures.py")
F = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(F)
receipt, record = F.receipt, F.record
import a19_lifecycle_campaign_read as read  # noqa: E402
import native_snapshot_restore_inputs as inputs  # noqa: E402
_redactor = importlib.util.spec_from_file_location("redactor", ROOT / "scripts/live-gates/redact-receipt.py")
REDACTOR = importlib.util.module_from_spec(_redactor)
_redactor.loader.exec_module(REDACTOR)


def rejects(test: unittest.TestCase, value: dict, message: str = "") -> None:
    with test.assertRaises(ValueError, msg=message):
        receipt.validate(value, F.COMMIT)


class CampaignReceiptSchema(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)

    def tearDown(self):
        self.temporary.cleanup()

    def test_ten_of_ten_is_the_only_pass_and_never_promotes(self):
        value = F.campaign(F.queue_job(self.root))
        self.assertTrue(value["pass"])
        self.assertEqual((value["run_count"], value["passes"], value["failures"], value["sample_count"],
                          value["required_run_count"]), (10, 10, 0, 10, 10))
        self.assertEqual((value["boots_attempted"], value["boots_passed"], value["natural_shutdown_count"]), (30, 30, 30))
        self.assertEqual(value["lane_ordinals"], list(range(1, 11)))
        self.assertEqual(value["replacement_policy"], "none")
        for field in ("claim_eligible", "criterion_pass", "capability_promotion", "three_d_injection"):
            self.assertIs(value[field], False)
            rejects(self, {**value, field: True}, field)
        self.assertEqual(REDACTOR.redact(value), value)

    def test_nine_of_ten_is_a_valid_failure_that_cannot_claim_a_pass(self):
        value = F.campaign(F.queue_job(self.root), failing=10)
        self.assertEqual((value["pass"], value["passes"], value["failures"], value["run_count"]), (False, 9, 1, 10))
        self.assertEqual((value["outcome"], value["failure_code"]), ("failed", "lane-failed"))
        rejects(self, {**value, "pass": True})
        rejects(self, {**value, "outcome": "completed", "failure_code": "none"})
        forged = copy.deepcopy(value)
        forged["lane_pass"][9] = True
        forged["passes"], forged["failures"] = 10, 0
        rejects(self, forged, "a lane with two shutdowns cannot be counted as passing")
        nine = F.campaign(F.queue_job(self.root, "t23-nine"), count=9, outcome="canceled")
        self.assertFalse(nine["pass"])
        rejects(self, {**nine, "outcome": "completed", "failure_code": "none"})
        rejects(self, {**nine, "pass": True})

    def test_missing_duplicate_reordered_and_replaced_lanes_are_rejected(self):
        value = F.campaign(F.queue_job(self.root))
        truncated = copy.deepcopy(value)
        for field in receipt.LANE_FIELDS:
            truncated[field] = truncated[field][:9]
        rejects(self, truncated, "missing lane")
        duplicate = copy.deepcopy(value)
        duplicate["lane_ordinals"][2] = 2
        rejects(self, duplicate, "duplicate lane ordinal")
        reordered = copy.deepcopy(value)
        reordered["lane_ordinals"][0:2] = [2, 1]
        rejects(self, reordered, "reordered lanes")
        boolean = copy.deepcopy(value)
        boolean["lane_ordinals"][0] = True
        rejects(self, boolean, "a boolean is not a lane ordinal")
        replayed = copy.deepcopy(value)
        for field in ("lane_original_marker_sha256", "lane_restored_marker_sha256"):
            replayed[field][1] = replayed[field][0]
        rejects(self, replayed, "a replayed lane marker cannot count as an independent lane")
        replayed["pass"] = False
        receipt.validate(replayed, F.COMMIT)
        replaced = copy.deepcopy(value)
        replaced["lane_pass"][2] = False
        replaced["lane_boots_passed"][2] = replaced["lane_natural_shutdown_counts"][2] = 2
        replaced.update(passes=9, failures=1, boots_passed=29, natural_shutdown_count=29, outcome="failed",
                        failure_code="lane-failed", **{"pass": False})
        rejects(self, replaced, "a failed lane followed by more lanes is a replacement")
        rejects(self, {**replaced, "failure_code": "internal-error"}, "replacement is refused on its own")
        eleven = copy.deepcopy(value)
        eleven["run_count"] = 11
        rejects(self, eleven)

    def test_unknown_private_and_malformed_fields_are_rejected(self):
        value = F.campaign(F.queue_job(self.root))
        rejects(self, {**value, "private_path": "/Users/example/disk.raw"})
        for field, wrong in (("sample_count", 9), ("required_run_count", 9), ("sample_count", True),
                             ("replacement_policy", "retry"), ("tier", "t20-a19-native-snapshot-restore"),
                             ("binary_source_commit", "f" * 40), ("image_sha256", "/Users/example/disk.raw"),
                             ("lane_pass", [True] * 10 + [True])):
            rejects(self, {**value, field: wrong}, field)
        pathy = copy.deepcopy(value)
        pathy["lane_exported_disk_sha256"][4] = "/Users/example/export.snapshot/disk.raw"
        rejects(self, pathy)
        with self.assertRaises(REDACTOR.RedactionError):
            REDACTOR.redact(pathy)


class CampaignSealAndPublication(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.job = F.queue_job(self.root)
        self.value = F.campaign(self.job)
        F.write_receipt(self.job, self.value)

    def tearDown(self):
        self.temporary.cleanup()

    def assert_withheld(self, fenced: bool = True):
        self.assertNotEqual(F.verify(self.job / "receipt.json").returncode, 0)
        self.assertNotEqual(F.publish(self.job).returncode, 0)
        self.assertFalse((self.job / "receipt.public.json").exists())
        if fenced:
            self.assertEqual(F.fence(self.job).returncode, 126)
            self.assertTrue((self.root / "queue/worker-cleanup-required").is_file())

    def test_sealed_campaign_publishes_serves_strictly_and_releases_the_worker(self):
        self.assertEqual(F.verify(self.job / "receipt.json").returncode, 0)
        result = F.publish(self.job)
        self.assertEqual(result.returncode, 0, result.stderr)
        public = self.job / "receipt.public.json"
        self.assertEqual(json.loads(public.read_text()), self.value)
        self.assertEqual(F.verify(public).returncode, 0)
        served = F.live_receipt(self.root, self.job.name)
        self.assertEqual(served.returncode, 0, served.stderr)
        self.assertEqual(json.loads(served.stdout), self.value)
        self.assertEqual(F.fence(self.job).returncode, 0)
        self.assertFalse((self.root / "queue/worker-cleanup-required").exists())

    def test_tampered_public_or_lane_evidence_is_never_served(self):
        self.assertEqual(F.publish(self.job).returncode, 0)
        public = self.job / "receipt.public.json"
        tampered = json.loads(public.read_text())
        tampered["lane_final_disk_sha256"][3] = F.sha("tampered")
        public.unlink()
        F.write_receipt(self.job, tampered, "receipt.public.json")
        served = F.live_receipt(self.root, self.job.name)
        self.assertNotEqual(served.returncode, 0)
        self.assertEqual(served.stdout, b"")
        public.unlink()
        path = self.job / "lanes/lane-06" / record.RECORD
        edited = json.loads(path.read_text())
        edited["final_vars_sha256"] = F.sha("edited")
        path.write_text(json.dumps(edited))
        self.assert_withheld()

    def test_missing_extra_and_duplicated_lane_records_are_rejected(self):
        original = (self.job / "lanes/lane-01" / record.RECORD).read_bytes()
        for mutation in ("missing", "extra", "copied", "symlink"):
            with self.subTest(mutation=mutation):
                lanes = self.job / "lanes"
                if mutation == "missing":
                    (lanes / "lane-05" / record.RECORD).unlink()
                elif mutation == "extra":
                    (lanes / "lane-11").mkdir()
                else:
                    target = lanes / "lane-02" / record.RECORD
                    target.unlink()
                    if mutation == "copied":
                        target.write_bytes(original)
                    else:
                        target.symlink_to(lanes / "lane-01" / record.RECORD)
                self.assert_withheld(fenced=False)
                shutil.rmtree(lanes)
                F.write_lanes(self.job, F.identity(self.job.name), 10)

    def test_cross_commit_and_cross_job_lane_records_are_rejected(self):
        ident = F.identity(self.job.name)
        for foreign in (F.identity(self.job.name, "f" * 40), F.identity("another-job"),
                        F.identity(self.job.name, manifest="f" * 64), F.identity(self.job.name, binary="f" * 64)):
            with self.subTest(foreign=foreign), self.assertRaises(ValueError):
                record.validate_record(F.lane(foreign, 3), ident, 3)
        foreign = F.identity(self.job.name, "f" * 40)
        path = self.job / "lanes/lane-03" / record.RECORD
        path.unlink()
        record.write_record(path, F.lane(foreign, 3), foreign, 3)
        self.assert_withheld(fenced=False)
        path.unlink()
        other = F.identity("another-job")
        record.write_record(path, F.lane(other, 3), other, 3)
        self.assert_withheld(fenced=False)
        with tempfile.TemporaryDirectory() as temporary:
            job = F.queue_job(Path(temporary), commit="f" * 40)
            value = F.campaign(job, commit="f" * 40)
            F.write_receipt(job, value)
            self.assertNotEqual(F.verify(job / "receipt.json").returncode, 0)
            self.assertNotEqual(F.publish(job).returncode, 0)

    def test_residue_in_any_lane_withholds_and_fences(self):
        for lane, name in (("lane-04", "live"), ("lane-07", "prepared-inputs")):
            with self.subTest(lane=lane, name=name):
                residue = self.job / "lanes" / lane / name
                residue.mkdir()
                self.assert_withheld()
                residue.rmdir()
                (self.root / "queue/worker-cleanup-required").unlink()

    def test_wrong_queue_seal_and_unsafe_ledger_are_rejected(self):
        entry = self.root / "queue/job-ledger" / self.job.name / "entry.env"
        entry.chmod(0o600)
        entry.write_text(entry.read_text().replace(F.BINARY, "f" * 64))
        entry.chmod(0o400)
        self.assert_withheld()
        (self.root / "queue/worker-cleanup-required").unlink()
        entry.chmod(0o600)
        entry.write_text(entry.read_text().replace("f" * 64, F.BINARY).replace(F.TIER, "t20-a19-native-snapshot-restore"))
        entry.chmod(0o400)
        self.assert_withheld()

    def test_strict_reader_dispatch_prefers_no_legacy_downgrade(self):
        self.assertIs(read.strict_reader(("t1-vtimer", None, None)), None)
        self.assertIs(read.strict_reader((F.TIER, F.TIER, F.TIER)), read.read_strict)
        self.assertIsNotNone(read.strict_reader(("t1-vtimer", None, "t20-a19-native-snapshot-restore")))
        self.assertIsNotNone(read.strict_reader(("t1-vtimer", None, F.TIER)))
        self.assertIs(read.strict_reader(("t1-vtimer", None, ["unhashable"])), None)
        self.assertEqual(F.publish(self.job).returncode, 0)
        job_env = self.job / "job.env"
        job_env.write_text(job_env.read_text().replace(F.TIER, "t1-vtimer"))
        served = F.live_receipt(self.root, self.job.name)
        self.assertNotEqual(served.returncode, 0)
        self.assertEqual(served.stdout, b"")


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

    def test_every_queue_dispatcher_names_the_campaign_tier(self):
        live = ROOT / "scripts/live-gates"
        for name in ("bridgevm-live", "run-tier.sh", "run-a19-snapshot-tiers.sh", "app-ui-manifest-dispatch.sh",
                     "verify-live-receipt.sh", "write-live-missing-receipt.sh", "write-missing-receipt.sh",
                     "recover-stale-receipt.sh", "t17-worker-cleanup-fence.sh", "a19-worker-cleanup-dispatch.sh"):
            with self.subTest(name=name):
                self.assertIn(F.TIER, (live / name).read_text())
        self.assertIn("run-a19-lifecycle-campaign-tier.py", (live / "run-a19-snapshot-tiers.sh").read_text())
        self.assertIn("bridgevm_t23_guard_or_fence", (live / "a19-worker-cleanup-dispatch.sh").read_text())


if __name__ == "__main__":
    unittest.main()
