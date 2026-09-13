"""Native disposable-image coverage, never Windows or WinPE boot evidence."""
from pathlib import Path
from unittest import TestCase, skipUnless
import hashlib
import os
import plistlib
import stat
import subprocess
import sys
import tempfile

from winpe_companion_mounts import mounted_image


@skipUnless(sys.platform == "darwin" and Path("/usr/bin/hdiutil").is_file(),
            "native mount contract requires macOS hdiutil")
class WinPENativeMountSafetyTests(TestCase):
    def inventory(self):
        result = subprocess.run(["/usr/bin/hdiutil", "info", "-plist"],
                                capture_output=True, check=True, timeout=10)
        value = plistlib.loads(result.stdout)
        self.assertIsInstance(value, dict)
        images = value.get("images")
        self.assertIsInstance(images, list)
        for item in images:
            self.assertIsInstance(item, dict)
            self.assertIsInstance(item.get("image-path"), str)
            self.assertTrue(Path(item["image-path"]).is_absolute())
            self.assertIsInstance(item.get("system-entities"), list)
            for entity in item["system-entities"]:
                self.assertIsInstance(entity, dict)
        return images

    def assert_absent(self, image, devices):
        images = self.inventory()
        self.assertTrue(all(Path(item["image-path"]).resolve() != image for item in images))
        observed = {entity.get("dev-entry") for item in images
                    for entity in item["system-entities"]}
        self.assertTrue(devices.isdisjoint(observed), "owned device still attached")

    def test_native_raw_readonly_mount_preserves_image_and_cleans_attachment(self):
        root = Path(tempfile.mkdtemp(prefix="bridgevm-d4-native-mount-")).resolve()
        root_info = root.lstat()
        root_identity = (root_info.st_dev, root_info.st_ino)
        image, mount_root = root / "fixture.cdr", root / "mount"
        image_identity = None
        devices = set()
        safe_to_remove = False
        try:
            result = subprocess.run([
                "/usr/bin/hdiutil", "create", "-size", "32m", "-layout", "GPTSPUD",
                "-fs", "HFS+", "-type", "UDTO", "-nospotlight", "-volname",
                "BridgeVMNativeMountFixture", str(image),
            ], capture_output=True, timeout=60)
            self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
            info = image.lstat()
            self.assertTrue(stat.S_ISREG(info.st_mode))
            image_identity = (info.st_dev, info.st_ino)
            self.assertEqual(info.st_size, 32 * 1024 * 1024)
            with image.open("rb") as stream:
                header = stream.read(1024)
            self.assertEqual(header[510:512], b"\x55\xaa")
            self.assertEqual(header[512:520], b"EFI PART")
            before = hashlib.sha256(image.read_bytes()).hexdigest()
            with mounted_image(image, mount_root) as entities:
                devices = {item["dev-entry"] for item in entities if "dev-entry" in item}
                points = [Path(item["mount-point"]) for item in entities if "mount-point" in item]
                self.assertTrue(devices)
                self.assertTrue(points)
                self.assertTrue(all(point.resolve().is_relative_to(mount_root) for point in points))
                self.assertTrue(all(os.statvfs(point).f_flag & os.ST_RDONLY for point in points))
                owned = [item for item in self.inventory()
                         if Path(item["image-path"]).resolve() == image]
                self.assertEqual(len(owned), 1)
                observed = {item.get("dev-entry") for item in owned[0]["system-entities"]}
                self.assertTrue(devices.issubset(observed))
            self.assert_absent(image, devices)
            self.assertFalse(mount_root.exists())
            self.assertEqual(hashlib.sha256(image.read_bytes()).hexdigest(), before)
            safe_to_remove = True
        finally:
            if safe_to_remove:
                self.assert_absent(image, devices)
                image_info, root_info = image.lstat(), root.lstat()
                self.assertTrue(stat.S_ISREG(image_info.st_mode))
                self.assertTrue(stat.S_ISDIR(root_info.st_mode))
                self.assertEqual((image_info.st_dev, image_info.st_ino), image_identity)
                self.assertEqual((root_info.st_dev, root_info.st_ino), root_identity)
                image.unlink()
                root.rmdir()
            else:
                print("native mount fixture retained after failure: " + str(root), file=sys.stderr)
