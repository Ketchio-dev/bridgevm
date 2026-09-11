#!/usr/bin/env python3
"""Execute only B6's initial asset copy, never its VM or guest commands."""
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class StagingTests(unittest.TestCase):
    def command(self):
        source = (ROOT / "scripts/b6-cell-capture.sh").read_text()
        matches = re.findall(r'^cp "\$REPO/scripts/win-assets/.*?"\$OUT/share/"', source, re.M | re.S)
        self.assertEqual(len(matches), 1, "one initial asset-copy command is required")
        return matches[0]

    def stage(self, command, root):
        (root / "share").mkdir()
        presentmon = root / "PresentMon.exe"
        presentmon.write_bytes(b"test-only placeholder")
        env = dict(os.environ, REPO=str(ROOT), OUT=str(root), PRESENTMON=str(presentmon))
        return subprocess.run(["bash", "-eu", "-c", command], env=env,
                              capture_output=True, text=True, timeout=10)

    def test_real_copy_stages_native_dependencies(self):
        with tempfile.TemporaryDirectory(prefix="b6 staging ") as directory:
            root = Path(directory)
            result = self.stage(self.command(), root)
            self.assertEqual(result.returncode, 0, result.stderr)
            for name in ("bv-b6-native-uia.cs", "bv-b6-physical-uia.cs",
                         "bv-b6-native-tip-point.ps1", "bv-b6-tip-owner.cs"):
                self.assertEqual((root / "share" / name).read_bytes(),
                                 (ROOT / "scripts/win-assets" / name).read_bytes())

    def test_fused_quoted_path_is_rejected(self):
        correct = 'bv-b6-native-uia.cs" "$REPO/scripts/win-assets/bv-b6-physical-uia.cs'
        command = self.command()
        self.assertIn(correct, command)
        command = command.replace(correct, "bv-b6-native-uia.cs bv-b6-physical-uia.cs", 1)
        with tempfile.TemporaryDirectory() as directory:
            self.assertNotEqual(self.stage(command, Path(directory)).returncode, 0)


if __name__ == "__main__":
    unittest.main()
