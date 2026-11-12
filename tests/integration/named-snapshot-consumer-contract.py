#!/usr/bin/env python3
"""Regression: retained native results bind timestamped children, not the removed fixed slot."""
import json
from pathlib import Path
import sys
import tempfile
import unittest
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts/live-gates'))
from a19_first_restore_collection import cli_result, VM_ID

class NamedSnapshotConsumerTests(unittest.TestCase):
    def test_create_and_restore_bind_the_same_timestamped_child(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp)
            library = output / 'live/library'
            parent = library / VM_ID / 'bundle.vmbridge/metadata/snapshots'
            def write(filename, command, path):
                (output / filename).write_text(json.dumps(dict(schema='bridgevm.app-snapshot.v1', command=command,
                    vmID=VM_ID, libraryPath=str(library), snapshotPath=str(path), complete=True)))
            snapshot = parent / '20261112-120000.snapshot'
            write('create.json', 'create', snapshot)
            write('restore-retry.json', 'restore', snapshot)
            cli_result(output, 'create.json', 'create')
            cli_result(output, 'restore-retry.json', 'restore')
            for bad in [parent / '20261112-130000.snapshot', parent.parent / '20261112-120000.snapshot', parent / 'other.snapshot']:
                write('restore-retry.json', 'restore', bad)
                with self.assertRaises(ValueError): cli_result(output, 'restore-retry.json', 'restore')

    def test_shell_gates_resolve_the_create_result_instead_of_a_fixed_slot(self):
        for name in ['verify-native-snapshot-restore-boots.sh', 'verify-native-snapshot-interrupted-restore.sh']:
            source = (ROOT / 'scripts' / name).read_text()
            self.assertNotIn('SNAP=$BUNDLE/metadata/snapshots/latest.snapshot', source)
            self.assertIn('native_snapshot_result.py', source)

if __name__ == '__main__': unittest.main()
