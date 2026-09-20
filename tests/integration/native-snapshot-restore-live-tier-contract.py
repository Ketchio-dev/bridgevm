#!/usr/bin/env python3
"""Deterministic contracts for the native app snapshot restore live tier."""
from __future__ import annotations

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
def module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value
INPUTS = module("native_inputs", ROOT / "scripts/live-gates/native_snapshot_restore_inputs.py")
RECEIPT = module("native_receipt", ROOT / "scripts/live-gates/native_snapshot_restore_receipt.py")
REDACTOR = module("receipt_redactor", ROOT / "scripts/live-gates/redact-receipt.py")
COMMIT = "a" * 40
class NativeSnapshotRestoreTierContract(unittest.TestCase):
    def fixture(self, root: Path, commit: str = COMMIT) -> tuple[Path, Path]:
        app = root / "BridgeVM.app"
        files = {
            "app_cli": app / "Contents/Resources/target/release/bridgevm",
            "app_executable": app / "Contents/MacOS/BridgeVMControl",
            "snapshot_helper": app / "Contents/Resources/target/release/examples/snapshot_pair_cli",
            "binary": app / "Contents/Resources/target/release/examples/hvf_gic_boot_probe",
        }
        for key, path in files.items():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(key.encode())
            path.chmod(0o700)
        artifacts = {"app_bundle": app, **files,
                     "image": root / "image.raw", "vars": root / "vars.fd"}
        for key in ("image", "vars"):
            artifacts[key].write_bytes(key.encode())
        manifest = root / "manifest.tsv"
        rows = []
        for key, path in artifacts.items():
            digest = INPUTS.tree_hash(path) if key == "app_bundle" else INPUTS.digest(path)
            rows.append(f"{key}\t{path}\t{digest}")
        rows.extend((f"source_commit\t{commit}", "app_profile\trelease",
                     "binary_profile\trelease", "binary_features\tvenus",
                     "rust_toolchain\t1.97.0"))
        manifest.write_text("\n".join(rows) + "\n")
        return manifest, artifacts["binary"]

    def test_manifest_authenticates_app_relations_and_cloned_pair(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest, binary = self.fixture(root)
            public, private = INPUTS.prepare(
                manifest, binary, COMMIT, root / "prepared", shutil.copyfile, shutil.copytree)
            self.assertEqual(public["image_sha256"], hashlib.sha256(b"image").hexdigest())
            self.assertEqual((root / "prepared/disk.raw").read_bytes(), b"image")
            self.assertEqual(Path(private["app_cli"]), root / "prepared/BridgeVM.app/Contents/Resources/target/release/bridgevm")
            self.assertEqual(Path(private["binary"]), root / "prepared/BridgeVM.app/Contents/Resources/target/release/examples/hvf_gic_boot_probe")
            INPUTS.authenticate(private["source_rows"], binary)
            INPUTS.authenticate_app(private["source_rows"], Path(private["sealed_app"]))
            Path(private["binary"]).write_bytes(b"changed")
            with self.assertRaises(ValueError):
                INPUTS.authenticate_app(private["source_rows"], Path(private["sealed_app"]))

    def test_mutation_and_relation_alias_fail_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest, binary = self.fixture(root)
            data = manifest.read_bytes()
            rows = INPUTS.parse_manifest(data, COMMIT)
            Path(rows["app_cli"][0]).write_bytes(b"changed")
            with self.assertRaises(ValueError):
                INPUTS.authenticate(rows, binary)
            damaged = data.replace(b"Contents/Resources/target/release/bridgevm",
                                   b"Contents/MacOS/other")
            with self.assertRaises(ValueError):
                INPUTS.parse_manifest(damaged, COMMIT)

    def test_passing_receipt_requires_three_shutdowns_and_all_hashes(self):
        value = RECEIPT.initial("native-fixture", COMMIT)
        for field in RECEIPT.HASHES:
            value[field] = "b" * 64
        value.update({"finished_at": "done", "host_model": "Mac17,9",
                      "macos_version": "26.0", "outcome": "completed", "pass": True,
                      "boots_attempted": 3, "boots_passed": 3,
                      "natural_shutdown_count": 3, "run_count": 1,
                      "worker_cleanup_verified": True})
        self.assertTrue(RECEIPT.validate(value, COMMIT)["pass"])
        public = REDACTOR.redact(value)
        self.assertEqual(RECEIPT.validate(public, COMMIT), value)
        for mutation in ({**value, "natural_shutdown_count": 2},
                         {**value, "three_d_injection": True},
                         {**value, "snapshot_restore_result_sha256": "absent"}, {**value, "snapshot_export_result_sha256": "absent"}):
            with self.assertRaises(ValueError):
                RECEIPT.validate(mutation, COMMIT)

    def test_queue_seals_manifest_and_probe_for_new_tier(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            head = subprocess.check_output(
                ["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True).strip()
            manifest, _ = self.fixture(root, head)
            queue = root / "queue"
            result = subprocess.run(
                ["bash", str(ROOT / "scripts/live-gates/bridgevm-live"), "submit",
                 RECEIPT.TIER, "--sha", head,
                 "--input-manifest", str(manifest), "--job-id", "native-fixture"],
                capture_output=True, text=True,
                env=dict(os.environ, BRIDGEVM_LIVE_ROOT=str(queue)), timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            staged = queue / "queued/native-fixture"
            self.assertTrue((staged / "input-manifest.tsv").is_file())
            self.assertTrue((staged / "hvf_gic_boot_probe").is_file())
            job = (staged / "job.env").read_text()
            self.assertRegex(job, r"input_manifest_sha256=[0-9a-f]{64}")
            self.assertRegex(job, r"sealed_binary_sha256=[0-9a-f]{64}")

    def test_marker_gate_routes_both_native_operations_through_app_cli(self):
        source = (ROOT / "scripts/verify-native-snapshot-restore-boots.sh").read_text()
        self.assertIn('"$NATIVE_SNAPSHOT_CLI" app snapshot-create', source)
        self.assertIn('"$NATIVE_SNAPSHOT_CLI" app snapshot-restore', source); self.assertIn("native_snapshot_export_and_select", source)
        self.assertIn('"experimental3DAllowed": False', source)
        self.assertIn('cp -c "$VARS"', source)
        self.assertIn("set -euo pipefail", source); self.assertNotIn('"unavailableReason":None', source)


if __name__ == "__main__":
    unittest.main()
