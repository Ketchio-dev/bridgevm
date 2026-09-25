#!/usr/bin/env python3
"""T22 marker transport and bounded filename completion contract."""
from __future__ import annotations

import hashlib
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
HOST = ROOT / "scripts/a19-t22-marker-share.sh"
TIER = ROOT / "scripts/verify-native-snapshot-interrupted-restore.sh"
GUEST = ROOT / "scripts/win-assets/bv-a19-t22-marker.ps1"
NONCE = "a" * 32
MARKER = "BV-ORIGINAL-" + "a" * 32


def run_wait(share: Path, output: Path, nonce: str = NONCE, action: str = "Read",
             expected: str = "") -> subprocess.CompletedProcess:
    env = dict(os.environ, SHARE=str(share), OUTPUT=str(output), NONCE=nonce,
               ACTION=action, EXPECTED=expected, HELPER=str(HOST))
    script = ('source "$HELPER"; STEP_TIMEOUT=1; SNAPSHOT_LAUNCHER=$$; '
              't22_marker_wait_result "$SHARE" "$NONCE" "$ACTION" "$EXPECTED" "$OUTPUT"')
    return subprocess.run(["bash", "-c", script], env=env, capture_output=True, text=True,
                          timeout=5)


def result(share: Path, value: str, nonce: str = NONCE, action: str = "Read") -> None:
    name = f"t22-{nonce}-{action}"
    data = value.encode("ascii")
    (share / f"{name}.txt").write_bytes(data)
    (share / f"{name}.done").write_text(hashlib.sha256(data).hexdigest(), encoding="ascii")


class GuestShareContract(unittest.TestCase):
    def test_transport_is_shared_file_and_independent_workload(self):
        host, tier, guest = HOST.read_text(), TIER.read_text(), GUEST.read_bytes()
        self.assertTrue(0 < len(guest) < 8 * 1024 * 1024)
        self.assertTrue(guest.endswith(b"\r\n"))
        self.assertEqual(guest.count(b"\n"), guest.count(b"\r\n"))
        self.assertIn('source "$REPO/scripts/a19-t22-marker-share.sh"', tier)
        self.assertNotIn("powershell -NoProfile -Command", tier)
        for required in ('--agent-share-host "$share"', "--agent-share-guest 'C:\\bridgevm-share'",
                         'shasum -a 256 "$share/bv-a19-t22-marker.ps1"',
                         '-Action Launch -WorkAction $action -Nonce $nonce -ExpectedSha256 $asset_sha',
                         '-File C:\\\\bridgevm-share\\\\bv-a19-t22-marker.ps1',
                         't22_marker_wait_result "$share" "$nonce" "$action"',
                         "tr '\\r' '\\n'"):
            self.assertIn(required, host)
        self.assertNotIn('-Command', host)
        self.assertLess(host.index("'^BVAGENT SERVICE start'"),
                        host.index("'^BVAGENT SHARE host->guest"))
        self.assertLess(host.index("'^BVAGENT SHARE host->guest"),
                        host.index('t22_marker_action "$share"'))
        self.assertLess(guest.index(b'Get-FileHash -LiteralPath $PSCommandPath'),
                        guest.index(b'Invoke-CimMethod -ClassName Win32_Process'))
        self.assertEqual(host.count('nonce=$(/usr/bin/openssl rand -hex 16)'), 2)
        self.assertIn('^BV-(ORIGINAL|CLOBBERED|POSTKILL|FINAL)-[0-9a-f]{32}$', host)
        self.assertIn(b"C:\\bv-snapshot-marker.txt", guest)
        self.assertIn(b"Set-Content -NoNewline", guest)
        self.assertIn(b"Get-FileHash", guest)
        self.assertIn(b"T22-MARKER-LAUNCHED-$Nonce", guest)
        self.assertIn(b"^BV-(ORIGINAL|CLOBBERED|POSTKILL|FINAL)-[0-9a-f]{32}$", guest)

    def test_csrandom_marker_and_early_log_match(self):
        script = ('source "$HELPER"; t22_random_marker ORIGINAL; '
                  't22_marker_log_has "$LOG" "^BVAGENT SERVICE start"')
        with tempfile.TemporaryDirectory() as temporary:
            log = Path(temporary) / "run.log"
            log.write_bytes(b"BVAGENT SERVICE start\r\n" + b"later record\r\n" * 100000)
            env = dict(os.environ, HELPER=str(HOST), LOG=str(log))
            completed = subprocess.run(["bash", "-euo", "pipefail", "-c", script], env=env,
                                       capture_output=True, text=True, timeout=5)
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertRegex(completed.stdout, r"^BV-ORIGINAL-[0-9a-f]{32}\n$")

    def test_unique_filename_read_and_exact_write(self):
        with tempfile.TemporaryDirectory() as temporary:
            share = Path(temporary)
            output = share / "observed.txt"
            result(share, MARKER)
            self.assertEqual(run_wait(share, output).returncode, 0)
            self.assertEqual(output.read_text(), MARKER + "\n")
            self.assertNotEqual(run_wait(share, share / "stale.txt", nonce="b" * 32).returncode, 0)
            result(share, MARKER, action="Write")
            self.assertEqual(run_wait(share, output, action="Write", expected=MARKER).returncode, 0)
            self.assertNotEqual(run_wait(share, output, action="Write", expected="BV-CLOBBERED-" + "b" * 32).returncode, 0)

    def test_missing_mismatched_or_unsafe_result_is_refused(self):
        with tempfile.TemporaryDirectory() as temporary:
            share = Path(temporary)
            output = share / "observed.txt"
            self.assertNotEqual(run_wait(share, output).returncode, 0)
            result(share, MARKER)
            (share / f"t22-{NONCE}-Read.done").write_text("b" * 64, encoding="ascii")
            self.assertNotEqual(run_wait(share, output).returncode, 0)
            (share / f"t22-{NONCE}-Read.done").unlink()
            (share / f"t22-{NONCE}-Read.done").symlink_to(share / f"t22-{NONCE}-Read.txt")
            self.assertNotEqual(run_wait(share, output).returncode, 0)
            self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
