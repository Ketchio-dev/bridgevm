"""T20 allocation, replacement and failed-seal regressions with disposable inputs."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from unittest.mock import patch

from native_snapshot_restore_cleanup_fixture import CleanupFixture, SCRIPT


def preexisting(root, prepared, kind):
    foreign = root / "foreign"
    foreign.mkdir()
    (foreign / "sentinel").write_bytes(b"preserve")
    if kind == "directory":
        prepared.mkdir()
        (prepared / "sentinel").write_bytes(b"preserve")
    elif kind == "file":
        prepared.write_bytes(b"preserve")
    elif kind != "absent":
        prepared.symlink_to(foreign if kind == "symlink" else root / "absent")
    return foreign


class PreparedCleanupCases(CleanupFixture):
    def assert_preserved(self, prepared, foreign, kind):
        self.assertEqual(os.path.lexists(prepared), kind != "absent")
        self.assertEqual((foreign / "sentinel").read_bytes(), b"preserve")
        if kind == "directory":
            self.assertEqual((prepared / "sentinel").read_bytes(), b"preserve")
        elif kind == "file":
            self.assertEqual(prepared.read_bytes(), b"preserve")
        elif kind != "absent":
            self.assertTrue(prepared.is_symlink())

    def test_actual_cli_failed_seal_preserves_unowned_inputs(self):
        if sys.platform != "darwin":
            self.skipTest("unmocked T20 host identity requires macOS")
        for kind in ("absent", "directory", "file", "symlink", "dangling-symlink"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                output = root / "failed-seal"
                output.mkdir()
                prepared = output / "prepared-inputs"
                foreign = preexisting(root, prepared, kind)
                result = subprocess.run([sys.executable, str(SCRIPT), str(output), output.name,
                                         str(root / "missing-manifest"), str(root / "missing-binary")],
                                        capture_output=True, text=True, timeout=20)
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn("T20 ledger directory is unsafe", result.stderr)
                self.last_error = result.stderr
                self.failed_receipt(output, kind == "absent")
                self.assert_preserved(prepared, foreign, kind)

    def test_real_prepare_refuses_preexisting_inputs_without_owning_them(self):
        for kind in ("directory", "file", "symlink", "dangling-symlink"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                output = root / "job"
                output.mkdir()
                prepared = output / "prepared-inputs"
                foreign = preexisting(root, prepared, kind)
                self.assertEqual(self.run_main(output), 1)
                self.assertIn("File exists", self.last_error)
                self.lifecycle_call.assert_not_called()
                self.failed_receipt(output, False)
                self.assert_preserved(prepared, foreign, kind)

    def test_refusal_before_allocation_leaves_existing_inputs_untouched(self):
        for failure in ("seal", "manifest"):
            for kind in ("absent", "directory"):
                with self.subTest(failure=failure, kind=kind), tempfile.TemporaryDirectory() as temporary:
                    root = Path(temporary)
                    output = root / "job"
                    output.mkdir()
                    prepared = output / "prepared-inputs"
                    foreign = preexisting(root, prepared, kind)
                    self.assertEqual(self.run_main(output, bad_manifest=failure == "manifest",
                                     seal_error=ValueError("seal refusal") if failure == "seal" else None), 1)
                    if failure == "seal":
                        self.preparation_call.assert_not_called()
                    self.lifecycle_call.assert_not_called()
                    self.failed_receipt(output, kind == "absent")
                    self.assert_preserved(prepared, foreign, kind)

    def test_partial_real_preparation_is_owned_before_cloning_can_fail(self):
        for failure in ("app", "disk.raw", "vars.fd", "cloned-hash"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                output = root / "job"
                output.mkdir()
                def tree(source, destination):
                    shutil.copytree(source, destination)
                    if failure == "app":
                        raise OSError("partial app clone")
                def file(source, destination):
                    shutil.copyfile(source, destination)
                    if failure == destination.name:
                        raise OSError("partial media clone")
                    if failure == "cloned-hash":
                        destination.write_bytes(b"corrupt")
                self.assertEqual(self.run_main(output, clone_file=file, clone_tree=tree), 1)
                self.lifecycle_call.assert_not_called()
                expected = ("cloned input hash mismatch" if failure == "cloned-hash" else
                            "partial app clone" if failure == "app" else "partial media clone")
                self.assertIn(expected, self.last_error)
                self.failed_receipt(output, True)
                self.assertEqual(list(output.iterdir()), [output / "receipt.json"])
                self.assertEqual((root / "source.raw").read_bytes(), b"image")
                self.assertEqual((root / "source.fd").read_bytes(), b"vars")

    def test_replacement_is_retained_during_prepare_or_lifecycle(self):
        for stage in ("prepare", "lifecycle"):
            for kind in ("directory", "file", "symlink", "absent"):
                with self.subTest(stage=stage, kind=kind), tempfile.TemporaryDirectory() as temporary:
                    root = Path(temporary)
                    output = root / "job"
                    output.mkdir()
                    prepared, retained = output / "prepared-inputs", root / "retained"
                    def replace():
                        prepared.rename(retained)
                        preexisting(root, prepared, kind)
                        if stage == "prepare":
                            raise OSError("path replaced during prepare")
                        return 1
                    def tree(source, destination):
                        shutil.copytree(source, destination)
                        replace()
                    self.assertEqual(self.run_main(output, replace if stage == "lifecycle" else None,
                                     clone_tree=tree if stage == "prepare" else shutil.copytree), 1)
                    self.failed_receipt(output, False)
                    self.assertTrue((retained / "BridgeVM.app").is_dir())
                    self.assert_preserved(prepared, root / "foreign", kind)
                    if stage == "prepare":
                        self.lifecycle_call.assert_not_called()
                    else:
                        self.lifecycle_call.assert_called_once()

    def test_completed_preparation_cleans_only_when_live_is_absent(self):
        for kind in ("absent", "directory", "file", "symlink", "dangling-symlink"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                output = root / "job"
                output.mkdir()
                def operation():
                    preexisting(root, output / "live", kind)
                    return 1
                self.assertEqual(self.run_main(output, operation), 1)
                self.lifecycle_call.assert_called_once()
                self.assertIn("marker lifecycle exited 1", self.last_error)
                self.failed_receipt(output, kind == "absent")
                self.assert_preserved(output / "live", root / "foreign", kind)
                prepared = output / "prepared-inputs"
                self.assertEqual(prepared.exists(), kind != "absent")
                if prepared.exists():
                    self.assertEqual((prepared / "disk.raw").read_bytes(), b"image")
                self.assertEqual((root / "source.raw").read_bytes(), b"image")
                self.assertEqual((root / "source.fd").read_bytes(), b"vars")

    def test_uncertain_lifecycle_retains_prepared_inputs_without_live_path(self):
        failures = (-9, OSError("launch failed"), subprocess.TimeoutExpired("lifecycle", 1), KeyboardInterrupt())
        for failure in failures:
            with self.subTest(failure=repr(failure)), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary) / "job"
                output.mkdir()
                def operation():
                    if isinstance(failure, BaseException):
                        raise failure
                    return failure
                if isinstance(failure, KeyboardInterrupt):
                    with self.assertRaises(KeyboardInterrupt):
                        self.run_main(output, operation)
                else:
                    self.assertEqual(self.run_main(output, operation), 1)
                self.failed_receipt(output, False)
                self.assertFalse(os.path.lexists(output / "live"))
                self.assertEqual((output / "prepared-inputs/disk.raw").read_bytes(), b"image")

    def test_quarantine_cleanup_error_is_not_verified_as_absence(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "job"
            output.mkdir()
            with patch("retained_windows_publication.clear_directory", side_effect=OSError("cleanup refused")):
                self.assertEqual(self.run_main(output), 1)
            self.failed_receipt(output, False)
            self.assertFalse(os.path.lexists(output / "prepared-inputs"))
            quarantined = list(output.glob(".prepared-inputs.cleanup-*"))
            self.assertEqual(len(quarantined), 1)
            self.assertEqual((quarantined[0] / "disk.raw").read_bytes(), b"image")
