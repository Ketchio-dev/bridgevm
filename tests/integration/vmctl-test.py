#!/usr/bin/env python3
"""Exercise CLI correlation, failed-run protection and real framebuffer bytes."""
import io
import base64
import os
from pathlib import Path
import struct
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch
import zlib

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))
import vmctl
from vmctl_frame import capture


def frame(sequence=2, width=2, stride=12):
    header = bytearray(64)
    struct.pack_into("<6IQ", header, 0, 0x42564642, 1, width, 1, stride, 0x34325258, sequence)
    return bytes(header) + b"\x00\x00\xff\x00\xff\x00\x00\x00PAD!"


class FrameTests(unittest.TestCase):
    def test_real_png_colors_and_stride(self):
        png, result = capture(io.BytesIO(frame()))
        at, data = 8, b""
        while at < len(png):
            length = struct.unpack_from(">I", png, at)[0]
            if png[at + 4:at + 8] == b"IDAT":
                data += png[at + 8:at + 8 + length]
            at += 12 + length
        self.assertEqual(zlib.decompress(data), b"\x00\xff\x00\x00\x00\x00\xff")
        self.assertEqual((result["width"], result["height"], result["sequence"]), (2, 1, 2))

    def test_retries_changed_sequence_instead_of_saving_torn_pixels(self):
        class ChangingFrame(io.BytesIO):
            changed = False

            def read(self, count=-1):
                if self.tell() == 64 and not self.changed:
                    self.changed = True
                    self.seek(0)
                    self.write(frame(sequence=4))
                    self.seek(64)
                return super().read(count)

        _, result = capture(ChangingFrame(frame()))
        self.assertEqual(result["sequence"], 4)

    def test_rejects_truncated_and_hostile_geometry(self):
        for data in (frame()[:-5], frame(width=20000)):
            with self.assertRaises(ValueError):
                capture(io.BytesIO(data))



class FailureTextTests(unittest.TestCase):
    """A failed query printed the status JSON or an internal phrase instead of what to do."""

    def test_a_failed_json_query_says_what_the_plain_command_says(self):
        options = SimpleNamespace(bridgevm="bridgevm", library=None)
        failed = SimpleNamespace(returncode=1, stdout='{\n  "unavailableReason" : "ownerUnavailable"\n}', stderr="")
        plain = SimpleNamespace(returncode=1, stdout="BridgeVM isn't open. Open BridgeVM, then try again.\n", stderr="")
        with patch.object(vmctl.subprocess, "run", side_effect=[failed, plain]) as run:
            with self.assertRaises(vmctl.ControlError) as raised:
                vmctl.native(options, "status", "work", query=True)
        self.assertEqual(str(raised.exception), "BridgeVM isn't open. Open BridgeVM, then try again.")
        self.assertEqual(run.call_args_list[1].args[0], ["bridgevm", "app", "status", "work"])

    def test_a_vm_that_is_not_running_says_how_to_start_it(self):
        guest = vmctl.Guest.__new__(vmctl.Guest)
        guest.options = SimpleNamespace(vm="work", command="exec")
        with patch.object(vmctl, "native", return_value={"sessions": []}):
            with self.assertRaises(vmctl.ControlError) as raised:
                guest.current()
        self.assertIn("Start it with: vmctl.py start work", str(raised.exception))

class GuestTests(unittest.TestCase):
    def test_resize_updates_the_vmm_then_waits_for_its_guest_mode_reply(self):
        # The probe prints each guest line after `| ` (guest_text.rs guest_lines); an older one bare.
        for prefix in ("| ", ""):
            with self.subTest(prefix=prefix):
                self.resize_reply(prefix)

    def resize_reply(self, prefix):
        with tempfile.TemporaryDirectory() as directory:
            bundle = Path(directory)
            (bundle / "logs/hvf").mkdir(parents=True)
            (bundle / "metadata").mkdir()
            log, control = bundle / "logs/hvf/run.log", bundle / "logs/hvf/input.ctl"
            log.touch()
            control.touch()
            agent = bundle / "metadata/hvf.ctl"
            agent.write_text("unchanged\n")
            guest = vmctl.Guest.__new__(vmctl.Guest)
            guest.bundle, guest.evidence = bundle, bundle / "logs/hvf"
            guest.options = SimpleNamespace(timeout=1)
            guest.identity = ("app", "run", os.getpid())

            def reply(_):
                with log.open("a") as output:
                    output.write("BVAGENT CMD DISPLAYMODE 2560 1600 exit=0\n"
                                 f"{prefix}BVDISPLAY target=2560x1600 after=2560x1600 result=0\r\n"
                                 f"{prefix}\n"
                                 "BVAGENT END DISPLAYMODE 2560 1600\n")

            with patch.object(guest, "check"), patch.object(vmctl.time, "sleep", side_effect=reply):
                code, result = guest.run("RESIZE 2560x1600", vmm=True, expected="DISPLAYMODE 2560 1600")
            self.assertEqual(code, 0)
            self.assertEqual(result, "BVDISPLAY target=2560x1600 after=2560x1600 result=0\r\n")
            self.assertEqual(control.read_text(), "RESIZE 2560x1600\n")
            self.assertEqual(agent.read_text(), "unchanged\n")

    def test_text_paces_complete_codepoints_and_stops_on_a_failed_receipt(self):
        def reply(command, **_):
            verb, request_id, *payload = command.split()
            if verb == "INPUTCAPS":
                return 0, ""
            value = base64.b64decode(payload[0]).decode()
            if value == "B":
                return 1, "failed"
            return 0, f"BVINPUT_INSERTED {request_id} {len(value.encode('utf-16-le'))}"

        guest = SimpleNamespace(options=SimpleNamespace(timeout=5), run=Mock(side_effect=reply))
        self.assertEqual(vmctl.send_text(guest, "A한🙂")[0], 0)
        commands = [call.args[0] for call in guest.run.call_args_list if call.args[0].startswith("TEXTINPUT ")]
        self.assertEqual([base64.b64decode(c.split()[2]).decode() for c in commands], ["A", "한", "🙂"])
        self.assertEqual(len(set(c.split()[1] for c in commands)), 3)
        guest.run.reset_mock()
        self.assertEqual(vmctl.send_text(guest, "ABC"), (1, "Text stopped after 1 codepoints.\nfailed"))
        self.assertEqual(guest.run.call_count, 3)

    def test_fallback_stops_if_relay_has_no_insertion_receipt(self):
        def reply(command, **_):
            verb, request_id, *payload = command.split()
            if verb == "INPUTCAPS":
                return 0, ""
            if base64.b64decode(payload[0]).decode() == "🙂":
                return 0, "<base64 decode error: invalid output>"
            return 0, f"BVINPUT_INSERTED {request_id} 2"

        guest = SimpleNamespace(options=SimpleNamespace(timeout=5), run=Mock(side_effect=reply))
        with self.assertRaisesRegex(vmctl.ControlError, "Invalid text insertion receipt after 1 confirmed codepoints"):
            vmctl.send_text(guest, "A🙂B")
        self.assertEqual(guest.run.call_count, 3)

    def test_queued_text_chunks_without_splitting_unicode_or_replaying_unknown_results(self):
        commands = []
        def reply(command, **_):
            commands.append(command)
            fields = command.split()
            if fields[0] == "INPUTCAPS":
                return 0, f"BVINPUT_TEXT {fields[1]} QUEUED_DISPATCH"
            value = base64.b64decode(fields[2]).decode()
            return 0, f"BVTEXT_DISPATCHED {fields[1]} {len(value.encode('utf-16-le')) // 2}"
        guest = SimpleNamespace(options=SimpleNamespace(timeout=5), run=reply)
        value = "한" * 22000 + "🙂"
        self.assertEqual(vmctl.send_text(guest, value)[0], 0)
        pieces = [base64.b64decode(c.split()[2]).decode() for c in commands[1:]]
        self.assertEqual("".join(pieces), value)
        self.assertTrue(all(len(p.encode()) <= 65536 for p in pieces))
        self.assertTrue(all(c.endswith(" S") for c in commands[1:]))
        def bad(command, **_):
            fields = command.split()
            if fields[0] == "INPUTCAPS":
                return 0, f"BVINPUT_TEXT {fields[1]} QUEUED_DISPATCH"
            return 0, f"BVTEXT_DISPATCHED {fields[1]} 999"
        guest.run = Mock(side_effect=bad)
        with self.assertRaisesRegex(vmctl.ControlError, "not replayed"):
            vmctl.send_text(guest, "한🙂")
        self.assertEqual(guest.run.call_count, 2)

    def test_unsupported_queued_text_falls_back_only_after_positive_no_dispatch(self):
        commands = []
        def reply(command, **_):
            commands.append(command)
            fields = command.split()
            if fields[0] == "INPUTCAPS":
                return 0, f"BVINPUT_TEXT {fields[1]} QUEUED_DISPATCH"
            if command.endswith(" S"):
                return 0, f"BVTEXT_UNSUPPORTED {fields[1]}"
            value = base64.b64decode(fields[2]).decode()
            return 0, f"BVINPUT_INSERTED {fields[1]} {len(value.encode('utf-16-le'))}"
        guest = SimpleNamespace(options=SimpleNamespace(timeout=5), run=reply)
        self.assertEqual(vmctl.send_text(guest, "한🙂")[0], 0)
        self.assertEqual([base64.b64decode(c.split()[2]).decode() for c in commands[2:]], ["한", "🙂"])

    def test_conflicting_queued_receipts_never_trigger_fallback(self):
        def reply(command, **_):
            verb, request_id, *_ = command.split()
            if verb == "INPUTCAPS":
                return 0, f"BVINPUT_TEXT {request_id} QUEUED_DISPATCH"
            return 0, f"BVTEXT_UNSUPPORTED {request_id}\nBVINPUT_INSERTED {request_id} 6"

        guest = SimpleNamespace(options=SimpleNamespace(timeout=5), run=Mock(side_effect=reply))
        with self.assertRaisesRegex(vmctl.ControlError, "Invalid text dispatch receipt"):
            vmctl.send_text(guest, "한🙂")
        self.assertEqual(guest.run.call_count, 2)

    def test_oversize_shell_source_is_rejected_before_sending(self):
        with self.assertRaisesRegex(vmctl.ControlError, "cmd.exe limit"):
            vmctl.powershell("Write-Output '" + "x" * 5000 + "'")

    def test_unique_reply_skips_old_results_and_input_payload_is_redacted(self):
        with tempfile.TemporaryDirectory() as directory:
            bundle = Path(directory)
            (bundle / "logs/hvf").mkdir(parents=True)
            (bundle / "metadata").mkdir()
            log = bundle / "logs/hvf/run.log"
            ctl = bundle / "metadata/hvf.ctl"
            ctl.write_bytes(b"")
            command = "KEYINPUT 12345678-1234-1234-1234-123456789abc ZW50ZXI="
            label = command.rsplit(" ", 1)[0]
            log.write_text(f"BVAGENT CMD {label} exit=9\nstale\nBVAGENT END {label}\n")
            guest = vmctl.Guest.__new__(vmctl.Guest)
            guest.bundle, guest.evidence = bundle, bundle / "logs/hvf"
            guest.options = SimpleNamespace(timeout=1)
            guest.identity = ("app", "run", os.getpid())

            def reply(_):
                with log.open("a") as output:
                    # Guest text after the probe's `| `: a printed END ends nothing.
                    output.write(f"BVAGENT CMD {label} exit=7\n| new result\n| BVAGENT END {label}\n"
                                 f"| BVSTORAGE durability-failed\nBVAGENT END {label}\n")

            with patch.object(guest, "check"), patch.object(vmctl.time, "sleep", side_effect=reply):
                self.assertEqual(guest.run(command),
                                 (7, f"new result\nBVAGENT END {label}\nBVSTORAGE durability-failed"))
            self.assertEqual(ctl.read_text(), command + "\n")

    def test_changed_run_does_not_write(self):
        guest = vmctl.Guest.__new__(vmctl.Guest)
        guest.identity = ("app", "old", 1)
        with patch.object(guest, "current", return_value=("app", "new", 2)):
            with self.assertRaises(vmctl.ControlError):
                guest.check()

    def test_timeout_reports_uncertainty_without_retransmission(self):
        guest = vmctl.Guest.__new__(vmctl.Guest)
        guest.identity = ("app", "run", os.getpid())
        guest.options = SimpleNamespace(timeout=0.001)
        with tempfile.NamedTemporaryFile() as file:
            with self.assertRaisesRegex(vmctl.ControlError, "may still run.*not retried"):
                guest.reply(file, Path(file.name), "unique command")

    def test_no_follow_refuses_symlink(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / "real"
            target.touch()
            link = Path(directory) / "link"
            link.symlink_to(target)
            with self.assertRaises(OSError):
                vmctl.regular(link, os.O_WRONLY | os.O_APPEND)


if __name__ == "__main__":
    unittest.main()
