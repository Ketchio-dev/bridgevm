#!/usr/bin/env python3
"""Disposable repositories must not leave automatic Git writers behind."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from t12_sealed_test_support import Fixture


class FixtureLifetime(unittest.TestCase):
    def test_fixture_does_not_start_automatic_maintenance(self):
        with tempfile.TemporaryDirectory() as directory:
            trace = Path(directory) / "git-trace.jsonl"
            with patch.dict(os.environ, {"GIT_TRACE2_EVENT": str(trace)}):
                fixture = Fixture()
                try:
                    events = [json.loads(row) for row in trace.read_text().splitlines()]
                    starts = [event["argv"] for event in events if event.get("event") == "start"]
                    self.assertTrue(any("commit" in argv for argv in starts), starts)
                    children = [event["argv"] for event in events if event.get("event") == "child_start"]
                    automatic = [argv for argv in children if "--auto" in argv
                                 and ("maintenance" in argv or "gc" in argv)]
                    self.assertEqual(automatic, [], "automatic Git writers can outlive the disposable fixture")
                finally:
                    fixture.close()
                self.assertFalse(fixture.base.exists())


if __name__ == "__main__": unittest.main()
