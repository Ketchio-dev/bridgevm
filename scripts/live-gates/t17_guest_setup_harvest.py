#!/usr/bin/env python3
"""Harvest a fixed Windows setup allowlist from a clone of a failed T17 lane disk.

The lane disk is never attached or written: its fclonefileat(2) clone (no copy
fallback) is attached read-only without automount, and only the largest NTFS
basic-data partition is mounted read-only. A harvest failure is recorded in the
private summary and never changes the lane result; an unproven release leaves
the work directory, which the tier's cleanup fence reports as cleanup-failed.
"""
from __future__ import annotations

from contextlib import ExitStack
import ctypes
import errno
import hashlib
import os
import plistlib
import re
import signal
import stat
import struct
import subprocess
import time
import uuid
import zlib

SCHEMA = "bridgevm.t17-guest-setup-harvest.v1"
HDIUTIL = "/usr/bin/hdiutil"
DISKUTIL = "/usr/sbin/diskutil"
TOOL_ENV = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "LC_ALL": "C"}
WORK_NAME = "guest-setup-harvest"
CLONE_NAME = "disk.raw"
MOUNT_NAME = "volume"
# SECONDS is checked only between tool calls, items and 1 MiB chunks; a blocked
# open or read on the raw slice or mounted guest volume is not bounded by it.
SECONDS = 120
TOOL_SECONDS = 60
RELEASE_SECONDS = 20  # each release call; SECONDS never cuts release short
ITEM_CAP = 32 * 1024 * 1024
SECTOR = 512
MAX_ENTRIES = 256
MAX_PARTITIONS = 16
CHUNK = 1024 * 1024
CLONE_NOFOLLOW = 0x1
CLONE_NOOWNERCOPY = 0x2
BASIC_DATA = "ebd0a0a2-b9e5-4433-87c0-68b6b72699c7"
TYPES = {"c12a7328-f81f-11d2-ba4b-00a0c93ec93b": "efi-system",
         "e3c9e316-0b5c-4db8-817d-f92df00215ae": "microsoft-reserved",
         BASIC_DATA: "basic-data",
         "de94bba4-06d1-4d40-a16a-bfd50179d6ac": "windows-recovery"}
NTFS_OEM = b"NTFS    "
FVE_OEM = b"-FVE-FS-"
SIGNATURES = ("ntfs", "bitlocker", "blank", "other")
# Fixed guest-relative allowlist, smallest and most diagnostic first so the
# packet total cap drops the bulk logs before the provisioning evidence.
ALLOWLIST = (
    ("firstboot_log", "BridgeVM/guest-tools-firstboot.log"),
    ("provisioned_marker", "BridgeVM/guest-tools-provisioned.json"),
    ("payload_receipt", "BridgeVM/provisioning/payload-receipt.tsv"),
    ("agent_log", "bvagent.log"),
    ("agent_task", "Windows/System32/Tasks/BridgeVM Guest Agent"),
    ("setup_state", "Windows/Setup/State/State.ini"),
    ("unattendgc_setupact", "Windows/Panther/UnattendGC/setupact.log"),
    ("unattendgc_setuperr", "Windows/Panther/UnattendGC/setuperr.log"),
    ("panther_setupact", "Windows/Panther/setupact.log"),
    ("panther_setuperr", "Windows/Panther/setuperr.log"),
    ("winlogon_evtx", "Windows/System32/winevt/Logs/Microsoft-Windows-Winlogon%4Operational.evtx"),
    ("shellcore_evtx", "Windows/System32/winevt/Logs/Microsoft-Windows-Shell-Core%4Operational.evtx"),
    ("taskscheduler_evtx", "Windows/System32/winevt/Logs/Microsoft-Windows-TaskScheduler%4Operational.evtx"),
    ("system_evtx", "Windows/System32/winevt/Logs/System.evtx"),
    ("setupapi_offline", "Windows/INF/setupapi.offline.log"),
    ("setupapi_dev", "Windows/INF/setupapi.dev.log"),
)
OUTCOMES = ("harvested", "disk-absent", "disk-unsafe", "work-unsafe", "clone-failed", "gpt-invalid",
            "no-ntfs-partition", "attach-failed", "mount-failed", "mount-not-read-only",
            "private-unsafe", "deadline-exceeded", "interrupted", "error")
ITEM_STATUSES = ("retained", "absent", "oversize", "over-budget", "unsafe", "error", "not-attempted")
SUMMARY_KEYS = {"schema_version", "outcome", "reason", "disk_bytes", "source_unchanged", "fve",
                "partitions", "selected_partition", "mounted", "filesystem", "cleanup",
                "deadline_seconds", "budget_bytes", "total_bytes", "items"}
PARTITION_KEYS = {"index", "type", "first_lba", "last_lba", "bytes", "signature"}
ITEM_KEYS = {"id", "guest_path", "status", "reason", "file", "bytes", "original_size", "sha256"}
DIR_FLAGS = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
READ_FLAGS = os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC | os.O_NONBLOCK
WRITE_FLAGS = os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC
WHOLE = re.compile(r"/dev/disk[0-9]{1,4}\Z")
REASON = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,63}\Z")
FILESYSTEM = re.compile(r"[a-z0-9]{1,16}\Z")
HEX = re.compile(r"[0-9a-f]{64}\Z")


class HarvestError(ValueError):
    """Stops the harvest with a recorded (outcome, reason)."""


class ItemError(Exception):
    """Stops one allowlist item; the harvest continues."""


def file_name(ordinal: int, identifier: str) -> str:
    return f"t17-diagnostic-lane-{ordinal}-guest-{identifier}.bin"


def natural(value: object) -> bool:
    return type(value) is int and value >= 0


def signature(sector: bytes) -> str:
    oem = sector[3:11]
    if oem == NTFS_OEM:
        return "ntfs"
    if oem == FVE_OEM:
        return "bitlocker"
    return "other" if any(sector) else "blank"


def read_gpt(fd: int, size: int) -> tuple[list[dict], dict[int, bytes]]:
    """Parse a 512-byte-sector GPT and classify each partition's first sector."""
    if size < 34 * SECTOR:
        raise HarvestError("gpt-invalid", "disk-too-small")
    header = os.pread(fd, SECTOR, SECTOR)
    if header[:8] != b"EFI PART":
        reason = "sector-4096" if os.pread(fd, 8, 4096) == b"EFI PART" else "signature"
        raise HarvestError("gpt-invalid", reason)
    (header_size, crc, _, current, _, first_usable, last_usable, _, table_lba, count,
     entry_size, table_crc) = struct.unpack_from("<IIIQQQQ16sQIII", header, 12)
    body = bytearray(header[:header_size])
    body[16:20] = bytes(4)
    if (not 92 <= header_size <= SECTOR or zlib.crc32(bytes(body)) != crc or current != 1
            or entry_size != 128 or not 1 <= count <= MAX_ENTRIES or table_lba < 2
            or not first_usable <= last_usable or (last_usable + 1) * SECTOR > size
            or table_lba * SECTOR + count * entry_size > size):
        raise HarvestError("gpt-invalid", "header")
    table = os.pread(fd, count * entry_size, table_lba * SECTOR)
    if len(table) != count * entry_size or zlib.crc32(table) != table_crc:
        raise HarvestError("gpt-invalid", "entries")
    partitions, sectors = [], {}
    for index in range(1, count + 1):
        entry = table[(index - 1) * entry_size:index * entry_size]
        if entry[:16] == bytes(16):
            continue
        first, last = struct.unpack_from("<QQ", entry, 32)
        if not first_usable <= first <= last <= last_usable or len(partitions) == MAX_PARTITIONS:
            raise HarvestError("gpt-invalid", "partition")
        sectors[index] = os.pread(fd, SECTOR, first * SECTOR)
        partitions.append({"index": index, "type": TYPES.get(str(uuid.UUID(bytes_le=entry[:16])), "other"),
                           "first_lba": first, "last_lba": last, "bytes": (last - first + 1) * SECTOR,
                           "signature": signature(sectors[index])})
    return partitions, sectors


def plist(data: bytes | None) -> dict:
    try:
        value = plistlib.loads(data) if data else {}
    except Exception:  # noqa: BLE001 - malformed tool output is an unverified state
        return {}
    return value if isinstance(value, dict) else {}


def device_sector(device: str) -> bytes:
    fd = os.open(device, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC)
    try:
        return os.pread(fd, SECTOR, 0)
    finally:
        os.close(fd)


class Harvest:
    def __init__(self, lane_root: str, lane: int, slug: str, private: int, ordinal: int,
                 budget: int, created: list[str]) -> None:
        self.lane_root, self.lane, self.slug = lane_root, lane, slug
        self.private, self.ordinal, self.created = private, ordinal, created
        self.deadline = time.monotonic() + SECONDS
        self.signal: str | None = None
        self.work: int | None = None
        self.paths: dict[str, str] = {}
        self.attached = self.mounted = False
        self.summary = {
            "schema_version": SCHEMA, "outcome": "error", "reason": "incomplete", "disk_bytes": None,
            "source_unchanged": None, "fve": False, "partitions": [], "selected_partition": None,
            "mounted": False, "filesystem": None, "cleanup": "not-needed", "deadline_seconds": SECONDS,
            "budget_bytes": max(0, budget), "total_bytes": 0,
            "items": [{"id": identifier, "guest_path": path, "status": "not-attempted", "reason": "none",
                       "file": None, "bytes": 0, "original_size": None, "sha256": None}
                      for identifier, path in ALLOWLIST]}

    def interrupt(self, number: int, _frame: object) -> None:
        # Record only, so release can run; a queue cancel SIGKILLs the group 5 s later.
        self.signal = signal.Signals(number).name.lower()

    def check(self) -> None:
        if self.signal is not None:
            raise HarvestError("interrupted", self.signal)
        if time.monotonic() >= self.deadline:
            raise HarvestError("deadline-exceeded")

    def tool(self, *argv: str, release: bool = False) -> bytes | None:
        if not release:
            self.check()
        limit = RELEASE_SECONDS if release else min(TOOL_SECONDS, self.deadline - time.monotonic())
        try:
            completed = subprocess.run(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                       stderr=subprocess.DEVNULL, env=TOOL_ENV, timeout=limit, check=False)
        except subprocess.TimeoutExpired as error:
            if release:
                return None
            raise HarvestError("deadline-exceeded", os.path.basename(argv[0])) from error
        except OSError:
            return None
        return completed.stdout if completed.returncode == 0 else None

    def run(self) -> dict:
        """Return the private summary; a harvest failure is recorded, never raised."""
        previous = {}
        for number in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
            try:
                previous[number] = signal.signal(number, self.interrupt)
            except ValueError:  # not the main thread; the deadline still bounds the harvest
                pass
        try:
            with ExitStack() as stack:
                disk, before = self.locate(stack)
                try:
                    self.collect(disk, stack)
                    self.summary["outcome"], self.summary["reason"] = "harvested", "none"
                finally:
                    if self.work is not None:
                        self.summary["cleanup"] = self.release()
                    after = os.fstat(disk)
                    self.summary["source_unchanged"] = (after.st_size, after.st_mtime_ns) == before
        except HarvestError as error:
            self.summary["outcome"] = error.args[0]
            self.summary["reason"] = error.args[1] if len(error.args) > 1 else "none"
        except Exception as error:  # noqa: BLE001 - a harvest defect must not discard the packet
            self.summary["outcome"], self.summary["reason"] = "error", type(error).__name__
        finally:
            for number, handler in previous.items():
                signal.signal(number, handler)
        return self.summary

    def locate(self, stack: ExitStack) -> tuple[int, tuple[int, int]]:
        device = os.fstat(self.lane).st_dev
        parent = self.lane
        for name in ("library", self.slug, "bundle.vmbridge", "disks", "hvf-target.raw"):
            try:
                before = os.stat(name, dir_fd=parent, follow_symlinks=False)
            except FileNotFoundError as error:
                raise HarvestError("disk-absent") from error
            regular = name == "hvf-target.raw"
            expected = stat.S_ISREG(before.st_mode) and before.st_nlink == 1 if regular else stat.S_ISDIR(before.st_mode)
            if not expected or before.st_uid != os.geteuid() or before.st_dev != device:
                raise HarvestError("disk-unsafe", name if not regular else "disk")
            fd = os.open(name, READ_FLAGS if regular else DIR_FLAGS, dir_fd=parent)
            stack.callback(os.close, fd)
            after = os.fstat(fd)
            if (before.st_dev, before.st_ino, before.st_mode, before.st_size) != (after.st_dev, after.st_ino, after.st_mode, after.st_size):
                raise HarvestError("disk-unsafe", "changed")
            parent = fd
        return parent, (after.st_size, after.st_mtime_ns)

    def collect(self, disk: int, stack: ExitStack) -> None:
        try:
            os.mkdir(WORK_NAME, 0o700, dir_fd=self.lane)
        except OSError as error:
            raise HarvestError("work-unsafe", errno.errorcode.get(error.errno, "unknown")) from error
        self.work = os.open(WORK_NAME, DIR_FLAGS, dir_fd=self.lane)
        stack.callback(os.close, self.work)
        os.fchmod(self.work, 0o700)
        info = os.fstat(self.work)
        real = os.path.realpath(f"{self.lane_root}/{WORK_NAME}")
        probe = os.stat(real, follow_symlinks=False)
        if (stat.S_IMODE(info.st_mode) != 0o700 or info.st_uid != os.geteuid()
                or (probe.st_dev, probe.st_ino) != (info.st_dev, info.st_ino)):
            raise HarvestError("work-unsafe", "identity")
        self.paths = {"clone": f"{real}/{CLONE_NAME}", "mount": f"{real}/{MOUNT_NAME}"}
        os.mkdir(MOUNT_NAME, 0o700, dir_fd=self.work)
        clone = self.clone(disk, stack)
        size = os.fstat(clone).st_size
        self.summary["disk_bytes"] = size
        partitions, sectors = read_gpt(clone, size)
        fve = any(item["signature"] == "bitlocker" for item in partitions)
        self.summary.update(partitions=partitions, fve=fve)
        candidates = [item for item in partitions if item["type"] == "basic-data" and item["signature"] == "ntfs"]
        if not candidates:
            raise HarvestError("no-ntfs-partition", "bitlocker" if fve else "none")
        chosen = max(candidates, key=lambda item: (item["bytes"], -item["index"]))["index"]
        self.summary["selected_partition"] = chosen
        device = self.mount(chosen, sectors[chosen])
        volume = os.open(MOUNT_NAME, DIR_FLAGS, dir_fd=self.work)
        try:
            # An open directory would keep the volume busy, so it closes before release.
            details = plist(self.tool(DISKUTIL, "info", "-plist", self.paths["mount"]))
            if os.fstat(volume).st_dev == info.st_dev:
                raise HarvestError("mount-failed", "not-mounted")
            if (not os.fstatvfs(volume).f_flag & os.ST_RDONLY or details.get("WritableMedia") is not False
                    or details.get("WritableVolume") is not False
                    or details.get("DeviceIdentifier") != device.removeprefix("/dev/")):
                raise HarvestError("mount-not-read-only", "volume")
            filesystem = details.get("FilesystemType")
            self.summary.update(mounted=True, filesystem=filesystem if isinstance(filesystem, str) and FILESYSTEM.fullmatch(filesystem) else "unknown")
            self.copy_items(volume)
        finally:
            os.close(volume)

    def copy_items(self, volume: int) -> None:
        remaining = self.summary["budget_bytes"]
        for item in self.summary["items"]:
            self.check()
            self.copy_item(volume, item, remaining)
            remaining -= item["bytes"]
            self.summary["total_bytes"] += item["bytes"]

    def clone(self, disk: int, stack: ExitStack) -> int:
        try:
            function = ctypes.CDLL(None, use_errno=True).fclonefileat
        except (AttributeError, OSError) as error:
            raise HarvestError("clone-failed", "unavailable") from error
        function.argtypes = (ctypes.c_int, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint32)
        function.restype = ctypes.c_int
        if function(disk, self.work, CLONE_NAME.encode(), CLONE_NOFOLLOW | CLONE_NOOWNERCOPY) != 0:
            raise HarvestError("clone-failed", errno.errorcode.get(ctypes.get_errno(), "unknown"))
        fd = os.open(CLONE_NAME, READ_FLAGS, dir_fd=self.work)
        stack.callback(os.close, fd)
        info, source = os.fstat(fd), os.fstat(disk)
        if (not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or info.st_size != source.st_size
                or info.st_dev != source.st_dev):
            raise HarvestError("clone-failed", "identity")
        return fd

    def mount(self, index: int, sector: bytes) -> str:
        self.attached = True
        attached = plist(self.tool(HDIUTIL, "attach", "-readonly", "-nomount", "-noverify", "-noautofsck",
                                   "-noautoopen", "-plist", "-imagekey", "diskimage-class=CRawDiskImage",
                                   self.paths["clone"]))
        entities = attached.get("system-entities")
        entities = [item for item in entities if isinstance(item, dict)] if isinstance(entities, list) else []
        wholes = [item.get("dev-entry") for item in entities if WHOLE.fullmatch(str(item.get("dev-entry")))]
        if len(wholes) != 1:
            raise HarvestError("attach-failed", "whole-disk")
        device = f"{wholes[0]}s{index}"
        hints = {str(item.get(key, "")).lower() for item in entities if item.get("dev-entry") == device
                 for key in ("content-hint", "unmapped-content-hint")}
        if not hints & {BASIC_DATA, "microsoft basic data"}:
            raise HarvestError("attach-failed", "partition-hint")
        try:
            matches = device_sector(device) == sector
        except OSError as error:
            raise HarvestError("attach-failed", errno.errorcode.get(error.errno, "unknown")) from error
        if not matches:
            raise HarvestError("attach-failed", "partition-sector")
        if plist(self.tool(DISKUTIL, "info", "-plist", device)).get("WritableMedia") is not False:
            raise HarvestError("mount-not-read-only", "media")
        self.mounted = True
        if self.tool(DISKUTIL, "mount", "readOnly", "nobrowse", "-mountPoint", self.paths["mount"], device) is None:
            raise HarvestError("mount-failed", "diskutil")
        return device

    def copy_item(self, volume: int, item: dict, remaining: int) -> None:
        parts = item["guest_path"].split("/")
        opened: list[int] = []
        try:
            try:
                for part in parts[:-1]:
                    opened.append(os.open(part, DIR_FLAGS, dir_fd=opened[-1] if opened else volume))
                source = os.open(parts[-1], READ_FLAGS, dir_fd=opened[-1] if opened else volume)
                opened.append(source)
                info = os.fstat(source)
            except FileNotFoundError:
                item["status"] = "absent"
                return
            except OSError as error:
                unsafe = error.errno in (errno.ELOOP, errno.ENOTDIR)
                item.update(status="unsafe" if unsafe else "error", reason=errno.errorcode.get(error.errno, "unknown"))
                return
            item["original_size"] = info.st_size
            if not stat.S_ISREG(info.st_mode):
                item.update(status="unsafe", reason="not-regular")
            elif info.st_size > ITEM_CAP:
                item.update(status="oversize", reason="32-mib-cap")
            elif info.st_size > remaining:
                item.update(status="over-budget", reason="packet-total-cap")
            else:
                self.copy_bytes(source, info, item)
        finally:
            for fd in reversed(opened):
                os.close(fd)

    def copy_bytes(self, source: int, info: os.stat_result, item: dict) -> None:
        name = file_name(self.ordinal, item["id"])
        try:
            output = os.open(name, WRITE_FLAGS, 0o600, dir_fd=self.private)
        except OSError as error:
            raise HarvestError("private-unsafe", errno.errorcode.get(error.errno, "unknown")) from error
        self.created.append(name)
        try:
            written = os.fstat(output)
            if (not stat.S_ISREG(written.st_mode) or written.st_uid != os.geteuid() or written.st_nlink != 1
                    or stat.S_IMODE(written.st_mode) != 0o600 or written.st_dev != os.fstat(self.private).st_dev):
                raise HarvestError("private-unsafe", "destination")
            checksum, position = hashlib.sha256(), 0
            while position < info.st_size:
                self.check()
                block = os.pread(source, min(CHUNK, info.st_size - position), position)
                if not block:
                    raise ItemError("short-read")
                view = memoryview(block)
                while view:
                    count = os.write(output, view)
                    if count <= 0:
                        raise ItemError("short-write")
                    view = view[count:]
                checksum.update(block)
                position += len(block)
            os.fsync(output)
            after = os.fstat(source)
            if (after.st_size, after.st_mtime_ns) != (info.st_size, info.st_mtime_ns):
                raise ItemError("changed")
            item.update(status="retained", reason="none", file=name, bytes=info.st_size, sha256=checksum.hexdigest())
        except (ItemError, OSError) as error:
            self.discard(name)
            item.update(status="error", reason=str(error) if isinstance(error, ItemError) else errno.errorcode.get(error.errno, "unknown"))
        except BaseException:
            self.discard(name)
            raise
        finally:
            os.close(output)

    def discard(self, name: str) -> None:
        os.unlink(name, dir_fd=self.private)
        self.created.remove(name)

    def attachments(self) -> list[str] | None:
        """Whole-disk devices still backed by the clone, or None when unverifiable."""
        images = plist(self.tool(HDIUTIL, "info", "-plist", release=True)).get("images")
        if not isinstance(images, list):
            return None
        devices = []
        for image in images:
            if isinstance(image, dict) and image.get("image-path") == self.paths["clone"]:
                entities = image.get("system-entities")
                wholes = [item.get("dev-entry") for item in entities if isinstance(item, dict)
                          and WHOLE.fullmatch(str(item.get("dev-entry")))] if isinstance(entities, list) else []
                if not wholes:
                    return None
                devices += wholes
        return devices

    def release(self) -> str:
        """Unmount, detach and remove the clone; any unproven step leaves it for the fence."""
        try:
            if self.mounted:
                self.tool(DISKUTIL, "unmount", self.paths["mount"], release=True)
            if self.attached:
                for force in ((), ("-force",), ("-force",)):
                    devices = self.attachments()
                    if not devices:
                        break
                    for device in devices:
                        self.tool(HDIUTIL, "detach", *force, device, release=True)
                if self.attachments() != []:
                    return "failed"
            work_device = os.fstat(self.work).st_dev
            try:
                if os.stat(MOUNT_NAME, dir_fd=self.work, follow_symlinks=False).st_dev != work_device:
                    return "failed"
                os.rmdir(MOUNT_NAME, dir_fd=self.work)
            except FileNotFoundError:
                pass
            try:
                os.unlink(CLONE_NAME, dir_fd=self.work)
            except FileNotFoundError:
                pass
            os.rmdir(WORK_NAME, dir_fd=self.lane)
            return "verified"
        except Exception:  # noqa: BLE001 - an unproven release is recorded and fenced by the tier
            return "failed"


def retained_digest(private: int, name: str, device: int, size: int) -> str:
    before = os.stat(name, dir_fd=private, follow_symlinks=False)
    if (not stat.S_ISREG(before.st_mode) or before.st_uid != os.geteuid() or before.st_dev != device
            or before.st_nlink != 1 or stat.S_IMODE(before.st_mode) != 0o600 or before.st_size != size):
        raise HarvestError("private guest setup file is missing or unsafe")
    fd = os.open(name, READ_FLAGS, dir_fd=private)
    try:
        if (os.fstat(fd).st_ino, os.fstat(fd).st_dev) != (before.st_ino, before.st_dev):
            raise HarvestError("private guest setup file changed while verifying")
        checksum, position = hashlib.sha256(), 0
        while position < size:
            block = os.pread(fd, min(CHUNK, size - position), position)
            if not block:
                raise HarvestError("private guest setup file had a short read")
            checksum.update(block)
            position += len(block)
        return checksum.hexdigest()
    finally:
        os.close(fd)


def verify(private: int, device: int, summary: object, ordinal: int) -> int:
    """Independently re-check a harvest summary and its retained bytes; return their total."""
    if (not isinstance(summary, dict) or set(summary) != SUMMARY_KEYS or summary["schema_version"] != SCHEMA
            or summary["outcome"] not in OUTCOMES or not isinstance(summary["reason"], str)
            or not REASON.fullmatch(summary["reason"]) or summary["cleanup"] not in ("not-needed", "verified", "failed")
            or summary["deadline_seconds"] != SECONDS or type(summary["fve"]) is not bool
            or type(summary["mounted"]) is not bool or type(summary["source_unchanged"]) not in (type(None), bool)
            or not natural(summary["budget_bytes"]) or not natural(summary["total_bytes"])
            or summary["disk_bytes"] is not None and not natural(summary["disk_bytes"])
            or summary["filesystem"] is not None and not (isinstance(summary["filesystem"], str) and (summary["filesystem"] == "unknown" or FILESYSTEM.fullmatch(summary["filesystem"])))
            or not isinstance(summary["partitions"], list) or len(summary["partitions"]) > MAX_PARTITIONS
            or not isinstance(summary["items"], list) or len(summary["items"]) != len(ALLOWLIST)):
        raise HarvestError("private guest setup summary schema differs")
    for partition in summary["partitions"]:
        if (not isinstance(partition, dict) or set(partition) != PARTITION_KEYS
                or type(partition["index"]) is not int or not 1 <= partition["index"] <= MAX_ENTRIES
                or partition["type"] not in (*TYPES.values(), "other") or partition["signature"] not in SIGNATURES
                or not natural(partition["first_lba"]) or not natural(partition["last_lba"])
                or partition["first_lba"] > partition["last_lba"] or not natural(partition["bytes"])
                or partition["bytes"] != (partition["last_lba"] - partition["first_lba"] + 1) * SECTOR):
            raise HarvestError("private guest setup partition differs")
    selected = summary["selected_partition"]
    if (summary["fve"] != any(item["signature"] == "bitlocker" for item in summary["partitions"])
            or selected is not None and (type(selected) is not int or not any(
                item["index"] == selected and item["type"] == "basic-data" and item["signature"] == "ntfs"
                for item in summary["partitions"]))
            or summary["mounted"] and (selected is None or summary["filesystem"] is None)):
        raise HarvestError("private guest setup volume selection differs")
    total = 0
    for (identifier, path), item in zip(ALLOWLIST, summary["items"]):
        if (not isinstance(item, dict) or set(item) != ITEM_KEYS or item["id"] != identifier
                or item["guest_path"] != path or item["status"] not in ITEM_STATUSES
                or not isinstance(item["reason"], str) or not REASON.fullmatch(item["reason"])
                or item["original_size"] is not None and not natural(item["original_size"])):
            raise HarvestError("private guest setup item schema differs")
        if item["status"] == "retained":
            name = file_name(ordinal, identifier)
            if (not summary["mounted"] or item["file"] != name or item["reason"] != "none"
                    or not natural(item["bytes"]) or item["bytes"] > ITEM_CAP or item["original_size"] != item["bytes"]
                    or not isinstance(item["sha256"], str) or not HEX.fullmatch(item["sha256"])):
                raise HarvestError("private guest setup retained item differs")
            if retained_digest(private, name, device, item["bytes"]) != item["sha256"]:
                raise HarvestError("private guest setup SHA-256 differs")
            total += item["bytes"]
        elif item["file"] is not None or item["bytes"] != 0 or item["sha256"] is not None:
            raise HarvestError("unretained guest setup item claims bytes")
        elif item["status"] == "oversize" and (item["original_size"] is None or item["original_size"] <= ITEM_CAP):
            raise HarvestError("oversize guest setup item is within cap")
    if total != summary["total_bytes"] or total > summary["budget_bytes"]:
        raise HarvestError("private guest setup total differs")
    return total
