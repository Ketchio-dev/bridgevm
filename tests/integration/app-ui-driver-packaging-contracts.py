#!/usr/bin/env python3
"""Actual signing and reconstruction; run only a harmless non-GUI C fixture."""
import hashlib
import importlib.util
import json
import platform
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
from app_ui_host_v2_bundle import reconstruct_bundles, verify_bundles, driver_bundle_metadata
from app_ui_host_v2_manifest import FILES, copy_input

spec = importlib.util.spec_from_file_location("packager", ROOT / "apps/macos/scripts/package-app-ui-host-inputs.py")
packager = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packager)


@unittest.skipUnless(sys.platform == "darwin", "actual codesign packaging requires macOS")
class PackagingContracts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory(prefix="bridgevm-static-signing-")
        cls.root = Path(cls.temporary.name).resolve()
        cls.source = cls.root / "fixture.c"
        cls.source.write_text("int main(void) { return 0; }\n")
        cls.host, cls.launcher = cls.root / "input-host", cls.root / "input-launcher"
        subprocess.run(["xcrun", "clang", "-target", platform.machine() + "-apple-macos14.0", str(cls.source),
                        "-o", str(cls.host)], check=True, timeout=30, capture_output=True)
        shutil.copyfile(cls.host, cls.launcher)
        cls.original = {path: path.read_bytes() for path in (cls.host, cls.launcher)}
        cls.commit = subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True).strip()
        cls.output = cls.root / "packaged"
        cls.record = packager.package(cls.host, cls.launcher, cls.commit, cls.output)
        cls.fields = {key: (Path(value["path"]), value["sha256"])
                      for key, value in cls.record["artifacts"].items()}
        cls.stage = cls.root / "sealed"
        cls.stage.mkdir(mode=0o700)
        for key, (path, sha256) in cls.fields.items():
            copy_input(key, path, cls.stage / FILES[key], sha256)

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def test_complete_fresh_bundle_signatures_and_identical_driver_bytes(self):
        self.assertEqual(set(self.fields), set(FILES))
        verify_bundles(self.output, self.fields)
        driver = self.output / "BridgeVMAppUIDriver.app/Contents/MacOS/AppUIHostLauncher"
        self.assertEqual(driver.read_bytes(), (self.output / "AppUIHostLauncher").read_bytes())
        self.assertNotEqual(driver.stat().st_ino, self.launcher.stat().st_ino)
        self.assertEqual(plistlib.loads((driver.parents[1] / "Info.plist").read_bytes()), driver_bundle_metadata())
        for key, (path, expected) in self.fields.items():
            self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), expected, key)
        for path, original in self.original.items():
            self.assertEqual(path.read_bytes(), original)
        self.assertFalse(self.record["gui_launched"])
        self.assertFalse(self.record["accessibility_trust_observed"])
        self.assertEqual(json.loads((self.output / "packaging-inputs.json").read_text()), self.record)
        for path in [self.output, *self.output.rglob("*")]:
            if path.is_dir():
                self.assertEqual(path.stat().st_mode & 0o777, 0o700, str(path))

    def test_reconstruction_preserves_both_actual_signatures_without_resigning(self):
        destination = self.root / "reconstructed"
        destination.mkdir(mode=0o700)
        reconstruct_bundles(destination, self.stage, ROOT, self.commit, self.fields)
        verify_bundles(destination, self.fields)
        for role in ("Host", "Driver"):
            app = destination / ("BridgeVMAppUI" + role + ".app")
            original = self.output / app.name
            for relative in ("Contents/Info.plist", "Contents/_CodeSignature/CodeResources"):
                self.assertEqual((app / relative).read_bytes(), (original / relative).read_bytes())

    def test_detached_signed_bytes_execute_only_harmless_c_fixture(self):
        bare = self.output / "AppUIHostLauncher"
        standalone = subprocess.run(["/usr/bin/codesign", "--verify", "--strict", str(bare)],
                                    timeout=10, capture_output=True, text=True)
        executed = subprocess.run([str(bare)], timeout=10, capture_output=True, text=True)
        print(json.dumps({"fixture": "int main(void) { return 0; }", "gui": False,
                          "standalone_full_bundle_verification": {"exit": standalone.returncode,
                              "stdout": standalone.stdout, "stderr": standalone.stderr},
                          "bare_fixture_execution": {"exit": executed.returncode,
                              "stdout": executed.stdout, "stderr": executed.stderr}}, sort_keys=True))
        self.assertEqual(executed.returncode, 0, executed.stderr)

    def test_signature_refuses_modified_metadata_and_missing_git_resource(self):
        for label in ("metadata", "resource"):
            app = self.root / (label + ".app")
            shutil.copytree(self.output / "BridgeVMAppUIHost.app", app)
            if label == "metadata":
                path = app / "Contents/Info.plist"
                path.chmod(0o600)
                data = plistlib.loads(path.read_bytes())
                data["CFBundleName"] = "Modified"
                path.write_bytes(plistlib.dumps(data))
            else:
                (app / "Contents/Resources/BridgeVMApp_BridgeVMControl.bundle" / packager.RESOURCES[0]).unlink()
            result = subprocess.run(["/usr/bin/codesign", "--verify", "--deep", "--strict", str(app)],
                                    timeout=10, capture_output=True)
            self.assertNotEqual(result.returncode, 0, label)

    def test_existing_output_symlink_inputs_and_noncanonical_paths_are_refused(self):
        with self.assertRaises(FileExistsError):
            packager.package(self.host, self.launcher, self.commit, self.output)
        alias = self.root / "alias"
        alias.symlink_to(self.host)
        with self.assertRaises(ValueError):
            packager.package(alias, self.launcher, self.commit, self.root / "refused")
        self.assertFalse((self.root / "refused").exists())
        with self.assertRaises(ValueError):
            packager.canonical(str(self.root) + "/./input-host")
        with self.assertRaises(ValueError):
            packager.package(self.host, self.launcher, "HEAD", self.root / "bad-commit")


if __name__ == "__main__":
    unittest.main()
