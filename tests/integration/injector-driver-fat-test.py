#!/usr/bin/env python3
"""Native FAT metadata regression using only a newly created temporary image."""
import os
from pathlib import Path
import platform
import plistlib
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipUnless(platform.system() == "Darwin", "native FAT and copyfile required")
class DriverFatTests(unittest.TestCase):
    def test_staged_fat_metadata_removed_without_changing_driver_data(self):
        builder = (ROOT / "scripts/build-hvf-windows-driver-injector.sh").read_text()
        statements = [line.strip() for line in builder.splitlines() if line.strip().startswith("/bin/cp -X ")]
        self.assertEqual(len(statements), 1)
        self.assertIn('/usr/sbin/dot_clean -m "$DST_VOL/drivers/$name"', statements[0])
        with tempfile.TemporaryDirectory(prefix="BridgeVM FAT metadata ") as temporary:
            root = Path(temporary)
            image = root / "metadata.dmg"
            subprocess.run(["hdiutil", "create", "-size", "32m", "-fs", "MS-DOS",
                            "-volname", "BVMDMETA", str(image)],
                           check=True, stdout=subprocess.DEVNULL)
            mount = root / "volume"
            mount.mkdir()
            attached = subprocess.run(["hdiutil", "attach", "-nobrowse", "-plist",
                                       "-mountpoint", str(mount), str(image)], check=True, capture_output=True)
            entities = plistlib.loads(attached.stdout)["system-entities"]
            devices = [item["dev-entry"] for item in entities
                       if re.fullmatch(r"/dev/disk[0-9]+", item.get("dev-entry", ""))]
            self.assertEqual(len(devices), 1, "unexpected attachment; do not detach unrelated disks")
            try:
                self.exercise_copy(root, mount, statements[0])
            finally:
                subprocess.run(["hdiutil", "detach", devices[0]], check=True, stdout=subprocess.DEVNULL)

    def exercise_copy(self, root, mount, statement):
        source = root / "source"
        source.mkdir()
        target = mount / "drivers/netkvm"
        target.mkdir(parents=True)
        payloads = {"netkvm.inf": b"[Version]\r\n", "netkvm.sys": b"driver\x00", "netkvm.cat": b"catalog\xff"}
        attribute = "com.bridgevm.fat-regression"
        for name, content in payloads.items():
            path = source / name
            path.write_bytes(content)
            subprocess.run(["xattr", "-w", attribute, "source-metadata", str(path)], check=True)
        environment = dict(os.environ)
        environment.pop("COPYFILE_DISABLE", None)
        subprocess.run(["/bin/cp", *(str(source / name) for name in payloads), str(target)],
                       env=environment, check=True)
        self.assertGreater(len(list(target.glob("._*"))), 0, "control did not reproduce FAT sidecars")
        environment.update(src=str(source), DST_VOL=str(mount), name="netkvm")
        subprocess.run(["bash", "-e", "-c", statement], env=environment, check=True)
        self.assertEqual(list(target.glob("._*")), [])
        self.assertEqual({path.name for path in target.iterdir()}, set(payloads))
        for name, content in payloads.items():
            self.assertEqual((target / name).read_bytes(), content)
            self.assertEqual(subprocess.check_output(["xattr", "-p", attribute, str(source / name)]).strip(), b"source-metadata")


if __name__ == "__main__":
    unittest.main()
