"""T21 ownership survives partial preparation, helper failures and name swaps."""
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
from unittest.mock import patch

from a19_quota_cleanup import OWNED
from a19_quota_cleanup_fixture import run_main, runner
import a19_quota_refusal_receipt as receipt


class QuotaOwnedCases:
    def test_partial_real_preparation_is_owned_before_clone_failure(self):
        for failure in ("app", "disk.raw", "vars.fd", "cloned-hash"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                output, manifest, binary = self.cleanup_fixture(root)
                def tree(source, destination):
                    shutil.copytree(source, destination)
                    if failure == "app":
                        raise OSError("partial app fixture")
                def file(source, destination):
                    shutil.copyfile(source, destination)
                    if failure == destination.name:
                        raise OSError("partial media fixture")
                    if failure == "cloned-hash":
                        destination.write_bytes(b"corrupt clone")
                with patch.object(runner, "exercise") as exercise:
                    result = run_main(output, manifest, binary, clone_tree=tree, clone_file=file)
                exercise.assert_not_called()
                self.assert_failed_cleanup(result, True)
                self.assertEqual(list(output.iterdir()), [output / "receipt.json"])
                self.assertEqual((root / "image.raw").read_bytes(), b"installed-Windows-fixture")
                self.assertEqual((root / "vars.fd").read_bytes(), b"UEFI-fixture")

    def test_replaced_prepared_tree_is_preserved_on_prepare_and_helper_failure(self):
        for stage in ("preparation", "helper"):
            for replacement in ("directory", "file", "symlink", "dangling-symlink", "missing"):
                with self.subTest(stage=stage, replacement=replacement), tempfile.TemporaryDirectory() as temporary:
                    root = Path(temporary)
                    output, manifest, binary = self.cleanup_fixture(root)
                    prepared, retained, foreign = output / "prepared-inputs", root / "retained", root / "foreign"
                    foreign.mkdir()
                    (foreign / "sentinel").write_bytes(b"foreign")
                    def replace(*_args, **_options):
                        prepared.rename(retained)
                        if replacement == "directory":
                            prepared.mkdir()
                            (prepared / "sentinel").write_bytes(b"replacement")
                        elif replacement == "file":
                            prepared.write_bytes(b"replacement")
                        elif replacement != "missing":
                            prepared.symlink_to(foreign if replacement == "symlink" else root / "absent")
                        raise OSError("replaced fixture")
                    def tree(source, destination):
                        shutil.copytree(source, destination)
                        replace()
                    options = {"clone_tree": tree} if stage == "preparation" else {"invoke": replace}
                    self.assert_failed_cleanup(run_main(output, manifest, binary, **options), False)
                    self.assertTrue((retained / "BridgeVM.app").is_dir())
                    self.assertEqual((foreign / "sentinel").read_bytes(), b"foreign")
                    if replacement == "directory":
                        self.assertEqual((prepared / "sentinel").read_bytes(), b"replacement")
                    elif replacement == "file":
                        self.assertEqual(prepared.read_bytes(), b"replacement")
                    elif replacement != "missing":
                        self.assertTrue(prepared.is_symlink())
                    else:
                        self.assertFalse(os.path.lexists(prepared))

    def test_helper_residue_and_invalid_quota_results_fail_but_owned_tree_is_cleaned(self):
        for mode in ("refusal-residue", "boundary-partial", "verify-failure", "wrong-message",
                     "mutate-input", "wrong-manifest", "mutate-app"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as temporary:
                output, manifest, binary = self.cleanup_fixture(Path(temporary), mode)
                self.assert_failed_cleanup(run_main(output, manifest, binary), True)
                self.assertEqual(list(output.iterdir()), [output / "receipt.json"])

    def test_helper_timeout_cleans_owned_staging_and_parent_lease(self):
        with tempfile.TemporaryDirectory() as temporary:
            output, manifest, binary = self.cleanup_fixture(Path(temporary))
            def timeout(_helper, verb, _disk, _vars, destination, *_args, **_options):
                self.assertEqual(verb, "create")
                parent = Path(destination).parent
                self.assertEqual(parent, output / "prepared-inputs")
                for name in (".quota-refusal.snapshot.staging", ".bridgevm-snapshot-parent-lease"):
                    (parent / name).mkdir()
                    (parent / name / "partial").write_bytes(b"owned")
                raise subprocess.TimeoutExpired("fixture", 60)
            self.assert_failed_cleanup(run_main(output, manifest, binary, invoke=timeout), True)
            self.assertEqual(list(output.iterdir()), [output / "receipt.json"])

    def test_late_loose_names_are_not_adopted_by_cleanup(self):
        with tempfile.TemporaryDirectory() as temporary:
            output, manifest, binary = self.cleanup_fixture(Path(temporary), "late-loose-output")
            self.assert_failed_cleanup(run_main(output, manifest, binary), False)
            self.assertFalse(os.path.lexists(output / "prepared-inputs"))
            for name in OWNED[1:]:
                self.assertEqual((output / name / "sentinel").read_bytes(), b"foreign late data")

    def test_quarantined_cleanup_failure_cannot_publish_a_true_cleanup_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            output, manifest, binary = self.cleanup_fixture(Path(temporary))
            with patch("retained_windows_publication.clear_directory", side_effect=OSError("cleanup refusal")):
                result = run_main(output, manifest, binary)
            self.assert_failed_cleanup(result, False)
            self.assertTrue(result[1]["success_verified"])
            self.assertFalse(os.path.lexists(output / "prepared-inputs"))
            quarantines = list(output.glob(".prepared-inputs.cleanup-*"))
            self.assertEqual(len(quarantines), 1)
            self.assertTrue((quarantines[0] / "quota-boundary.snapshot/disk.raw").is_file())
            with self.assertRaisesRegex(ValueError, "cannot publish before private-media cleanup"):
                receipt.validate_seal(result[1], output)

    def test_subprocess_fixture_preserves_quota_bytes_manifest_and_app_authentication(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output, manifest, binary = self.cleanup_fixture(root)
            invocations = []
            invoke = runner.invoke
            def call(helper, *args, **options):
                if args[0] == "create":
                    self.assertEqual(Path(args[3]).parent, output / "prepared-inputs")
                invocations.append(args)
                return invoke(helper, *args, **options)
            status, value, error = run_main(output, manifest, binary, invoke=call)
            self.assertEqual(status, 0, error)
            self.assertTrue(value["success_verified"])
            self.assertTrue(value["worker_cleanup_verified"])
            pair = (root / "image.raw").stat().st_size + (root / "vars.fd").stat().st_size
            self.assertEqual((value["pair_bytes"], value["rejected_quota_bytes"], value["accepted_quota_bytes"]),
                             (pair, pair - 1, pair))
            self.assertEqual(value["refusal_output_sha256"], hashlib.sha256(receipt.quota_error(pair)).hexdigest())
            self.assertEqual([call[0] for call in invocations], ["create", "create", "verify"])
            self.assertEqual((invocations[0][-1], invocations[1][-1]), (str(pair - 1), str(pair)))
            for field in ("prepared_image_sha256", "snapshot_disk_sha256", "final_disk_sha256"):
                self.assertEqual(value[field], value["image_sha256"])
            for field in ("prepared_vars_sha256", "snapshot_vars_sha256", "final_vars_sha256"):
                self.assertEqual(value[field], value["vars_sha256"])
            self.assertEqual(list(output.iterdir()), [output / "receipt.json"])
