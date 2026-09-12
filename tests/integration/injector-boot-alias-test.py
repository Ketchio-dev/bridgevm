#!/usr/bin/env python3
"""Staging-byte contracts, not firmware execution or signature verification."""
from pathlib import Path
import platform
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "scripts/stage-injector-boot-alias.sh"


@unittest.skipUnless(platform.system() == "Darwin", "native packaging tools required")
class BootAliasTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="BridgeVM EFI alias ")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / "efi/boot").mkdir(parents=True)
        (self.root / "efi/microsoft/boot").mkdir(parents=True)
        self.source = self.root / "efi/boot/bootaa64.efi"
        self.destination = self.root / "efi/microsoft/boot/bootmgfw.efi"
        self.source.write_bytes(b"synthetic opaque loader bytes\x00\xff")

    def invoke(self, expected=0):
        result = subprocess.run(["bash", str(HELPER), str(self.root)], capture_output=True, text=True)
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)

    def test_missing_alias_is_exact_copy_and_repeat_preserves_it(self):
        self.invoke()
        self.assertEqual(self.destination.read_bytes(), self.source.read_bytes())
        before = self.destination.stat().st_mtime_ns
        self.invoke()
        self.assertEqual(self.destination.stat().st_mtime_ns, before)

    def test_existing_distinct_iso_loader_is_not_overwritten(self):
        self.destination.write_bytes(b"existing ISO boot manager")
        self.invoke()
        self.assertEqual(self.destination.read_bytes(), b"existing ISO boot manager")

    def test_missing_or_empty_media_loader_refused(self):
        self.source.unlink()
        self.invoke(1)
        self.assertFalse(self.destination.exists())
        self.source.touch()
        self.invoke(1)
        self.assertFalse(self.destination.exists())

    def test_symlink_source_and_destination_refused(self):
        outside = self.root / "outside"
        outside.write_bytes(b"untouched")
        self.source.unlink()
        self.source.symlink_to(outside)
        self.invoke(1)
        self.source.unlink()
        self.source.write_bytes(b"loader")
        self.destination.symlink_to(outside)
        self.invoke(1)
        self.assertEqual(outside.read_bytes(), b"untouched")

    def test_directory_alias_refused_and_builder_wiring_order(self):
        directory = self.root / "efi/microsoft/boot"
        directory.rmdir()
        directory.symlink_to(self.root / "efi/boot", target_is_directory=True)
        self.invoke(1)
        builder = (ROOT / "scripts/build-hvf-windows-driver-injector.sh").read_text()
        marker = 'bash "$(dirname "${BASH_SOURCE[0]}")/stage-injector-boot-alias.sh" "$DST_VOL"'
        self.assertEqual(builder.count(marker), 1)
        self.assertGreater(builder.index(marker), builder.index('rsync -a "$ISO_MNT/efi"'))
        self.assertLess(builder.index(marker), builder.index('log "staging driver'))


if __name__ == "__main__":
    unittest.main()
