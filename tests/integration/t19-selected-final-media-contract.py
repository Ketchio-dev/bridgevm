#!/usr/bin/env python3
"""T19 authenticates final selected generations, rejecting stale logical originals."""
from __future__ import annotations

import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

import product_e2e_identity_fixtures as fixture
from windows_selected_media_cli import CLI, selected


class SelectedImportMedia(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.cli = Path(subprocess.check_output(
            ["bash", str(fixture.ROOT / "scripts/prepare-hvf-import-test-helper.sh")],
            cwd=fixture.ROOT, text=True).strip())

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bridgevm-import-e2e-selected-", dir="/private/tmp")
        self.root = Path(self.temporary.name).resolve() / "lane-1"
        self.request_path, self.result_path = fixture.import_request(self.root)
        self.request = json.loads(self.request_path.read_text())
        self.result = json.loads(self.result_path.read_text())
        helper = Path(self.request["app_bundle_path"]) / CLI
        shutil.copyfile(self.cli, helper); helper.chmod(0o700)
        disk, variables = map(Path, (self.request["disk_path"], self.request["vars_path"]))
        with variables.open("r+b") as output:
            output.truncate(64 * 1024 * 1024)
        self.snapshot = Path(self.request["snapshot_path"])
        self.snapshot.parent.mkdir(parents=True)
        self.call("create", disk, variables, self.snapshot, self.request["vm_slug"], 70 * 1024 * 1024)
        disk.write_bytes(b"mutated logical original")
        with variables.open("r+b") as output:
            output.write(b"mutated logical vars")
        self.call("restore", self.snapshot, disk, variables)
        self.originals = {"final_disk_sha256": fixture.T19.digest(disk),
                          "final_vars_sha256": fixture.T19.digest(variables)}
        self.pair = selected(self.request)
        self.stamp = self.root.parent / "stamp.json"

    def tearDown(self):
        for path in self.root.parent.rglob("*"):
            if not path.is_symlink():
                path.chmod(0o700 if path.is_dir() else 0o600)
        self.temporary.cleanup()

    def call(self, *arguments):
        subprocess.run([str(self.cli), *map(str, arguments)], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)

    def audit(self, final_hashes):
        self.result_path.write_text(json.dumps({**self.result, **final_hashes}))
        fixture.T19.authenticate(self.request_path, self.result_path, self.stamp,
                                 fixture.JOB, fixture.COMMIT, "pilot", 1,
                                 fixture.T19.digest(self.request_path))

    def test_stale_original_hashes_refused_selected_hashes_authenticated(self):
        expected = {"final_disk_sha256": self.pair["disk_sha256"],
                    "final_vars_sha256": self.pair["vars_sha256"]}
        self.assertNotEqual(self.originals["final_disk_sha256"], expected["final_disk_sha256"])
        self.assertNotEqual(self.originals["final_vars_sha256"], expected["final_vars_sha256"])
        with self.assertRaisesRegex(ValueError, "artifacts differ"):
            self.audit(self.originals)
        self.assertFalse(self.stamp.exists())
        self.audit(expected)
        self.assertTrue(self.stamp.is_file())
        for field, path in (("final_disk_sha256", "disk_path"), ("final_vars_sha256", "vars_path")):
            self.assertEqual(fixture.T19.digest(Path(self.request[path])), self.originals[field])

    def test_missing_or_symlinked_packaged_helper_cannot_authenticate(self):
        helper = Path(self.request["app_bundle_path"]) / CLI
        helper.unlink()
        expected = {"final_disk_sha256": self.pair["disk_sha256"],
                    "final_vars_sha256": self.pair["vars_sha256"]}
        for symlink in (False, True):
            if symlink:
                helper.symlink_to(self.cli)
            with self.assertRaisesRegex(ValueError, "missing or unsafe"):
                self.audit(expected)
            self.assertFalse(self.stamp.exists())


if __name__ == "__main__":
    unittest.main()
