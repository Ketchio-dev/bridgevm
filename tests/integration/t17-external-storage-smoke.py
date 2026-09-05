#!/usr/bin/env python3
"""Deterministic storage guards; fixtures never constitute Windows evidence."""
import importlib.util
import json
from pathlib import Path
import re
import sys
import tempfile
import unittest
from unittest.mock import patch

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts"))
import t17_external_storage as storage


def load(name, relative):
    spec = importlib.util.spec_from_file_location(name, REPO / relative)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


cli = load("storage_cli", "scripts/live-gates/t17-storage.py")
manifest = load("manifest", "scripts/live-gates/windows-product-e2e-manifest.py")
UUID = "0A306B2D-D4D3-4B9C-8A9E-007657927166"
ROOT = f"/Volumes/PortableSSD/BridgeVM/live-t17/{UUID}"


class StorageTests(unittest.TestCase):
    def test_volume_identity_is_fail_closed(self):
        mount = Path("/Volumes/PortableSSD")
        info = dict(MountPoint=str(mount), VolumeUUID=UUID, FilesystemType="apfs", Internal=False, WritableVolume=True)
        storage.require_volume(info, mount, UUID)
        for key, value in [("MountPoint", "/"), ("VolumeUUID", "OTHER"), ("FilesystemType", "exfat"), ("Internal", True), ("WritableVolume", False)]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                storage.require_volume({**info, key: value}, mount, UUID)
        for key in info:
            with self.subTest(missing=key), self.assertRaises(ValueError):
                storage.require_volume({k: v for k, v in info.items() if k != key}, mount, UUID)

    def test_sealed_metadata_rejects_aliases_duplicates_and_uuid_changes(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "inputs.tsv"
            row = f"storage_root\t{ROOT}\t{UUID}\n"
            path.write_text("campaign_mode\tpilot\n" + row)
            self.assertEqual(storage.storage_row(path), {"root": ROOT, "volume_uuid": UUID})
            for invalid in [row + row, row.replace(UUID, "bad", 1), row.replace("/BridgeVM/", "/../BridgeVM/"), "storage_root\n"]:
                path.write_text(invalid)
                with self.assertRaises(ValueError): storage.storage_row(path)
            alias = Path(temporary) / "alias"
            alias.symlink_to(path)
            with self.assertRaises(ValueError): storage.storage_row(alias)

    def test_no_internal_fallback_for_missing_external_root(self):
        with patch.object(storage, "volume_info", side_effect=AssertionError("must reject missing directory first")):
            with self.assertRaises(ValueError):
                storage.check_storage({"root": ROOT.replace("PortableSSD", "absent-t17-volume"), "volume_uuid": UUID})

    def test_allocation_space_cleanup_and_symlink_refusal(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve() / UUID
            root.mkdir()
            identity = {"root": str(root), "volume_uuid": UUID}
            mount = Path(*root.parts[:3])
            info = dict(MountPoint=str(mount), VolumeUUID=UUID, FilesystemType="apfs", Internal=False, WritableVolume=True)
            regex = re.compile(re.escape(str(root.parent)) + rf"/({storage.UUID})")
            with patch.object(storage, "ROOT", regex), patch.object(storage, "volume_info", return_value=info):
                available = storage.check_storage(identity)
                with self.assertRaises(ValueError): storage.check_storage(identity, minimum=available + 1024**3)
                root_alias = root.parent / "alias"
                root_alias.symlink_to(root)
                with self.assertRaises(ValueError): storage.check_storage({"root": str(root_alias), "volume_uuid": UUID})
                verified = root.parent / "verified.json"
                verified.write_text(json.dumps({"storage": identity}))
                work = root / "bridgevm-e2e-fixture.abcdef"
                work.mkdir()
                Path(str(verified) + ".work.json").write_text(json.dumps({"work": str(work), "device": work.stat().st_dev, "inode": work.stat().st_ino}))
                sentinel = root.parent / "preserved"
                sentinel.mkdir()
                (work / "alias").symlink_to(sentinel)
                with patch.object(sys, "argv", ["storage", "cleanup", str(verified), "fixture", str(work)]):
                    with self.assertRaises(ValueError): cli.main()
                    self.assertTrue(sentinel.is_dir())
                    (work / "alias").unlink()
                    cli.main()
                    self.assertFalse(work.exists())
                with patch.object(storage, "volume_info", return_value={**info, "VolumeUUID": "changed"}):
                    with self.assertRaises(ValueError): storage.check_storage(identity)

    def test_installer_cannot_use_arbitrary_external_files(self):
        for path in ["/Volumes/PortableSSD/canonical.raw", ROOT + "/bridgevm-appinstall-any-target.raw", ROOT + "/../bridgevm-appinstall-x-vars.fd"]:
            with self.assertRaises(ValueError): storage.installer_path(path)

    def test_worker_checks_manifest_identity_before_external_space(self):
        source = (REPO / "scripts/live-gates/bridgevm-live-worker.sh").read_text()
        self.assertLess(source.index('actual_manifest="$(shasum'), source.index('t17-storage.py" space'))
        self.assertIn('MIN_FREE_GIB:-100', source)


if __name__ == "__main__":
    unittest.main()
