#!/usr/bin/env python3
"""Packaging publication safety; stub signatures/images, real native archive operations."""
import os
import tarfile
import unittest

from package_publication_fixture import Fixture, snapshot


class PackagePublicationTests(unittest.TestCase):
    def fixture(self, kind):
        f = Fixture(kind)
        self.addCleanup(f.close)
        return f

    def assert_clean(self, f):
        self.assertEqual(list(f.tmp.iterdir()), [])
        self.assertEqual(list(f.output.parent.glob(".bridgevm-package-*")), [])

    def test_existing_entries_are_refused_without_touching_input_or_output(self):
        for kind in ("dmg", "tar"):
            for entry in ("file", "directory", "symlink", "dangling"):
                with self.subTest(kind=kind, entry=entry):
                    f = self.fixture(kind)
                    if entry == "file": f.output.write_bytes(b"previous artifact")
                    elif entry == "directory":
                        f.output.mkdir()
                        (f.output / "sentinel").write_bytes(b"previous artifact")
                    else: f.output.symlink_to(f.target if entry == "symlink" else "missing")
                    before = (snapshot(f.app), snapshot(f.output), snapshot(f.target))
                    result = f.run()
                    self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertEqual((snapshot(f.app), snapshot(f.output), snapshot(f.target)), before)
                    self.assertEqual(f.commands(), [])
                    self.assert_clean(f)

    def test_output_in_input_app_is_refused_including_parent_aliases(self):
        for kind in ("dmg", "tar"):
            for alias in ("direct", "parent", "input", "existing", "escape"):
                with self.subTest(kind=kind, alias=alias):
                    f = self.fixture(kind)
                    app = f.app
                    f.output = f.app / "Contents" / "new" / "archive"
                    if alias == "existing": f.output = f.payload
                    elif alias == "parent":
                        link = f.root / "output-parent"
                        link.symlink_to(f.app / "Contents", target_is_directory=True)
                        f.output = link / "new" / "archive"
                    elif alias == "input":
                        app = f.root / "input-alias.app"
                        app.symlink_to(f.app, target_is_directory=True)
                    elif alias == "escape":
                        link = f.app / "Contents" / "outside"
                        link.symlink_to(f.root, target_is_directory=True)
                        f.output = link / "archive"
                    before = snapshot(f.app)
                    old_output = snapshot(f.output)
                    result = f.run(app=app)
                    self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertEqual(snapshot(f.app), before)
                    self.assertEqual(snapshot(f.output), old_output)
                    self.assertEqual(f.commands(), [])
                    self.assert_clean(f)

    def test_creation_and_verification_failures_leave_output_absent_and_allow_retry(self):
        for kind in ("dmg", "tar"):
            for mode in ("input-fail", "create-fail", "verify-fail", "extract-fail"):
                if kind == "dmg" and mode == "extract-fail": continue
                with self.subTest(kind=kind, mode=mode):
                    f = self.fixture(kind)
                    before = snapshot(f.app)
                    result = f.run(mode)
                    self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertFalse(os.path.lexists(f.output))
                    self.assertEqual(snapshot(f.app), before)
                    self.assert_clean(f)
                    retry = f.run()
                    self.assertEqual(retry.returncode, 0, retry.stdout + retry.stderr)
                    self.assertTrue(f.output.is_file())
                    self.assertEqual(snapshot(f.app), before)
                    self.assert_clean(f)

    def test_concurrent_output_creation_is_never_replaced(self):
        for kind in ("dmg", "tar"):
            for entry in ("file", "directory", "symlink", "dangling"):
                with self.subTest(kind=kind, entry=entry):
                    f = self.fixture(kind)
                    before = (snapshot(f.app), snapshot(f.target))
                    result = f.run("race", race=entry)
                    self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertEqual((snapshot(f.app), snapshot(f.target)), before)
                    if entry == "file": self.assertEqual(f.output.read_bytes(), b"concurrent artifact")
                    elif entry == "directory":
                        self.assertTrue(f.output.is_dir(), "packager published before the competing directory could be created")
                        self.assertEqual((f.output / "sentinel").read_bytes(), b"concurrent artifact")
                    else:
                        self.assertTrue(f.output.is_symlink())
                        self.assertEqual(os.readlink(f.output), str(f.target) if entry == "symlink" else "absent-target")
                    self.assert_clean(f)

    def test_fresh_outputs_publish_verified_artifacts_and_preserve_native_tar_metadata(self):
        for kind in ("dmg", "tar"):
            with self.subTest(kind=kind):
                f = self.fixture(kind)
                f.output = f.root / "new" / "nested" / f.output.name
                before = snapshot(f.app)
                result = f.run()
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertEqual(snapshot(f.app), before)
                if kind == "dmg":
                    self.assertEqual(f.output.read_bytes(), b"synthetic disk image")
                    self.assertEqual([c[1] for c in f.commands() if c[0] == "hdiutil"], ["create", "verify"])
                else:
                    with tarfile.open(f.output) as archive:
                        payload = archive.getmember("BridgeVM.app/Contents/payload")
                        self.assertEqual(payload.mode, 0o751)
                        self.assertEqual(archive.extractfile(payload).read(), b"original app bytes\n")
                        link = archive.getmember("BridgeVM.app/Contents/payload-link")
                        self.assertTrue(link.issym())
                        self.assertEqual(link.linkname, "payload")
                self.assert_clean(f)


if __name__ == "__main__":
    unittest.main(verbosity=2)
