"""No-device tests: bounded owned commands and strict attachment identities."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import time
import unittest

from a19_exfat_fixture_io import Commands
from a19_exfat_fixture_mount import entities
from a19_exfat_fixture_runner import SOURCE, invoke, validated

spec = importlib.util.spec_from_file_location('group', SOURCE / 'scripts/live-gates/guest_input_owned_group.py')
group = importlib.util.module_from_spec(spec)
spec.loader.exec_module(group)


class FixtureTests(unittest.TestCase):
    def test_exact_boundary_eof_is_complete(self):
        with tempfile.TemporaryDirectory(prefix='exfat-no-device-boundary-') as temporary:
            commands = Commands(Path(temporary), group.stop, time.monotonic())
            code, raw, _ = commands.run(['/usr/bin/python3', '-c', 'import os; os.write(1,b"x"*524288)'], 5)
            self.assertEqual(code, 0)
            self.assertEqual(len(raw), 524288)
            self.assertTrue(commands.cleanup_verified)

    def test_overflow_refuses_without_valid_truncation(self):
        with tempfile.TemporaryDirectory(prefix='exfat-no-device-overflow-') as temporary:
            commands = Commands(Path(temporary), group.stop, time.monotonic())
            with self.assertRaises(ValueError):
                commands.run(['/usr/bin/python3', '-c', 'import os; os.write(1,b"x"*524289)'], 5)
            self.assertTrue(commands.records[0]['stdout']['overflow'])
            self.assertFalse(commands.records[0]['stdout']['complete'])
            self.assertEqual(commands.records[0]['stdout']['stored'], 524288)
            self.assertTrue(commands.cleanup_verified)

    def test_timeout_reaps_only_its_owned_leader(self):
        with tempfile.TemporaryDirectory(prefix='exfat-no-device-timeout-') as temporary:
            commands = Commands(Path(temporary), group.stop, time.monotonic())
            with self.assertRaises(ValueError):
                commands.run(['/usr/bin/python3', '-c', 'import time; time.sleep(20)'], .1)
            self.assertEqual(commands.records[0]['error'], 'TimeoutExpired')
            self.assertTrue(commands.records[0]['group_cleanup_verified'])
            self.assertTrue(commands.cleanup_verified)
            self.assertEqual(group.state(commands.records[0]['pid']), 'absent')

    def test_inventory_raw_is_not_written(self):
        with tempfile.TemporaryDirectory(prefix='exfat-no-device-private-') as temporary:
            root = Path(temporary)
            commands = Commands(root, group.stop, time.monotonic())
            _, raw, _ = commands.run(['/usr/bin/python3', '-c',
                                     'print(bytes.fromhex("666f726569676e2d696d6167652d70617468").decode())'], 5,
                                     private_inventory=True)
            self.assertIn(b'foreign-image-path', raw)
            self.assertEqual(list(root.glob('*.log')), [])
            self.assertNotIn('foreign-image-path', next(root.glob('*.json')).read_text())

    def test_remaining_budget_does_not_spawn(self):
        with tempfile.TemporaryDirectory(prefix='exfat-no-device-budget-') as temporary:
            commands = Commands(Path(temporary), group.stop, time.monotonic() - 250)
            with self.assertRaises(TimeoutError):
                commands.run(['/usr/bin/python3', '-c', 'raise RuntimeError("must not spawn")'])
            self.assertEqual(commands.records, [])

    def test_owned_whole_device_and_exact_mount(self):
        path = Path('/owned/mount')
        value = [{'dev-entry': '/dev/disk777'}, {'dev-entry': '/dev/disk777s1', 'mount-point': str(path)}]
        self.assertEqual(entities(value, path), ('/dev/disk777', ['/dev/disk777s1']))
        for bad in ([{'dev-entry': '/dev/disk777'}, {'dev-entry': '/dev/disk778'}],
                    [{'dev-entry': '/dev/disk777', 'mount-point': '/foreign/mount'}],
                    [{'dev-entry': '/dev/disk777'}, {'dev-entry': '/dev/disk777'}]):
            with self.assertRaises(ValueError):
                entities(bad, path)

    def test_command_refusal_retains_both_case_path_states(self):
        class Refuse:
            def run(self, *_args, **_kwargs):
                raise ValueError('modeled command refusal')
        with tempfile.TemporaryDirectory(prefix='exfat-no-device-case-state-') as temporary:
            root = Path(temporary)
            dest = root / 'dest'
            dest.mkdir()
            result = {'cases': []}
            with self.assertRaises(ValueError):
                invoke(Refuse(), root / 'unused-cli', result, root, 'refused', dest, 8192, [])
            self.assertTrue(result['cases'][0]['after']['present'])
            self.assertFalse(result['cases'][0]['staging_after']['present'])
            self.assertTrue((root / 'case-refused.private.json').is_file())

    def test_extra_member_is_not_a_product_verify_failure(self):
        expected = [{'sha256': 'a'}, {'sha256': 'b'}]
        value = {'entries': {'disk.raw': {'bytes': 4096, 'sha256': 'a'},
                            'vars.fd': {'bytes': 4096, 'sha256': 'b'}, 'manifest.json': {}, '._disk.raw': {}},
                 'manifest': {'disk_bytes': 4096, 'vars_bytes': 4096, 'disk_sha256': 'a', 'vars_sha256': 'b'}}
        validated(value, expected)
        with self.assertRaises(ValueError):
            validated(value, expected, exact=True)


if __name__ == '__main__':
    unittest.main()
