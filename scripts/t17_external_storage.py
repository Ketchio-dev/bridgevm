"""Pinned external APFS storage boundary shared by T17 and the installer."""
import os
from pathlib import Path
import plistlib
import re
import subprocess

UUID = r"[0-9A-F]{8}(?:-[0-9A-F]{4}){3}-[0-9A-F]{12}"
ROOT = re.compile(rf"/Volumes/[^/\s]+/BridgeVM/live-t17/({UUID})")
JOB = r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}"
MIN_BYTES = 100 * 1024**3


def storage_row(manifest):
    path = Path(manifest)
    if path.is_symlink() or not path.is_file() or not 0 < path.stat().st_size <= 32768:
        raise ValueError("storage manifest is not a bounded regular file")
    rows = [line.split("\t") for line in path.read_text().splitlines()
            if line.split("\t")[0] == "storage_root"]
    if not rows:
        return None
    if len(rows) != 1 or len(rows[0]) != 3:
        raise ValueError("duplicate or malformed storage_root")
    _, root, uuid = rows[0]
    match = ROOT.fullmatch(root)
    if not match or uuid != match[1]:
        raise ValueError("storage root must contain its exact volume UUID")
    return {"root": root, "volume_uuid": uuid}


def volume_info(mount):
    try:
        result = subprocess.run(["/usr/sbin/diskutil", "info", "-plist", str(mount)],
                                check=True, capture_output=True, timeout=15)
        return plistlib.loads(result.stdout)
    except (subprocess.SubprocessError, plistlib.InvalidFileException) as error:
        raise ValueError("external volume information unavailable") from error


def require_volume(info, mount, uuid):
    if (info.get("MountPoint") != str(mount) or info.get("VolumeUUID") != uuid
            or info.get("FilesystemType") != "apfs" or info.get("Internal") is not False
            or info.get("WritableVolume") is not True):
        raise ValueError("external APFS mount identity or writability changed")


def check_storage(storage, *, minimum=0):
    root = Path(storage["root"])
    match = ROOT.fullmatch(str(root))
    if not match or match[1] != storage["volume_uuid"]:
        raise ValueError("invalid external root")
    mount = Path(*root.parts[:3])
    for part in [root, *root.parents]:
        if not part.is_dir() or part.is_symlink():
            raise ValueError("missing or symlinked storage ancestor")
    require_volume(volume_info(mount), mount, storage["volume_uuid"])
    if root.resolve() != root or root.stat().st_dev != mount.stat().st_dev:
        raise ValueError("storage root crosses a volume boundary")
    stats = os.statvfs(root)
    available = stats.f_bavail * stats.f_frsize
    if available < minimum:
        raise ValueError("external APFS volume has less than 100 GiB available")
    return available


def external_lane(path, job=None, lane=None):
    raw = str(path)
    pattern = rf"({ROOT.pattern})/bridgevm-e2e-({JOB})\.[A-Za-z0-9]{{6}}/lane-([1-3])"
    match = re.fullmatch(pattern, raw)
    if not match or (job is not None and match[3] != job) or (lane is not None and int(match[4]) != lane):
        raise ValueError("external lane is outside its fixed job boundary")
    storage = {"root": match[1], "volume_uuid": match[2]}
    check_storage(storage)
    root = Path(path)
    if root.is_symlink() or root.parent.is_symlink() or root.resolve() != root:
        raise ValueError("external lane traverses a symlink")
    if root.stat().st_dev != Path(storage["root"]).stat().st_dev:
        raise ValueError("external lane crosses a volume boundary")
    return storage


def installer_path(path):
    path = Path(path)
    match = re.fullmatch(rf"bridgevm-appinstall-({JOB})-(target\.raw|vars\.fd)", path.name)
    if not match:
        raise ValueError("not an external installer scratch file")
    external_lane(path.parent)
    if path.is_symlink() or (path.exists() and not path.is_file()):
        raise ValueError("installer scratch file is aliased or not regular")


def validate_lane(root, verified, job, lane):
    storage = verified.get("storage")
    if storage:
        if external_lane(root, job, lane) != storage:
            raise ValueError("lane does not belong to sealed storage")
    elif not str(root).startswith("/tmp/bridgevm-e2e-"):
        raise ValueError("lane is outside the internal temporary boundary")


if __name__ == "__main__":
    import sys
    try:
        installer_path(sys.argv[1])
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        sys.exit(f"external installer storage refused: {error}")
