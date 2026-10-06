"""Synthetic attach/info metadata exercises production Container.create without mounting."""
from contextlib import ExitStack
import copy
from pathlib import Path
import plistlib
import shutil
import tempfile
from types import SimpleNamespace
from unittest.mock import Mock, patch

import d11_fixture_test_support  # install the existing production import paths
from d11_fixture_mounts import Container


class MountFixture:
    def __init__(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.output = Path(self.temporary.name).resolve()
        self.processes = Mock()
        self.box = Container(self.output, self.output / "unused.iso", self.processes, 22)
        self.attach = {"system-entities": [
            {"dev-entry": "/dev/disk80", "content-hint": "GUID_partition_scheme"},
            {"dev-entry": "/dev/disk81", "content-hint": "EF57347C-0000-11AA-AA11-00306543ECAC"},
            {"dev-entry": "/dev/disk81s1", "volume-kind": "apfs",
             "content-hint": "41504653-0000-11AA-AA11-00306543ECAC",
             "mount-point": str(self.box.mount)}]}
        self.info = {"image-path": str(self.box.backing), "system-entities":
                     copy.deepcopy(self.attach["system-entities"])}
        # hdiutil info omits the type that attach exposes for the same APFS leaf.
        del self.info["system-entities"][-1]["volume-kind"]
        self.attach_data = None
        self.alias_plist = False
        self.capacity = 22 << 30
        self.distinct_device = True

    def close(self):
        self.temporary.cleanup()

    def invoke(self):
        def run(argv, log, timeout):
            if argv[1] == "create":
                self.box.backing.write_bytes(b"synthetic sparse backing")
            else:
                data = self.attach_data if self.attach_data is not None else plistlib.dumps(self.attach)
                if self.alias_plist:
                    target = self.output / "other.plist"
                    target.write_bytes(data)
                    log.symlink_to(target)
                else:
                    log.write_bytes(data)
        self.processes.run.side_effect = run
        original = Path.stat
        def observed_stat(path, *args, **kwargs):
            info = original(path, *args, **kwargs)
            if path == self.box.mount and self.distinct_device:
                values = dict((name, getattr(info, name)) for name in
                              ("st_dev", "st_ino", "st_mode", "st_uid", "st_nlink", "st_size",
                               "st_mtime_ns", "st_ctime_ns"))
                values["st_dev"] += 1
                return SimpleNamespace(**values)
            return info
        with ExitStack() as stack:
            stack.enter_context(patch("d11_fixture_mounts.inventory", side_effect=[[], [self.info]]))
            stack.enter_context(patch("d11_fixture_mounts.shutil.disk_usage",
                                      return_value=shutil._ntuple_diskusage(256 << 30, 128 << 30, 128 << 30)))
            stack.enter_context(patch("d11_fixture_mounts.os.statvfs",
                                      return_value=SimpleNamespace(f_blocks=self.capacity // 4096, f_frsize=4096)))
            stack.enter_context(patch.object(Path, "stat", observed_stat))
            self.box.create()

    def allow_old_info_type(self):
        self.info["system-entities"][-1]["volume-kind"] = "apfs"

    def attach_leaf(self):
        return self.attach["system-entities"][-1]

    def info_leaf(self):
        return self.info["system-entities"][-1]
