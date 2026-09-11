#!/usr/bin/env python3
"""Early diagnostic status remains separate and cannot overwrite an earlier run."""
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
STATUS = "reference_inventory=unavailable\nobservation_only=true\nreference_accepted=false\ncriterion_pass=false\n"


class EarlyInventoryContract(unittest.TestCase):
    def test_separate_status_and_no_overwrite(self):
        for existing in (False, True):
            with self.subTest(existing=existing), tempfile.TemporaryDirectory(prefix="b6 early inventory ") as directory:
                root = Path(directory)
                scripts = root / "scripts"
                scripts.mkdir()
                (scripts / "b6-collect-reference-inventory.sh").write_text(
                    'printf called > "$OUT/collector-called"\n'
                    'printf "%s" "$FIXTURE_STATUS" > "$OUT/reference-inventory-status.txt"\n')
                target = root / "reference-inventory-before-readiness-status.txt"
                if existing:
                    target.write_text("preserve previous evidence\n")
                command = 'set -eu; REPO="$1"; OUT="$2"; FIXTURE_STATUS="$3"; source "$4"'
                result = subprocess.run(
                    ["bash", "-c", command, "contract", str(root), str(root), STATUS,
                     str(ROOT / "scripts/b6-pre-readiness-inventory.sh")],
                    capture_output=True, text=True, timeout=5)
                if existing:
                    self.assertNotEqual(result.returncode, 0)
                    self.assertEqual(target.read_text(), "preserve previous evidence\n")
                    self.assertFalse((root / "collector-called").exists())
                else:
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(target.read_text(), STATUS)
                    self.assertFalse((root / "reference-inventory-status.txt").exists())


if __name__ == "__main__":
    unittest.main()
