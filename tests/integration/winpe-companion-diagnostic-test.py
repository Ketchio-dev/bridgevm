#!/usr/bin/env python3
"""Deterministic D4 boundaries; no VM or Windows boot is simulated as proof."""
from winpe_companion_policy_cases import assert_vars_sizes, assert_wrapper_policy
import importlib.util
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import winpe_companion_inputs as inputs
import winpe_companion_receipt as receipts
from winpe_companion_inspect import compare
spec = importlib.util.spec_from_file_location("winpe_runner", ROOT / "scripts/live-gates/run-winpe-companions.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class DiagnosticTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.job = dict(tier=receipts.TIER, job_id="d4-fixture", commit="a" * 40,
                        input_manifest_sha256="b" * 64)

    def test_claim_flags_and_count_refuse_promotion(self):
        value = receipts.initial(self.job)
        receipts.validate(value, self.job)
        for key in receipts.FLAGS:
            bad = dict(value, **{key: True})
            with self.assertRaises(ValueError): receipts.validate(bad, self.job)
        for change in ({"sample_count": True}, {"passes": 1}, {"outcome": "diagnostic-complete"},
                       {"commit": "c" * 40}, {"tier": "t17-windows-hvf-product-e2e"}):
            with self.assertRaises(ValueError): receipts.validate(dict(value, **change), self.job)

    def test_complete_nonzero_process_and_matching_files_are_not_boot_proof(self):
        value = dict(receipts.initial(self.job), outcome="diagnostic-complete", sample_count=1,
                     execution_exit_code=42, source_integrity_verified=True, post_files_match=True)
        receipts.validate(value, self.job)
        self.assertTrue(all(value[key] is False for key in receipts.FLAGS))
        known = {"available": True, "files": {"a": "digest"}}
        self.assertEqual(compare(known, known, known["files"]), dict(post_files_match=True,
                         files_changed=False, pre_post_available=True, winpe_boot_proven=False))
        self.assertFalse(compare({"available": False}, known, known["files"])["files_changed"])

    def test_finalize_cancel_and_publication_drop_private_data(self):
        (self.root / "job.env").write_text("".join(k + "=" + v + "\n" for k, v in self.job.items()))
        receipts.finalize(self.root, self.job["commit"])
        value = receipts.initial(self.job)
        value["private_path"] = "/private/windows.img"
        receipts.write(self.root / "receipt.json", value)
        (self.root / "cancel.requested").touch()
        receipts.finalize(self.root, self.job["commit"], True)
        public = json.loads((self.root / "receipt.public.json").read_text())
        self.assertEqual(public["outcome"], "canceled")
        self.assertNotIn("private_path", public)
        with self.assertRaises(FileExistsError): receipts.finalize(self.root, self.job["commit"], True)

    def fixture(self):
        records = {}
        for index, key in enumerate(inputs.ASSETS):
            path = self.root / (inputs.FIRMWARE if key == "firmware" else key)
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(bytes([index + 1]) * (64 * 1024 * 1024 if key == "vars" else 4096))
            records[key] = path, inputs.file_hash(path)
        text = "".join(k + "\t" + v + "\n" for k, v in inputs.FIXED.items())
        text += "".join(k + "\t" + str(p) + "\t" + h + "\n" for k, (p, h) in records.items())
        manifest = self.root / "manifest.tsv"
        manifest.write_text(text)
        return manifest, records, text

    def test_manifest_refuses_duplicates_changes_aliases_and_wrong_profile(self):
        manifest, records, text = self.fixture()
        self.assertEqual(inputs.load(manifest, self.root, records["binary"][0]), records)
        for broken in (text + "purpose\tdiagnostic-only\n", text.replace("no-3d-winpe-300s", "gpu"),
                       text.replace("image\t", "unknown\t"), text.replace(records["vars"][1], "f" * 64)):
            manifest.write_text(broken)
            with self.assertRaises(ValueError): inputs.load(manifest, self.root, records["binary"][0])
        # D4 uses full-slot pflash inputs, not every native loader input shape.
        assert_vars_sizes(self, records)

    def test_clone_separation_and_fixed_no_3d_command(self):
        _, records, _ = self.fixture()
        def fake_copy(command, check):
            self.assertEqual(command[:2], ["cp", "-c"])
            shutil.copyfile(command[2], command[3])
        clones = inputs.clone(records, self.root / "work", fake_copy)
        for key in inputs.MEDIA:
            self.assertNotEqual(clones[key].stat().st_ino, records[key][0].stat().st_ino)
        command = runner.command(ROOT, records, clones, self.root / "evidence")
        self.assertEqual(command[command.index("--watchdog-ms") + 1], "300000")
        self.assertEqual(command[command.index("--placeholder-nsid1") + 1], str(clones["injector"]))
        self.assertNotIn("--virtio-gpu-3d", command)
        self.assertNotIn("--enable-xhci", command)
        assert_wrapper_policy(self, command)


if __name__ == "__main__":
    unittest.main()
