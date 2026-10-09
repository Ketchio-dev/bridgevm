"""T21 regressions: failed admission never owns existing output names."""
import json
import os
from pathlib import Path
import tempfile
from unittest.mock import patch

from a19_quota_cleanup import OWNED
from a19_quota_cleanup_fixture import helper_cases, run_main, runner, seal
from a19_quota_owned_cases import QuotaOwnedCases


class QuotaCleanupCases(QuotaOwnedCases):
    def test_missing_seal_preserves_preexisting_prepared_and_snapshot_trees(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "quota-fixture"
            output.mkdir()
            names = ("prepared-inputs", "quota-boundary.snapshot")
            for name in names:
                (output / name).mkdir()
                (output / name / "sentinel").write_bytes(b"foreign data")
            result = self.run_tier(output)
            self.assertEqual(result.returncode, 1, result.stderr)
            value = json.loads((output / "receipt.json").read_text())
            surviving = [name for name in names if os.path.lexists(output / name / "sentinel")]
            self.assertEqual(surviving, list(names),
                             f"surviving={surviving}; worker_cleanup_verified={value['worker_cleanup_verified']}")
            for name in names:
                self.assertEqual((output / name / "sentinel").read_bytes(), b"foreign data")
            self.assertFalse(value["worker_cleanup_verified"])
            self.assertFalse(value["pass"])

    def cleanup_fixture(self, root, mode="normal"):
        namespace = self.fixture.__globals__
        with patch.dict(namespace, HELPER=helper_cases(namespace["HELPER"])):
            manifest, binary, _ = self.fixture(root, mode)
        output = root / "quota-fixture"
        output.mkdir()
        return output, manifest, binary

    def assert_failed_cleanup(self, result, cleaned):
        status, value, error = result
        self.assertEqual(status, 1, error)
        self.assertFalse(value["pass"])
        self.assertEqual(value["worker_cleanup_verified"], cleaned, error)
        self.assertEqual((value["run_count"], value["quota_case_count"]), (0, 0))
        for key in ("claim_eligible", "criterion_pass", "capability_promotion", "three_d_injection"):
            self.assertFalse(value[key])

    def test_missing_seal_preserves_each_legacy_name_and_entry_kind(self):
        for name in OWNED:
            for kind in ("directory", "file", "symlink", "dangling-symlink"):
                with self.subTest(name=name, kind=kind), tempfile.TemporaryDirectory() as temporary:
                    output = Path(temporary) / "quota-fixture"
                    output.mkdir()
                    foreign = Path(temporary) / "foreign"
                    foreign.mkdir()
                    (foreign / "sentinel").write_bytes(b"outside")
                    path = output / name
                    if kind == "directory":
                        path.mkdir()
                        (path / "sentinel").write_bytes(b"preserve")
                    elif kind == "file":
                        path.write_bytes(b"preserve")
                    else:
                        path.symlink_to(foreign if kind == "symlink" else foreign / "absent")
                    result = self.run_tier(output)
                    value = json.loads((output / "receipt.json").read_text())
                    self.assert_failed_cleanup((result.returncode, value, result.stderr), False)
                    self.assertTrue(os.path.lexists(path))
                    self.assertEqual((foreign / "sentinel").read_bytes(), b"outside")
                    if kind == "directory":
                        self.assertEqual((path / "sentinel").read_bytes(), b"preserve")
                    elif kind == "file":
                        self.assertEqual(path.read_bytes(), b"preserve")
                    else:
                        self.assertTrue(path.is_symlink())

    def test_freshness_guard_preserves_all_preexisting_legacy_trees(self):
        for name in OWNED:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                output, manifest, binary = self.cleanup_fixture(Path(temporary))
                path = output / name
                path.mkdir()
                (path / "sentinel").write_bytes(b"foreign")
                with patch.object(runner, "invoke") as invoked:
                    result = run_main(output, manifest, binary)
                invoked.assert_not_called()
                self.assert_failed_cleanup(result, False)
                self.assertIn("FileExistsError" if name == "prepared-inputs" else "ValueError", result[2])
                self.assertEqual((path / "sentinel").read_bytes(), b"foreign")
                if name != "prepared-inputs":
                    self.assertFalse(os.path.lexists(output / "prepared-inputs"))

    def test_sealed_subprocess_flow_without_queue_submission(self):
        for existing in (None, "prepared-inputs", "quota-boundary.snapshot"):
            with self.subTest(existing=existing), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                manifest, binary, _ = self.fixture(root)
                output = root / "sealed/quota-fixture"
                output.mkdir(parents=True)
                seal(output, manifest, binary, self.fixture.__globals__["COMMIT"])
                if existing:
                    (output / existing).mkdir()
                    (output / existing / "sentinel").write_bytes(b"foreign")
                result = self.run_tier(output)
                value = json.loads((output / "receipt.json").read_text())
                if existing:
                    self.assert_failed_cleanup((result.returncode, value, result.stderr), False)
                    self.assertEqual((output / existing / "sentinel").read_bytes(), b"foreign")
                else:
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertTrue(value["pass"])
                    self.assertTrue(value["worker_cleanup_verified"])
                    self.assertFalse(any(os.path.lexists(output / name) for name in OWNED))

    def test_existing_archive_guard_rejects_nested_helper_residue(self):
        # Only local synthetic files and the reader function; no queue CLI.
        from a19_archived_receipt_fixtures import fixture
        from a19_archived_receipt_read import read_quota
        with tempfile.TemporaryDirectory() as temporary:
            output, _ = fixture(Path(temporary), 21)
            read_quota(output / "receipt.public.json", output)
            nested = output / "prepared-inputs/quota-boundary.snapshot"
            nested.mkdir(parents=True)
            (nested / "sentinel").write_bytes(b"private residue fixture")
            with self.assertRaisesRegex(ValueError, "private-media residue"):
                read_quota(output / "receipt.public.json", output)
            self.assertEqual((nested / "sentinel").read_bytes(), b"private residue fixture")

    def test_unallocated_absent_paths_need_no_cleanup(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "quota-fixture"
            output.mkdir()
            result = self.run_tier(output)
            value = json.loads((output / "receipt.json").read_text())
            self.assert_failed_cleanup((result.returncode, value, result.stderr), True)
            self.assertEqual(list(output.iterdir()), [output / "receipt.json"])
