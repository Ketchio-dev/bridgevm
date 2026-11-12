#!/usr/bin/env python3
"""Workflow syntax checks reject duplicate keys rather than silently dropping one."""
import contextlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest import mock

SCRIPT = Path(__file__).resolve().parents[2] / "scripts/check-workflow-yaml.py"
SPEC = importlib.util.spec_from_file_location("workflow_yaml", SCRIPT)
checker = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(checker)


class WorkflowYamlTest(unittest.TestCase):
    @unittest.skipUnless(importlib.util.find_spec("yaml"), "PyYAML is not installed")
    def test_duplicate_keys_fail_at_any_depth(self):
        for body in (
            "on:\n  workflow_dispatch:\n  workflow_dispatch:\njobs: {}\n",
            "on: {workflow_dispatch: {}, workflow_dispatch: {}}\njobs: {}\n",
            "jobs:\n  test:\n    runs-on: ubuntu-latest\n    runs-on: windows-latest\n",
        ):
            with self.subTest(body=body), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary)
                (directory / "fixture.yml").write_text(body)
                stderr = io.StringIO()
                with mock.patch.object(checker, "WORKFLOWS", directory), contextlib.redirect_stderr(stderr):
                    self.assertEqual(checker.main(), 1)
                self.assertIn("duplicate mapping key", stderr.getvalue())

    def test_current_workflows_pass(self):
        self.assertEqual(checker.main(), 0)


if __name__ == "__main__":
    unittest.main()
