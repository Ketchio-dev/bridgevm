#!/usr/bin/env python3
"""Fresh display proof must name the requested supported mode and current size."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

HELPER = Path(__file__).resolve().parents[2] / "scripts/b6-display-proof.sh"


class DisplayProofTests(unittest.TestCase):
    def run_proof(self, fresh, *, initial="", width=1280, height=720, status=0):
        with tempfile.TemporaryDirectory(prefix="b6 display ") as directory:
            log = Path(directory) / "run.log"
            command = Path(directory) / "command.txt"
            log.write_text(initial)
            env = dict(os.environ, RUN_LOG=str(log), COMMAND_FILE=str(command),
                       FRESH=fresh, WIDTH=str(width), HEIGHT=str(height), COMMAND_STATUS=str(status))
            script = '''source "$1"
    send_ok() {
      printf '%s\\n' "$1" > "$COMMAND_FILE"
      printf '%s' "$FRESH" >> "$RUN_LOG"
      return "$COMMAND_STATUS"
    }
    b6_display_matches "$WIDTH" "$HEIGHT"
    '''
            result = subprocess.run(["bash", "-eu", "-o", "pipefail", "-c", script, "_", str(HELPER)],
                                    env=env, capture_output=True, text=True, timeout=10)
            sent = command.read_text().strip() if command.exists() else None
            return result.returncode, sent

    @staticmethod
    def proof(width=1280, height=720, modes=28, supported="True"):
        return (f"BVF2 device=\\\\.\\DISPLAY2 current={width}x{height} modes={modes} "
                f"has_{width}x{height}={supported}\r\n")

    def test_supported_modes(self):
        for width, height in [(1280,720), (1600,900), (1920,1080)]:
            with self.subTest(width=width, height=height):
                code, command = self.run_proof(self.proof(width,height), width=width, height=height)
                self.assertEqual(code, 0)
                self.assertEqual(command, "powershell -NoProfile -ExecutionPolicy Bypass -File "
                                 f"C:\\BridgeVMClosure\\bv-windows-closure-proof.ps1 -Action Display -RequestedMode {width}x{height}")

    def test_stale_proof_cannot_pass(self):
        self.assertNotEqual(self.run_proof("new reply\n", initial=self.proof())[0], 0)

    def test_wrong_current_size(self):
        self.assertNotEqual(self.run_proof(self.proof().replace("current=1280x720", "current=1600x900"))[0], 0)

    def test_legacy_field_cannot_pass_smaller_cell(self):
        self.assertNotEqual(self.run_proof(self.proof().replace("has_1280x720", "has_1600x900"))[0], 0)

    def test_missing_requested_mode(self):
        self.assertNotEqual(self.run_proof(self.proof(supported="False"))[0], 0)

    def test_single_mode_is_rejected(self):
        self.assertNotEqual(self.run_proof(self.proof(modes=1))[0], 0)

    def test_failed_command_cannot_pass(self):
        self.assertNotEqual(self.run_proof(self.proof(), status=1)[0], 0)

    def test_duplicate_proofs_are_ambiguous(self):
        self.assertNotEqual(self.run_proof(self.proof()+self.proof())[0], 0)

    def test_unsupported_size_never_sends(self):
        code, command = self.run_proof(self.proof(), width=1024, height=768)
        self.assertNotEqual(code, 0)
        self.assertIsNone(command)

    def test_fresh_proof_after_stale_wrong_size(self):
        self.assertEqual(self.run_proof(self.proof(), initial=self.proof(1600,900))[0], 0)


if __name__ == "__main__":
    unittest.main()
