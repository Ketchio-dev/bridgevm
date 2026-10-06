#!/usr/bin/env python3
"""Loss of an attempted preparation's output cannot prove absent writers."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import MagicMock, patch

from d11_fixture_test_support import COMMIT, binding, rows
from d11_fixture_files import document, record
from d11_fixture_receipt import collect, empty
from d11_fixture_queue import run


class Attempt(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.job = self.root / 'job'; self.job.mkdir()
        self.output = self.root / 'absent-output'
        self.binding = binding()

    def collect(self):
        with patch('d11_fixture_receipt.bound', return_value=(self.binding, rows(self.root))), \
             patch('d11_fixture_receipt.output_path', return_value=self.output):
            return collect(self.job, COMMIT)

    def test_never_started_missing_output_can_be_clean_refusal(self):
        result = self.collect()
        self.assertTrue(result['worker_cleanup_verified'])
        self.assertEqual(result['reason'], 'clean-refusal')

    def test_attempted_missing_output_cannot_be_clean_refusal(self):
        record(self.job / 'd11-attempt.private.json', self.binding)
        result = self.collect()
        self.assertFalse(result['worker_cleanup_verified'])
        self.assertEqual(result['reason'], 'cleanup-unproved')

    def test_attempt_is_recorded_before_output_creation(self):
        def create(name):
            self.assertEqual(document(self.job / 'd11-attempt.private.json'), self.binding)
            self.assertEqual((self.job / 'd11-attempt.private.json').stat().st_mode & 0o222, 0)
            self.output.mkdir()
            return self.output
        with patch(run.__module__ + '.bound', return_value=(self.binding, rows(self.root))), \
             patch(run.__module__ + '.Inputs', return_value=MagicMock()), \
             patch(run.__module__ + '.create_output', side_effect=create), \
             patch(run.__module__ + '.execute') as execute, \
             patch(run.__module__ + '.require_source'), \
             patch(run.__module__ + '.collect', return_value=empty(self.binding, 'incomplete')):
            self.assertEqual(run(self.job, self.root, COMMIT,
                                 self.job / 'input-manifest.tsv', self.job / 'hvf_gic_boot_probe'), 1)
        execute.assert_called_once()


if __name__ == '__main__': unittest.main()
