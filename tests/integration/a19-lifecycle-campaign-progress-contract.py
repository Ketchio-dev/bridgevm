#!/usr/bin/env python3
"""T23 keeps retained lanes when a later runner error aborts before returning."""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

import a19_lifecycle_campaign_fixtures as fixtures
import a19_lifecycle_campaign_read as read

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "campaign_runner", ROOT / "scripts/live-gates/run-a19-lifecycle-campaign-tier.py")
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)


class CampaignProgressContract(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.job = fixtures.queue_job(self.root, state="running")
        self.calls: list[int] = []

    def tearDown(self):
        self.temporary.cleanup()

    def campaign(self, error: str) -> dict:
        def run_lane(lanes, ordinal, identity, *_arguments):
            self.calls.append(ordinal)
            if error in ("unexpected", "incomplete-lane") and ordinal == 2:
                if error == "incomplete-lane":
                    (lanes / fixtures.record.lane_name(ordinal) / "live").mkdir(parents=True)
                raise RuntimeError("unexpected runner error after the first retained lane")
            (lanes / fixtures.record.lane_name(ordinal)).mkdir()
            value = fixtures.lane(identity, ordinal)
            public = fixtures.inputs()
            if ordinal == 2:
                public["app_cli_sha256"] = fixtures.sha("different sealed app CLI")
            return value, public

        arguments = ["runner", str(self.job), self.job.name, str(self.root / "manifest.tsv"),
                     str(self.root / "probe")]
        with (mock.patch.object(runner.sys, "argv", arguments),
              mock.patch.object(runner, "source_commit", return_value=fixtures.COMMIT),
              mock.patch.object(runner, "host_identity", return_value={"host_model": "Mac17,9", "macos_version": "27.0"}),
              mock.patch.object(runner, "preflight"),
              mock.patch.object(runner, "run_lane", side_effect=run_lane)):
            self.assertEqual(runner.main(), 1)
        return fixtures.receipt.validate(json.loads((self.job / "receipt.json").read_text()), fixtures.COMMIT)

    def assert_retained(self, value: dict, count: int):
        self.assertEqual(self.calls, [1, 2])
        self.assertEqual((value["outcome"], value["failure_code"], value["pass"]),
                         ("failed", "internal-error", False))
        self.assertEqual((value["run_count"], value["passes"], value["failures"]), (count, count, 0))
        self.assertEqual((value["boots_attempted"], value["boots_passed"], value["natural_shutdown_count"]),
                         (3 * count,) * 3)
        self.assertEqual(value["lane_ordinals"], list(range(1, count + 1)))
        self.assertTrue(value["worker_cleanup_verified"])
        self.assertFalse((self.job / "lanes/lane-03").exists())
        records = fixtures.record.read_records(self.job / "lanes", fixtures.identity(self.job.name), count)
        self.assertEqual(value["lane_record_sha256"], [digest for _, digest in records])
        read.validate_seal(value, self.job)
        self.assertEqual(fixtures.publish(self.job).returncode, 0)
        self.assertFalse(json.loads((self.job / "receipt.public.json").read_text())["pass"])
        self.assertEqual(fixtures.fence(self.job).returncode, 0)

    def test_changed_second_lane_inputs_keep_both_retained_records(self):
        value = self.campaign("different-inputs")
        self.assert_retained(value, 2)
        self.assertEqual(value["app_cli_sha256"], fixtures.inputs()["app_cli_sha256"])

    def test_unexpected_later_error_keeps_the_first_retained_record(self):
        self.assert_retained(self.campaign("unexpected"), 1)

    def test_incomplete_later_lane_keeps_prior_evidence_and_withholds_cleanup(self):
        value = self.campaign("incomplete-lane")
        self.assertEqual((value["run_count"], value["passes"], value["failures"]), (1, 1, 0))
        self.assertEqual(value["lane_ordinals"], [1])
        self.assertFalse(value["pass"])
        self.assertFalse(value["worker_cleanup_verified"])
        self.assertTrue((self.job / "lanes/lane-02/live").is_dir())
        self.assertFalse((self.job / "lanes/lane-02" / fixtures.record.RECORD).exists())
        with self.assertRaises(ValueError):
            read.validate_seal(value, self.job)
        self.assertNotEqual(fixtures.publish(self.job).returncode, 0)
        self.assertEqual(fixtures.fence(self.job).returncode, 126)


if __name__ == "__main__":
    unittest.main()
