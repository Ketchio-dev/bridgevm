#!/usr/bin/env python3
"""A matching guest JSON digest cannot replace nonce-bound raw journey evidence."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import product_e2e_identity_fixtures as fixture
from product_e2e_import_fixture_base import prepare


class ImportGuestEvidence(unittest.TestCase):
    def setUp(self):
        prepare(self, "bridgevm-import-proof-")
        self.request = json.loads(self.request_path.read_text())
        self.evidence_path = Path(self.request["guest_evidence_path"])
        self.evidence = json.loads(self.evidence_path.read_text())
        self.stamp = self.root.parent / "stamp.json"
        self.seal = fixture.T19.digest(self.request_path)

    def tearDown(self):
        self.temporary.cleanup()

    def reseal_result(self):
        result = json.loads(self.result_path.read_text())
        result["guest_evidence_sha256"] = fixture.T19.digest(self.evidence_path)
        self.result_path.write_text(json.dumps(result))

    def audit(self):
        fixture.T19.authenticate(self.request_path, self.result_path, self.stamp,
                                 fixture.JOB, fixture.COMMIT, "pilot", 1, self.seal)

    def assert_refused(self):
        self.reseal_result()
        with self.assertRaises((OSError, ValueError)):
            self.audit()
        self.assertFalse(self.stamp.exists())

    def write_evidence(self):
        self.evidence_path.write_text(json.dumps(self.evidence))

    def test_complete_raw_fixture_authenticates(self):
        self.audit()
        self.assertTrue(self.stamp.is_file())

    def test_digest_valid_minimal_evidence_cannot_authenticate(self):
        self.evidence_path.write_text(json.dumps({"nonce": fixture.NONCE, "status": "fixture"}))
        self.assert_refused()
        command = [sys.executable, str(fixture.ROOT / "scripts/live-gates/write-windows-import-product-e2e-receipt.py"),
                   "--check-lane", str(self.result_path), "--request", str(self.request_path),
                   "--stamp", str(self.stamp), "--job-id", fixture.JOB, "--commit", fixture.COMMIT,
                   "--mode", "pilot", "--ordinal", "1", "--expected-request-sha256", self.seal]
        completed = subprocess.run(command, capture_output=True, text=True, timeout=30)
        self.assertEqual(completed.returncode, 1)
        self.assertIn("guest evidence", completed.stderr)
        self.assertFalse(self.stamp.exists())

    def test_guest_identity_must_match_request_nonce(self):
        self.evidence["nonce"] = "d" * 64
        self.write_evidence()
        self.assert_refused()

    def test_guest_boolean_lane_and_audio_error_are_not_integer_evidence(self):
        original = dict(self.evidence)
        for field, invalid in (("lane", True), ("lane", 1.0), ("audio_error_count", False)):
            with self.subTest(field=field, invalid=invalid):
                self.evidence = {**original, field: invalid}
                self.write_evidence()
                self.assert_refused()

    def test_agent_identity_and_counter_types_are_authenticated(self):
        agent_path = self.evidence_path.parent / "product-e2e/agent-result.json"
        original = json.loads(agent_path.read_text())
        shared = Path(self.request["share_path"]) / f"t17-agent-result-{fixture.NONCE[:12]}.json"
        for field, invalid in (("lane", True), ("audio_error_count", False)):
            with self.subTest(field=field, invalid=invalid):
                raw = json.dumps({**original, field: invalid}).encode()
                agent_path.write_bytes(raw); shared.write_bytes(raw)
                self.evidence["agent_result_sha256"] = hashlib.sha256(raw).hexdigest()
                self.write_evidence()
                self.assert_refused()

    def test_raw_observation_corruption_cannot_hide_behind_json_hash(self):
        share = Path(self.request["share_path"])
        (share / f"t17-network-{fixture.NONCE[:12]}.txt").write_bytes(b"forged")
        self.assert_refused()

    def test_missing_agent_result_refuses_complete_stage_flags(self):
        (self.evidence_path.parent / "product-e2e/agent-result.json").unlink()
        self.assert_refused()

    def test_mutation_log_shutdown_requires_host_terminal_report(self):
        path = self.evidence_path.parent / "product-e2e/mutation-run.log"
        data = path.read_bytes().replace(b"stop: PSCI 0x84000008 (system off)", b"stop: PSCI SYSTEM_OFF")
        path.write_bytes(data)
        self.evidence["mutation_run_log_sha256"] = hashlib.sha256(data).hexdigest()
        self.write_evidence()
        self.assert_refused()

    def test_host_audio_failure_refuses_guest_success_counter(self):
        path = self.evidence_path.parent / "product-e2e/first-run.log"
        data = path.read_bytes().replace(b"frames_rendered=480", b"frames_rendered=0")
        path.write_bytes(data)
        self.evidence["first_run_log_sha256"] = hashlib.sha256(data).hexdigest()
        self.write_evidence()
        self.assertEqual(self.evidence["audio_playback_count"], 1)
        self.assert_refused()


if __name__ == "__main__":
    unittest.main()
