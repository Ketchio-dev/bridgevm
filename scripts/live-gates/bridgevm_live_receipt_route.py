"""Stdlib-only D10 admission routing; unrelated historical receipts retain their reader."""
import json
import os
from pathlib import Path
import re
import stat
import sys

D10, D11 = "d10-t22-owned-pair-preparation", "d11-native-fixture-preparation"


def identity(info):
    return (info.st_dev, info.st_ino, info.st_mode, info.st_nlink,
            info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def snapshot(path, limit, records):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        before = os.fstat(fd)
        if not stat.S_ISREG(before.st_mode) or not 0 < before.st_size <= limit:
            raise ValueError("receipt routing input is not bounded regular data")
        data = b""
        while len(data) <= limit:
            block = os.read(fd, min(8192, limit + 1 - len(data)))
            if not block: break
            data += block
        after = os.fstat(fd)
    finally: os.close(fd)
    current = os.stat(path, follow_symlinks=False)
    if len(data) != before.st_size or identity(before) != identity(after) or identity(after) != identity(current):
        raise ValueError("receipt routing input changed")
    records.append((path, identity(current)))
    return data


def environment(path, records):
    rows = {}
    for line in snapshot(path, 4096, records).decode("utf-8").splitlines():
        key, separator, value = line.partition("=")
        if not separator or key in rows: raise ValueError("receipt routing seal has malformed or repeated fields")
        rows[key] = value
    return rows


def route(directory, job_id):
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}", job_id) or directory.name != job_id:
        raise ValueError("receipt job id is not canonical")
    records = []
    job = environment(directory / "job.env", records)
    tiers = [job.get("tier")]
    ledger_root = directory.parent.parent / "job-ledger"
    ledger = ledger_root / job_id
    if ledger_root.is_symlink() or ledger.is_symlink(): raise ValueError("receipt ledger path is unsafe")
    if os.path.lexists(ledger / "entry.env"): tiers.append(environment(ledger / "entry.env", records).get("tier"))
    def hints(pairs):
        tiers.extend(value for key, value in pairs if key == "tier")
        return dict(pairs)
    try:
        json.loads(snapshot(directory / "receipt.public.json", 65_536, records), object_pairs_hook=hints)
    except (OSError, UnicodeError, ValueError, RecursionError): pass
    for path, observed in records:
        if identity(os.stat(path, follow_symlinks=False)) != observed: raise ValueError("receipt routing input changed")
    here = Path(__file__).resolve().parent
    if D10 in tiers or D11 in tiers:
        argv = ["/bin/bash", "--noprofile", "--norc", "-p", str(here / ("d11-fixture-archive-dispatch.sh" if D11 in tiers else "t22-pair-archive-dispatch.sh")),
                str(directory), job_id, job.get("commit", "")]
    else:
        argv = [sys.executable, "-B", "-s", "-E", str(here / "bridgevm_live_receipt_legacy.py"), str(directory), job_id]
    os.execv(argv[0], argv)


if __name__ == "__main__":
    try: route(Path(sys.argv[1]), sys.argv[2])
    except (OSError, UnicodeError, ValueError, IndexError) as error:
        raise SystemExit(f"receipt refused: {error}") from None
