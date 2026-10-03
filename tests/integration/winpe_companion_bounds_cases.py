"""Owned byte fixtures verify D4's affirmative bounds and refusal boundary."""
import importlib.util
import json
import os
from pathlib import Path
import sys
import subprocess
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
from bounded_output import BoundedOutput
import winpe_companion_bounds as bounds


class WinPEBoundsTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.private = Path(self.temp.name)
        self.frames = self.private / "boot/ramfb"
        self.frames.mkdir(parents=True)

    def output(self, path, status, cap, value=b"ok\n"):
        with BoundedOutput(path, status, cap) as sink:
            os.write(sink.fileno(), value)
        sink.check()

    def fixture(self):
        for name, status, cap in (("run.log", "run-log-bound.json", bounds.LOG_LIMIT),
                                  ("target-stat.txt", "target-stat-bound.json", bounds.LOG_LIMIT),
                                  ("cleanup.txt", "cleanup-bound.json", bounds.WRAPPER_LIMIT)):
            self.output(self.private / "boot" / name, self.private / "boot" / status, cap)
        self.output(self.private / "wrapper.log", self.private / "wrapper-bound.json", bounds.WRAPPER_LIMIT)
        self.policy = self.frames / "capture-bound-status.json"
        self.policy.write_text(json.dumps(dict(schema="bridgevm.ramfb-capture-bound.v1",
                                              raw_limit_bytes=bounds.RAW_LIMIT,
                                              ppm_limit_bytes=bounds.PPM_LIMIT,
                                              complete=True, refused=False)))
        self.frame("ramfb")

    def frame(self, name):
        (self.frames / (name + ".xrgb8888")).write_bytes(bytes([1, 2, 3, 4]))
        (self.frames / (name + ".ppm")).write_bytes(b"P6\n1 1\n255\n" + bytes([3, 2, 1]))

    def test_complete_small_frames_and_logs_pass_without_altering_bytes(self):
        self.fixture()
        before = {p: p.read_bytes() for p in self.private.rglob("*") if p.is_file()}
        bounds.verify(self.private)
        self.assertEqual(before, {p: p.read_bytes() for p in before})

    def test_missing_producer_record_is_refused(self):
        self.fixture()
        (self.private / "boot/run-log-bound.json").unlink()
        with self.assertRaises((ValueError, OSError)): bounds.verify(self.private)

    def test_log_refusal_or_wrong_size_cannot_be_complete(self):
        self.fixture()
        path = self.private / "boot/run-log-bound.json"
        original = json.loads(path.read_text())
        for changes in ({"overflow": True}, {"complete": False}, {"stored_bytes": 2},
                        {"limit_bytes": True}, {"error": "output-io"}):
            with self.subTest(changes=changes):
                path.write_text(json.dumps(dict(original, **changes)))
                with self.assertRaises(ValueError): bounds.verify(self.private)

    def test_capture_records_are_strict_and_bounded(self):
        self.fixture()
        original = self.policy.read_text()
        values = ["[]", "null", original[:-1] + ',"complete":true}', "x" * 4097]
        value = json.loads(original)
        values += [json.dumps(dict(value, **change)) for change in
                   ({"refused": True}, {"complete": False}, {"raw_limit_bytes": True},
                    {"raw_limit_bytes": bounds.RAW_LIMIT + 1}, {"extra": 1})]
        for text in values:
            with self.subTest(text=text[:60]):
                self.policy.write_text(text)
                with self.assertRaises((ValueError, OSError)): bounds.verify(self.private)

    def test_prior_affirmative_capture_does_not_mask_later_record_failure(self):
        self.fixture()
        for name in ("capture-bound-status.pending", "capture-bound-refused.json"):
            path = self.frames / name
            path.touch()
            with self.assertRaises(ValueError): bounds.verify(self.private)
            path.unlink()
        (self.private / "boot/run.log").unlink()
        (self.private / "boot/run-log-bound.json").unlink()
        self.output(self.private / "boot/run.log", self.private / "boot/run-log-bound.json",
                    bounds.LOG_LIMIT, b"x" * 65530 + b"CapturePolicyError")
        with self.assertRaises(ValueError): bounds.verify(self.private)

    def test_fifo_refusal_marker_and_failed_reservation_cannot_use_positive_logs(self):
        self.fixture()
        marker = self.private / "boot/output-bound-refused.json"
        marker.write_bytes(b"owned collision")
        with self.assertRaises(ValueError): bounds.verify(self.private)
        result = subprocess.run(["/bin/bash", "-c", 'ROOT="$1"; EVIDENCE_DIR="$2"; source "$ROOT/scripts/run-hvf-windows-installed-boot-output.sh"; record_bounded_output_refusal',
                                 "_", str(ROOT), str(self.private / "boot")], capture_output=True, timeout=5)
        self.assertEqual(result.returncode, 0)
        self.assertEqual(marker.read_bytes(), b"owned collision")
        marker.unlink()
        for name in ("wrapper.log", "wrapper-bound.json"): (self.private / name).unlink()
        # An absent owned parent causes real reservation ENOENT. The bounded log is the fallback refusal.
        with BoundedOutput(self.private / "wrapper.log", self.private / "wrapper-bound.json", bounds.WRAPPER_LIMIT) as sink:
            result = subprocess.run(["/bin/bash", "-c", 'ROOT="$1"; EVIDENCE_DIR="$2/missing"; source "$ROOT/scripts/run-hvf-windows-installed-boot-output.sh"; record_bounded_output_refusal',
                                     "_", str(ROOT), str(self.private / "boot")], stdout=sink,
                                    stderr=subprocess.STDOUT, timeout=5)
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(sink.check()["complete"])
        self.assertFalse(marker.exists())
        self.assertIn(b"DiagnosticOutputBoundRefusal", (self.private / "wrapper.log").read_bytes())
        with self.assertRaises(ValueError): bounds.verify(self.private)

    def test_partial_unknown_or_excess_fixed_capture_is_refused(self):
        self.fixture()
        (self.frames / "unknown.txt").touch()
        with self.assertRaises(ValueError): bounds.verify(self.private)
        (self.frames / "unknown.txt").unlink()
        (self.frames / "ramfb.ppm").unlink()
        with self.assertRaises(ValueError): bounds.verify(self.private)
        self.frame("ramfb")
        for index in range(8): self.frame("checkpoint-" + str(index))
        with self.assertRaises(ValueError): bounds.verify(self.private)

    def test_geometry_truncation_and_sparse_overlimit_are_refused(self):
        self.fixture()
        path = self.frames / "ramfb.ppm"
        for value in (b"P6\n0 1\n255\n", b"P6\n4294967296 1\n255\n", b"P6\n1 1\n255\nxx"):
            path.write_bytes(value)
            with self.assertRaises(ValueError): bounds.verify(self.private)
        self.frame("ramfb")
        with (self.frames / "ramfb.xrgb8888").open("wb") as stream:
            stream.truncate(bounds.RAW_LIMIT + 1)
        with self.assertRaises(ValueError): bounds.verify(self.private)

    def test_reused_log_and_symlink_are_refused_without_modification(self):
        self.fixture()
        path = self.private / "wrapper.log"
        original = path.read_bytes()
        with self.assertRaises(FileExistsError):
            bounds.execute([sys.executable, "-c", "print('must not start')"], self.private, {}, {})
        self.assertEqual(path.read_bytes(), original)
        path.unlink()
        path.symlink_to(self.private / "boot/run.log")
        with self.assertRaises(ValueError): bounds.verify(self.private)

    def test_real_owned_emitter_proves_wrapper_completion_and_overflow_refusal(self):
        data = {}
        bounds.execute([sys.executable, "-c", "import os; os.write(1,b'\\xffabc')"], self.private,
                       dict(os.environ), data)
        self.assertEqual((data["execution_exit_code"], data["cleanup_complete"]), (0, True))
        self.assertEqual((self.private / "wrapper.log").read_bytes(), b"\xffabc")
        for p in (self.private / "wrapper.log", self.private / "wrapper-bound.json"): p.unlink()
        with mock.patch.object(bounds, "WRAPPER_LIMIT", 16):
            with self.assertRaises(ValueError):
                bounds.execute([sys.executable, "-c", "import os; os.write(1,b'x'*17)"], self.private,
                               dict(os.environ), data)
        self.assertTrue(data["cleanup_complete"])
        self.assertTrue(data["output_bounds_refused"])
        self.assertEqual((self.private / "wrapper.log").stat().st_size, 16)

    def test_real_caller_refuses_missing_bounds_before_inspection_hash_or_chmod(self):
        spec = importlib.util.spec_from_file_location("bounds_runner", ROOT / "scripts/live-gates/run-winpe-companions.py")
        runner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(runner)
        job = dict(tier="t20-windows-hvf-winpe-companions", job_id="owned-fixture", commit="a" * 40,
                   input_manifest_sha256="b" * 64, sealed_binary_sha256="c" * 64)
        # Use the actual receipt tier and actual bounded execution; only host/media discovery is stubbed.
        import winpe_companion_receipt as receipts
        job["tier"] = receipts.TIER
        out = self.private / "job"; out.mkdir()
        args = type("Args", (), dict(out=out, job_id=job["job_id"], input_manifest=self.private / "manifest",
                                     sealed_binary=self.private / "binary"))()
        clones = {key: self.private / key for key in ("image", "vars", "injector")}
        for path in clones.values(): path.write_bytes(b"owned")
        records = dict(binary=(args.sealed_binary, "c" * 64))
        before = dict(available=False, files={})
        with mock.patch.object(runner, "job_fields", return_value=job), \
             mock.patch.object(runner.subprocess, "check_output", return_value="a" * 40), \
             mock.patch.object(runner.platform, "system", return_value="Darwin"), \
             mock.patch.object(runner.platform, "machine", return_value="arm64"), \
             mock.patch.object(runner, "file_hash", return_value="b" * 64) as digest, \
             mock.patch.object(runner, "load", return_value=records), \
             mock.patch.object(runner, "clone", return_value=clones), \
             mock.patch.object(runner, "inspect", return_value=before) as inspect, \
             mock.patch.object(runner, "verify") as verify, \
             mock.patch.object(runner, "command", return_value=[sys.executable, "-c", "print('owned emitter')"]):
            self.assertEqual(runner.run(args), 1)
        value = json.loads((out / "receipt.json").read_text())
        self.assertEqual((value["failure_stage"], value["outcome"]), ("output-bounds", "diagnostic-incomplete"))
        self.assertTrue(value["cleanup_complete"])
        self.assertTrue(value["output_bounds_refused"])
        self.assertFalse(value["output_bounds_verified"])
        self.assertEqual(inspect.call_count, 1)
        verify.assert_not_called()
        self.assertEqual(digest.call_count, 1 + len(runner.COMPANIONS))
        self.assertTrue(all(path.read_bytes() == b"owned" and path.stat().st_mode & 0o200 for path in clones.values()))
