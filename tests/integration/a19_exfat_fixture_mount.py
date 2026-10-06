"""New-image exact backing ownership; no force detach or guessed device IDs."""
import os
from pathlib import Path
import plistlib
import re
import stat

from a19_exfat_fixture_io import content, save

HDI = "/usr/bin/hdiutil"
DISK = "/usr/sbin/diskutil"


def identity(path):
    info = path.lstat()
    if path.is_symlink():
        raise ValueError("owned path became a symlink")
    return [info.st_dev, info.st_ino]


def entities(items, mount):
    if not isinstance(items, list) or not items or len(items) > 32 or any(
            not isinstance(item, dict) for item in items):
        raise ValueError("invalid attachment entities")
    whole = [item["dev-entry"] for item in items
             if re.fullmatch(r"/dev/disk[0-9]+", str(item.get("dev-entry", "")))]
    if len(whole) != 1:
        raise ValueError("ambiguous whole device")
    device, mounted = whole[0], []
    for item in items:
        child = item.get("dev-entry")
        if child is not None and not re.fullmatch(re.escape(device) + r"(?:s[0-9]+)*", str(child)):
            raise ValueError("foreign attachment device")
        point = item.get("mount-point")
        if point is not None:
            if point != str(mount):
                raise ValueError("foreign attachment mount")
            mounted.append(child)
    if len(mounted) > 1:
        raise ValueError("ambiguous mounted volume")
    return device, mounted


class Mount:
    def __init__(self, root, commands):
        self.root, self.commands = root, commands
        self.image, self.path = root / "fs.dmg", root / "mount"
        self.path.mkdir(mode=0o700)
        self.before_mount = identity(self.path)
        self.image_identity = None
        self.device = self.volume = None
        self.attempted = False
        self.records = {}

    def snapshot(self, *, cleanup=False):
        code, raw, _ = self.commands.run([HDI, "info", "-plist"], 10,
                                        cleanup=cleanup, private_inventory=True)
        if code != 0:
            raise ValueError("image inventory refused")
        parsed = plistlib.loads(raw)
        images = parsed.get("images") if isinstance(parsed, dict) else None
        if not isinstance(images, list):
            raise ValueError("invalid image inventory")
        matches, devices = [], set()
        for item in images:
            if not isinstance(item, dict) or not isinstance(item.get("image-path"), str):
                raise ValueError("unidentified inventory entry")
            path, parts = Path(item["image-path"]), item.get("system-entities")
            if not path.is_absolute() or not isinstance(parts, list):
                raise ValueError("unsafe image inventory")
            devices.update(x["dev-entry"] for x in parts if isinstance(x, dict)
                           and isinstance(x.get("dev-entry"), str))
            if path.resolve() == self.image:
                device, mounted = entities(parts, self.path)
                safe = [{key: entity[key] for key in ('dev-entry', 'mount-point', 'content-hint')
                         if isinstance(entity.get(key), str)} for entity in parts]
                matches.append({"device": device, "mounted": mounted, "entities": safe})
        if len(matches) > 1:
            raise ValueError("ambiguous owned image mapping")
        return matches[0] if matches else None, devices

    def check_image(self):
        info = self.image.lstat()
        if (not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or info.st_size > 80 * 1024**2
                or identity(self.image) != self.image_identity):
            raise ValueError("owned image identity changed")

    def create_attach(self):
        code, _, _ = self.commands.run([HDI, "create", "-size", "64m", "-type", "UDIF",
            "-layout", "NONE", "-fs", "ExFAT", "-volname", "BV-A19-FS", "-nospotlight", str(self.image)], 60)
        if code != 0:
            raise ValueError("new ExFAT image creation refused")
        self.image_identity = identity(self.image)
        self.records["image_before"] = content(self.image, 80 * 1024**2)
        previous, devices = self.snapshot()
        if previous is not None:
            raise ValueError("new image already attached")
        self.attempted = True
        code, raw, _ = self.commands.run([HDI, "attach", "-plist", "-noautoopen",
                                        "-mountpoint", str(self.path), str(self.image)], 60)
        if code != 0:
            raise ValueError("new image attach refused")
        self.device, volumes = entities(plistlib.loads(raw).get("system-entities"), self.path)
        if self.device in devices or len(volumes) != 1:
            raise ValueError("attachment not fresh or not mounted")
        observed, _ = self.snapshot()
        if observed is None or observed["device"] != self.device or observed["mounted"] != volumes:
            raise ValueError("exact backing ownership unconfirmed")
        self.volume = volumes[0]
        self.check_image()
        code, raw, _ = self.commands.run([DISK, "info", "-plist", self.volume], 30)
        details = plistlib.loads(raw) if code == 0 else {}
        if (details.get("FilesystemType", "").lower() != "exfat"
                or details.get("DeviceIdentifier") != self.volume.removeprefix("/dev/")
                or details.get("MountPoint") != str(self.path)
                or details.get("WritableMedia") is not True or details.get("WritableVolume") is not True
                or os.statvfs(self.path).f_flag & os.ST_RDONLY
                or identity(self.path)[0] == self.before_mount[0]):
            raise ValueError("writable genuine ExFAT volume unconfirmed")
        self.records["attachment"] = observed
        self.records["volume_details"] = {key: details.get(key) for key in (
            'FilesystemType', 'DeviceIdentifier', 'MountPoint', 'WritableMedia', 'WritableVolume')}
        self.records["mounted_identity"] = identity(self.path)

    def release(self):
        if not self.attempted:
            return identity(self.path) == self.before_mount
        self.check_image()
        current, devices = self.snapshot(cleanup=True)
        if current is not None:
            if self.device is not None and current["device"] != self.device:
                raise ValueError("owned whole device changed")
            self.device = current["device"]
            code, _, _ = self.commands.run([HDI, "detach", self.device], 20, cleanup=True)
            if code != 0:
                raise ValueError("owned detach refused")
        after, devices = self.snapshot(cleanup=True)
        if after is not None or (self.device is not None and self.device in devices):
            raise ValueError("owned attachment residue")
        owned_nodes = {item.get('dev-entry') for item in self.records.get('attachment', {}).get('entities', [])}
        if self.device is not None:
            owned_nodes.add(self.device)
        if any(node and os.path.lexists(node) for node in owned_nodes):
            raise ValueError("recorded owned device node still present")
        if identity(self.path) != self.before_mount:
            raise ValueError("underlying mount directory not restored")
        self.check_image()
        self.records["release"] = {"whole_device": self.device, "backing_absent": True,
                                   "absent_from_image_inventory": True, "recorded_device_nodes_absent": True,
                                   "underlying_identity_restored": True}
        self.records["image_after"] = content(self.image, 80 * 1024**2)
        save(self.root / "mount-release.private.json", self.records)
        return True
