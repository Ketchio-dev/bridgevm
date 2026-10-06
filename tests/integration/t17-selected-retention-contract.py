#!/usr/bin/env python3
"""Exercise T17 retention against the real helper after a managed-pair restore."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
from retained_windows_media import export_selected
from windows_selected_media_cli import CLI, selected

spec = importlib.util.spec_from_file_location("retention", ROOT / "scripts/live-gates/retain-windows-import-source.py")
retention = importlib.util.module_from_spec(spec)
spec.loader.exec_module(retention)


class SelectedRetention(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.cli = Path(subprocess.check_output(["bash", str(ROOT / "scripts/prepare-hvf-import-test-helper.sh")], cwd=ROOT, text=True).strip())

    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="bridgevm-selected-retention-")
        self.root = Path(self.scratch.name).resolve()
        self.app = self.root / "Fixture.app"
        helper = self.app / CLI
        helper.parent.mkdir(parents=True)
        shutil.copyfile(self.cli, helper); helper.chmod(0o700)
        self.disk = self.root / "logical.raw"; self.disk.write_bytes(b"authenticated snapshot disk")
        self.variables = self.root / "logical.fd"
        with self.variables.open("wb") as output:
            output.write(b"authenticated snapshot vars"); output.truncate(64 * 1024 * 1024)
        self.snapshot = self.root / "snapshot"
        self.call("create", self.disk, self.variables, self.snapshot, "fixture-vm", 70 * 1024 * 1024)
        self.disk.write_bytes(b"mutated original disk")
        with self.variables.open("r+b") as output:
            output.write(b"mutated original vars")
        self.call("restore", self.snapshot, self.disk, self.variables)
        self.request = {"app_bundle_path": str(self.app), "disk_path": str(self.disk), "vars_path": str(self.variables), "vm_slug": "fixture-vm"}
        self.expected = {"final_disk_sha256": retention.T19.file_hash(self.snapshot / "disk.raw"), "final_vars_sha256": retention.T19.file_hash(self.snapshot / "vars.fd")}

    def tearDown(self):
        for path in self.root.rglob("*"):
            if not path.is_symlink():
                path.chmod(0o700 if path.is_dir() else 0o600)
        self.scratch.cleanup()

    def call(self, *arguments):
        subprocess.run([str(self.cli), *map(str, arguments)], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)

    def test_export_retains_selected_generation_and_preserves_originals(self):
        before = tuple(retention.T19.file_hash(path) for path in (self.disk, self.variables))
        staging = self.root / "staging"; staging.mkdir()
        disk, variables = export_selected(self.request, self.expected, staging)
        self.assertEqual(disk.read_bytes(), b"authenticated snapshot disk")
        self.assertEqual(retention.T19.file_hash(variables), self.expected["final_vars_sha256"])
        self.assertEqual(tuple(retention.T19.file_hash(path) for path in (self.disk, self.variables)), before)
        self.assertNotEqual(before[0], self.expected["final_disk_sha256"])

    def test_wrong_authenticated_hash_refuses_before_export(self):
        staging = self.root / "staging"; staging.mkdir()
        for field in self.expected:
            with self.subTest(field=field), self.assertRaises(ValueError):
                export_selected(self.request, {**self.expected, field: "0" * 64}, staging)
            self.assertEqual(list(staging.iterdir()), [])

    def test_retained_t19_manifest_verifies_and_existing_source_stays_intact(self):
        executable = self.app / "Contents/MacOS/BridgeVMControl"; executable.parent.mkdir()
        executable.write_text('#!/bin/sh\nshift 2\nwhile [ "$#" -gt 0 ]; do case "$1" in --package) printf "{}\\n" > "$2";; --recovery-code-file) printf "fixture\\n" > "$2"; chmod 600 "$2";; esac; shift 2; done\n'); executable.chmod(0o700)
        product_helper = self.app / "Contents/Helpers/BridgeVMProductE2E.app/Contents/MacOS/BridgeVMProductE2E"
        product_helper.parent.mkdir(parents=True); product_helper.write_bytes(b"fixture helper")
        runner = self.app / "Contents/Resources/target/release/hvf-runner"; runner.write_bytes(b"fixture runner")
        state = self.root / "vtpm"; state.mkdir(); (state / "state.bin").write_bytes(b"synthetic private state")
        request = {**self.request, "app_executable_path": str(executable), "runner_path": str(runner), "vtpm_state_path": str(state), "job_id": "fixture", "commit": "a" * 40, "campaign_mode": "pilot", "lane": 1}
        app_paths = {"app_bundle": self.app, "app_executable": executable, "product_helper": product_helper, "runner": runner}
        verified = {"verified": True, "campaign_mode": "pilot", "assets": {key: {"path": str(path), "sha256": retention.T19.tree_hash(path, allow_symlinks=True) if key == "app_bundle" else retention.T19.file_hash(path)} for key, path in app_paths.items()}}
        args = argparse.Namespace(**{key: self.root / f"{key}.json" for key in ("request", "result", "stamp", "verified", "status")}, destination=self.root / "retained/source")
        args.request.write_text(json.dumps(request)); args.verified.write_text(json.dumps(verified))
        args.stamp.write_text(json.dumps({"request_sha256": retention.T17.digest(args.request)}))
        result = {**self.expected, **{stage: True for stage in retention.T17.LANE_STAGES}, "cleanup_verified": True}
        with patch.object(retention.T17, "lane", return_value=result):
            retention.retain(args)
            manifest = args.destination / "t19-input-manifest.tsv"
            self.assertTrue(retention.T19.verify(manifest)["verified"])
            self.assertEqual(args.destination.stat().st_mode & 0o777, 0o500)
            retained_disk = args.destination / "selected-media/disk.raw"
            self.assertEqual(retained_disk.stat().st_mode & 0o777, 0o400)
            before = retention.T19.file_hash(manifest)
            with self.assertRaisesRegex(ValueError, "already exists"):
                retention.retain(args)
            self.assertEqual(retention.T19.file_hash(manifest), before)
            self.assertTrue(json.loads(args.status.read_text())["verified"])

    def test_digest_protocol_rejects_invalid_sizes_and_unsafe_helper(self):
        valid = "disk_bytes 1\ndisk_sha256 " + "a" * 64 + "\nvars_bytes 2\nvars_sha256 " + "b" * 64 + "\n"
        for bad in ("0", "-1", "1e2", "01", str(2**64)):
            with self.subTest(size=bad), patch("windows_selected_media_cli.run", return_value=valid.replace("disk_bytes 1\n", f"disk_bytes {bad}\n")), self.assertRaises(ValueError):
                selected(self.request)
        helper = self.app / CLI; helper.unlink(); helper.symlink_to(self.cli)
        with self.assertRaisesRegex(ValueError, "unsafe"):
            selected(self.request)


if __name__ == "__main__":
    unittest.main()
