"""Read-only, hash-only inspection; an existing file is not proof of a boot."""
import plistlib
import re
import subprocess
from pathlib import Path

from b6_cell_inputs import file_hash
from winpe_companion_inputs import COMPANIONS


def inspect(image, mount_root):
    mount_root.mkdir(mode=0o700, exist_ok=False)
    result = subprocess.run(["hdiutil", "attach", "-imagekey", "diskimage-class=CRawDiskImage",
                             "-readonly", "-nobrowse", "-plist", "-mountroot", str(mount_root),
                             str(image)], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        raise ValueError("read-only attach failed; inspect host mounts before retry")
    entities = plistlib.loads(result.stdout).get("system-entities", [])
    devices = [item["dev-entry"] for item in entities
               if re.fullmatch(r"/dev/disk[0-9]+", item.get("dev-entry", ""))]
    if len(devices) != 1:
        raise ValueError("ambiguous attached device; inspect host mounts before retry")
    try:
        volumes = [Path(item["mount-point"]) for item in entities if "mount-point" in item]
        volumes = [path for path in volumes if (path / "Windows/System32").is_dir()]
        if len(volumes) != 1:
            return {"available": False, "files": {}}
        volume = volumes[0]
        hashes = {}
        for name in COMPANIONS:
            path = volume / name
            if not path.exists() and not path.is_symlink():
                hashes[name] = "missing"
            elif path.is_symlink() or not path.is_file() or path.stat().st_size > 1024 * 1024:
                hashes[name] = "unreadable"
            else:
                hashes[name] = file_hash(path)
        return {"available": True, "files": hashes}
    finally:
        subprocess.run(["hdiutil", "detach", devices[0]], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)


def compare(before, after, expected):
    available = before.get("available") is True and after.get("available") is True
    matches = after.get("available") is True and after.get("files") == expected
    changed = available and any(before["files"].get(name) != after["files"].get(name) for name in expected)
    return {"post_files_match": matches, "files_changed": changed,
            "pre_post_available": available, "winpe_boot_proven": False}
