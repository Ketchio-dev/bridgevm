"""Read-only, hash-only inspection; an existing file is not proof of a boot."""
from pathlib import Path

from b6_cell_inputs import file_hash
from winpe_companion_inputs import COMPANIONS
from winpe_companion_mounts import mounted_image


def inspect(image, mount_root):
    with mounted_image(image, mount_root) as entities:
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


def compare(before, after, expected):
    available = before.get("available") is True and after.get("available") is True
    matches = after.get("available") is True and after.get("files") == expected
    changed = available and any(before["files"].get(name) != after["files"].get(name) for name in expected)
    return {"post_files_match": matches, "files_changed": changed,
            "pre_post_available": available, "winpe_boot_proven": False}
