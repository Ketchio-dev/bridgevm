#!/usr/bin/env python3
"""Allocation identity regressions; no GUI, guest or private media."""
from pathlib import Path
import tempfile
import unittest

from product_e2e_work_fixture import WORK, allocate


class WorkBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="e2e-work-boundary-")
        self.addCleanup(self.temporary.cleanup)
        self.lane = allocate(self.temporary.name, job="bound", kind="e2e")
        self.request = {"lane_root": str(self.lane), "job_id": "bound", "lane": 1,
                        **WORK.capture(self.lane, "bound", "e2e", 1)}

    def test_exact_bound_work_is_accepted(self):
        self.assertEqual(WORK.validate(self.request, "e2e"), self.lane.parent)

    def test_parent_job_lane_and_identity_cannot_be_rebound(self):
        for key, value in (("job_id", "other"), ("lane", 2), ("lane", True),
                           ("work_identity", "0:0"), ("work_parent_identity", "0:0"),
                           ("work_parent", str(self.lane.parent)), ("lane_root", str(self.lane) + "/../lane-1")):
            with self.subTest(key=key), self.assertRaises(ValueError):
                WORK.validate({**self.request, key: value}, "e2e")

    def test_replaced_work_directory_refuses_original_identity(self):
        work = self.lane.parent
        work.rename(work.with_name("held"))
        work.mkdir(mode=0o700); self.lane.mkdir(mode=0o700)
        with self.assertRaises(ValueError):
            WORK.validate(self.request, "e2e")

    def test_parent_or_work_symlink_and_public_permissions_refuse(self):
        work = self.lane.parent
        for directory in (work, work.parent, self.lane):
            directory.chmod(0o755)
            with self.assertRaises(ValueError):
                WORK.validate(self.request, "e2e")
            directory.chmod(0o700)
        held = work.with_name("held")
        work.rename(held); work.symlink_to(held, target_is_directory=True)
        with self.assertRaises(ValueError):
            WORK.validate(self.request, "e2e")

    def test_offline_shape_keeps_exact_parent_suffix_and_spelling(self):
        for root in (str(self.lane) + "/", str(self.lane).replace("Ab12Cd", "Ab12Cd7"),
                     str(self.lane).replace("lane-1", "lane-2")):
            with self.assertRaises(ValueError):
                WORK.layout(root, self.request["work_parent"], "bound", "e2e", 1)


if __name__ == "__main__":
    unittest.main()
