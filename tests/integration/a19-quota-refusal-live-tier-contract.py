#!/usr/bin/env python3
"""Deterministic queue, runner, receipt and cleanup contracts for A19 T21."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import a19_quota_refusal_receipt as receipt
import native_snapshot_restore_inputs as inputs

COMMIT = subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True).strip()
HELPER = '''#!/usr/bin/env python3
import hashlib, json, pathlib, shutil, sys
mode = "MODE"
verb, *args = sys.argv[1:]
if verb == "create":
    disk, vars, destination, vm_id, quota = args
    pair = pathlib.Path(disk).stat().st_size + pathlib.Path(vars).stat().st_size
    if pair > int(quota):
        if mode == "mutate-input":
            pathlib.Path(disk).write_bytes(b"changed")
        text = f"snapshot would write {pair} bytes, over the {quota} byte quota"
        if mode == "wrong-message": text = "wrong quota result"
        print(text, file=sys.stderr)
        sys.exit(1)
    root = pathlib.Path(destination)
    root.mkdir()
    shutil.copyfile(disk, root / "disk.raw")
    shutil.copyfile(vars, root / "vars.fd")
    digest = lambda path: hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()
    manifest = {"format_version": 1, "vm_id": vm_id,
                "disk_bytes": pathlib.Path(disk).stat().st_size,
                "disk_sha256": digest(disk),
                "vars_bytes": pathlib.Path(vars).stat().st_size,
                "vars_sha256": digest(vars)}
    (root / "manifest.json").write_text(json.dumps(manifest))
    print("created")
elif verb == "verify":
    root = pathlib.Path(args[0])
    manifest = json.loads((root / "manifest.json").read_text())
    for name, key in (("disk.raw", "disk"), ("vars.fd", "vars")):
        path = root / name
        assert hashlib.sha256(path.read_bytes()).hexdigest() == manifest[key + "_sha256"]
        assert path.stat().st_size == manifest[key + "_bytes"]
    print("verified")
else:
    sys.exit(2)
'''


class QuotaTierContract(unittest.TestCase):
    def fixture(self, root: Path, mode: str = "normal") -> tuple[Path, Path, Path]:
        app = root / "BridgeVM.app"
        files = {
            "app_cli": app / "Contents/Resources/target/release/bridgevm",
            "app_executable": app / "Contents/MacOS/BridgeVMControl",
            "snapshot_helper": app / "Contents/Resources/target/release/examples/snapshot_pair_cli",
            "binary": app / "Contents/Resources/target/release/examples/hvf_gic_boot_probe",
        }
        for key, path in files.items():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(HELPER.replace("MODE", mode) if key == "snapshot_helper" else key)
            path.chmod(0o700)
        artifacts = {"app_bundle": app, **files, "image": root / "image.raw", "vars": root / "vars.fd"}
        artifacts["image"].write_bytes(b"installed-Windows-fixture")
        artifacts["vars"].write_bytes(b"UEFI-fixture")
        manifest = root / "manifest.tsv"
        rows = []
        for key, path in artifacts.items():
            digest = inputs.tree_hash(path) if key == "app_bundle" else inputs.digest(path)
            rows.append(f"{key}\t{path}\t{digest}")
        rows.extend((f"source_commit\t{COMMIT}", "app_profile\trelease",
                     "binary_profile\trelease", "binary_features\tvenus",
                     "rust_toolchain\t1.97.0"))
        manifest.write_text("\n".join(rows) + "\n")
        return manifest, files["binary"], app

    def submit(self, root: Path, manifest: Path) -> Path:
        queue = root / "queue"
        result = subprocess.run(
            ["bash", str(ROOT / "scripts/live-gates/bridgevm-live"), "submit", receipt.TIER,
             "--sha", COMMIT, "--input-manifest", str(manifest), "--job-id", "quota-fixture"],
            capture_output=True, text=True,
            env=dict(os.environ, BRIDGEVM_LIVE_ROOT=str(queue)), timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        job = queue / "queued/quota-fixture"
        self.assertTrue((job / "input-manifest.tsv").is_file())
        self.assertTrue((job / "hvf_gic_boot_probe").is_file())
        return job

    def run_tier(self, job: Path) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(ROOT / "scripts/live-gates/run-a19-quota-refusal-tier.py"),
             str(job), "quota-fixture", str(job / "input-manifest.tsv"),
             str(job / "hvf_gic_boot_probe")], capture_output=True, text=True, timeout=120,
        )

    def fence(self, job: Path) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["bash", "-c", 'source "$REPO/scripts/live-gates/t17-worker-cleanup-fence.sh"; bridgevm_t17_guard_or_fence "$TIER" "$JOB" "$REPO" "$COMMIT" quota-fixture "$QUEUE"'],
            capture_output=True, text=True,
            env=dict(os.environ, REPO=str(ROOT), TIER=receipt.TIER, JOB=str(job),
                     COMMIT=COMMIT, QUEUE=str(job.parent.parent)), timeout=30,
        )

    def test_sealed_quota_boundary_publishes_path_free_receipt_and_fences_residue(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest, _, _ = self.fixture(root)
            job = self.submit(root, manifest)
            result = self.run_tier(job)
            self.assertEqual(result.returncode, 0, result.stderr)
            private = receipt.validate(json.loads((job / "receipt.json").read_text()), COMMIT)
            receipt.validate_seal(private, job)
            self.assertEqual((private["quota_case_count"], private["sample_count"]), (1, 0))
            self.assertEqual(private["refusal_output_sha256"],
                             hashlib.sha256(receipt.quota_error(private["pair_bytes"])).hexdigest())
            self.assertEqual(self.fence(job).returncode, 0)
            published = subprocess.run(
                ["bash", str(ROOT / "scripts/live-gates/publish-receipt.sh"),
                 receipt.TIER, str(job), str(ROOT), COMMIT], capture_output=True, text=True,
            )
            self.assertEqual(published.returncode, 0, published.stderr)
            public = json.loads((job / "receipt.public.json").read_text())
            self.assertEqual(private, public)
            (job / "prepared-inputs").mkdir()
            self.assertEqual(self.fence(job).returncode, 126)
            self.assertTrue((job.parent.parent / "worker-cleanup-required").is_file())

    def test_wrong_error_and_modified_media_cannot_pass(self):
        for mode in ("wrong-message", "mutate-input"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                manifest, _, _ = self.fixture(root, mode)
                job = self.submit(root, manifest)
                self.assertEqual(self.run_tier(job).returncode, 1)
                value = receipt.validate(json.loads((job / "receipt.json").read_text()), COMMIT)
                self.assertFalse(value["pass"])
                self.assertEqual(value["quota_case_count"], 0)
                self.assertTrue(value["worker_cleanup_verified"])
                self.assertEqual(self.fence(job).returncode, 0)

    def test_receipt_arithmetic_seal_and_missing_cleanup_fail_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest, _, _ = self.fixture(root)
            job = self.submit(root, manifest)
            self.assertEqual(self.run_tier(job).returncode, 0)
            value = json.loads((job / "receipt.json").read_text())
            for field, wrong in (("rejected_quota_bytes", value["pair_bytes"]),
                                 ("quota_case_count", True), ("snapshot_vars_sha256", "a" * 64),
                                 ("claim_eligible", True)):
                with self.subTest(field=field):
                    changed = {**value, field: wrong}
                    with self.assertRaises(ValueError):
                        receipt.validate(changed, COMMIT)
            ledger = job.parent.parent / "job-ledger/quota-fixture/entry.env"
            ledger.chmod(0o600)
            ledger.write_text(ledger.read_text().replace("input_manifest_sha256=", "broken_input_manifest_sha256="))
            with self.assertRaises(ValueError):
                receipt.validate_seal(value, job)
            self.assertEqual(self.fence(job).returncode, 126)
            self.assertNotEqual(subprocess.run(
                ["bash", str(ROOT / "scripts/live-gates/publish-receipt.sh"),
                 receipt.TIER, str(job), str(ROOT), COMMIT], capture_output=True,
            ).returncode, 0)

    def test_missing_receipt_is_failing_and_cannot_release_private_media(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest, _, _ = self.fixture(root)
            job = self.submit(root, manifest)
            missing = subprocess.run(
                ["bash", str(ROOT / "scripts/live-gates/write-missing-receipt.sh"),
                 receipt.TIER, str(job), str(ROOT), "quota-fixture", COMMIT],
                capture_output=True, text=True,
            )
            self.assertEqual(missing.returncode, 0, missing.stderr)
            value = receipt.validate(json.loads((job / "receipt.json").read_text()), COMMIT)
            self.assertFalse(value["pass"])
            self.assertFalse(value["worker_cleanup_verified"])
            self.assertEqual(self.fence(job).returncode, 126)
            self.assertNotEqual(subprocess.run(
                ["bash", str(ROOT / "scripts/live-gates/publish-receipt.sh"),
                 receipt.TIER, str(job), str(ROOT), COMMIT], capture_output=True,
            ).returncode, 0)
            self.assertFalse((job / "receipt.public.json").exists())

    def test_wrong_source_manifest_is_refused_before_queueing(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest, _, _ = self.fixture(root)
            manifest.write_text(manifest.read_text().replace(COMMIT, "f" * 40))
            result = subprocess.run(
                ["bash", str(ROOT / "scripts/live-gates/bridgevm-live"), "submit", receipt.TIER,
                 "--sha", COMMIT, "--input-manifest", str(manifest), "--job-id", "bad-source"],
                capture_output=True, text=True,
                env=dict(os.environ, BRIDGEVM_LIVE_ROOT=str(root / "queue")), timeout=30,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((root / "queue/queued/bad-source").exists())

    def test_bounded_seal_reads_reject_symlink_oversize_and_replacement(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest, _, _ = self.fixture(root)
            job = self.submit(root, manifest)
            self.assertEqual(self.run_tier(job).returncode, 0)
            value = receipt.load_receipt(job / "receipt.json")
            receipt.validate_seal(value, job)

            sealed = job / "job.env"
            saved = job / "job.env.saved"
            sealed.rename(saved)
            sealed.symlink_to(saved)
            with self.assertRaises((OSError, ValueError)):
                receipt.validate_seal(value, job)
            sealed.unlink()
            saved.rename(sealed)

            ledger = job.parent.parent / "job-ledger/quota-fixture/entry.env"
            ledger.chmod(0o600)
            ledger.write_bytes(b"x" * 4097)
            with self.assertRaises(ValueError):
                receipt.validate_seal(value, job)

            public = job / "receipt.public.json"
            public.symlink_to(job / "receipt.json")
            with self.assertRaises((OSError, ValueError)):
                receipt.load_receipt(public)
            public.unlink()
            public.write_bytes(b" " * 65_537)
            with self.assertRaises(ValueError):
                receipt.load_receipt(public)

            fifo = root / "blocked.env"
            os.mkfifo(fifo)
            with self.assertRaises(ValueError):
                receipt.read_bounded_regular(fifo, 4096)

            source = root / "changing.env"
            replacement = root / "replacement.env"
            source.write_bytes(b"first")
            replacement.write_bytes(b"other")
            original_read = os.read
            replaced = False
            def swap_after_open(descriptor: int, count: int) -> bytes:
                nonlocal replaced
                data = original_read(descriptor, count)
                if not replaced:
                    os.replace(replacement, source)
                    replaced = True
                return data
            with patch.object(receipt.os, "read", side_effect=swap_after_open):
                with self.assertRaises(ValueError):
                    receipt.read_bounded_regular(source, 4096)


if __name__ == "__main__":
    unittest.main()
