#!/usr/bin/env python3
"""Bounded stop, receipt, queue and cleanup contracts for A19 T22."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import a19_interrupt_restore_child as observer
import a19_interrupted_restore_receipt as receipt
import native_snapshot_restore_inputs as inputs
from importlib.util import module_from_spec, spec_from_file_location

spec = spec_from_file_location("interrupt_runner", ROOT / "scripts/live-gates/run-a19-interrupted-restore-tier.py")
runner = module_from_spec(spec)
spec.loader.exec_module(runner)
COMMIT = subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True).strip()
SHA_A = "a" * 64
SHA_B = "b" * 64


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


def passing(job_id: str = "interrupt-fixture") -> dict:
    value = receipt.initial(job_id, COMMIT)
    value.update(dict.fromkeys(receipt.HASHES, SHA_A), host_model="Mac17,9", macos_version="27.0", finished_at=value["started_at"])
    value.update({field: True for field in receipt.FLAGS[:8]})
    value.update({"pass": True, "outcome": "completed", "interruption_stage": "staged-disk-verify-read",
                  "boots_attempted": 4, "boots_passed": 4, "natural_shutdown_count": 4,
                  "interruption_case_count": 1, "sample_count": 1, "run_count": 1,
                  "clobber_marker_sha256": SHA_B, "postkill_marker_sha256": SHA_B})
    return receipt.validate(value, COMMIT)


class InterruptedRestoreContract(unittest.TestCase):
    def make_child(self, root: Path, publish: bool = False) -> tuple[Path, Path, Path, Path, Path]:
        disk, vars, snapshot, output = (root / "disk.raw", root / "vars.fd", root / "snapshot", root / "output")
        write(disk, b"old-disk")
        write(vars, b"old-vars")
        snapshot.mkdir()
        output.mkdir()
        managed = observer.stable_root(disk.resolve(), vars.resolve())
        helper = root / "helper.py"
        helper.write_text("#!/usr/bin/env python3\n"
                          "import os, pathlib, sys, time\n"
                          f"managed=pathlib.Path({str(managed)!r})\n"
                          "managed.mkdir(mode=0o700, exist_ok=True)\n"
                          "stage=managed/'staging'; stage.mkdir(mode=0o700)\n"
                          + ("(managed/'current').mkdir()\n" if publish else "")
                          + "for name in ('disk.raw','vars.fd','manifest.json'):\n"
                          "    with (stage/name).open('wb') as output:\n"
                          "        output.write(b'staged'); output.flush(); os.fsync(output.fileno())\n"
                          "with (stage/'disk.raw').open('rb') as staged:\n"
                          "    time.sleep(30)\n")
        helper.chmod(0o700)
        return helper, snapshot, disk, vars, output

    def test_exact_child_read_fd_stop_and_missed_window(self):
        if not Path(observer.LSOF).is_file():
            if sys.platform == "darwin":
                self.fail("macOS T22 test needs owner-readable lsof")
            self.skipTest("lsof is only required on the physical Mac and hosted macOS")
        with tempfile.TemporaryDirectory() as temporary:
            helper, snapshot, disk, vars, output = self.make_child(Path(temporary))
            result = observer.run(helper, snapshot, disk, vars, output, 5)
            self.assertEqual(result["interruption_stage"], "staged-disk-verify-read")
            self.assertTrue(result["helper_killed_and_reaped"])
            self.assertEqual(result["stop_fd_log_sha256"], digest(output / "interrupt-helper-fd.private.log"))
            self.assertEqual((output / "interrupt-helper-fd.private.log").stat().st_mode & 0o777, 0o600)
            managed = observer.stable_root(disk.resolve(), vars.resolve())
            self.assertFalse((managed / "current").exists())
            with self.assertRaises(ValueError):
                observer.run(helper, snapshot, disk, vars, output, 1)
        with tempfile.TemporaryDirectory() as temporary:
            helper, snapshot, disk, vars, output = self.make_child(Path(temporary), publish=True)
            with self.assertRaises(TimeoutError):
                observer.run(helper, snapshot, disk, vars, output, 1)
            self.assertFalse((output / "interrupt-helper-fd.private.log").exists())

    def test_fd_parser_rejects_wrong_pid_and_write_access(self):
        path = Path("/private/staging/disk.raw")
        sample = "p123\nf3\nar\nn/private/staging/disk.raw\n"
        self.assertTrue(observer.read_fd_observed(sample, 123, path))
        self.assertFalse(observer.read_fd_observed(sample, 124, path))
        self.assertFalse(observer.read_fd_observed(sample.replace("ar", "aw"), 123, path))
        self.assertFalse(observer.read_fd_observed(sample, 123, Path("/wrong/disk.raw")))

    def test_lost_child_and_missing_stage_cannot_create_an_observation(self):
        with tempfile.TemporaryDirectory() as temporary:
            helper, snapshot, disk, vars, output = self.make_child(Path(temporary))
            helper.write_text("#!/usr/bin/env python3\nraise SystemExit(1)\n")
            with self.assertRaises(RuntimeError):
                observer.run(helper, snapshot, disk, vars, output, 1)
            self.assertFalse((output / "interrupt-helper-fd.private.log").exists())
        with tempfile.TemporaryDirectory() as temporary:
            helper, snapshot, disk, vars, output = self.make_child(Path(temporary))
            helper.write_text("#!/usr/bin/env python3\nimport time\ntime.sleep(10)\n")
            with self.assertRaises(TimeoutError):
                observer.run(helper, snapshot, disk, vars, output, 1)
            self.assertFalse((output / "interrupt-helper-fd.private.log").exists())

    def test_receipt_rejects_mixed_pair_missing_proof_and_promotion(self):
        value = passing()
        for field, wrong in (("postkill_vars_sha256", SHA_B), ("postretry_disk_sha256", SHA_B),
                             ("postkill_marker_sha256", SHA_A), ("helper_stop_verified", False),
                             ("worker_cleanup_verified", False), ("interruption_stage", "absent"),
                             ("sample_count", 2), ("claim_eligible", True)):
            with self.subTest(field=field), self.assertRaises(ValueError):
                receipt.validate({**value, field: wrong}, COMMIT)
        failed = receipt.initial("interrupt-fixture", COMMIT)
        failed["finished_at"] = failed["started_at"]
        self.assertFalse(receipt.validate(failed, COMMIT)["pass"])
        with self.assertRaises(ValueError):
            receipt.validate({**failed, "sample_count": 1}, COMMIT)

    def test_retained_stop_and_guest_identity_aggregation(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            original, clobber = b"BV-ORIGINAL\n", b"BV-CLOBBERED\n"
            files = {
                "create.json": b"create", "restore-retry.json": b"restore",
                "phase1-original/marker-after.txt": original,
                "phase3-clobber/marker-after.txt": clobber,
                "phase5-postkill/marker-before.txt": clobber,
                "phase7-restored/marker-before.txt": original,
                "interrupt-helper-fd.private.log": b"p123\nf3\nar\nn/private/staging/disk.raw\n",
            }
            for name, data in files.items():
                write(output / name, data)
            hashes = {"disk": SHA_A, "vars": SHA_B}
            write(output / "snapshot-created-manifest.json", json.dumps({
                "format_version": 1, "vm_id": runner.VM_ID, "disk_bytes": 1,
                "disk_sha256": SHA_A, "vars_bytes": 1, "vars_sha256": SHA_B,
            }).encode())
            for name, value in (("pre-interrupt-disk", SHA_A), ("pre-interrupt-vars", SHA_B),
                                ("postkill-disk", SHA_A), ("postkill-vars", SHA_B)):
                write(output / f"{name}.sha256", (value + "\n").encode())
            for folder in ("postkill-export", "postretry-export"):
                data = {"schema": "bridgevm.native-snapshot-export-evidence.v1", "vm_id": runner.VM_ID,
                        "result_sha256": SHA_A, "manifest_sha256": SHA_B,
                        "disk_sha256": hashes["disk"], "vars_sha256": hashes["vars"]}
                write(output / folder / "export-evidence.json", json.dumps(data).encode())
            stop = {"interruption_stage": "staged-disk-verify-read", "helper_stop_verified": True,
                    "staged_file_sync_order_verified": True, "old_selection_before_kill": True,
                    "helper_killed_and_reaped": True,
                    "stop_fd_log_sha256": digest(output / "interrupt-helper-fd.private.log")}
            write(output / "interrupt-observation.json", json.dumps(stop).encode())
            value = receipt.initial("interrupt-fixture", COMMIT)
            runner.collect(output, value)
            self.assertTrue(value["postkill_pair_unchanged"] and value["postretry_original_restored"])
            write(output / "postretry-export/export-evidence.json", json.dumps({**data, "vars_sha256": SHA_A}).encode())
            with self.assertRaises(ValueError):
                runner.collect(output, receipt.initial("interrupt-fixture", COMMIT))
            write(output / "postretry-export/export-evidence.json", json.dumps(data).encode())
            manifest = json.loads((output / "snapshot-created-manifest.json").read_text())
            write(output / "snapshot-created-manifest.json", json.dumps({**manifest, "vm_id": "wrong-vm"}).encode())
            with self.assertRaises(ValueError):
                runner.collect(output, receipt.initial("interrupt-fixture", COMMIT))
            write(output / "snapshot-created-manifest.json", json.dumps(manifest).encode())
            write(output / "phase5-postkill/marker-before.txt", original)
            with self.assertRaises(ValueError):
                runner.collect(output, receipt.initial("interrupt-fixture", COMMIT))

    def test_queue_seal_public_receipt_and_cleanup_fence(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            app = root / "BridgeVM.app"
            artifacts = {"app_bundle": app, "image": root / "image.raw", "vars": root / "vars.fd"}
            for key, relation in inputs.RELATIONS.items():
                artifacts[key] = app / relation
            for key, path in artifacts.items():
                if key == "app_bundle":
                    continue
                write(path, key.encode())
                if key in inputs.RELATIONS:
                    path.chmod(0o700)
            manifest = root / "manifest.tsv"
            rows = [f"{key}\t{path}\t{inputs.tree_hash(path) if key == 'app_bundle' else inputs.digest(path)}"
                    for key, path in artifacts.items()]
            rows += [f"source_commit\t{COMMIT}", "app_profile\trelease", "binary_profile\trelease",
                     "binary_features\tvenus", "rust_toolchain\t1.97.0"]
            manifest.write_text("\n".join(rows) + "\n")
            queue = root / "queue"
            submitted = subprocess.run(["bash", str(ROOT / "scripts/live-gates/bridgevm-live"), "submit",
                                        receipt.TIER, "--sha", COMMIT, "--input-manifest", str(manifest),
                                        "--job-id", "interrupt-fixture"], capture_output=True, text=True,
                                       env=dict(os.environ, BRIDGEVM_LIVE_ROOT=str(queue)), timeout=30)
            self.assertEqual(submitted.returncode, 0, submitted.stderr)
            job = queue / "queued/interrupt-fixture"
            value = passing()
            value["input_manifest_sha256"] = digest(job / "input-manifest.tsv")
            value["binary_hash"] = digest(job / "hvf_gic_boot_probe")
            receipt.validate_seal(value, job)
            receipt.write_new(job / "receipt.json", value)
            published = subprocess.run(["bash", str(ROOT / "scripts/live-gates/publish-receipt.sh"),
                                        receipt.TIER, str(job), str(ROOT), COMMIT],
                                       capture_output=True, text=True, timeout=30)
            self.assertEqual(published.returncode, 0, published.stderr)
            self.assertEqual(json.loads((job / "receipt.public.json").read_text()), value)
            command = 'source "$REPO/scripts/live-gates/t17-worker-cleanup-fence.sh"; bridgevm_t17_guard_or_fence "$TIER" "$JOB" "$REPO" "$COMMIT" interrupt-fixture "$QUEUE"'
            env = dict(os.environ, REPO=str(ROOT), TIER=receipt.TIER, JOB=str(job),
                       COMMIT=COMMIT, QUEUE=str(queue))
            self.assertEqual(subprocess.run(["bash", "-c", command], env=env).returncode, 0)
            (job / "prepared-inputs").mkdir()
            self.assertEqual(subprocess.run(["bash", "-c", command], env=env).returncode, 126)
            self.assertTrue((queue / "worker-cleanup-required").is_file())


if __name__ == "__main__":
    unittest.main()
