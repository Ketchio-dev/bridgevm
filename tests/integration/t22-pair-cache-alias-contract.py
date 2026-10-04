#!/usr/bin/env python3
"""Case-insensitive filesystem cache aliases must fail closed too."""
import unittest

from t22_pair_cache_fixtures import CacheFixture


class CacheAliases(CacheFixture):
    def test_uppercase_timestamp_cache_directory_cannot_certify_invalid_receipt(self):
        (self.directory / "receipt.json").write_text('{"unverified":true}\n')
        self.cache("scripts/live-gates/t22_pair_queue_proof.py")
        path = self.worktree / "scripts/live-gates/__pycache__"
        path.rename(path.with_name("__PYCACHE__"))
        result = self.inventory()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("cached repository code refused", result.stderr)
        self.guard()

    def test_case_variants_in_both_import_roots_refuse(self):
        for root in ("scripts", "scripts/live-gates"):
            for name in ("__PyCache__", "module.PYC", "module.PYO"):
                with self.subTest(root=root, name=name):
                    path = self.worktree / root / name
                    path.symlink_to(self.fixture.root / "missing-external-cache")
                    result = self.inventory()
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("cached repository code refused", result.stderr)
                    path.unlink()


if __name__ == "__main__": unittest.main()
