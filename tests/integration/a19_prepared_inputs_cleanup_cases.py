"""Prepared-input allocation and identity-bound cleanup through the T22 runner."""
import json
import os
from pathlib import Path
import shutil
import tempfile
from unittest.mock import patch

import native_snapshot_restore_inputs as inputs
from a19_process_cleanup_fixture import receipt


def real_preparation(root: Path, clone_file=shutil.copyfile, clone_tree=shutil.copytree):
    """Tiny authenticated files, no private media, APFS cloning or queue operations."""
    app = root / "source.app"
    artifacts = {"app_bundle": app, "image": root / "source.raw", "vars": root / "source.fd"}
    artifacts.update({key: app / relative for key, relative in inputs.RELATIONS.items()})
    for key, path in artifacts.items():
        if key != "app_bundle":
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(key.encode())
            path.chmod(0o700)
    manifest = root / "manifest.tsv"
    rows = [f"{key}\t{path}\t{inputs.tree_hash(path) if key == 'app_bundle' else inputs.digest(path)}"
            for key, path in artifacts.items()]
    rows += [f"source_commit\t{'c' * 40}", "app_profile\trelease", "binary_profile\trelease",
             "binary_features\tvenus", "rust_toolchain\t1.97.0"]
    manifest.write_text("\n".join(rows) + "\n")

    def prepare(_manifest, _binary, commit, directory, **options):
        return inputs.prepare(manifest, artifacts["binary"], commit, directory,
                              clone_file, clone_tree, **options)
    return prepare


class PreparedInputsCleanupCases:
    def failed_cleanup_receipt(self, output: Path, cleaned: bool):
        value = receipt.validate(json.loads((output / "receipt.json").read_text()))
        self.assertFalse(value["pass"])
        self.assertEqual(value["worker_cleanup_verified"], cleaned, self.last_error)
        self.assertEqual([value[key] for key in ("sample_count", "run_count", "interruption_case_count")], [0, 0, 0])
        return value

    def test_preexisting_prepared_inputs_are_not_owned_or_deleted(self):
        for kind in ("directory", "file", "symlink", "dangling-symlink"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                output = root / "cleanup-fixture"
                output.mkdir()
                prepared, foreign = output / "prepared-inputs", root / "foreign"
                foreign.mkdir()
                (foreign / "sentinel").write_bytes(b"do not delete")
                if kind == "directory":
                    prepared.mkdir()
                    (prepared / "sentinel").write_bytes(b"do not delete")
                elif kind == "file":
                    prepared.write_bytes(b"do not delete")
                else:
                    prepared.symlink_to(foreign if kind == "symlink" else root / "absent")
                self.assertEqual(self.run_main(output, None, preparation=real_preparation(root)), 1)
                self.assertIn("FileExistsError", self.last_error)
                self.lifecycle_call.assert_not_called()
                value = json.loads((output / "receipt.json").read_text())
                self.assertTrue(os.path.lexists(prepared),
                                f"unowned {kind} deleted; worker_cleanup_verified={value['worker_cleanup_verified']}")
                self.failed_cleanup_receipt(output, False)
                if kind == "directory":
                    self.assertEqual((prepared / "sentinel").read_bytes(), b"do not delete")
                elif kind == "file":
                    self.assertEqual(prepared.read_bytes(), b"do not delete")
                else:
                    self.assertTrue(prepared.is_symlink())
                self.assertEqual((foreign / "sentinel").read_bytes(), b"do not delete")

    def test_seal_refusal_never_allocates_or_cleans_preexisting_inputs(self):
        for preexisting in (False, True):
            with self.subTest(preexisting=preexisting), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary) / "cleanup-fixture"
                output.mkdir()
                prepared = output / "prepared-inputs"
                if preexisting:
                    prepared.mkdir()
                    (prepared / "sentinel").write_bytes(b"preserve")
                self.assertEqual(self.run_main(output, None, seals=ValueError("fixture seal refusal")), 1)
                self.preparation_call.assert_not_called()
                self.lifecycle_call.assert_not_called()
                self.failed_cleanup_receipt(output, not preexisting)
                self.assertEqual(os.path.lexists(prepared), preexisting)
                if preexisting:
                    self.assertEqual((prepared / "sentinel").read_bytes(), b"preserve")

    def test_partial_real_preparation_is_owned_before_cloning_can_fail(self):
        for failure in ("app", "disk.raw", "vars.fd", "cloned-hash"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                output = root / "cleanup-fixture"
                output.mkdir()
                def tree(source, destination):
                    shutil.copytree(source, destination)
                    if failure == "app":
                        raise OSError("fixture partial app clone")
                def file(source, destination):
                    shutil.copyfile(source, destination)
                    if failure == destination.name:
                        raise OSError("fixture partial media clone")
                    if failure == "cloned-hash":
                        destination.write_bytes(b"corrupt clone")
                self.assertEqual(self.run_main(output, None,
                                 preparation=real_preparation(root, file, tree)), 1)
                self.lifecycle_call.assert_not_called()
                self.failed_cleanup_receipt(output, True)
                self.assertFalse(os.path.lexists(output / "prepared-inputs"))
                self.assertFalse(list(output.glob(".prepared-inputs.cleanup-*")))
                self.assertEqual((root / "source.raw").read_bytes(), b"image")
                self.assertEqual((root / "source.fd").read_bytes(), b"vars")

    def test_replaced_prepared_path_is_refused_even_when_prepare_raises(self):
        for stage in ("preparation", "lifecycle"):
            for replacement in ("directory", "symlink", "missing"):
                with self.subTest(stage=stage, replacement=replacement), tempfile.TemporaryDirectory() as temporary:
                    root = Path(temporary)
                    output = root / "cleanup-fixture"
                    output.mkdir()
                    prepared, retained, foreign = output / "prepared-inputs", root / "retained", root / "foreign"
                    foreign.mkdir()
                    (foreign / "sentinel").write_bytes(b"foreign")
                    def replace():
                        prepared.rename(retained)
                        if replacement == "directory":
                            prepared.mkdir()
                            (prepared / "sentinel").write_bytes(b"replacement")
                        elif replacement == "symlink":
                            prepared.symlink_to(foreign)
                        raise OSError("fixture path replaced")
                    def tree(source, destination):
                        shutil.copytree(source, destination)
                        replace()
                    def operation(_repo, _environment):
                        replace()
                    preparation = real_preparation(root, clone_tree=tree) if stage == "preparation" else None
                    self.assertEqual(self.run_main(output, operation, preparation=preparation), 1)
                    self.failed_cleanup_receipt(output, False)
                    self.assertTrue(retained.is_dir())
                    self.assertEqual((foreign / "sentinel").read_bytes(), b"foreign")
                    if stage == "preparation":
                        self.lifecycle_call.assert_not_called()
                        self.assertTrue((retained / "BridgeVM.app").is_dir())
                    else:
                        self.assertEqual((retained / "disk.raw").read_bytes(), b"fixture disk")
                    if replacement == "directory":
                        self.assertEqual((prepared / "sentinel").read_bytes(), b"replacement")
                    elif replacement == "symlink":
                        self.assertTrue(prepared.is_symlink())
                    else:
                        self.assertFalse(os.path.lexists(prepared))

    def test_completed_real_preparation_cleans_only_its_allocated_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / "cleanup-fixture"
            output.mkdir()
            preparation = real_preparation(root)
            seals = {"input_manifest_sha256": inputs.digest(root / "manifest.tsv"),
                     "binary_hash": inputs.digest(root / "source.app" / inputs.RELATIONS["binary"])}
            self.assertEqual(self.run_main(output, lambda *_args: 1, preparation=preparation,
                                          seals=lambda *_args: seals), 1)
            self.lifecycle_call.assert_called_once()
            self.assertIn("T22 lifecycle exited 1", self.last_error)
            self.failed_cleanup_receipt(output, True)
            self.assertEqual(list(output.iterdir()), [output / "receipt.json"])
            self.assertEqual((root / "source.raw").read_bytes(), b"image")
            self.assertEqual((root / "source.fd").read_bytes(), b"vars")

    def test_cleanup_failure_after_quarantine_is_not_verified_as_absence(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "cleanup-fixture"
            output.mkdir()
            with patch("retained_windows_publication.clear_directory", side_effect=OSError("fixture cleanup refusal")):
                self.assertEqual(self.run_main(output, lambda *_args: 1), 1)
            self.failed_cleanup_receipt(output, False)
            self.assertFalse(os.path.lexists(output / "prepared-inputs"))
            quarantined = list(output.glob(".prepared-inputs.cleanup-*"))
            self.assertEqual(len(quarantined), 1)
            self.assertEqual((quarantined[0] / "disk.raw").read_bytes(), b"fixture disk")

    def test_owned_cleanup_and_live_path_retention(self):
        for live_kind in ("absent", "directory", "dangling-symlink"):
            with self.subTest(live_kind=live_kind), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary) / "cleanup-fixture"
                output.mkdir()
                def operation(_repo, _environment):
                    if live_kind == "directory":
                        (output / "live").mkdir()
                    elif live_kind == "dangling-symlink":
                        (output / "live").symlink_to(output / "absent")
                    return 1
                self.assertEqual(self.run_main(output, operation), 1)
                self.lifecycle_call.assert_called_once()
                self.failed_cleanup_receipt(output, live_kind == "absent")
                self.assertEqual(os.path.lexists(output / "prepared-inputs"), live_kind != "absent")
                self.assertEqual(os.path.lexists(output / "live"), live_kind != "absent")
