#!/usr/bin/env python3
"""D11 publication keeps stages, cancellation and cleanup evidence distinct."""
import copy
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from d11_fixture_test_support import COMMIT, HASH, binding, rows
from d11_fixture_files import record, identity
from d11_fixture_receipt import empty, checked, collect, guard, publish


class Receipts(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.job = self.root / "job"; self.job.mkdir()
        self.output = self.root / "output"; self.output.mkdir()
        self.binding = binding()
        self.core = {"schema": "bridgevm.d11-fixture-private.v1", "job_id": self.binding["job_id"],
            "commit": COMMIT, "input_manifest_sha256": HASH, "binary_sha256": HASH,
            "output_identity": identity(self.output.lstat())[:2], "t15_ready": False,
            "installed": True, "ready_stopped": True, "sealed_fixture": True,
            "cleanup_verified": True, "failure": "none", "processes": [
                {"pid": 1234, "terminal_observed": True, "exit_code": 0, "group_absent": True}],
            **{k: HASH for k in ("disk_sha256", "vars_sha256", "container_sha256", "guest_result_sha256", "source_sha256")}}
        self.backing = self.output / "fixture.sparseimage"
        self.backing.write_bytes(b"synthetic backing, no media"); self.backing.chmod(0o400)
        self.core["backing_identity"] = identity(self.backing.lstat())
        self.contexts = [patch("d11_fixture_receipt.bound", return_value=(self.binding, rows(self.root))),
                         patch("d11_fixture_receipt.output_path", return_value=self.output),
                         patch("d11_fixture_receipt.inventory", return_value=[]),
                         patch("d11_fixture_receipt.state", return_value="absent")]
        for context in self.contexts:
            context.start(); self.addCleanup(context.stop)

    def store(self): record(self.output / "preparation.private.json", self.core)

    def test_success_remains_nonpromoting_and_not_t15_ready(self):
        self.store(); value = collect(self.job, COMMIT)
        self.assertTrue(value["sealed_fixture"])
        for key in ("t15_ready", "pass", "claim_eligible", "criterion_pass"): self.assertIs(value[key], False)
        self.assertNotIn("backing_identity", value)
        self.assertNotIn("processes", value)

    def test_unknown_public_field_refuses_secret_or_path(self):
        value = empty(self.binding, "incomplete")
        for key in ("password", "output_path", "raw_log"):
            with self.assertRaises(ValueError): checked({**value, key: "private"}, self.binding)

    def test_unordered_or_unproved_stages_refused(self):
        for changes in ({"ready_stopped": True}, {"sealed_fixture": True}, {"t15_ready": True}, {"pass": True}):
            with self.assertRaises(ValueError): checked({**empty(self.binding, "incomplete"), **changes}, self.binding)

    def test_partial_install_and_clean_failure_preserved(self):
        self.core.update(ready_stopped=False, sealed_fixture=False, failure="incomplete")
        self.store(); value = collect(self.job, COMMIT)
        self.assertTrue(value["installed"]); self.assertFalse(value["ready_stopped"])
        self.assertTrue(value["worker_cleanup_verified"]); self.assertEqual(value["reason"], "incomplete")

    def test_cancel_does_not_rewrite_sealed_output_as_success(self):
        self.store(); (self.job / "cancel.requested").touch()
        value = collect(self.job, COMMIT)
        self.assertEqual(value["reason"], "canceled"); self.assertFalse(value["sealed_fixture"])
        self.assertTrue(value["worker_cleanup_verified"])

    def test_live_group_or_mount_fences(self):
        self.store()
        with patch("d11_fixture_receipt.state", return_value="denied"):
            value = collect(self.job, COMMIT)
        self.assertFalse(value["worker_cleanup_verified"])
        self.assertEqual(value["reason"], "cleanup-unproved")
        with patch("d11_fixture_receipt.inventory", return_value=[{"image-path": str(self.backing)}]):
            self.assertFalse(collect(self.job, COMMIT)["worker_cleanup_verified"])

    def test_preparation_from_other_job_refused(self):
        self.core["job_id"] = "other"; self.store()
        with self.assertRaises(ValueError): collect(self.job, COMMIT)

    def test_backing_change_refuses_publication(self):
        self.store(); self.backing.chmod(0o600); self.backing.write_bytes(b"changed")
        with self.assertRaises(ValueError): collect(self.job, COMMIT)

    def test_archival_facts_survive_later_owned_backing_retirement(self):
        self.store(); before = collect(self.job, COMMIT, current=False)
        self.backing.unlink()
        self.assertEqual(collect(self.job, COMMIT, current=False), before)
        with self.assertRaises(FileNotFoundError): collect(self.job, COMMIT)

    def test_integrity_refusal_prevents_recovering_success_from_old_core(self):
        self.store(); record(self.job / "d11-refusal.private.json", {"reason": "integrity"})
        value = collect(self.job, COMMIT)
        self.assertFalse(value["sealed_fixture"]); self.assertFalse(value["worker_cleanup_verified"])

    def test_publication_copies_only_exact_allowlist(self):
        self.store(); value = collect(self.job, COMMIT)
        record(self.job / "receipt.json", value)
        publish(self.job, COMMIT)
        raw = (self.job / "receipt.public.json").read_text()
        self.assertNotIn(str(self.root), raw)
        self.assertNotIn("pid", raw)
        self.assertEqual(set(__import__("json").loads(raw)), set(empty(self.binding, "incomplete")))


if __name__ == "__main__": unittest.main()
