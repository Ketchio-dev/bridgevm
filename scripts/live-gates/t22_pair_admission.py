"""Bounded observations for development-only pair preparation, never a gate."""
import hashlib
import json
import os
from pathlib import Path
import re
import stat

from native_snapshot_restore_public import _reject_constant, _unique_fields
from native_snapshot_restore_seal import read_bounded_regular
from t17_guest_setup_harvest import read_gpt
from hvf_host_media import nvme_write_back_offset
from hvf_run_log import read_run_log
from hvf_terminal_report import system_off_offset

from t22_pair_query import validate_query, METHODS


def screen_disk(path):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode):
            raise ValueError("screen requires a regular disk")
        partitions, sectors = read_gpt(fd, info.st_size)
    finally:
        os.close(fd)
    if not partitions or not any(p["type"] == "basic-data" for p in partitions):
        raise ValueError("missing installed Windows basic-data partition")
    ranges = sorted((p["first_lba"], p["last_lba"]) for p in partitions)
    if any(a[1] >= b[0] for a, b in zip(ranges, ranges[1:])):
        raise ValueError("overlapping GPT partitions")
    if sum(p["type"] == "efi-system" for p in partitions) != 1:
        raise ValueError("missing or duplicate EFI partition")
    for part in partitions:
        sector = sectors[part["index"]]
        kind = part["type"]
        if part["signature"] == "bitlocker" or len(sector) != 512:
            raise ValueError("FVE or unreadable partition refuses preparation")
        if kind in ("basic-data", "windows-recovery"):
            if part["signature"] != "ntfs" or sector[510:] != b"\x55\xaa":
                raise ValueError("unknown Windows partition refuses preparation")
        elif kind == "efi-system":
            if (sector[82:90] != b"FAT32   " and sector[54:62] != b"FAT16   ") or sector[510:] != b"\x55\xaa":
                raise ValueError("unknown EFI filesystem refuses preparation")
        elif kind != "microsoft-reserved" or part["signature"] != "blank":
            raise ValueError("unknown partition refuses preparation")
    return {"partition_count": len(partitions), "ntfs_partition_count": sum(p["signature"] == "ntfs" for p in partitions)}




def read_query(share, nonce, script_hash):
    if (type(nonce) is not str or not re.fullmatch(r"[0-9a-f]{32}", nonce)
            or type(script_hash) is not str or not re.fullmatch(r"[0-9a-f]{64}", script_hash)):
        raise ValueError("invalid query filename identity")
    raw = read_bounded_regular(share / f"result-{nonce}.json", 65_536)
    checksum = read_bounded_regular(share / f"result-{nonce}.done", 64)
    digest = hashlib.sha256(raw).hexdigest()
    if checksum != digest.encode("ascii"):
        raise ValueError("Windows encryption observation hash mismatch")
    value = json.loads(raw, object_pairs_hook=_unique_fields, parse_constant=_reject_constant)
    return validate_query(value, nonce, script_hash), digest


def shutdown_observed(status, run_log):
    raw = read_run_log(Path(run_log), 64 << 20)
    if (type(status) is not int or status != 0 or system_off_offset(raw) is None
            or nvme_write_back_offset(raw) is None):
        raise ValueError("natural shutdown and host write-back were not observed")
    return hashlib.sha256(raw).hexdigest()
