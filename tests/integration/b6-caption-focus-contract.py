"""Host parser contracts; these do not prove native hit-testing or guest focus."""
import os
import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
HELPER = ROOT / "scripts/b6-caption-focus.sh"


class CaptionFocusContracts(unittest.TestCase):
    def invoke(self, response, width=1600, height=900, stale="", failed=False):
        with tempfile.TemporaryDirectory() as directory:
            log = pathlib.Path(directory) / "run.log"
            log.write_text(stale)
            env = dict(os.environ, RUN_LOG=str(log), WIDTH=str(width), HEIGHT=str(height),
                       RESPONSE=response, FAILED="1" if failed else "0", HELPER=str(HELPER))
            return subprocess.run(["bash", "-c", '''
source "$HELPER"
send_ok() { [[ "$FAILED" == 0 ]] || return 1; [[ -z "$RESPONSE" ]] || printf '%s\n' "$RESPONSE" >> "$RUN_LOG"; return 0; }
b6_caption_hid_point 42
'''], env=env, capture_output=True, text=True)

    def test_physical_points_at_three_scales(self):
        for y in (78, 98, 116):
            with self.subTest(y=y):
                result = self.invoke(f"BVCAPTIONPOINT hwnd=42 x=400 y={y} dpi=144 hit=2 owner=42\r")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout, f"{400 * 32767 // 1599}x{y * 32767 // 899}")

    def test_resolution_conversion(self):
        result = self.invoke("BVCAPTIONPOINT hwnd=42 x=400 y=116 dpi=144 hit=2 owner=42", 1280, 720)
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, f"{400 * 32767 // 1279}x{116 * 32767 // 719}")

    def test_rejects_stale_response(self):
        result = self.invoke("", stale="BVCAPTIONPOINT hwnd=42 x=400 y=116 dpi=144 hit=2 owner=42\n")
        self.assertNotEqual(result.returncode, 0)

    def test_rejects_ambiguous_wrong_owner_and_offscreen_points(self):
        valid = "BVCAPTIONPOINT hwnd=42 x=400 y=116 dpi=144 hit=2 owner=42"
        cases = [valid + "\n" + valid, valid.replace("hwnd=42", "hwnd=43"),
                 valid.replace("owner=42", "owner=43"), valid.replace("hit=2", "hit=1"),
                 valid.replace("x=400", "x=1600"), valid.replace("y=116", "y=900"),
                 valid.replace("x=400", "x=-1"), "unrelated output"]
        for value in cases:
            with self.subTest(value=value):
                self.assertNotEqual(self.invoke(value).returncode, 0)

    def test_rejects_failed_agent_command(self):
        self.assertNotEqual(self.invoke("BVCAPTIONPOINT hwnd=42 x=400 y=116 dpi=144 hit=2 owner=42", failed=True).returncode, 0)


if __name__ == "__main__":
    unittest.main()
