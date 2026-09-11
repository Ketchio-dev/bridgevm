#!/usr/bin/env python3
"""Host-side collection is not an accepted graphics reference."""
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class CollectionContract(unittest.TestCase):
    def test_collection_boundaries(self):
        for mode in ("normal", "send_failure", "missing", "empty", "oversize", "symlink", "bad_nonce"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as directory:
                script = r'''
set -eu
REPO="$1"; OUT="$2"; mode="$3"
mkdir "$OUT/share"
uuidgen() { if [[ "$mode" == bad_nonce ]]; then printf invalid; else printf 01234567-89AB-CDEF-0123-456789ABCDEF; fi; }
wait_for() { return 0; }
send_ok() {
  local output="$OUT/share/$b6_reference_name.json"
  case "$mode" in
    missing) return 0 ;;
    empty) : > "$output" ;;
    oversize) head -c 65537 /dev/zero > "$output" ;;
    symlink) ln -s "$OUT/source.json" "$output" ;;
    *) printf '{"reference_accepted":false}\n' > "$output" ;;
  esac
  [[ "$mode" != send_failure ]]
}
printf '{"reference_accepted":false}\n' > "$OUT/source.json"
source "$REPO/scripts/b6-collect-reference-inventory.sh"
'''
                subprocess.run(["bash", "-c", script, "contract", str(ROOT), directory, mode],
                               capture_output=True, text=True, timeout=5, check=True)
                status = dict(line.split("=", 1) for line in
                              (Path(directory) / "reference-inventory-status.txt").read_text().splitlines())
                self.assertEqual(status["reference_inventory"], "collected" if mode == "normal" else "unavailable")
                self.assertEqual(status["observation_only"], "true")
                self.assertEqual(status["reference_accepted"], "false")
                self.assertEqual(status["criterion_pass"], "false")


if __name__ == "__main__":
    unittest.main()
