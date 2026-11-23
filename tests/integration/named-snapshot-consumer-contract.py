#!/usr/bin/env python3
"""Regression: retained native results bind timestamped children, not the removed fixed slot."""
import json
from pathlib import Path
import sys
import tempfile
import unittest
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts/live-gates'))
from native_snapshot_result import snapshot_path

class NamedSnapshotConsumerTests(unittest.TestCase):
    def test_path_boundary_rejects_symlinks_and_accepts_legacy(self):
        with tempfile.TemporaryDirectory() as tmp:
            bundle = Path(tmp) / 'bundle'
            parent = bundle / 'metadata/snapshots'
            parent.mkdir(parents=True)
            outside = Path(tmp) / 'outside'
            outside.mkdir()
            target = parent / '20261112-120000.snapshot'
            target.symlink_to(outside, target_is_directory=True)
            with self.assertRaises(ValueError): snapshot_path(str(target), bundle)
            self.assertEqual(snapshot_path(str(parent / 'latest.snapshot'), bundle), parent / 'latest.snapshot')
            target.unlink()
            parent.rmdir()
            parent.symlink_to(outside, target_is_directory=True)
            with self.assertRaises(ValueError): snapshot_path(str(target), bundle)

    def test_shell_gates_resolve_the_create_result_instead_of_a_fixed_slot(self):
        for name in ['verify-native-snapshot-restore-boots.sh']:
            source = (ROOT / 'scripts' / name).read_text()
            self.assertNotIn('SNAP=$BUNDLE/metadata/snapshots/latest.snapshot', source)
            self.assertIn('native_snapshot_result.py', source)

if __name__ == '__main__': unittest.main()
