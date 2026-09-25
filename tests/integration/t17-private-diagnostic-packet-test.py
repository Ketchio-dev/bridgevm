#!/usr/bin/env python3
"""Synthetic T17 private packet safety, integrity, and absence contracts."""
from __future__ import annotations

import argparse
from contextlib import ExitStack
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import secrets
import shutil
import stat
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

SCRIPT = Path(__file__).resolve().parents[2] / "scripts/live-gates/t17_private_diagnostic_packet.py"
ROOT = SCRIPT.parents[2]
SPEC = importlib.util.spec_from_file_location("t17_packet", SCRIPT)
assert SPEC and SPEC.loader
packet = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(packet)
JOB = "packet-fixture"
COMMIT = "c" * 40
NONCE = "a" * 64
STOP_NONCE = "b" * 32


def json_file(path: Path, value: dict) -> str:
    data = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()
    path.write_bytes(data)
    return hashlib.sha256(data).hexdigest()


class PacketTest(unittest.TestCase):
    def setUp(self) -> None:
        self.work = Path("/tmp") / f"bridgevm-e2e-{JOB}.{secrets.token_hex(3)}"
        self.work.mkdir(mode=0o700)
        self.work = self.work.resolve()
        self.lane = self.work / "lane-1"
        self.lane.mkdir(mode=0o700)
        self.output = Path(tempfile.mkdtemp(prefix="t17-packet-out-")).resolve()
        self.private = self.output / "private"
        self.private.mkdir(mode=0o700)
        self.slug = "bridgevm-t17-lane-1-" + NONCE[:12]
        self.evidence = self.lane / "library" / self.slug / "bundle.vmbridge" / "logs" / "hvf"
        self.evidence.mkdir(parents=True)
        self.frames = self.evidence / "ramfb"
        self.frames.mkdir()
        self.raw_name = "ramfb-1x1-1000-0123456789abcdef.xrgb8888"
        self.ppm_name = self.raw_name.removesuffix(".xrgb8888") + ".ppm"
        self.request = {"schema_version": packet.REQUEST_SCHEMA,
                        "job_id": JOB, "commit": COMMIT, "campaign_mode": "pilot", "lane": 1,
                        "nonce": NONCE, "lane_root": str(self.lane), "vm_slug": self.slug}
        self.result = {"schema_version": packet.LANE_SCHEMA,
                       "job_id": JOB, "commit": COMMIT, "campaign_mode": "pilot", "lane": 1,
                       "nonce": NONCE, "failure_code": "guest-evidence-missing", "first_ready": False,
                       "failure_detail": f"first boot has no BVAGENT READY/PONG evidence; host_stop=status=complete,generation=7,nonce={STOP_NONCE},report=complete,helper=terminal,log_offset=0"}
        self.refresh_seal()
        self.args = argparse.Namespace(private=self.private, lane_root=self.lane,
                                       job_id=JOB, commit=COMMIT, mode="pilot", lane=1)

    def tearDown(self) -> None:
        shutil.rmtree(self.work)
        shutil.rmtree(self.output)

    def refresh_seal(self) -> None:
        request_sha = json_file(self.lane / "request.json", self.request)
        result_sha = json_file(self.private / "lane-1-result.json", self.result)
        stamp = {"schema_version": packet.STAMP_SCHEMA, "job_id": JOB, "commit": COMMIT,
                 "lane": 1, "nonce": NONCE, "request_sha256": request_sha,
                 "result_sha256": result_sha}
        json_file(self.private / "lane-1-authenticated.json", stamp)

    def frame_log(self, *, secret: str = "synthetic-private-guest-bytes") -> bytes:
        return (f"BVAGENT WAIT {secret}\n"
                f"HOST-DIAGNOSTIC-STOP: generation=7 nonce={STOP_NONCE} request consumed; ending run through final report\n"
                "=== EDK2 boot probe (with Apple hv_gic) ===\n"
                "stop: host diagnostic stop requested\n"
                f"ramfb framebuffer artifact: raw={self.frames / self.raw_name}\n"
                f"ramfb framebuffer artifact: ppm={self.frames / self.ppm_name}\n"
                "--- serial (tail) ---\nsynthetic firmware text\n--- end ---\n").encode()

    def make_display(self, sequence: int = 2) -> None:
        header = bytearray(64)
        struct.pack_into("<6I", header, 0, 0x42564642, 1, 1, 1, 4, 0x34325258)
        struct.pack_into("<Q", header, 24, sequence)
        (self.evidence / "display.fb").write_bytes(header + b"ABCD")

    def complete_sources(self) -> None:
        (self.evidence / "run.log").write_bytes(self.frame_log())
        (self.frames / self.raw_name).write_bytes(b"ABCD")
        (self.frames / self.ppm_name).write_bytes(b"P6\n1 1\n255\n\x00\x00\x00")
        self.make_display()

    def index(self) -> dict:
        return json.loads((self.private / "t17-diagnostic-lane-1-index.json").read_text())

    def test_complete_packet_binds_identity_and_keeps_private_bytes_out_of_index(self) -> None:
        self.complete_sources()
        public = self.output / "receipt.public.json"
        public.write_text('{"public":"unchanged"}\n')
        packet.capture(self.args)
        packet.verify(self.args)
        index = self.index()
        self.assertEqual(index["observed_generation"], 7)
        self.assertEqual([item["status"] for item in index["artifacts"]], ["retained"] * 4)
        self.assertEqual(index["total_bytes"], sum(item["bytes"] for item in index["artifacts"]))
        self.assertIn("synthetic-private-guest-bytes", (self.private / index["artifacts"][0]["file"]).read_text())
        self.assertNotIn("synthetic-private-guest-bytes", json.dumps(index))
        self.assertEqual(public.read_text(), '{"public":"unchanged"}\n')
        self.assertEqual(stat.S_IMODE(self.private.stat().st_mode), 0o700)
        for item in index["artifacts"]:
            self.assertEqual(stat.S_IMODE((self.private / item["file"]).stat().st_mode), 0o600)

    def test_missing_and_oversize_frame_are_recorded_without_fabricating_pixels(self) -> None:
        self.result["failure_detail"] = "first boot has no BVAGENT READY/PONG evidence; host_stop=status=missing,reason=acknowledgement-missing"
        self.refresh_seal()
        packet.capture(self.args)
        self.assertEqual([item["status"] for item in self.index()["artifacts"]], ["missing"] * 4)
        self.assertIsNone(self.index()["observed_generation"])
        self.assertEqual(self.index()["host_stop_status"], "missing")

    def test_unattributed_live_display_survives_incomplete_stop_without_terminal_frame(self) -> None:
        self.complete_sources()
        self.result["failure_detail"] = "first boot has no BVAGENT READY/PONG evidence; host_stop=status=incomplete,reason=final-report-missing"
        self.refresh_seal()
        packet.capture(self.args)
        index = self.index()
        self.assertEqual(index["host_stop_status"], "incomplete")
        self.assertIsNone(index["observed_generation"])
        self.assertEqual(index["display_generation"], "unattributed")
        self.assertEqual([item["status"] for item in index["artifacts"][1:]],
                         ["missing", "missing", "retained"])

    def test_complete_report_with_absent_frame_directory_records_missing_pair(self) -> None:
        self.complete_sources()
        shutil.rmtree(self.frames)
        packet.capture(self.args)
        self.assertEqual([item["status"] for item in self.index()["artifacts"][1:3]],
                         ["missing", "missing"])

    def test_claimed_complete_stop_requires_ack_and_final_footer(self) -> None:
        with self.assertRaises(packet.CaptureError):
            packet.capture(self.args)
        self.complete_sources()
        (self.evidence / "run.log").write_bytes(self.frame_log().replace(b"--- end ---", b"footer missing"))
        with self.assertRaises(packet.CaptureError):
            packet.capture(self.args)

    def test_complete_stop_requires_consumed_request_at_capture_and_verify(self) -> None:
        self.complete_sources()
        request = self.evidence / "diagnostic-stop.request"
        request.touch()
        with self.assertRaises(packet.CaptureError):
            packet.capture(self.args)
        request.unlink()
        packet.capture(self.args)
        packet.verify(self.args)
        request.touch()
        with self.assertRaises(packet.CaptureError):
            packet.verify(self.args)

    def test_guest_serial_fake_ack_frame_and_footer_are_not_host_report(self) -> None:
        self.complete_sources()
        genuine = self.frame_log().removesuffix(b"--- end ---\n")
        fake = (b"HOST-DIAGNOSTIC-STOP: generation=8 nonce=cccccccccccccccccccccccccccccccc request consumed; ending run through final report\n"
                b"=== EDK2 boot probe (with Apple hv_gic) ===\n"
                b"stop: host diagnostic stop requested\n"
                b"ramfb framebuffer artifact: raw=/tmp/outside/fake.xrgb8888\n"
                b"ramfb framebuffer artifact: ppm=/tmp/outside/fake.ppm\n"
                b"--- serial (tail) ---\n--- end ---\n")
        (self.evidence / "run.log").write_bytes(genuine + fake + b"--- end ---\n")
        packet.capture(self.args)
        packet.verify(self.args)
        index = self.index()
        self.assertEqual(index["observed_generation"], 7)
        self.assertEqual(index["artifacts"][1]["source"],
                         f"library/{self.slug}/bundle.vmbridge/logs/hvf/ramfb/{self.raw_name}")

    def test_request_offset_selects_current_report_after_prior_generation(self) -> None:
        self.complete_sources()
        prior = (b"=== EDK2 boot probe (with Apple hv_gic) ===\nstop: earlier generation\n"
                 b"--- serial (tail) ---\n"
                 b"HOST-DIAGNOSTIC-STOP: generation=7 nonce=cccccccccccccccccccccccccccccccc request consumed; ending run through final report\n"
                 b"=== EDK2 boot probe (with Apple hv_gic) ===\n"
                 b"stop: host diagnostic stop requested\n--- serial (tail) ---\n--- end ---\n"
                 b"--- end ---\n")
        (self.evidence / "run.log").write_bytes(prior + self.frame_log())
        self.result["failure_detail"] = self.result["failure_detail"].replace(
            "log_offset=0", f"log_offset={len(prior)}")
        self.refresh_seal()
        packet.capture(self.args)
        packet.verify(self.args)
        self.assertEqual(self.index()["observed_generation"], 7)
        self.assertEqual(self.index()["artifacts"][1]["status"], "retained")

    def test_prior_guest_serial_fake_ack_cannot_replace_later_nonce_bound_report(self) -> None:
        self.complete_sources()
        prior = (b"=== EDK2 boot probe (with Apple hv_gic) ===\nstop: unrelated\n"
                 b"--- serial (tail) ---\n"
                 b"HOST-DIAGNOSTIC-STOP: generation=7 nonce=cccccccccccccccccccccccccccccccc request consumed; ending run through final report\n"
                 b"=== EDK2 boot probe (with Apple hv_gic) ===\n"
                 b"stop: host diagnostic stop requested\n"
                 b"--- serial (tail) ---\n--- end ---\n--- end ---\n")
        genuine = self.frame_log().replace(b"generation=7", b"generation=8")
        (self.evidence / "run.log").write_bytes(prior + genuine)
        self.result["failure_detail"] = self.result["failure_detail"].replace("generation=7", "generation=8")
        self.refresh_seal()
        packet.capture(self.args)
        packet.verify(self.args)
        index = self.index()
        self.assertEqual(index["observed_generation"], 8)
        self.assertEqual(index["artifacts"][1]["source"],
                         f"library/{self.slug}/bundle.vmbridge/logs/hvf/ramfb/{self.raw_name}")

    def test_oversize_pair_never_copies_only_one_frame(self) -> None:
        self.complete_sources()
        with (self.frames / self.raw_name).open("r+b") as output:
            output.truncate(packet.FRAME_CAP + 1)
        packet.capture(self.args)
        items = self.index()["artifacts"]
        self.assertEqual(items[1]["status"], "oversize")
        self.assertEqual(items[2]["status"], "missing")
        self.assertFalse((self.private / "t17-diagnostic-lane-1-final_ppm.ppm").exists())

    def test_log_tail_has_exact_offset_and_bounded_length(self) -> None:
        self.complete_sources()
        log = self.evidence / "run.log"
        with log.open("wb") as output:
            output.truncate(packet.LOG_CAP + 11)
            output.seek(packet.LOG_CAP + 11)
            output.write(self.frame_log())
        self.result["failure_detail"] = self.result["failure_detail"].replace(
            "log_offset=0", f"log_offset={packet.LOG_CAP + 11}")
        self.refresh_seal()
        packet.capture(self.args)
        item = self.index()["artifacts"][0]
        self.assertEqual(item["bytes"], packet.LOG_CAP)
        self.assertEqual(item["offset"], item["original_size"] - packet.LOG_CAP)
        self.assertTrue(item["truncated"])
        self.assertEqual((self.private / item["file"]).stat().st_size, packet.LOG_CAP)

    def test_odd_and_changing_display_sequences_are_unavailable(self) -> None:
        self.complete_sources()
        self.make_display(sequence=3)
        packet.capture(self.args)
        self.assertEqual(self.index()["artifacts"][3]["status"], "unavailable")
        self.assertFalse((self.private / "t17-diagnostic-lane-1-display.fb").exists())

    def test_changing_sequence_during_copy_is_not_retained(self) -> None:
        self.complete_sources()
        original = packet.copy_file
        def change(*args, **kwargs):
            result = original(*args, **kwargs)
            if args[2] == "display.fb":
                with (self.evidence / "display.fb").open("r+b") as output:
                    output.seek(24)
                    output.write(struct.pack("<Q", 4))
            return result
        with mock.patch.object(packet, "copy_file", side_effect=change):
            packet.capture(self.args)
        self.assertEqual(self.index()["artifacts"][3]["reason"], "sequence-changed")
        self.assertFalse((self.private / "t17-diagnostic-lane-1-display.fb").exists())

    def test_symlink_hardlink_and_directory_sources_are_refused(self) -> None:
        for kind in ("symlink", "hardlink", "directory"):
            with self.subTest(kind=kind):
                path = self.evidence / "run.log"
                other = self.evidence / "other.log"
                if kind == "symlink":
                    other.write_bytes(b"x")
                    path.symlink_to(other)
                elif kind == "hardlink":
                    path.write_bytes(b"x")
                    os.link(path, other)
                else:
                    path.mkdir()
                with self.assertRaises((packet.CaptureError, OSError)):
                    packet.capture(self.args)
                self.assertFalse((self.private / "t17-diagnostic-lane-1-index.json").exists())
                if path.is_dir() and not path.is_symlink():
                    path.rmdir()
                else:
                    path.unlink()
                if other.exists():
                    other.unlink()

    def test_device_and_wrong_owner_are_refused(self) -> None:
        with ExitStack() as stack:
            dev = packet.open_absolute_directory(Path("/dev"), stack)
            with mock.patch.object(packet.os, "geteuid", return_value=0):
                with self.assertRaises(packet.CaptureError):
                    packet.source_file(dev, "null", os.fstat(dev).st_dev, stack)
        self.complete_sources()
        with mock.patch.object(packet.os, "geteuid", return_value=os.geteuid() + 1):
            with self.assertRaises(packet.CaptureError):
                packet.capture(self.args)

    def test_source_mutation_or_replacement_cannot_be_copied(self) -> None:
        self.complete_sources()
        original = packet.copy_file
        def mutate(*args, **kwargs):
            if args[2] == "run.log":
                with (self.evidence / "run.log").open("ab") as output:
                    output.write(b"changed")
            return original(*args, **kwargs)
        with mock.patch.object(packet, "copy_file", side_effect=mutate):
            with self.assertRaises(packet.CaptureError):
                packet.capture(self.args)
        self.assertFalse((self.private / "t17-diagnostic-lane-1-run-log.bin").exists())

    def test_short_read_and_replaced_source_identity_are_refused(self) -> None:
        self.complete_sources()
        log = self.evidence / "run.log"
        inode = log.stat().st_ino
        original = packet.os.pread
        def short(fd, size, offset):
            return b"" if os.fstat(fd).st_ino == inode else original(fd, size, offset)
        with mock.patch.object(packet.os, "pread", side_effect=short):
            with self.assertRaises(packet.CaptureError):
                packet.capture(self.args)
        with ExitStack() as stack:
            evidence = packet.open_absolute_directory(self.evidence, stack)
            opened = packet.source_file(evidence, "run.log", os.fstat(evidence).st_dev, stack)
            assert opened is not None
            replacement = self.evidence / "replacement.log"
            replacement.write_bytes(log.read_bytes())
            os.replace(replacement, log)
            with self.assertRaises(packet.CaptureError):
                packet.unchanged(evidence, "run.log", *opened)

    def test_duplicate_destination_and_hash_mismatch_fail_verification(self) -> None:
        self.complete_sources()
        packet.capture(self.args)
        with self.assertRaises(FileExistsError):
            packet.capture(self.args)
        packet.verify(self.args)
        item = self.index()["artifacts"][0]
        with (self.private / item["file"]).open("r+b") as output:
            output.write(b"X")
        with self.assertRaises(packet.CaptureError):
            packet.verify(self.args)

    def test_authentication_and_unsafe_report_path_are_refused(self) -> None:
        self.complete_sources()
        self.result["failure_code"] = "none"
        self.refresh_seal()
        with self.assertRaises(packet.CaptureError):
            packet.capture(self.args)
        self.result["failure_code"] = "guest-evidence-missing"
        self.refresh_seal()
        (self.evidence / "run.log").write_bytes(self.frame_log().replace(str(self.frames).encode(), b"/tmp/outside"))
        with self.assertRaises(packet.CaptureError):
            packet.capture(self.args)
        self.assertFalse((self.private / "t17-diagnostic-lane-1-index.json").exists())

    def test_independent_verifier_rejects_result_and_stamp_identity_tampering(self) -> None:
        self.complete_sources()
        packet.capture(self.args)
        original_result = dict(self.result)
        for field, changed in (("job_id", "other-job"), ("commit", "d" * 40),
                               ("lane", 2), ("campaign_mode", "release"),
                               ("failure_code", "none"), ("first_ready", True),
                               ("failure_detail", "later stage failed")):
            with self.subTest(result_field=field):
                self.result[field] = changed
                result_sha = json_file(self.private / "lane-1-result.json", self.result)
                stamp = json.loads((self.private / "lane-1-authenticated.json").read_text())
                stamp["result_sha256"] = result_sha
                json_file(self.private / "lane-1-authenticated.json", stamp)
                with self.assertRaises(packet.CaptureError):
                    packet.verify(self.args)
                self.result = dict(original_result)
                self.refresh_seal()
        stamp_path = self.private / "lane-1-authenticated.json"
        original_stamp = json.loads(stamp_path.read_text())
        for field, changed in (("schema_version", "wrong"), ("job_id", "other-job"),
                               ("commit", "d" * 40), ("lane", 2),
                               ("request_sha256", "d" * 64)):
            with self.subTest(stamp_field=field):
                modified = dict(original_stamp)
                modified[field] = changed
                json_file(stamp_path, modified)
                with self.assertRaises(packet.CaptureError):
                    packet.verify(self.args)
        json_file(stamp_path, original_stamp)
        packet.verify(self.args)


def augment_synthetic_helper(path: Path) -> None:
    path.write_text(path.read_text() + '''
if r["job_id"] in ("diagnostic-success-fixture", "diagnostic-capture-fail-fixture"):
    stop_nonce = "b" * 32
    for stage in stages[5:]: result[stage] = False
    result["failure_code"] = "guest-evidence-missing"
    result["failure_detail"] = f"first boot has no BVAGENT READY/PONG evidence; host_stop=status=complete,generation=7,nonce={stop_nonce},report=complete,helper=terminal,log_offset=0"
    pathlib.Path(a.result).write_text(json.dumps(result, sort_keys=True) + "\\n")
    frame_dir = final_log.parent / "ramfb"; frame_dir.mkdir()
    raw = frame_dir / "ramfb-1x1-1000-0123456789abcdef.xrgb8888"
    ppm = raw.with_suffix(".ppm")
    raw.write_bytes(b"ABCD"); ppm.write_bytes(b"P6\\n1 1\\n255\\n\\x00\\x00\\x00")
    frame_root = "/tmp/outside" if "capture-fail" in r["job_id"] else str(frame_dir)
    final_log.write_text(f"HOST-DIAGNOSTIC-STOP: generation=7 nonce={stop_nonce} request consumed; ending run through final report\\n"
        "=== EDK2 boot probe (with Apple hv_gic) ===\\n"
        "stop: host diagnostic stop requested\\n"
        f"ramfb framebuffer artifact: raw={frame_root}/{raw.name}\\n"
        f"ramfb framebuffer artifact: ppm={frame_root}/{ppm.name}\\n"
        "--- serial (tail) ---\\nsynthetic-only\\n--- end ---\\n")
''')


def run_tier_fixtures(tier: Path, manifest: Path, temporary: Path, commit: str) -> None:
    publisher = ROOT / "scripts/live-gates/publish-receipt.sh"
    for name, capture_ok in (("diagnostic-success-fixture", True),
                             ("diagnostic-capture-fail-fixture", False)):
        out = temporary / f"{name}-out"
        completed = subprocess.run([str(tier), "--out", str(out), "--input-manifest", str(manifest),
                                    "--job-id", name], capture_output=True, text=True, check=False)
        assert completed.returncode != 0, "synthetic first-READY failure unexpectedly passed"
        receipt = json.loads((out / "receipt.json").read_text())
        private = out / "private"
        result = json.loads((private / "lane-1-result.json").read_text())
        assert receipt["outcome"] == "failed" and receipt["failure_code"] == "first-boot-failed"
        assert receipt["worker_cleanup_verified"] is True and receipt["pass"] is False
        assert result["failure_code"] == "guest-evidence-missing" and result["first_ready"] is False
        helper_log = (private / "lane-1-helper.log").read_text()
        lane_root = Path(next(line.removeprefix("lane_root=") for line in helper_log.splitlines()
                              if line.startswith("lane_root=")))
        assert not os.path.lexists(lane_root.parent), "tier did not remove the owned lane"
        index = private / "t17-diagnostic-lane-1-index.json"
        marker = private / "lane-1-diagnostic-capture-failed"
        if capture_ok:
            assert index.exists() and not marker.exists(), f"diagnostic capture refused: {completed.stderr[-1200:]}"
            subprocess.run([sys.executable, str(SCRIPT), "verify", "--private", str(private.resolve()),
                            "--job-id", name, "--commit", commit, "--campaign-mode", "pilot",
                            "--lane", "1"], check=True)
            assert json.loads(index.read_text())["observed_generation"] == 7
        else:
            assert not index.exists() and marker.exists()
        subprocess.run([str(publisher), "t17-windows-hvf-product-e2e", str(out),
                        str(ROOT), commit], check=True, capture_output=True, text=True)
        public = (out / "receipt.public.json").read_text()
        assert public == (out / "receipt.json").read_text()
        assert str(lane_root) not in public and "t17-diagnostic-lane-1" not in public


if __name__ == "__main__":
    if sys.argv[1:2] == ["--augment-helper"]:
        augment_synthetic_helper(Path(sys.argv[2]))
    elif sys.argv[1:2] == ["--tier-fixtures"]:
        run_tier_fixtures(Path(sys.argv[2]), Path(sys.argv[3]), Path(sys.argv[4]), sys.argv[5])
    else:
        unittest.main()
