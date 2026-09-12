#!/usr/bin/env python3
"""Exercise the gate's real clone block with immutable source permissions."""
import os
from pathlib import Path
import platform
import stat
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SOURCE = (ROOT / "scripts/verify-snapshot-restore-boots.sh").read_text()
START = SOURCE.index('cp -c "$DISK"')
BLOCK = SOURCE[START:SOURCE.index("\nsend_wait()", START)]


class ClonePermissionsContract(unittest.TestCase):
    def exercise(self, deny_chmod=False):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            disk, variables, work = root / "disk", root / "vars", root / "work"
            work.mkdir()
            for path in (disk, variables):
                path.write_bytes(path.name.encode())
                path.chmod(0o400)
            script = 'DISK=$1; VARS=$2; WORK=$3\nfail() { exit 1; }\n'
            if platform.system() != "Darwin":
                # Hosted Linux checks permissions, not APFS clone semantics.
                script += 'cp() { [[ "$1" != -c ]] || shift; command cp -p "$@"; }\n'
            if deny_chmod:
                script += 'chmod() { return 1; }\n'
            result = subprocess.run(["bash", "-c", script + BLOCK, "contract",
                                     str(disk), str(variables), str(work)], timeout=10)
            self.assertEqual(result.returncode, 1 if deny_chmod else 0)
            for original, name in ((disk, "disk.raw"), (variables, "vars.fd")):
                self.assertEqual(stat.S_IMODE(original.stat().st_mode), 0o400)
                self.assertEqual(original.read_bytes(), original.name.encode())
                if not deny_chmod:
                    copied = work / name
                    self.assertTrue(copied.stat().st_mode & stat.S_IWUSR)
                    self.assertEqual(copied.read_bytes(), original.read_bytes())
                    descriptor = os.open(copied, os.O_RDWR)
                    os.close(descriptor)

    def test_only_private_clones_become_writable(self):
        self.exercise()

    def test_permission_update_failure_stops_gate(self):
        self.exercise(deny_chmod=True)


if __name__ == "__main__":
    unittest.main()
