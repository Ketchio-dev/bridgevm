#!/usr/bin/env python3
"""Small live APFS storage probe, explicitly not Windows acceptance evidence."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from t17_external_storage import MIN_BYTES, check_storage


def probe(root):
    root = Path(root)
    identity = {"root": str(root), "volume_uuid": root.name}
    available = check_storage(identity, minimum=MIN_BYTES)
    with tempfile.TemporaryDirectory(prefix="storage-check-", dir=root) as temporary:
        work = Path(temporary)
        disk, variables = work / "source.raw", work / "source.fd"
        disk.write_bytes(os.urandom(1024 * 1024))
        variables.write_bytes(os.urandom(65536))
        originals = [hashlib.sha256(path.read_bytes()).hexdigest() for path in (disk, variables)]
        inodes = {disk.stat().st_ino, variables.stat().st_ino}
        for index in (1, 2):
            lane = work / f"lane-{index}"
            lane.mkdir()
            for source in (disk, variables):
                target = lane / source.name
                subprocess.run(["/bin/cp", "-c", str(source), str(target)], check=True)
                assert target.stat().st_dev == root.stat().st_dev
                assert target.stat().st_ino not in inodes
                inodes.add(target.stat().st_ino)
                assert target.read_bytes() == source.read_bytes()
                with target.open("r+b") as output:
                    output.write(b"lane mutation")
            assert originals == [hashlib.sha256(path.read_bytes()).hexdigest() for path in (disk, variables)]
        check_storage(identity)
    return {"gate": "t17-external-apfs-storage", "pass": True, "volume_uuid": root.name,
            "independent_disk_vars_pairs": 2, "source_unchanged": True,
            "available_gib": available // 1024**3, "windows_install_proven": False}


if __name__ == "__main__":
    print(json.dumps(probe(sys.argv[1]), sort_keys=True))
