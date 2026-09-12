#!/usr/bin/env python3
"""Deterministic restore-tier input seals; no guest disks or VM launches."""
import hashlib
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("inputs", ROOT / "scripts/live-gates/snapshot_restore_inputs.py")
INPUTS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(INPUTS)
COMMIT = "a" * 40


class RestoreSealContract(unittest.TestCase):
    def fixture(self, root):
        rows = []
        for key in ("image", "vars", "binary"):
            path = root / key
            path.write_bytes(key.encode())
            rows.append(f"{key}\t{path}\t{hashlib.sha256(key.encode()).hexdigest()}")
        rows += [f"binary_source_commit\t{COMMIT}", "binary_profile\trelease",
                 "binary_features\tvenus", "rust_toolchain\t1.97.0"]
        manifest = root / "manifest"
        manifest.write_text("\n".join(rows) + "\n")
        return manifest

    def test_strict_manifest_fields_and_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            data = self.fixture(Path(temporary)).read_bytes()
            self.assertEqual(len(INPUTS.parse_manifest(data, COMMIT)), 7)
            for damaged in (data + b"binary_profile\trelease\n", data + b"unknown\tx\n",
                            data.replace(b"\trelease\n", b"\tdebug\n"),
                            data.replace(COMMIT.encode(), b"b" * 40),
                            data.replace(b"vars\t", b"missing\t", 1), b"x" * 65537):
                with self.assertRaises(ValueError):
                    INPUTS.parse_manifest(damaged, COMMIT)

    def test_stages_only_matching_clones_and_records_hashes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = self.fixture(root)
            receipt = INPUTS.prepare(manifest, root / "binary", COMMIT, root / "staged", shutil.copyfile)
            self.assertEqual((root / "staged/disk.raw").read_bytes(), b"image")
            self.assertEqual((root / "staged/vars.fd").read_bytes(), b"vars")
            self.assertEqual(receipt["image_sha256"], hashlib.sha256(b"image").hexdigest())
            self.assertEqual((root / "image").read_bytes(), b"image")

    def test_corrupt_clone_and_wrong_binary_are_refused(self):
        for corrupt_binary in (False, True):
            with tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                manifest = self.fixture(root)
                if corrupt_binary:
                    (root / "binary").write_bytes(b"wrong")
                def corrupt(source, target):
                    Path(target).write_bytes(b"corrupt")
                with self.assertRaises(ValueError):
                    INPUTS.prepare(manifest, root / "binary", COMMIT, root / "stage", corrupt)

    def test_symlink_and_existing_stage_are_refused(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = self.fixture(root)
            (root / "linked").symlink_to(root / "binary")
            with self.assertRaises(ValueError):
                INPUTS.prepare(manifest, root / "linked", COMMIT, root / "stage", shutil.copyfile)
            (root / "stage").mkdir()
            with self.assertRaises(FileExistsError):
                INPUTS.prepare(manifest, root / "binary", COMMIT, root / "stage", shutil.copyfile)

    def test_queue_requires_manifest_before_creating_job(self):
        with tempfile.TemporaryDirectory() as temporary:
            result = subprocess.run(["bash", str(ROOT / "scripts/live-gates/bridgevm-live"),
                                     "submit", "t1-restore-boot"], capture_output=True, text=True,
                                    env=dict(os.environ, BRIDGEVM_LIVE_ROOT=temporary), timeout=5)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("needs --input-manifest", result.stderr)
            self.assertEqual(list(Path(temporary).iterdir()), [])


if __name__ == "__main__":
    unittest.main()
