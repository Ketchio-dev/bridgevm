#!/usr/bin/env python3
"""Deterministic contracts; copied fixture bytes are not APFS or guest proof."""
import hashlib
import importlib.util
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import b6_cell_inputs as inputs

spec = importlib.util.spec_from_file_location("b6_runner", ROOT / "scripts/live-gates/run-b6-cell-observation.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class CellContracts(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = pathlib.Path(self.tmp.name)
        self.paths = {}
        for key in inputs.KEYS - {"viogpu_dir"}:
            path = self.root / key
            path.write_bytes(key.encode())
            self.paths[key] = path
        store = self.root / "driver-store"
        store.mkdir()
        (store / "driver.sys").write_bytes(b"driver fixture")
        self.paths["viogpu_dir"] = store
        self.paths["config"].write_text(json.dumps(dict(width=1600, height=900, logpixels=96)))
        for key in ("image", "vars"):
            self.paths[key].chmod(0o400)
        self.manifest = self.root / "inputs.tsv"
        self.seal()

    def seal(self):
        self.records = {key: (path, inputs.tree_hash(path) if key == "viogpu_dir" else inputs.file_hash(path))
                        for key, path in self.paths.items()}
        self.manifest.write_text("".join(key + "\t" + str(path) + "\t" + digest + "\n"
                                         for key, (path, digest) in sorted(self.records.items())))

    def load(self):
        return inputs.load_inputs(self.manifest, self.paths["binary"])

    def test_original_cell_is_accepted(self):
        records, config = self.load()
        self.assertEqual(set(records), inputs.KEYS)
        self.assertEqual(config, dict(width=1600, height=900, logpixels=96))

    def test_geometry_override_is_refused(self):
        self.paths["config"].write_text(json.dumps(dict(width=640, height=480, logpixels=96)))
        self.seal()
        with self.assertRaises(ValueError):
            self.load()

    def test_boolean_or_sample_override_is_refused(self):
        for config in (dict(width=True, height=900, logpixels=96),
                       dict(width=1600, height=900, logpixels=96, runs=1)):
            self.paths["config"].write_text(json.dumps(config))
            self.seal()
            with self.assertRaises(ValueError):
                self.load()

    def test_changed_input_is_refused(self):
        self.paths["presentmon"].write_bytes(b"replacement")
        with self.assertRaises(ValueError):
            self.load()

    def test_writable_canonical_image_is_refused(self):
        self.paths["image"].chmod(0o600)
        with self.assertRaises(ValueError):
            self.load()

    def test_duplicate_missing_and_relative_rows_are_refused(self):
        original = self.manifest.read_text()
        for changed in (original + original.splitlines()[0] + "\n",
                        "\n".join(original.splitlines()[1:]) + "\n",
                        original.replace(str(self.paths["config"]), "relative")):
            self.manifest.write_text(changed)
            with self.assertRaises(ValueError):
                self.load()

    def test_symlink_input_is_refused(self):
        link = self.root / "link"
        link.symlink_to(self.paths["presentmon"])
        self.manifest.write_text(self.manifest.read_text().replace(str(self.paths["presentmon"]), str(link)))
        with self.assertRaises(ValueError):
            self.load()

    def test_driver_store_symlink_is_refused(self):
        (self.paths["viogpu_dir"] / "link").symlink_to(self.paths["presentmon"])
        with self.assertRaises(ValueError):
            self.load()

    def test_wrong_sealed_binary_is_refused(self):
        wrong = self.root / "wrong"
        wrong.write_bytes(b"not the declared binary")
        with self.assertRaises(ValueError):
            inputs.load_inputs(self.manifest, wrong)

    def test_clone_commands_and_independent_vars(self):
        records, _ = self.load()
        commands = []
        def copy(command, check):
            commands.append(command)
            shutil.copyfile(command[-2], command[-1])
        disk, variables = inputs.clone_pair(records, self.root / "lane", copy=copy)
        self.assertEqual(commands[0][:2], ["cp", "-c"])
        self.assertEqual(commands[1][0], "cp")
        for key, destination in (("image", disk), ("vars", variables)):
            self.assertNotEqual(destination.stat().st_ino, self.paths[key].stat().st_ino)
            self.assertEqual(destination.stat().st_mode & 0o777, 0o600)
            self.assertEqual(self.paths[key].stat().st_mode & 0o777, 0o400)
        variables.write_bytes(b"lane change")
        self.assertEqual(self.paths["vars"].read_bytes(), b"vars")

    def test_reused_lane_is_refused_before_copy(self):
        lane = self.root / "lane"
        lane.mkdir()
        def forbidden(*args, **kwargs):
            self.fail("copy was attempted")
        with self.assertRaises(FileExistsError):
            inputs.clone_pair(self.records, lane, copy=forbidden)

    def test_hardlink_clone_is_refused_before_chmod(self):
        def link(command, check):
            os.link(command[-2], command[-1])
        with self.assertRaises(ValueError):
            inputs.clone_pair(self.records, self.root / "lane", copy=link)
        self.assertEqual(self.paths["image"].stat().st_mode & 0o777, 0o400)
        self.assertEqual(self.paths["vars"].stat().st_mode & 0o777, 0o400)

    def test_receipt_starts_with_no_claims(self):
        report = runner.receipt("Contract.cell", "a" * 40)
        for key in ("pass", "valid", "claim_eligible", "criterion_pass", "capability_promotion"):
            self.assertIs(report[key], False)
        self.assertEqual(report["required_run_count"], 27)
        self.assertEqual(report["run_count"], 0)

    def test_failed_preflight_retains_false_receipt(self):
        out = self.root / "out"
        out.mkdir()
        self.paths["presentmon"].write_bytes(b"changed")
        result = subprocess.run([sys.executable, str(ROOT / "scripts/live-gates/run-b6-cell-observation.py"),
                                 "--out", str(out), "--job-id", "Contract.failed",
                                 "--input-manifest", str(self.manifest), "--sealed-binary", str(self.paths["binary"])],
                                capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 1, result.stderr.decode())
        report = json.loads((out / "receipt.json").read_text())
        self.assertEqual(report["failure_code"], "input-failed")
        self.assertFalse(report["valid"])
        self.assertFalse(report["criterion_pass"])
        self.assertFalse((out / "capture").exists())

    def test_queue_copies_and_seals_new_tier(self):
        env = dict(os.environ, BRIDGEVM_LIVE_ROOT=str(self.root / "queue"))
        result = subprocess.run([str(ROOT / "scripts/live-gates/bridgevm-live"), "submit",
                                 runner.TIER, "--input-manifest", str(self.manifest),
                                 "--job-id", "Contract.sealed"], env=env, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        job = self.root / "queue/queued/Contract.sealed"
        self.assertEqual((job / "input-manifest.tsv").read_bytes(), self.manifest.read_bytes())
        self.assertEqual(inputs.file_hash(job / "hvf_gic_boot_probe"), self.records["binary"][1])
        self.assertIn("sealed_binary_sha256=" + self.records["binary"][1], (job / "job.env").read_text())


if __name__ == "__main__":
    unittest.main()
