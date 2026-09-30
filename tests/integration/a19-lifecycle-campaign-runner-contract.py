#!/usr/bin/env python3
"""A19 T23 campaign runner: ten sequential isolated lanes, no replacement, fenced cleanup."""
from __future__ import annotations

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
_spec = importlib.util.spec_from_file_location("campaign_fixtures", ROOT / "tests/integration/a19_lifecycle_campaign_fixtures.py")
F = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(F)
receipt, record = F.receipt, F.record
import a19_lifecycle_campaign_lane as lane  # noqa: E402
import a19_lifecycle_campaign_read as read  # noqa: E402
import native_snapshot_restore_inputs as inputs  # noqa: E402
_runner = importlib.util.spec_from_file_location("campaign_runner", ROOT / "scripts/live-gates/run-a19-lifecycle-campaign-tier.py")
runner = importlib.util.module_from_spec(_runner)
_runner.loader.exec_module(runner)
HOST = {"host_model": "Mac17,9", "macos_version": "27.0"}


def manifest_fixture(root: Path) -> tuple[Path, Path]:
    app = root / "media/BridgeVM.app"
    artifacts = {"app_bundle": app, "image": root / "media/disk.raw", "vars": root / "media/vars.fd"}
    for key, relation in inputs.RELATIONS.items():
        artifacts[key] = app / relation
    for key, path in artifacts.items():
        if key != "app_bundle":
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(key.encode())
            path.chmod(0o700)
    rows = [f"{key}\t{path}\t{inputs.tree_hash(path) if key == 'app_bundle' else inputs.digest(path)}"
            for key, path in artifacts.items()]
    rows += [f"source_commit\t{F.COMMIT}", "app_profile\trelease", "binary_profile\trelease",
             "binary_features\tvenus", "rust_toolchain\t1.97.0"]
    manifest = root / "input-manifest.tsv"
    manifest.write_text("\n".join(rows) + "\n")
    return manifest, artifacts["binary"]


def fake_lifecycle(seen: list, fail_ordinal: int | None = None, leave_live: int | None = None):
    """Write what verify-native-snapshot-restore-boots.sh retains, per lane."""
    def lifecycle(_repo: Path, env: dict) -> int:
        out, vm = Path(env["OUT"]), env["NATIVE_SNAPSHOT_VM_ID"]
        disk, variables = Path(env["DISK"]), Path(env["VARS"])
        seen.append((out.name, disk, variables, vm, disk.stat().st_ino, variables.stat().st_ino))
        (out / "live").mkdir()
        original, clobber = f"BV-ORIGINAL-{vm}".encode(), f"BV-CLOBBERED-{vm}".encode()
        for phase, before, after in (("phase1-original", b"BV-NO-MARKER", original),
                                     ("phase3-clobber", original, clobber),
                                     ("phase5-restored", original, b"BV-FINAL")):
            (out / phase).mkdir()
            (out / phase / "marker-before.txt").write_bytes(before)
            (out / phase / "marker-after.txt").write_bytes(after)
            if not (fail_ordinal == int(vm[-2:]) and phase == "phase5-restored"):
                (out / phase / "system-off").write_bytes(b"")
        for name in ("create.json", "restore.json"):
            (out / name).write_bytes(f"{vm}-{name}".encode())
        for name in ("final-disk.sha256", "final-vars.sha256"):
            (out / name).write_text(F.sha(vm + name) + "\n")
        (out / "export-evidence.json").write_text(json.dumps({
            "schema": "bridgevm.native-snapshot-export-evidence.v1", "vm_id": vm,
            **{field: F.sha(vm + field) for field in ("result_sha256", "manifest_sha256", "disk_sha256", "vars_sha256")}}))
        if leave_live != int(vm[-2:]):
            shutil.rmtree(out / "live")
        return 1 if fail_ordinal == int(vm[-2:]) else 0
    return lifecycle


def shutdowns(output: Path, phases: tuple[str, ...]) -> int:
    return sum((output / phase / "system-off").is_file() for phase in phases)


class CampaignRunner(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)

    def tearDown(self):
        self.temporary.cleanup()

    def run_main(self, job: Path, manifest: Path, binary: Path) -> int:
        with (mock.patch.object(runner.sys, "argv", ["runner", str(job), job.name, str(manifest), str(binary)]),
              mock.patch.object(runner, "source_commit", return_value=F.COMMIT),
              mock.patch.object(runner, "host_identity", return_value=HOST)):
            return runner.main()

    def synthetic(self, fail_at: int | None = None, residue_at: int | None = None, cancel_after: int | None = None):
        job = F.queue_job(self.root, state="running")
        calls: list[int] = []

        def run_lane(lanes, ordinal, ident, _manifest, _binary, _repo):
            for previous in range(1, ordinal):
                self.assertFalse(any(os.path.lexists(lanes / record.lane_name(previous) / name) for name in record.OWNED))
            directory = lanes / record.lane_name(ordinal)
            directory.mkdir(mode=0o700)
            calls.append(ordinal)
            value = F.lane(ident, ordinal, ordinal != fail_at)
            if ordinal == residue_at:
                (directory / "live").mkdir()
                value.update(cleanup_verified=False, **{"pass": False})
            if ordinal == cancel_after:
                (job / "cancel.requested").write_text("")
            return value, F.inputs()

        with (mock.patch.object(runner, "run_lane", side_effect=run_lane),
              mock.patch.object(runner, "preflight")):
            status = self.run_main(job, self.root / "manifest.tsv", self.root / "probe")
        value = receipt.validate(json.loads((job / "receipt.json").read_text()), F.COMMIT)
        return job, calls, status, value

    def test_ten_lanes_run_in_order_each_after_the_previous_cleanup(self):
        job, calls, status, value = self.synthetic()
        self.assertEqual((status, calls), (0, list(range(1, 11))))
        self.assertTrue(value["pass"])
        self.assertEqual((value["outcome"], value["run_count"], value["passes"]), ("completed", 10, 10))
        read.validate_seal(value, job)

    def test_first_failed_lane_stops_the_campaign_without_replacement(self):
        job, calls, status, value = self.synthetic(fail_at=3)
        self.assertEqual((status, calls), (1, [1, 2, 3]))
        self.assertFalse((job / "lanes/lane-04").exists())
        self.assertEqual((value["outcome"], value["failure_code"], value["run_count"], value["passes"], value["failures"]),
                         ("failed", "lane-failed", 3, 2, 1))
        self.assertFalse(value["pass"])
        read.validate_seal(value, job)

    def test_lane_cleanup_failure_stops_withholds_and_fences(self):
        job, calls, status, value = self.synthetic(residue_at=2)
        self.assertEqual((status, calls), (1, [1, 2]))
        self.assertEqual((value["outcome"], value["failure_code"], value["worker_cleanup_verified"]),
                         ("failed", "cleanup-failed", False))
        with self.assertRaises(ValueError):
            read.validate_seal(value, job)
        self.assertEqual(F.fence(job).returncode, 126)

    def test_cancel_between_lanes_is_not_a_pass(self):
        job, calls, status, value = self.synthetic(cancel_after=2)
        self.assertEqual((status, calls, value["outcome"], value["run_count"]), (1, [1, 2], "canceled", 2))
        self.assertFalse(value["pass"])

    def test_cross_volume_pair_is_preflight_blocked_before_any_lane(self):
        manifest, binary = manifest_fixture(self.root)
        job = F.queue_job(self.root, state="running", manifest=inputs.digest(manifest), binary=inputs.digest(binary))
        device = os.stat(job).st_dev
        with (mock.patch.object(lane, "volume", side_effect=lambda path: device + (Path(path).name == "disk.raw")),
              mock.patch.object(runner, "run_lane") as run_lane):
            self.assertEqual(self.run_main(job, manifest, binary), 1)
        run_lane.assert_not_called()
        value = receipt.validate(json.loads((job / "receipt.json").read_text()), F.COMMIT)
        self.assertEqual((value["outcome"], value["failure_code"], value["run_count"]), ("preflight-blocked", "invalid-input", 0))
        self.assertFalse((job / "lanes").exists())

    def test_lane_environments_give_every_lane_its_own_disk_and_vars(self):
        private = {"binary": "/sealed/probe", "app_cli": "/sealed/bridgevm"}
        environments = [lane.lane_environment(self.root / record.lane_name(ordinal), ordinal, private)
                        for ordinal in range(1, 11)]
        for key in ("DISK", "VARS", "OUT", "NATIVE_SNAPSHOT_VM_ID"):
            self.assertEqual(len({env[key] for env in environments}), 10, key)
        for ordinal, env in enumerate(environments, 1):
            lane_dir = self.root / record.lane_name(ordinal)
            self.assertEqual(Path(env["VARS"]).parent, lane_dir / "prepared-inputs")
            self.assertEqual(Path(env["DISK"]).parent, lane_dir / "prepared-inputs")
            self.assertNotEqual(env["DISK"], env["VARS"])

    def test_isolation_refuses_shared_or_linked_lane_media(self):
        source_disk, source_vars = self.root / "disk.raw", self.root / "vars.fd"
        source_disk.write_bytes(b"disk")
        source_vars.write_bytes(b"vars")
        for mutation in ("clone", "hardlink-source", "shared-pair", "symlink-source"):
            with self.subTest(mutation=mutation):
                prepared = self.root / mutation
                prepared.mkdir()
                shutil.copyfile(source_disk, prepared / "disk.raw")
                if mutation == "clone":
                    shutil.copyfile(source_vars, prepared / "vars.fd")
                    lane.verify_isolation(prepared, (source_disk, source_vars))
                    continue
                if mutation == "hardlink-source":
                    os.link(source_vars, prepared / "vars.fd")
                elif mutation == "symlink-source":
                    (prepared / "vars.fd").symlink_to(source_vars)
                else:
                    os.link(prepared / "disk.raw", prepared / "vars.fd")
                with self.assertRaises(ValueError):
                    lane.verify_isolation(prepared, (source_disk, source_vars))

    def lifecycle_campaign(self, **fake) -> tuple[Path, int, dict, list]:
        manifest, binary = manifest_fixture(self.root)
        job = F.queue_job(self.root, state="running", manifest=inputs.digest(manifest), binary=inputs.digest(binary))
        seen: list = []
        real_prepare = lane.prepare
        with (mock.patch.object(lane, "prepare", side_effect=lambda *args: real_prepare(*args, shutil.copyfile, shutil.copytree)),
              mock.patch.object(lane, "run_lifecycle", side_effect=fake_lifecycle(seen, **fake)),
              mock.patch.object(lane, "shutdown_count", side_effect=shutdowns)):
            status = self.run_main(job, manifest, binary)
        return job, status, receipt.validate(json.loads((job / "receipt.json").read_text()), F.COMMIT), seen

    def test_real_lane_logic_passes_ten_isolated_lifecycles_and_publishes(self):
        job, status, value, seen = self.lifecycle_campaign()
        self.assertEqual(status, 0)
        self.assertTrue(value["pass"])
        self.assertEqual([item[0] for item in seen], [record.lane_name(ordinal) for ordinal in range(1, 11)])
        canonical = {(self.root / "media" / name).stat().st_ino for name in ("disk.raw", "vars.fd")}
        for field in (1, 2, 3):
            self.assertEqual(len({item[field] for item in seen}), 10)
        for _name, _disk, _vars, _vm, disk_inode, vars_inode in seen:
            self.assertNotEqual(disk_inode, vars_inode)
            self.assertFalse({disk_inode, vars_inode} & canonical)
        self.assertEqual(value["lane_prepared_image_sha256"], [hashlib.sha256(b"image").hexdigest()] * 10)
        self.assertEqual(value["app_artifact_sha256"], inputs.tree_hash(self.root / "media/BridgeVM.app"))
        for ordinal in range(1, 11):
            self.assertFalse(any(os.path.lexists(job / "lanes" / record.lane_name(ordinal) / name) for name in record.OWNED))
        read.validate_seal(value, job)
        result = F.publish(job)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(F.fence(job).returncode, 0)

    def test_real_lane_logic_stops_at_a_missing_natural_shutdown(self):
        job, status, value, seen = self.lifecycle_campaign(fail_ordinal=4)
        self.assertEqual((status, len(seen), value["run_count"], value["passes"]), (1, 4, 4, 3))
        self.assertEqual((value["lane_boots_attempted"][3], value["lane_natural_shutdown_counts"][3]), (3, 2))
        self.assertEqual((value["outcome"], value["failure_code"], value["pass"]), ("failed", "lane-failed", False))
        read.validate_seal(value, job)

    def test_real_lane_logic_fences_a_live_library_left_behind(self):
        job, status, value, seen = self.lifecycle_campaign(leave_live=6)
        self.assertEqual((status, len(seen), value["failure_code"]), (1, 6, "cleanup-failed"))
        self.assertFalse(value["lane_cleanup_verified"][5])
        self.assertTrue((job / "lanes/lane-06/live").is_dir())
        self.assertNotEqual(F.publish(job).returncode, 0)
        self.assertEqual(F.fence(job).returncode, 126)


if __name__ == "__main__":
    unittest.main()
