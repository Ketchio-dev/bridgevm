"""Own an APFS capacity limit; reconcile nested mounts even after partial attach."""
import os
from pathlib import Path
import plistlib
import re
import shutil
import subprocess

from d11_fixture_files import identity, read
from d11_fixture_process import capacity
from t22_pair_environment import controlled_env


def inventory():
    raw = subprocess.check_output(["/usr/bin/hdiutil", "info", "-plist"], timeout=30,
                                  env=controlled_env())
    if len(raw) > 4 << 20: raise ValueError("mount inventory exceeds bound")
    value = plistlib.loads(raw)
    if type(value.get("images")) is not list: raise ValueError("mount inventory unavailable")
    return value["images"]


def device(image):
    candidates = [row.get("dev-entry", "") for row in image.get("system-entities", [])
                  if row.get("content-hint") == "GUID_partition_scheme"]
    if not candidates:
        candidates = [row.get("dev-entry", "") for row in image.get("system-entities", [])
                      if re.fullmatch(r"/dev/disk[0-9]+", row.get("dev-entry", ""))]
    if len(candidates) != 1 or not re.fullmatch(r"/dev/disk[0-9]+", candidates[0]):
        raise ValueError("owned whole-disk identity unproved")
    return candidates[0]


class Container:
    def __init__(self, output, iso, processes, gib):
        self.output, self.iso, self.processes, self.gib = output, iso, processes, gib
        self.backing, self.mount = output / "fixture.sparseimage", output / "mount"
        self.expected = {str(self.backing), str(self.mount / "source.raw"), str(self.mount / "target.raw")}
        self.backing_identity = None
        self.mounted_identity = None
        self.attempted = False

    def belongs(self, image):
        name = image.get("image-path")
        if name in self.expected: return True
        return name == str(self.iso) and any(
            isinstance(row.get("mount-point"), str)
            and Path(row["mount-point"]).is_relative_to(self.mount)
            for row in image.get("system-entities", []))

    def create(self):
        capacity(shutil.disk_usage(self.output).free, self.gib)
        if os.path.lexists(self.backing) or os.path.lexists(self.mount):
            raise ValueError("fixture output already exists")
        if any(self.belongs(row) for row in inventory()): raise ValueError("fixture mount already exists")
        self.mount.mkdir(mode=0o700)
        self.attempted = True
        self.processes.run(["/usr/bin/hdiutil", "create", "-size", f"{self.gib}g", "-type", "SPARSE",
            "-fs", "APFS", "-volname", "BridgeVM-Development-Fixture", str(self.backing)],
            self.output / "container-create.private.log", 120)
        info = self.backing.lstat()
        if not self.backing.is_file() or self.backing.is_symlink() or info.st_nlink != 1:
            raise ValueError("unsafe container backing")
        self.backing_identity = identity(info)[:2]
        self.processes.run(["/usr/bin/hdiutil", "attach", "-plist", "-nobrowse", "-mountpoint",
            str(self.mount), str(self.backing)], self.output / "container-attach.private.plist", 60)
        mounted = [r for r in inventory() if r.get("image-path") == str(self.backing)]
        if len(mounted) != 1: raise ValueError("container mount identity missing")
        device(mounted[0])
        entries = [r for r in mounted[0].get("system-entities", [])
                   if r.get("mount-point") == str(self.mount) and r.get("volume-kind") == "apfs"]
        info = self.mount.stat()
        if len(entries) != 1 or info.st_dev == self.output.stat().st_dev:
            raise ValueError("container is not its owned APFS filesystem")
        sizes = os.statvfs(self.mount)
        if not 0 < sizes.f_blocks * sizes.f_frsize <= self.gib << 30:
            raise ValueError("container capacity differs")
        self.mounted_identity = identity(info)[:2]

    def verify(self):
        if (identity(self.backing.lstat())[:2] != self.backing_identity
                or identity(self.mount.stat())[:2] != self.mounted_identity):
            raise ValueError("container identity changed")

    def cleanup(self):
        # Nested raw/ISO attachments must disappear before the APFS container.
        rows = [row for row in inventory() if self.belongs(row)]
        names = [row.get("image-path") for row in rows]
        if len(rows) > 4 or len(set(names)) != len(names):
            raise ValueError("ambiguous or excessive owned mount inventory")
        if self.backing_identity is not None and identity(self.backing.lstat())[:2] != self.backing_identity:
            raise ValueError("container backing replaced before detach")
        if self.mounted_identity is not None and os.path.ismount(self.mount):
            if identity(self.mount.stat())[:2] != self.mounted_identity:
                raise ValueError("container mount replaced before detach")
        rows.sort(key=lambda row: row.get("image-path") == str(self.backing))
        for index, row in enumerate(rows):
            argv = ["/usr/bin/hdiutil", "detach", device(row)]
            # Cleanup ignores cancellation/reserve checks; never uses force.
            from guest_input_owned_group import stop
            process = subprocess.Popen(argv, env=controlled_env(), stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
            try:
                code = process.wait(timeout=30)
            finally:
                if not stop(process): raise ValueError("detach process cleanup unproved")
            if code: raise ValueError("normal fixture detach failed")
        if any(self.belongs(row) for row in inventory()): raise ValueError("fixture mount survived detach")
        if self.mount.exists() and os.path.ismount(self.mount): raise ValueError("fixture mount still active")
        return True
