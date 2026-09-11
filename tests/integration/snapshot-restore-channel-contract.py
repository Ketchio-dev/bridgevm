#!/usr/bin/env python3
"""Restore gate receipts and owned shutdown, without a VM or private media."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import threading
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("channel", ROOT / "scripts/snapshot-restore-channel.py")
CHANNEL = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHANNEL)


class RestoreReceiptContract(unittest.TestCase):
    def test_unrelated_and_failed_receipts_do_not_succeed(self):
        for record in (b"BVAGENT CMD other exit=1\nBVAGENT END other\n",
                       b"BVAGENT CMD marker exit=1\nBVAGENT END marker\n",
                       b"BVAGENT CMD marker exit=00\nBVAGENT END marker\n",
                       b"BVAGENT END marker\n"):
            self.assertIsNone(CHANNEL.Receipt("marker").feed(record))

    def test_exact_receipt_requires_end_and_isolates_output(self):
        receipt = CHANNEL.Receipt("marker")
        self.assertIsNone(receipt.feed(b"unrelated\nBVAGENT CMD marker exit=0\r\nBV-ORIGINAL\r\n"))
        self.assertEqual(receipt.feed(b"BVAGENT END marker\r\n"), b"BV-ORIGINAL\n")

    def test_split_records(self):
        receipt = CHANNEL.Receipt("marker")
        record = b"BVAGENT CMD marker exit=0\nvalue\nBVAGENT END marker\n"
        for byte in record[:-1]:
            self.assertIsNone(receipt.feed(bytes([byte])))
        self.assertEqual(receipt.feed(record[-1:]), b"value\n")

    def test_interleaved_command_and_service_restart_are_rejected(self):
        for record in (b"BVAGENT CMD other exit=0\n", b"BVAGENT END other\n", b"BVAGENT SERVICE start\n"):
            with self.assertRaises(ValueError):
                CHANNEL.Receipt("marker").feed(b"BVAGENT CMD marker exit=0\n" + record)

    def test_output_and_record_bounds(self):
        for record in (b"x" * 262145, b"x" * 262145 + b"\n",
                       b"BVAGENT CMD marker exit=0\n" + b"x" * 65536 + b"\n"):
            with self.assertRaises(ValueError):
                CHANNEL.Receipt("marker").feed(record)

    def test_stale_success_cannot_replace_fresh_matching_receipt(self):
        for fresh, accepted in ((b"BVAGENT CMD other exit=1\nBVAGENT END other\n", False),
                                (b"BVAGENT CMD marker exit=0\nnew\nBVAGENT END marker\n", True)):
            with tempfile.TemporaryDirectory() as directory:
                ctl, log = Path(directory) / "ctl", Path(directory) / "log"
                ctl.touch()
                log.write_bytes(b"BVAGENT CMD marker exit=0\nold\nBVAGENT END marker\n")
                def respond():
                    deadline = time.monotonic() + 2
                    while not ctl.stat().st_size and time.monotonic() < deadline:
                        time.sleep(0.01)
                    with log.open("ab") as stream:
                        stream.write(fresh)
                writer = threading.Thread(target=respond)
                writer.start()
                try:
                    if accepted:
                        self.assertEqual(CHANNEL.exchange(ctl, log, "marker", 0.3), b"new\n")
                    else:
                        with self.assertRaises(TimeoutError):
                            CHANNEL.exchange(ctl, log, "marker", 0.3)
                finally:
                    writer.join(timeout=3)
                self.assertEqual(ctl.read_text(), "marker\n")

    def test_shutdown_requires_natural_success_and_both_receipts(self):
        for log, status, expected in (("stop: PSCI x (system off)\nNVMe disk written back: x\n", 0, 0),
                                      ("stop: PSCI x (system off)\n", 0, 1),
                                      ("NVMe disk written back: x\n", 0, 1),
                                      ("stop: PSCI x (system off)\nNVMe disk written back: x\n", 2, 1)):
            with tempfile.TemporaryDirectory() as directory:
                path = Path(directory)
                (path / "log").write_text(log)
                script = '''source "$1/scripts/snapshot-restore-lifecycle.sh"
STEP_TIMEOUT=2
(sleep 0.05; exit "$3") & SNAPSHOT_LAUNCHER=$!
snapshot_shutdown "$2/ctl" "$2/log"
'''
                result = subprocess.run(["bash", "-c", script, "contract", str(ROOT), directory, str(status)], timeout=5)
                self.assertEqual(result.returncode, expected)

    def test_timeout_fails_without_killing_unrelated_process(self):
        with tempfile.TemporaryDirectory() as directory:
            script = '''source "$1/scripts/snapshot-restore-lifecycle.sh"
sleep 10 & unrelated=$!
trap 'kill "$unrelated" 2>/dev/null; wait "$unrelated" 2>/dev/null' EXIT
sleep 10 & owned=$!; SNAPSHOT_LAUNCHER=$owned
STEP_TIMEOUT=0
if snapshot_shutdown "$2/ctl" "$2/log"; then exit 9; fi
kill -0 "$unrelated" || exit 8
if kill -0 "$owned" 2>/dev/null; then exit 7; fi
[[ -z "$SNAPSHOT_LAUNCHER" ]]
'''
            subprocess.run(["bash", "-c", script, "contract", str(ROOT), directory], check=True, timeout=5)


if __name__ == "__main__":
    unittest.main()
