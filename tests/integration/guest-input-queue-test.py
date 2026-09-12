#!/usr/bin/env python3
"""Queue seal and public receipt contracts, no physical VM execution."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import guest_input_queue as queue
import guest_input_queue_receipt as receipts
from guest_input_live_inputs import PROFILE, digest


class QueueContracts(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.directory = self.root / "job"
        self.directory.mkdir()
        self.commit = "a" * 40
        self.identity = dict(job_id="d5-test", tier="d5-guest-input", commit=self.commit,
                             input_manifest_sha256="b" * 64, sealed_binary_sha256="c" * 64)
        self.write_job()

    def write_job(self):
        (self.directory / "job.env").write_text("".join(k + "=" + v + "\n" for k, v in self.identity.items()))

    def test_missing_receipt_and_publish(self):
        receipts.finalize(self.directory, self.commit)
        receipts.publish(self.directory, self.commit)
        value = json.loads((self.directory / "receipt.public.json").read_text())
        self.assertEqual(value["reason"], "incomplete")
        self.assertFalse(value["passed"])
        self.assertFalse(value["claim_eligible"])
        with self.assertRaises(FileExistsError):
            receipts.publish(self.directory, self.commit)

    def test_claim_and_private_fields_refused(self):
        value = receipts.empty(self.identity, "incomplete")
        for key, bad in (("passed", True), ("claim_eligible", True), ("production_ui_proven", True),
                         ("guest_application_observed", True), ("commit", "d" * 40),
                         ("complete", 1), ("private_path", "/secret")):
            with self.assertRaises(ValueError):
                receipts.checked(dict(value, **{key: bad}), self.identity)

    def test_cancel_preserves_prior_receipt(self):
        value = receipts.empty(self.identity, "collected")
        value.update({key: True for key in receipts.BOOLS})
        receipts.write_new(self.directory / "receipt.json", value)
        (self.directory / "cancel.requested").touch()
        receipts.finalize(self.directory, self.commit)
        receipts.publish(self.directory, self.commit)
        self.assertEqual(json.loads((self.directory / "receipt.json").read_text())["reason"], "canceled")
        self.assertEqual(json.loads((self.directory / "receipt.before-finalize.json").read_text()), value)

    def test_invalid_receipt_preserved(self):
        (self.directory / "receipt.json").write_text('{"private_path":"/secret"}')
        receipts.finalize(self.directory, self.commit)
        self.assertEqual(json.loads((self.directory / "receipt.json").read_text())["reason"], "invalid-receipt")
        self.assertIn("private_path", (self.directory / "receipt.before-finalize.json").read_text())

    def test_duplicate_job_identity_refused(self):
        with (self.directory / "job.env").open("a") as stream:
            stream.write("commit=" + self.commit + "\n")
        with self.assertRaises(ValueError):
            receipts.job(self.directory, self.commit)

    def test_effective_queue_owned_assets(self):
        firmware = self.root / "crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd"
        firmware.parent.mkdir(parents=True)
        binary = self.directory / "hvf_gic_boot_probe"
        paths = {"firmware": firmware, "binary": binary, "image": self.root / "image", "vars": self.root / "vars"}
        for name, path in paths.items():
            with path.open("wb") as stream:
                stream.truncate(64 * 1024 * 1024 if name == "vars" else 512)
        value = {"schema": "bridgevm.guest-input-live.v1", "purpose": "diagnostic-only", "profile": PROFILE,
                 "assets": {name: {"path": str(path), "sha256": digest(path)} for name, path in paths.items()}}
        source = self.directory / "input-manifest.tsv"
        source.write_text(json.dumps(value))
        self.identity.update(input_manifest_sha256=digest(source), sealed_binary_sha256=digest(binary))
        self.write_job()
        identity, target = queue.effective(self.directory, self.root, self.commit, source, binary)
        self.assertEqual(identity, self.identity)
        effective = json.loads(target.read_text())
        self.assertEqual(effective["assets"]["binary"]["path"], str(binary))
        result = subprocess.check_output([sys.executable, str(ROOT / "scripts/live-gates/guest_input_queue.py"),
                                          "binary-hash", str(source)], text=True).strip()
        self.assertEqual(result, self.identity["sealed_binary_sha256"])
        binary.write_bytes(b"modified")
        with self.assertRaisesRegex(ValueError, "binary seal"):
            queue.effective(self.directory, self.root, self.commit, source, binary)


if __name__ == "__main__":
    unittest.main()
