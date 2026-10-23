"""The retained T19 source's vTPM export never puts product output in the tier log.

The tier runs retain-windows-import-source.py with its stdout and stderr in the
log, where only booleans, sizes, hashes and fixed tokens may appear. Each case
runs main() as the tier does, with file descriptors 1 and 2 captured, so output
a child process inherits is captured too.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "retain_fixture", ROOT / "scripts/live-gates/retain-windows-import-source.py")
RETAIN = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RETAIN)
DISK, VARS_BYTES = b"authenticated disk", 64 * 1024 * 1024
STUB = ('#!/bin/sh\nshift 2\nwhile [ "$#" -gt 0 ]; do case "$1" in --package) package=$2;; '
        '--recovery-code-file) code=$2;; esac; shift 2; done\n')
WRITE = 'printf "{}\\n" > "$package"; printf "fixture\\n" > "$code"; chmod 600 "$code"\n'
REFUSED = "T17-to-T19 handoff refused: packaged vTPM export "


def run_main(argv: list[str]) -> tuple[int, str]:
    """Return main's exit status and everything written to file descriptors 1 and 2 while it ran."""
    with tempfile.TemporaryFile() as log, mock.patch.object(sys, "argv", ["retain", *argv]):
        sys.stdout.flush(); sys.stderr.flush()
        saved = [os.dup(1), os.dup(2)]
        try:
            os.dup2(log.fileno(), 1); os.dup2(log.fileno(), 2)
            status = RETAIN.main()
        finally:
            sys.stdout.flush(); sys.stderr.flush()
            for descriptor, copy in zip((1, 2), saved):
                os.dup2(copy, descriptor); os.close(copy)
        log.seek(0)
        return status, log.read().decode("utf-8", "replace")


class RetainedExportOutputCases(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="bridgevm-retention-export-")
        self.root = Path(self.temporary.name).resolve()
        self.app = self.root / "Fixture.app"
        self.executable = self.app / "Contents/MacOS/BridgeVMControl"
        self.helper = self.app / "Contents/Helpers/BridgeVMProductE2E.app/Contents/MacOS/BridgeVMProductE2E"
        self.runner = self.app / "Contents/Resources/target/release/hvf-runner"
        for path in (self.executable, self.helper, self.runner):
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"fixture " + path.name.encode())
        self.state = self.root / "vtpm"
        self.state.mkdir()
        (self.state / "state.bin").write_bytes(b"synthetic private state")
        self.args = argparse.Namespace(**{key: self.root / f"{key}.json" for key in (
            "request", "result", "stamp", "verified", "status")}, destination=self.root / "retained/source")
        self.result = {**{stage: True for stage in RETAIN.T17.LANE_STAGES}, "cleanup_verified": True,
                       "final_disk_sha256": hashlib.sha256(DISK).hexdigest(),
                       "final_vars_sha256": hashlib.sha256(bytes(VARS_BYTES)).hexdigest()}

    def tearDown(self):
        for path in self.root.rglob("*"):
            if not path.is_symlink():
                path.chmod(0o700 if path.is_dir() else 0o600)
        self.temporary.cleanup()

    def export_selected(self, request, authenticated, staging):
        media = staging / "selected-media"
        media.mkdir()
        (media / "disk.raw").write_bytes(DISK)
        with (media / "vars.fd").open("wb") as output:
            output.truncate(VARS_BYTES)
        return media / "disk.raw", media / "vars.fd"

    def retain(self, body: str, mode: int = 0o700) -> tuple[int, str]:
        self.executable.write_text(STUB + body)
        self.executable.chmod(mode)
        paths = {"app_bundle": self.app, "app_executable": self.executable,
                 "product_helper": self.helper, "runner": self.runner}
        self.args.request.write_text(json.dumps({
            "app_bundle_path": str(self.app), "app_executable_path": str(self.executable),
            "runner_path": str(self.runner), "vtpm_state_path": str(self.state), "vm_slug": "fixture-vm",
            "job_id": "fixture", "commit": "a" * 40, "campaign_mode": "pilot", "lane": 1}))
        self.args.verified.write_text(json.dumps({"verified": True, "campaign_mode": "pilot", "assets": {
            key: {"path": str(path), "sha256": RETAIN.T19.tree_hash(path, allow_symlinks=True)
                  if key == "app_bundle" else RETAIN.T19.file_hash(path)} for key, path in paths.items()}}))
        self.args.stamp.write_text(json.dumps({"request_sha256": RETAIN.T17.digest(self.args.request)}))
        argv = [f"--{key}={getattr(self.args, key)}"
                for key in ("request", "result", "stamp", "verified", "destination", "status")]
        with mock.patch.object(RETAIN.T17, "lane", return_value=self.result), \
                mock.patch.object(RETAIN, "export_selected", side_effect=self.export_selected):
            return run_main(argv)

    def assert_refused(self, status: int, output: str, token: str):
        self.assertEqual((status, output), (1, REFUSED + token + "\n"))
        self.assertFalse(os.path.lexists(self.args.destination) or os.path.lexists(self.args.status))
        self.assertEqual(list(self.args.destination.parent.glob(".source.stage-*")), [])

    def test_failed_export_prints_only_a_fixed_token_with_its_exit_status(self):
        status, output = self.retain('echo "exported_package=$package"\n'
                                     'echo "ERROR: refusing to overwrite existing output: $package" >&2\nexit 3\n')
        self.assert_refused(status, output, "failed with exit status 3")
        self.assertNotIn("/", output)

    def test_export_output_over_64_kib_is_refused_and_never_echoed(self):
        status, output = self.retain(WRITE + "head -c 65537 /dev/zero | tr '\\0' x >&2\n")
        self.assert_refused(status, output, "output exceeds 65536 bytes")

    def test_export_that_cannot_start_is_refused_without_its_path(self):
        status, output = self.retain(WRITE, mode=0o600)
        self.assert_refused(status, output, "could not start (EACCES)")

    def test_hung_export_is_killed_at_its_timeout_and_refused(self):
        pid_file, real_run = self.root / "export.pid", subprocess.run

        def shortened(*arguments, **options):  # The tool's 120 s bound, shortened to keep the test quick.
            return real_run(*arguments, **{**options, "timeout": 1} if "timeout" in options else options)
        with mock.patch("subprocess.run", shortened):
            status, output = self.retain(f'echo $$ > "{pid_file}"\nexec sleep 5\n')
        self.assert_refused(status, output, "did not finish within 120 s")
        with self.assertRaises(ProcessLookupError):
            os.kill(int(pid_file.read_text()), 0)

    def test_successful_export_still_publishes_verified_inputs_silently(self):
        status, output = self.retain(WRITE + 'echo "exported_package=$package"\n')
        self.assertEqual((status, output), (0, ""))
        self.assertTrue(json.loads(self.args.status.read_text())["verified"])
        self.assertTrue(RETAIN.T19.verify(self.args.destination / "t19-input-manifest.tsv")["verified"])
