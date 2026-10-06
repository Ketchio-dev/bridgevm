#!/usr/bin/env python3
"""A pair changed after digest cannot become an authenticated retained T19 source."""
from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "selected_retention_fixture", ROOT / "tests/integration/t17-selected-retention-contract.py")
FIXTURE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(FIXTURE)
retention = FIXTURE.retention


class SelectedRetentionMutation(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        FIXTURE.SelectedRetention.setUpClass()

    def setUp(self):
        self.fixture = FIXTURE.SelectedRetention()
        self.fixture.setUp()

    def tearDown(self):
        self.fixture.tearDown()

    def retention_inputs(self) -> tuple[argparse.Namespace, dict]:
        root, app = self.fixture.root, self.fixture.app
        executable = app / "Contents/MacOS/BridgeVMControl"
        executable.parent.mkdir()
        executable.write_text('#!/bin/sh\nshift 2\nwhile [ "$#" -gt 0 ]; do case "$1" in --package) printf "{}\\n" > "$2";; --recovery-code-file) printf "fixture\\n" > "$2"; chmod 600 "$2";; esac; shift 2; done\n')
        executable.chmod(0o700)
        helper = app / "Contents/Helpers/BridgeVMProductE2E.app/Contents/MacOS/BridgeVMProductE2E"
        helper.parent.mkdir(parents=True)
        helper.write_bytes(b"fixture helper")
        runner = app / "Contents/Resources/target/release/hvf-runner"
        runner.write_bytes(b"fixture runner")
        state = root / "vtpm"
        state.mkdir()
        (state / "state.bin").write_bytes(b"synthetic private state")
        request = {**self.fixture.request, "app_executable_path": str(executable), "runner_path": str(runner),
                   "vtpm_state_path": str(state), "job_id": "mutation-fixture", "commit": "a" * 40,
                   "campaign_mode": "pilot", "lane": 1}
        paths = {"app_bundle": app, "app_executable": executable, "product_helper": helper, "runner": runner}
        assets = {key: {"path": str(path), "sha256": retention.T19.tree_hash(path, allow_symlinks=True)
                       if key == "app_bundle" else retention.T19.file_hash(path)} for key, path in paths.items()}
        args = argparse.Namespace(**{key: root / f"{key}.json"
                                    for key in ("request", "result", "stamp", "verified", "status")},
                                  destination=root / "retained/source")
        args.request.write_text(json.dumps(request))
        args.verified.write_text(json.dumps({"verified": True, "campaign_mode": "pilot", "assets": assets}))
        args.stamp.write_text(json.dumps({"request_sha256": retention.T17.digest(args.request)}))
        result = {**self.fixture.expected, **{stage: True for stage in retention.T17.LANE_STAGES},
                  "cleanup_verified": True}
        return args, result

    def assert_changed_pair_refused(self, filename: str):
        args, result = self.retention_inputs()
        selected_paths = list(self.fixture.root.glob(f".bridgevm-pair-v2-*/current/{filename}"))
        self.assertEqual(len(selected_paths), 1)
        selected_path = selected_paths[0]
        logical_before = tuple(retention.T19.file_hash(path)
                               for path in (self.fixture.disk, self.fixture.variables))
        calls = []

        def digest_then_mutate(request):
            pair = FIXTURE.selected(request)  # Real helper takes and releases the lease.
            calls.append(pair)
            with selected_path.open("r+b") as output:
                first = output.read(1)
                output.seek(0)
                output.write(bytes([first[0] ^ 0xff]))  # Preserve size; the hash must expose this.
            return pair

        with (patch.object(retention.T17, "lane", return_value=result),
              patch("retained_windows_media.selected", side_effect=digest_then_mutate)):
            with self.assertRaisesRegex(ValueError, "retained media differs from authenticated T17 output"):
                retention.retain(args)  # The real helper's create exports the changed selected generation.
        self.assertEqual(len(calls), 1)
        self.assertFalse(args.destination.exists())
        self.assertFalse(args.status.exists())
        self.assertEqual(list(args.destination.parent.glob(".source.stage-*")), [])
        self.assertEqual(tuple(retention.T19.file_hash(path)
                               for path in (self.fixture.disk, self.fixture.variables)), logical_before)

    def test_selected_disk_mutation_is_rejected_and_staging_is_removed(self):
        self.assert_changed_pair_refused("disk.raw")

    def test_selected_vars_mutation_is_rejected_and_staging_is_removed(self):
        self.assert_changed_pair_refused("vars.fd")


if __name__ == "__main__":
    unittest.main()
