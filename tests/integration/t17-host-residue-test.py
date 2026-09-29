#!/usr/bin/env python3
"""T17 cleanup-fence contracts: unreadable host probes fail closed, and an
unreleased guest-setup harvest keeps the job tree and its cleanup unverified."""
from __future__ import annotations

import json
from pathlib import Path
import plistlib
import re
import secrets
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import t17_host_residue as residue  # noqa: E402

LEFTOVER_JOB = "diagnostic-harvest-leftover-fixture"
ROOT_LINE = "/dev/disk3s1s1 on / (apfs, sealed, local, read-only, journaled)"


class ResidueTest(unittest.TestCase):
    def setUp(self) -> None:
        if sys.platform != "darwin":
            self.skipTest("the T17 cleanup fence reads the macOS mount table and hdiutil image list")
        work = Path("/tmp") / f"bridgevm-e2e-residue-fixture.{secrets.token_hex(3)}"
        work.mkdir(mode=0o700)
        self.addCleanup(shutil.rmtree, work)
        self.work = work.resolve()
        (self.work / "lane-1").mkdir()
        self.private = Path(tempfile.mkdtemp(prefix="t17-residue-private-")).resolve()
        self.addCleanup(shutil.rmtree, self.private)
        self.tools = Path(tempfile.mkdtemp(prefix="t17-residue-tools-")).resolve()
        self.addCleanup(shutil.rmtree, self.tools)
        self.images = plistlib.dumps({"images": []}).decode()

    def tool(self, name: str, body: str) -> str:
        path = self.tools / name
        path.write_text("#!/bin/sh\n" + body + "\n")
        path.chmod(0o700)
        return str(path)

    def check(self, mount: str | None = None, hdiutil: str | None = None) -> str | None:
        mount = f"printf '%s\\n' '{ROOT_LINE}'" if mount is None else mount
        hdiutil = f"cat <<'PLIST'\n{self.images}PLIST" if hdiutil is None else hdiutil
        with mock.patch.object(residue, "MOUNT", self.tool("mount", mount)), \
                mock.patch.object(residue, "HDIUTIL", self.tool("hdiutil", hdiutil)):
            return residue.residue(str(self.work), str(self.private))

    def index(self, value: object) -> None:
        (self.private / "t17-diagnostic-lane-1-index.json").write_text(json.dumps(value))

    def test_clean_host_state_allows_removal(self) -> None:
        self.assertIsNone(self.check())
        self.index({"guest_setup": {"cleanup": "verified"}})
        self.assertIsNone(self.check())
        self.index({"guest_setup": {"cleanup": "not-needed"}})
        self.assertIsNone(self.check())

    def test_unreadable_or_unbounded_probes_fail_closed(self) -> None:
        with mock.patch.object(residue.HARVEST, "RELEASE_SECONDS", 0.5):
            for probe, body, reason in (
                    ("hdiutil", "exit 1", "image-list-unreadable"),
                    ("hdiutil", "echo not-a-plist", "image-list-unreadable"),
                    ("hdiutil", "exec sleep 5", "image-list-unreadable"),
                    ("mount", "exit 1", "mount-table-unreadable"),
                    ("mount", "exit 0", "mount-table-unreadable"),
                    ("mount", "exec sleep 5", "mount-table-unreadable")):
                with self.subTest(probe=probe, body=body):
                    self.assertEqual(self.check(**{probe: body}), reason)
        missing = mock.patch.object(residue, "HDIUTIL", str(self.tools / "absent"))
        with missing, mock.patch.object(residue, "MOUNT", self.tool("mount", f"echo '{ROOT_LINE}'")):
            self.assertEqual(residue.residue(str(self.work), str(self.private)), "image-list-unreadable")

    def test_listed_mount_or_image_under_the_tree_is_residue(self) -> None:
        volume = f"{self.work}/lane-1/guest-setup-harvest/volume"
        self.assertEqual(self.check(mount=f"printf '%s\\n' '{ROOT_LINE}' '/dev/disk99s3 on {volume} (ntfs, local, read-only)'"),
                         "mounted")
        self.images = plistlib.dumps({"images": [{"image-path": f"{self.work}/lane-1/guest-setup-harvest/disk.raw",
                                                  "system-entities": [{"dev-entry": "/dev/disk99"}]}]}).decode()
        self.assertEqual(self.check(), "attached")

    def test_unreleased_or_unproven_harvest_is_residue(self) -> None:
        harvest_dir = self.work / "lane-1" / residue.HARVEST.WORK_NAME
        harvest_dir.mkdir()
        self.assertEqual(self.check(), "guest-setup-harvest-unreleased")
        harvest_dir.rmdir()
        for value in ({"guest_setup": {"cleanup": "failed"}}, {"guest_setup": {}}, {"guest_setup": None}, []):
            with self.subTest(index=value):
                self.index(value)
                self.assertEqual(self.check(), "guest-setup-release-unproven")
        index = self.private / "t17-diagnostic-lane-1-index.json"
        index.write_text("{")
        self.assertEqual(self.check(), "guest-setup-release-unproven")
        index.unlink()
        outside = self.work / "verified.json"
        outside.write_text(json.dumps({"guest_setup": {"cleanup": "verified"}}))
        index.symlink_to(outside)
        self.assertEqual(self.check(), "guest-setup-release-unproven")

    def test_tree_outside_the_job_boundary_is_refused(self) -> None:
        self.assertEqual(residue.residue("/tmp/bridgevm-e2e-x.1234567", str(self.private)), "work-outside-job-boundary")
        self.assertEqual(residue.residue(str(self.private), str(self.private)), "work-outside-job-boundary")

    def test_this_host_probes_read_and_show_no_residue_for_a_fresh_tree(self) -> None:
        self.assertIsNone(residue.residue(str(self.work), str(self.private)))


def augment_synthetic_helper(path: Path) -> None:
    """Leave a harvest work directory in the lane, as an unproven release would."""
    path.write_text(path.read_text() + f'''
if r["job_id"] == "{LEFTOVER_JOB}": (pathlib.Path(r["lane_root"]) / "guest-setup-harvest").mkdir(mode=0o700)
''')


def run_tier_fixture(tier: Path, manifest: Path, temporary: Path) -> None:
    out = temporary / f"{LEFTOVER_JOB}-out"
    completed = subprocess.run([str(tier), "--out", str(out), "--input-manifest", str(manifest),
                                "--job-id", LEFTOVER_JOB], capture_output=True, text=True, check=False)
    private = out / "private"
    helper_log = (private / "lane-1-helper.log").read_text()
    lane_root = Path(next(line.removeprefix("lane_root=") for line in helper_log.splitlines()
                          if line.startswith("lane_root=")))
    work = lane_root.parent
    assert re.fullmatch(r"/(?:private/)?tmp/bridgevm-e2e-" + re.escape(LEFTOVER_JOB) + r"\.[A-Za-z0-9]{6}", str(work))
    try:
        assert completed.returncode != 0, "an unreleased harvest let the tier pass"
        receipt = json.loads((out / "receipt.json").read_text())
        assert receipt["outcome"] == receipt["failure_code"] == "cleanup-failed", (receipt["outcome"], completed.stderr[-1200:])
        assert receipt["worker_cleanup_verified"] is False and receipt["pass"] is False
        assert "host residue guest-setup-harvest-unreleased" in completed.stderr, completed.stderr[-1200:]
        assert (private / "cleanup-failed").is_file()
        assert (lane_root / "guest-setup-harvest").is_dir(), "the fence removed an unreleased harvest"
        setup = json.loads((private / "t17-diagnostic-lane-1-index.json").read_text())["guest_setup"]
        assert (setup["outcome"], setup["reason"], setup["cleanup"]) == ("work-unsafe", "EEXIST", "not-needed"), setup
    finally:
        if work.exists():
            shutil.rmtree(work)


if __name__ == "__main__":
    if sys.argv[1:2] == ["--augment-helper"]:
        augment_synthetic_helper(Path(sys.argv[2]))
    elif sys.argv[1:2] == ["--tier-fixture"]:
        run_tier_fixture(Path(sys.argv[2]), Path(sys.argv[3]), Path(sys.argv[4]))
    else:
        unittest.main()
