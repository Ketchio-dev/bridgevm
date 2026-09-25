#!/usr/bin/env python3
"""Exercise the private T17 collector on post-footer host shutdown lines."""
from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest

FIXTURE = Path(__file__).with_name("t17-private-diagnostic-packet-test.py")
SPEC = importlib.util.spec_from_file_location("t17_packet_fixture", FIXTURE)
assert SPEC and SPEC.loader
fixture = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(fixture)
packet = fixture.packet

AUDIO_TAIL = (
    "hda CoreAudio callback enqueue: state=stopping reason=stopping-enqueue-during-reset osstatus=-66632 expected=true\n"
    "hda CoreAudio callback enqueue: state=stopping reason=stopping-enqueue-during-reset osstatus=-66632 expected=true\n"
    "hda CoreAudio callback enqueue: state=stopping reason=stopping-enqueue-during-reset osstatus=-66632 expected=true\n"
    "hda CoreAudio lifecycle: operation=stop osstatus=0 success=true\n"
    "hda CoreAudio lifecycle: operation=dispose osstatus=0 success=true\n"
    "hda CoreAudio stats: frames_rendered=173871 drops=0 callback_errors=3\n"
).encode()


class T17TerminalReportTailTest(unittest.TestCase):
    def setUp(self) -> None:
        self.case = fixture.PacketTest(methodName="test_complete_packet_binds_identity_and_keeps_private_bytes_out_of_index")
        self.case.setUp()
        self.case.complete_sources()
        self.log = self.case.evidence / "run.log"

    def tearDown(self) -> None:
        self.case.tearDown()

    def test_coreaudio_tail_retains_final_frames_and_verifies(self) -> None:
        modern = self.log.read_bytes().replace(b"serial bytes: 23", b"serial raw bytes: 23 output bytes: 23")
        self.log.write_bytes(modern + AUDIO_TAIL)
        packet.capture(self.case.args)
        packet.verify(self.case.args)
        index = self.case.index()
        self.assertEqual(index["observed_generation"], 7)
        self.assertEqual([item["status"] for item in index["artifacts"]], ["retained"] * 4)

    def test_arbitrary_or_partial_tail_refuses_complete_claim(self) -> None:
        for tail in (b"guest-forged-trailer\n", AUDIO_TAIL[:-1], AUDIO_TAIL * 9):
            with self.subTest(tail_bytes=len(tail)):
                self.log.write_bytes(self.case.frame_log() + tail)
                with self.assertRaises(packet.CaptureError):
                    packet.capture(self.case.args)
                self.assertFalse((self.case.private / "t17-diagnostic-lane-1-index.json").exists())

    def test_missing_footer_or_wrong_nonce_stays_unverified(self) -> None:
        for body in (self.case.frame_log().replace(b"--- end ---\n", b"footer missing\n"),
                     self.case.frame_log().replace(fixture.STOP_NONCE.encode(), b"f" * 32)):
            with self.subTest(body_bytes=len(body)):
                self.log.write_bytes(body + AUDIO_TAIL)
                with self.assertRaises(packet.CaptureError):
                    packet.capture(self.case.args)
                self.assertFalse((self.case.private / "t17-diagnostic-lane-1-index.json").exists())

    def test_guest_serial_footer_cannot_replace_host_footer(self) -> None:
        serial = b"firmware\n--- end ---\n"
        before = self.case.frame_log().split(b"--- serial (tail) ---\n", 1)[0]
        before = before.replace(b"serial bytes: 23", f"serial bytes: {len(serial)}".encode())
        self.log.write_bytes(before + b"--- serial (tail) ---\n" + serial)
        with self.assertRaises(packet.CaptureError):
            packet.capture(self.case.args)

    def test_lossy_utf8_serial_count_fails_closed(self) -> None:
        before = self.case.frame_log().split(b"--- serial (tail) ---\n", 1)[0]
        before = before.replace(b"serial bytes: 23", b"serial bytes: 1")
        self.log.write_bytes(before + b"--- serial (tail) ---\n\xef\xbf\xbd\n--- end ---\n")
        with self.assertRaises(packet.CaptureError):
            packet.capture(self.case.args)

    def test_earlier_same_nonce_forged_report_fails_closed(self) -> None:
        genuine = self.case.frame_log()
        fake = genuine.replace(b"serial bytes: 23", b"serial bytes: 4").replace(b"synthetic firmware text", b"fake")
        self.log.write_bytes(fake + genuine)
        with self.assertRaises(packet.CaptureError):
            packet.capture(self.case.args)

    def test_lossy_new_format_counts_past_guest_footer_and_keeps_frames(self) -> None:
        raw_serial = b"\xff" * 7 + b"\n--- end ---\n" + b"X"
        displayed = raw_serial.decode("utf-8", errors="replace").encode()
        before = self.case.frame_log().split(b"--- serial (tail) ---\n", 1)[0]
        before = before.replace(b"serial bytes: 23", f"serial raw bytes: {len(raw_serial)} output bytes: {len(displayed)}".encode())
        self.log.write_bytes(before + b"--- serial (tail) ---\n" + displayed + b"\n--- end ---\n" + AUDIO_TAIL)
        packet.capture(self.case.args)
        packet.verify(self.case.args)
        self.assertEqual([item["status"] for item in self.case.index()["artifacts"]], ["retained"] * 4)

    def test_partial_lossy_guest_footer_rejects_legacy_and_new_format(self) -> None:
        before = self.case.frame_log().split(b"--- serial (tail) ---\n", 1)[0]
        for padding in (b"", b"A" * 5):
            raw_serial = b"\xff" * 7 + padding + b"\n--- end ---\n" + b"X"
            displayed = raw_serial.decode("utf-8", errors="replace").encode()
            partial = displayed[:len(raw_serial) + len(b"\n--- end ---\n")]
            for count in (f"serial bytes: {len(raw_serial)}", f"serial raw bytes: {len(raw_serial)} output bytes: {len(displayed)}"):
                with self.subTest(count=count):
                    self.log.write_bytes(before.replace(b"serial bytes: 23", count.encode()) + b"--- serial (tail) ---\n" + partial)
                    with self.assertRaises(packet.CaptureError):
                        packet.capture(self.case.args)
                    self.assertFalse((self.case.private / "t17-diagnostic-lane-1-index.json").exists())


if __name__ == "__main__":
    unittest.main()
