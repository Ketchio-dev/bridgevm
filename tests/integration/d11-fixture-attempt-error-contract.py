#!/usr/bin/env python3
"""Expected preparation errors retain a nonpromoting, cleanup-unproved receipt."""
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import MagicMock, patch

from d11_fixture_test_support import COMMIT, binding, rows
from d11_fixture_files import document
from d11_fixture_queue import run


class AttemptErrors(unittest.TestCase):
    def test_expected_errors_after_attempt_keep_receipt_and_refusal(self):
        for phase in ('create_output', 'execute'):
            for error in (OSError('synthetic private detail'), ValueError('synthetic private detail'),
                          subprocess.SubprocessError('synthetic private detail')):
                with self.subTest(phase=phase, error=type(error).__name__), tempfile.TemporaryDirectory() as temp:
                    root = Path(temp).resolve(); job = root / 'job'; job.mkdir()
                    value = binding(); inputs = MagicMock()
                    with patch(run.__module__ + '.bound', return_value=(value, rows(root))), \
                         patch(run.__module__ + '.Inputs', return_value=inputs), \
                         patch(run.__module__ + '.create_output', return_value=root / 'output') as create, \
                         patch(run.__module__ + '.execute') as execute:
                        (create if phase == 'create_output' else execute).side_effect = error
                        result = run(job, root, COMMIT, job / 'input-manifest.tsv', job / 'hvf_gic_boot_probe')
                    self.assertEqual(result, 1)
                    self.assertEqual(document(job / 'd11-attempt.private.json'), value)
                    self.assertEqual(document(job / 'd11-refusal.private.json'),
                                     {'reason': 'admission-or-integrity-refused'})
                    receipt = document(job / 'receipt.json')
                    self.assertEqual(receipt['reason'], 'cleanup-unproved')
                    for key in ('worker_cleanup_verified', 'sealed_fixture', 'criterion_pass', 'claim_eligible', 'pass'):
                        self.assertIs(receipt[key], False)
                    self.assertNotIn('synthetic private detail', (job / 'receipt.json').read_text())
                    inputs.__exit__.assert_called_once()


if __name__ == '__main__': unittest.main()
