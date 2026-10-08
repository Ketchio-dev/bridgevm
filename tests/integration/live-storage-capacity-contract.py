#!/usr/bin/env python3
"""Controlled filesystem providers and the conditional worker admission boundary."""
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
LIVE = ROOT / "scripts/live-gates"
spec = importlib.util.spec_from_file_location("capacity", LIVE / "live_storage_capacity.py")
capacity = importlib.util.module_from_spec(spec); spec.loader.exec_module(capacity)


class Capacity(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="live-capacity-"); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.queue = self.root / "queue"; self.queue.mkdir()
        self.work = self.root / "work"; self.work.mkdir()
        self.paths = list(map(str, (self.queue, self.work)))

    def filesystem(self, free, **fields):
        return SimpleNamespace(**dict(dict(f_frsize=capacity.GIB, f_bavail=free, f_blocks=4096, f_flag=0), **fields))

    def measure(self, devices, rows):
        actual = os.fstat; iterator = iter(devices)
        def fstat(fd): return SimpleNamespace(st_mode=actual(fd).st_mode, st_dev=next(iterator))
        with patch.object(capacity.os, "fstat", side_effect=fstat), patch.object(capacity.os, "fstatvfs", side_effect=rows) as probe:
            result = capacity.available(self.paths)
            return result, probe.call_count

    def test_independent_filesystems_use_minimum_not_sum(self):
        for values in ((99, 200), (200, 99), (100, 100)):
            free, count = self.measure((1, 2), [self.filesystem(v) for v in values])
            self.assertEqual(free, min(values) * capacity.GIB); self.assertEqual(count, 2)
            self.assertEqual(free >= capacity.threshold("100"), min(values) >= 100)

    def test_same_filesystem_is_probed_once(self):
        self.assertEqual(self.measure((1, 1), [self.filesystem(100)]), (100 * capacity.GIB, 1))

    def test_invalid_or_readonly_provider_refuses_even_at_zero_threshold(self):
        for fields in ({"f_frsize": 0}, {"f_bavail": -1}, {"f_bavail": 4097}, {"f_flag": os.ST_RDONLY}):
            with self.subTest(fields=fields), patch.object(capacity.os, "fstatvfs", return_value=self.filesystem(100, **fields)):
                with self.assertRaises(ValueError): capacity.available(self.paths)
        with patch.object(capacity.os, "fstatvfs", side_effect=OSError("owned provider failure")):
            with self.assertRaises(OSError): capacity.available(self.paths)

    def test_missing_storage_refuses_but_estimate_is_explicit(self):
        path = str(self.root / "missing-volume/work")
        with self.assertRaises(FileNotFoundError): capacity.available([path])
        self.assertGreaterEqual(capacity.available([path], estimate=True), 0)
        self.assertFalse((self.root / "missing-volume").exists())

    def test_unmounted_volume_and_leaf_aliases_refuse_before_preparation(self):
        with patch.object(capacity.os.path, "ismount", return_value=False):
            with self.assertRaises(ValueError): capacity.storage_path("/Volumes/Absent/work")
        alias = self.root / "alias"; alias.symlink_to(self.work, target_is_directory=True)
        with self.assertRaises(ValueError): capacity.storage_path(str(alias))
        missing = self.root / "absent-parent/work"
        for minimum in ("invalid", "0"):
            result = subprocess.run([sys.executable, str(LIVE / "live_storage_capacity.py"), "--prepare-work", "--minimum", minimum, str(self.queue), str(missing)], capture_output=True, timeout=10)
            self.assertEqual(result.returncode, 2)
            self.assertFalse(missing.parent.exists())

    def test_configuration_is_canonical_and_bounded(self):
        for value in ("", "-1", "01", "+1", "1.0", "4097", "1\n", "9" * 100):
            with self.assertRaises(ValueError): capacity.threshold(value)
        self.assertEqual(capacity.threshold("0"), 0)
        self.assertEqual(capacity.threshold("4096"), 4096 * capacity.GIB)
        for value in ("relative", "/tmp/", "/tmp/..", "/tmp/./work", "/tmp/a\nb"):
            with self.assertRaises(ValueError): capacity.storage_path(value)

    def test_actual_conditional_worker_stops_before_git_on_probe_refusal(self):
        source = (LIVE / "bridgevm-live-worker.sh").read_text()
        function = source[source.index("run_job() {"):source.index("\nmain() {")]
        (self.queue / "job.env").write_text("job_id=fixture\ntier=t1-vtimer\ncommit=" + "a" * 40 + "\n")
        marker = self.root / "git-called"
        command = 'set -euo pipefail\nlog() { :; }\ngit() { touch "$MARKER"; return 77; }\n' + function + '\nrun_job "$JOB" || exit 29\n'
        bin_dir = self.root / "bin"; bin_dir.mkdir(); provider = bin_dir / "python3"
        for body, expected in (("exit 2", "refused-storage-unavailable"), ("echo 99; exit 3", "refused-free-space"), ("echo malformed", "refused-storage-unavailable")):
            provider.write_text("#!/bin/sh\n" + body + "\n"); provider.chmod(0o700)
            env = dict(os.environ, REPO=str(ROOT), JOB=str(self.queue), WORK_ROOT=str(self.work), MIN_FREE_GIB="100", MARKER=str(marker), PATH=str(bin_dir) + os.pathsep + os.environ["PATH"])
            result = subprocess.run(["/bin/bash", "-c", command], env=env, capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 29, result.stderr)
            self.assertFalse(marker.exists())
            self.assertIn("result=" + expected, (self.queue / "result.env").read_text())


if __name__ == "__main__": unittest.main()
