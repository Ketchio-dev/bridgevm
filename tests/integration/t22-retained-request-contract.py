#!/usr/bin/env python3
"""Both retained request generations are offline; neither opens guest/key data."""
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from t22_pair_fixtures import PairFixture, write_json, JOB
from t22_pair_provenance import admit
import product_e2e_work as WORK


class RetainedRequestContract(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="bridgevm-retained-request-")
        self.root = Path(self.temp.name).resolve()

    def tearDown(self):
        for path in self.root.rglob("*"):
            if not path.is_symlink(): path.chmod(0o700 if path.is_dir() else 0o600)
        self.temp.cleanup()

    def reseal(self, fixture, request):
        fixture.stamp["request_sha256"] = write_json(fixture.request_path, request)
        write_json(fixture.stamp_path, fixture.stamp); fixture.refresh()

    def current(self, fixture, tier):
        parent = self.root / "deleted-output/private"
        kind = "e2e" if tier == "T17" else "import-e2e"
        lane = parent / f"bridgevm-{kind}-{JOB}.Ab12Z9/lane-1"
        self.assertFalse(parent.exists())
        return {**fixture.request, "work_parent": str(parent), "lane_root": str(lane),
                "work_parent_identity": "123:456", "work_identity": "123:789"}

    def test_both_generations_and_tiers_admit_without_touching_deleted_paths_or_keys(self):
        real_open = os.open
        for tier in ("T17", "T19"):
            directory = self.root / tier; directory.mkdir()
            fixture = PairFixture(directory, tier)
            for request in (fixture.request, self.current(fixture, tier)):
                self.reseal(fixture, request)
                def guarded(path, *args, **kwargs):
                    self.assertNotIn("never-open-keys", str(path))
                    self.assertNotIn("deleted-output", str(path))
                    return real_open(path, *args, **kwargs)
                with patch("os.open", side_effect=guarded):
                    admit(*fixture.args())

    def test_partial_malformed_and_wrong_layout_current_requests_refuse_even_when_resealed(self):
        for tier in ("T17", "T19"):
            directory = self.root / tier; directory.mkdir()
            fixture = PairFixture(directory, tier); current = self.current(fixture, tier)
            invalid = [{k:v for k,v in current.items() if k != missing} for missing in WORK.FIELDS]
            invalid += [{**current, key:value} for key,value in (
                ("work_parent_identity", None), ("work_identity", True), ("work_identity", "123"),
                ("work_identity", "-1:4"), ("work_parent", str(self.root)),
                ("lane_root", current["lane_root"].replace(JOB, "wrong-job")),
                ("lane_root", current["lane_root"].replace("lane-1", "lane-2")),
                ("unknown_field", "extra"))]
            for request in invalid:
                self.reseal(fixture, request)
                with self.assertRaises(ValueError): admit(*fixture.args())


if __name__ == "__main__":
    unittest.main()
