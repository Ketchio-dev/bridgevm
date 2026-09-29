#!/usr/bin/env python3
"""Synthetic T17 guest-setup harvest contracts: GPT classification, bounded
allowlist copies, fail-closed release and binding into the private packet."""
from __future__ import annotations

import argparse
from contextlib import ExitStack
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import plistlib
import secrets
import shutil
import signal
import stat
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
import uuid
import zlib

ROOT = Path(__file__).resolve().parents[2]
GATES = ROOT / "scripts/live-gates"
sys.path.insert(0, str(GATES))
import t17_guest_setup_harvest as harvest  # noqa: E402

SPEC = importlib.util.spec_from_file_location("t17_packet_for_harvest", GATES / "t17_private_diagnostic_packet.py")
assert SPEC and SPEC.loader
packet = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(packet)
JOB = "harvest-fixture"
COMMIT = "c" * 40
NONCE = "a" * 64
ESP = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b"
MSR = "e3c9e316-0b5c-4db8-817d-f92df00215ae"
RECOVERY = "de94bba4-06d1-4d40-a16a-bfd50179d6ac"


def boot_sector(oem: bytes) -> bytes:
    sector = bytearray(512)
    sector[0:3] = b"\xebR\x90"
    sector[3:11] = oem
    sector[510:512] = b"\x55\xaa"
    return bytes(sector) if oem else bytes(512)


NTFS = boot_sector(b"NTFS    ")
FVE = boot_sector(b"-FVE-FS-")
FAT = boot_sector(b"MSDOS5.0")
WINDOWS_LAYOUT = [(ESP, 2048, 4095, FAT), (MSR, 4096, 6143, b""),
                  (harvest.BASIC_DATA, 6144, 100000, NTFS), (RECOVERY, 120001, 130000, NTFS)]


def gpt_image(path: Path, partitions: list, *, size: int = 64 * 1024 * 1024, sector: int = 512) -> None:
    """Write a sparse GPT disk; each partition is (type, first_lba, last_lba, first_sector)."""
    count, entry_size, last = 128, 128, size // 512 - 1
    table = bytearray(count * entry_size)
    for index, (kind, first, end, _) in enumerate(partitions):
        struct.pack_into("<16s16sQQQ", table, index * entry_size, uuid.UUID(kind).bytes_le,
                         uuid.uuid4().bytes_le, first, end, 0)
    header = bytearray(92)
    struct.pack_into("<8sIIIIQQQQ16sQIII", header, 0, b"EFI PART", 0x10000, 92, 0, 0, 1, last,
                     34, last - 33, uuid.uuid4().bytes_le, 2, count, entry_size, zlib.crc32(table))
    struct.pack_into("<I", header, 16, zlib.crc32(header))
    with path.open("wb") as output:
        output.truncate(size)
        output.seek(sector)
        output.write(header)
        output.seek(2 * sector)
        output.write(table)
        for _, first, _, body in partitions:
            output.seek(first * 512)
            output.write(body)


def file_hash(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def json_file(path: Path, value: dict) -> str:
    data = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()
    path.write_bytes(data)
    return hashlib.sha256(data).hexdigest()


class FakeTools:
    """Stand-in for hdiutil/diskutil that records every invocation."""

    def __init__(self, *, mount_ok: bool = False, detach_ok: bool = True, on_attach=None) -> None:
        self.calls: list[tuple[str, ...]] = []
        self.clone: str | None = None
        self.attached = False
        self.mount_ok, self.detach_ok, self.on_attach = mount_ok, detach_ok, on_attach

    def __call__(self, argv, **kwargs):
        self.calls.append(tuple(argv))
        output, status = b"", 0
        if argv[:2] == (harvest.HDIUTIL, "attach"):
            self.clone, self.attached = argv[-1], True
            output = plistlib.dumps({"system-entities": [
                {"dev-entry": "/dev/disk99", "content-hint": "GUID_partition_scheme"},
                {"dev-entry": "/dev/disk99s3", "content-hint": "Microsoft Basic Data",
                 "unmapped-content-hint": harvest.BASIC_DATA.upper()}]})
            if self.on_attach:
                self.on_attach()
        elif argv[:2] == (harvest.HDIUTIL, "info"):
            images = [{"image-path": self.clone, "system-entities": [{"dev-entry": "/dev/disk99"}]}] if self.attached else []
            output = plistlib.dumps({"images": images})
        elif argv[:2] == (harvest.HDIUTIL, "detach"):
            self.attached = self.attached and not self.detach_ok
        elif argv[:3] == (harvest.DISKUTIL, "info", "-plist"):
            output = plistlib.dumps({"WritableMedia": False})
        elif argv[:2] == (harvest.DISKUTIL, "mount"):
            status = 0 if self.mount_ok else 1
        return subprocess.CompletedProcess(argv, status, output, b"")


class LaneFixture(unittest.TestCase):
    def setUp(self) -> None:
        if sys.platform != "darwin":
            self.skipTest("the harvest clones with fclonefileat(2) and attaches with hdiutil, both macOS-only")
        self.work = (Path("/tmp") / f"bridgevm-e2e-{JOB}.{secrets.token_hex(3)}")
        self.work.mkdir(mode=0o700)
        self.work = self.work.resolve()
        self.lane = self.work / "lane-1"
        self.lane.mkdir(mode=0o700)
        self.output = Path(tempfile.mkdtemp(prefix="t17-harvest-out-")).resolve()
        self.private = self.output / "private"
        self.private.mkdir(mode=0o700)
        self.slug = "bridgevm-t17-lane-1-" + NONCE[:12]
        self.disks = self.lane / "library" / self.slug / "bundle.vmbridge" / "disks"
        self.disks.mkdir(parents=True)
        self.disk = self.disks / "hvf-target.raw"
        request = {"schema_version": packet.REQUEST_SCHEMA, "job_id": JOB, "commit": COMMIT,
                   "campaign_mode": "pilot", "lane": 1, "nonce": NONCE, "lane_root": str(self.lane),
                   "vm_slug": self.slug}
        result = {"schema_version": packet.LANE_SCHEMA, "job_id": JOB, "commit": COMMIT,
                  "campaign_mode": "pilot", "lane": 1, "nonce": NONCE,
                  "failure_code": "guest-evidence-missing", "first_ready": False,
                  "failure_detail": "first boot has no BVAGENT READY/PONG evidence; host_stop=status=missing,reason=acknowledgement-missing"}
        request_sha = json_file(self.lane / "request.json", request)
        result_sha = json_file(self.private / "lane-1-result.json", result)
        json_file(self.private / "lane-1-authenticated.json", {
            "schema_version": packet.STAMP_SCHEMA, "job_id": JOB, "commit": COMMIT, "lane": 1,
            "nonce": NONCE, "request_sha256": request_sha, "result_sha256": result_sha})
        self.args = argparse.Namespace(private=self.private, lane_root=self.lane, job_id=JOB,
                                       commit=COMMIT, mode="pilot", lane=1)

    def tearDown(self) -> None:
        shutil.rmtree(self.work)
        shutil.rmtree(self.output)

    def sealed(self) -> dict[str, str]:
        return {path.name: file_hash(path) for path in (self.lane / "request.json",
                self.private / "lane-1-result.json", self.private / "lane-1-authenticated.json")}

    def capture(self) -> dict:
        before = self.sealed()
        packet.capture(self.args)
        packet.verify(self.args)
        self.assertEqual(self.sealed(), before, "the harvest changed the sealed lane result")
        index = json.loads((self.private / "t17-diagnostic-lane-1-index.json").read_text())
        return index["guest_setup"]

    def direct(self, budget: int = packet.TOTAL_CAP) -> dict:
        with ExitStack() as stack:
            lane = os.open(self.lane, os.O_RDONLY | os.O_DIRECTORY)
            stack.callback(os.close, lane)
            private = os.open(self.private, os.O_RDONLY | os.O_DIRECTORY)
            stack.callback(os.close, private)
            return harvest.Harvest(str(self.lane), lane, self.slug, private, 1, budget, []).run()

    def assert_released(self) -> None:
        self.assertFalse((self.lane / harvest.WORK_NAME).exists())


class ClassificationTest(LaneFixture):
    def test_gpt_types_and_first_sector_signatures_are_classified(self) -> None:
        gpt_image(self.disk, WINDOWS_LAYOUT + [(harvest.BASIC_DATA, 100001, 120000, FVE)])
        fd = os.open(self.disk, os.O_RDONLY)
        try:
            partitions, sectors = harvest.read_gpt(fd, self.disk.stat().st_size)
        finally:
            os.close(fd)
        self.assertEqual([(item["index"], item["type"], item["signature"]) for item in partitions],
                         [(1, "efi-system", "other"), (2, "microsoft-reserved", "blank"),
                          (3, "basic-data", "ntfs"), (4, "windows-recovery", "ntfs"),
                          (5, "basic-data", "bitlocker")])
        self.assertEqual(partitions[2]["bytes"], (100000 - 6144 + 1) * 512)
        self.assertEqual(sectors[3], NTFS)

    def test_bitlocker_windows_volume_records_fve_without_attaching(self) -> None:
        gpt_image(self.disk, [WINDOWS_LAYOUT[0], (harvest.BASIC_DATA, 6144, 100000, FVE)])
        disk_hash = file_hash(self.disk)
        with mock.patch.object(harvest.subprocess, "run") as run:
            summary = self.capture()
        run.assert_not_called()
        self.assertEqual((summary["outcome"], summary["reason"], summary["fve"]),
                         ("no-ntfs-partition", "bitlocker", True))
        self.assertEqual((summary["cleanup"], summary["source_unchanged"]), ("verified", True))
        self.assertEqual({item["status"] for item in summary["items"]}, {"not-attempted"})
        self.assertEqual(file_hash(self.disk), disk_hash)
        self.assert_released()

    def test_invalid_or_foreign_gpt_is_refused_before_any_tool(self) -> None:
        def crc(path: Path) -> None:
            with path.open("r+b") as output:
                output.seek(512 + 24)
                output.write(b"\x02")
        def entries(path: Path) -> None:
            with path.open("r+b") as output:
                output.seek(1024 + 40)
                output.write(b"\x07")
        cases = (("header", lambda path: (gpt_image(path, WINDOWS_LAYOUT), crc(path))),
                 ("entries", lambda path: (gpt_image(path, WINDOWS_LAYOUT), entries(path))),
                 ("sector-4096", lambda path: gpt_image(path, WINDOWS_LAYOUT, sector=4096)),
                 ("disk-too-small", lambda path: path.write_bytes(b"disk1")))
        for reason, build in cases:
            with self.subTest(reason=reason):
                build(self.disk)
                with mock.patch.object(harvest.subprocess, "run") as run:
                    summary = self.direct()
                run.assert_not_called()
                self.assertEqual((summary["outcome"], summary["reason"], summary["cleanup"]),
                                 ("gpt-invalid", reason, "verified"))
                self.assert_released()
                self.disk.unlink()

    def test_absent_or_unsafe_disk_is_recorded_without_a_work_tree(self) -> None:
        outside = self.work / "outside.raw"
        gpt_image(outside, WINDOWS_LAYOUT)
        self.assertEqual(self.direct()["outcome"], "disk-absent")
        for kind in ("symlink", "hardlink", "directory", "linked-parent"):
            with self.subTest(kind=kind):
                if kind == "symlink":
                    self.disk.symlink_to(outside)
                elif kind == "hardlink":
                    os.link(outside, self.disk)
                elif kind == "directory":
                    self.disk.mkdir()
                else:
                    shutil.rmtree(self.disks)
                    self.disks.symlink_to(self.work)
                summary = self.direct()
                self.assertEqual((summary["outcome"], summary["cleanup"]), ("disk-unsafe", "not-needed"))
                self.assert_released()
                if self.disks.is_symlink():
                    self.disks.unlink()
                elif self.disk.is_dir() and not self.disk.is_symlink():
                    self.disk.rmdir()
                else:
                    self.disk.unlink()


class ReleaseTest(LaneFixture):
    def setUp(self) -> None:
        super().setUp()
        gpt_image(self.disk, WINDOWS_LAYOUT)
        self.disk_hash = file_hash(self.disk)
        self.sector = mock.patch.object(harvest, "device_sector", return_value=NTFS)
        self.sector.start()
        self.addCleanup(self.sector.stop)

    def test_mount_failure_detaches_only_the_clone_and_keeps_the_packet(self) -> None:
        tools = FakeTools()
        with mock.patch.object(harvest.subprocess, "run", side_effect=tools):
            summary = self.capture()
        self.assertEqual((summary["outcome"], summary["selected_partition"], summary["mounted"]),
                         ("mount-failed", 3, False))
        self.assertEqual(summary["cleanup"], "verified")
        attach = next(call for call in tools.calls if call[1] == "attach")
        self.assertTrue({"-readonly", "-nomount", "diskimage-class=CRawDiskImage"} <= set(attach))
        self.assertEqual(attach[-1], os.path.realpath(self.lane / harvest.WORK_NAME / harvest.CLONE_NAME))
        self.assertNotIn(str(self.disk), [argument for call in tools.calls for argument in call])
        mount = next(call for call in tools.calls if call[:2] == (harvest.DISKUTIL, "mount"))
        self.assertEqual(mount[2:4], ("readOnly", "nobrowse"))
        self.assertIn((harvest.HDIUTIL, "detach", "/dev/disk99"), tools.calls)
        self.assertFalse(any("-force" in call for call in tools.calls))
        self.assertEqual(file_hash(self.disk), self.disk_hash)
        self.assert_released()

    def test_unreleasable_attachment_is_recorded_and_left_for_the_tier_fence(self) -> None:
        tools = FakeTools(detach_ok=False)
        with mock.patch.object(harvest.subprocess, "run", side_effect=tools):
            summary = self.capture()
        self.assertEqual((summary["outcome"], summary["cleanup"]), ("mount-failed", "failed"))
        self.assertIn((harvest.HDIUTIL, "detach", "-force", "/dev/disk99"), tools.calls)
        self.assertTrue((self.lane / harvest.WORK_NAME / harvest.CLONE_NAME).is_file())
        runner = (GATES / "run-windows-product-e2e-tier.sh").read_text()
        self.assertIn('{ mount; /usr/bin/hdiutil info || :; } 2>/dev/null | grep -F "$WORK"', runner)

    def test_unverifiable_attachment_state_fails_closed(self) -> None:
        tools = FakeTools()
        def broken(argv, **kwargs):
            completed = tools(argv, **kwargs)
            return subprocess.CompletedProcess(argv, 1, b"", b"") if argv[1] == "info" and argv[0] == harvest.HDIUTIL else completed
        with mock.patch.object(harvest.subprocess, "run", side_effect=broken):
            summary = self.direct()
        self.assertEqual(summary["cleanup"], "failed")

    def test_deadline_and_signal_stop_the_harvest_but_not_its_release(self) -> None:
        tools = FakeTools(mount_ok=True, on_attach=lambda: os.kill(os.getpid(), signal.SIGTERM))
        previous = signal.getsignal(signal.SIGTERM)
        with mock.patch.object(harvest.subprocess, "run", side_effect=tools):
            summary = self.direct()
        self.assertEqual((summary["outcome"], summary["reason"], summary["cleanup"]),
                         ("interrupted", "sigterm", "verified"))
        self.assertIs(signal.getsignal(signal.SIGTERM), previous)
        self.assertFalse(any(call[:2] == (harvest.DISKUTIL, "mount") for call in tools.calls))
        tools = FakeTools()
        with mock.patch.object(harvest, "SECONDS", 0), mock.patch.object(harvest.subprocess, "run", side_effect=tools):
            summary = self.direct()
        self.assertEqual((summary["outcome"], summary["cleanup"]), ("deadline-exceeded", "verified"))
        self.assertFalse(any(call[1] == "attach" for call in tools.calls))
        self.assert_released()

    def test_harvest_defect_is_recorded_and_never_raises(self) -> None:
        with mock.patch.object(harvest, "read_gpt", side_effect=RuntimeError("defect")):
            summary = self.capture()
        self.assertEqual((summary["outcome"], summary["reason"], summary["cleanup"]),
                         ("error", "RuntimeError", "verified"))


class CopyTest(LaneFixture):
    def setUp(self) -> None:
        super().setUp()
        self.volume = Path(tempfile.mkdtemp(prefix="t17-harvest-volume-")).resolve()
        self.addCleanup(shutil.rmtree, self.volume)
        (self.volume / "BridgeVM/provisioning").mkdir(parents=True)
        (self.volume / "BridgeVM/guest-tools-firstboot.log").write_bytes(b"BVAGENT PROVISION START\r\n")
        (self.volume / "BridgeVM/provisioning/payload-receipt.tsv").write_bytes(b"schema\tv1\r\n")
        (self.work / "secret.json").write_text("outside the guest volume")
        (self.volume / "BridgeVM/guest-tools-provisioned.json").symlink_to(self.work / "secret.json")
        (self.volume / "bvagent.log").write_bytes(b"x" * 65)
        (self.work / "panther").mkdir()
        (self.work / "panther/setupact.log").write_text("outside")
        (self.volume / "Windows/Setup/State/State.ini").mkdir(parents=True)
        (self.volume / "Windows/Panther").symlink_to(self.work / "panther")
        (self.volume / "Windows/System32/winevt/Logs").mkdir(parents=True)
        (self.volume / "Windows/System32/winevt/Logs/System.evtx").write_bytes(b"E" * 40)
        (self.volume / "Windows/INF").mkdir()
        os.mkfifo(self.volume / "Windows/INF/setupapi.dev.log")

    def copy(self, budget: int) -> dict:
        with ExitStack() as stack:
            volume = os.open(self.volume, harvest.DIR_FLAGS)
            stack.callback(os.close, volume)
            private = os.open(self.private, harvest.DIR_FLAGS)
            stack.callback(os.close, private)
            created: list[str] = []
            work = harvest.Harvest(str(self.lane), -1, self.slug, private, 1, budget, created)
            work.summary.update(mounted=True, filesystem="ntfs", selected_partition=3,
                                partitions=[{"index": 3, "type": "basic-data", "first_lba": 6144,
                                             "last_lba": 100000, "bytes": (100000 - 6144 + 1) * 512,
                                             "signature": "ntfs"}])
            with mock.patch.object(harvest, "ITEM_CAP", 64):
                work.copy_items(volume)
            self.assertEqual(sorted(created), sorted(item["file"] for item in work.summary["items"] if item["file"]))
            return work.summary

    def test_allowlist_is_bounded_hashed_private_and_refuses_links(self) -> None:
        before = sorted(os.listdir(self.private))
        summary = self.copy(budget=60)
        status = {item["id"]: (item["status"], item["reason"]) for item in summary["items"]}
        self.assertEqual(status["firstboot_log"], ("retained", "none"))
        self.assertEqual(status["payload_receipt"], ("retained", "none"))
        self.assertEqual(status["provisioned_marker"][0], "unsafe")
        self.assertEqual(status["agent_log"], ("oversize", "32-mib-cap"))
        self.assertEqual(status["setup_state"], ("unsafe", "not-regular"))
        self.assertEqual(status["panther_setupact"][0], "unsafe")
        self.assertEqual(status["system_evtx"], ("over-budget", "packet-total-cap"))
        self.assertEqual(status["setupapi_dev"], ("unsafe", "not-regular"))
        self.assertEqual(status["setupapi_offline"], ("absent", "none"))
        first = summary["items"][0]
        self.assertEqual(first["sha256"], file_hash(self.volume / "BridgeVM/guest-tools-firstboot.log"))
        self.assertEqual(summary["total_bytes"], 25 + 11)
        created = sorted(set(os.listdir(self.private)) - set(before))
        self.assertEqual(created, ["t17-diagnostic-lane-1-guest-firstboot_log.bin",
                                   "t17-diagnostic-lane-1-guest-payload_receipt.bin"])
        for name in created:
            self.assertEqual(stat.S_IMODE((self.private / name).stat().st_mode), 0o600)
        self.assertNotIn("BVAGENT PROVISION START", json.dumps(summary))
        self.assertEqual((self.work / "secret.json").read_text(), "outside the guest volume")
        with ExitStack() as stack:
            private = os.open(self.private, harvest.DIR_FLAGS)
            stack.callback(os.close, private)
            device = os.fstat(private).st_dev
            with mock.patch.object(harvest, "ITEM_CAP", 64):
                self.assertEqual(harvest.verify(private, device, summary, 1), 36)
                for mutate in (lambda value: value["items"][3].update(status="oversize", original_size=64),
                               lambda value: value["items"][12].update(bytes=1),
                               lambda value: value.update(total_bytes=35),
                               lambda value: value.update(budget_bytes=35),
                               lambda value: value.update(mounted=False),
                               lambda value: value["items"][0].update(guest_path="Users/secret.txt"),
                               lambda value: value.update(fve=True),
                               lambda value: value.update(extra=True)):
                    tampered = json.loads(json.dumps(summary))
                    mutate(tampered)
                    with self.assertRaises(harvest.HarvestError):
                        harvest.verify(private, device, tampered, 1)
                (self.private / created[0]).write_bytes(b"BVAGENT PROVISION STARX\r\n")
                with self.assertRaises(harvest.HarvestError):
                    harvest.verify(private, device, summary, 1)
                os.chmod(self.private / created[1], 0o644)
                with self.assertRaises(harvest.HarvestError):
                    harvest.verify(private, device, summary, 1)

    def test_packet_verifier_rejects_a_tampered_guest_setup_summary(self) -> None:
        self.capture()
        index_path = self.private / "t17-diagnostic-lane-1-index.json"
        index = json.loads(index_path.read_text())
        index["guest_setup"]["items"][0].update(status="retained", file="x", bytes=1)
        index_path.write_text(json.dumps(index))
        with self.assertRaises(ValueError):
            packet.verify(self.args)


class AttachedImageTest(LaneFixture):
    """Real hdiutil/diskutil plumbing on a synthetic FAT GPT image (NTFS cannot be created on macOS)."""

    def test_read_only_attach_copy_and_detach_leave_no_host_state(self) -> None:
        source = Path(tempfile.mkdtemp(prefix="t17-harvest-src-")).resolve()
        self.addCleanup(shutil.rmtree, source)
        (source / "BridgeVM").mkdir()
        (source / "BridgeVM/guest-tools-firstboot.log").write_bytes(b"BVAGENT PROVISION START\r\n")
        (source / "bvagent.log").write_bytes(b"A" * 100)
        (source / "Windows/Panther/UnattendGC").mkdir(parents=True)
        (source / "Windows/Panther/UnattendGC/setupact.log").write_bytes(b"FirstLogonCommands\r\n")
        image = source.parent / f"{source.name}.cdr"
        self.addCleanup(lambda: image.unlink() if image.exists() else None)
        created = subprocess.run([harvest.HDIUTIL, "create", "-quiet", "-srcfolder", str(source), "-fs", "MS-DOS FAT16",
                                  "-layout", "GPTSPUD", "-format", "UDTO", "-volname", "BVHARVEST",
                                  "-o", str(source.parent / source.name)], capture_output=True, timeout=120, check=False)
        if created.returncode != 0 or not image.is_file():
            self.skipTest("this host's hdiutil cannot build a synthetic GPT FAT image: " + created.stderr.decode(errors="replace")[-200:])
        probe = subprocess.run([harvest.HDIUTIL, "attach", "-readonly", "-nomount", "-plist", "-imagekey",
                                "diskimage-class=CRawDiskImage", str(image)], capture_output=True, timeout=120, check=False)
        entities = harvest.plist(probe.stdout).get("system-entities", [])
        for device in {item["dev-entry"] for item in entities if harvest.WHOLE.fullmatch(item.get("dev-entry", ""))}:
            subprocess.run([harvest.HDIUTIL, "detach", device], capture_output=True, timeout=120, check=True)
        if probe.returncode != 0:
            self.skipTest("this host refuses a read-only hdiutil attach: " + probe.stderr.decode(errors="replace")[-200:])
        shutil.copyfile(image, self.disk)
        fd = os.open(self.disk, os.O_RDONLY)
        try:
            partitions, sectors = harvest.read_gpt(fd, self.disk.stat().st_size)
        finally:
            os.close(fd)
        self.assertEqual([(item["index"], item["type"]) for item in partitions], [(1, "basic-data")])
        disk_hash = file_hash(self.disk)
        with mock.patch.object(harvest, "NTFS_OEM", sectors[1][3:11]), mock.patch.object(harvest, "ITEM_CAP", 64):
            summary = self.capture()
        self.assertEqual((summary["outcome"], summary["selected_partition"], summary["mounted"],
                          summary["filesystem"], summary["cleanup"], summary["source_unchanged"]),
                         ("harvested", 1, True, "msdos", "verified", True))
        status = {item["id"]: item for item in summary["items"]}
        self.assertEqual(status["firstboot_log"]["sha256"], file_hash(source / "BridgeVM/guest-tools-firstboot.log"))
        self.assertEqual(status["unattendgc_setupact"]["sha256"], file_hash(source / "Windows/Panther/UnattendGC/setupact.log"))
        self.assertEqual((status["agent_log"]["status"], status["system_evtx"]["status"]), ("oversize", "absent"))
        self.assertEqual(file_hash(self.disk), disk_hash)
        self.assert_released()
        info = subprocess.run([harvest.HDIUTIL, "info"], capture_output=True, text=True, check=True).stdout
        mounts = subprocess.run(["/sbin/mount"], capture_output=True, text=True, check=True).stdout
        self.assertNotIn(str(self.work), info + mounts)


if __name__ == "__main__":
    unittest.main()
