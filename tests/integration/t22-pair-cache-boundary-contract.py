#!/usr/bin/env python3
"""Clean Git source cannot authorize D10 via ignored repository bytecode."""
import subprocess
import unittest

from t22_pair_cache_fixtures import CacheFixture


class CacheBoundary(CacheFixture):
    def test_clean_timestamp_cache_cannot_certify_invalid_receipt(self):
        (self.directory / "receipt.json").write_text('{"unverified":true}\n')
        self.cache("scripts/live-gates/t22_pair_queue_proof.py")
        self.guard()

    def test_dynamic_verifier_root_cache_cannot_certify_invalid_receipt(self):
        (self.directory / "receipt.json").write_text('{"unverified":true}\n')
        self.cache("scripts/verify-windows-product-e2e-receipt.py")
        self.guard()

    def test_stale_valid_proofs_with_ignored_cache_never_move_done(self):
        self.fixture.emit(self.directory, self.worktree, "success")
        self.cache("scripts/live-gates/t22_pair_queue_proof.py")
        status, output = self.fixture.recover()
        self.fixture.assert_fenced(status, output)
        self.assertFalse(self.marker.exists())
        self.assertTrue((self.fixture.home / "BridgeVM/t22-prepared-pairs/a-fixture").is_dir())

    def test_all_dispatch_modes_refuse_cache_before_python_execution(self):
        (self.directory / "receipt.json").write_text('{"unverified":true}\n')
        self.cache("scripts/live-gates/t22_pair_queue_proof.py")
        for mode in ("validate", "seal", "run", "finalize", "publish", "guard"):
            with self.subTest(mode=mode):
                argv = [str(self.directory / "input-manifest.tsv"), self.fixture.commit]
                if mode == "seal": argv.append(str(self.directory))
                if mode not in ("validate", "seal"):
                    argv = [str(self.directory), str(self.worktree), self.fixture.commit]
                    if mode == "guard": argv.append("a-fixture")
                    if mode == "run": argv.extend((str(self.directory / "input-manifest.tsv"), str(self.directory / "hvf_gic_boot_probe")))
                result = subprocess.run(["/bin/bash", "--noprofile", "--norc", "-p",
                    str(self.worktree / "scripts/live-gates/t22-pair-queue-dispatch.sh"), mode, *argv],
                    env=self.fixture.env(), capture_output=True, text=True, timeout=35)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("cached repository code refused", result.stderr)
                self.assertFalse(self.marker.exists())

    def test_bytecode_names_and_cache_symlink_refuse_without_following(self):
        for root in ("scripts", "scripts/live-gates"):
            for name in ("__pycache__", "module.pyc", "module.pyo"):
                with self.subTest(root=root, name=name):
                    path = self.worktree / root / name
                    external = self.fixture.root / "external-cache"; external.mkdir(exist_ok=True)
                    path.symlink_to(external)
                    self.assertNotEqual(self.inventory().returncode, 0)
                    self.assertEqual(list(external.iterdir()), [])
                    path.unlink()
            empty = self.worktree / root / "__pycache__"; empty.mkdir()
            self.assertNotEqual(self.inventory().returncode, 0); empty.rmdir()

    def test_missing_or_symlink_import_root_refuses(self):
        root = self.fixture.root / "inventory"; root.mkdir(); (root / "scripts").mkdir()
        self.assertNotEqual(self.inventory(root).returncode, 0)
        (root / "scripts/live-gates").symlink_to(self.worktree / "scripts/live-gates")
        self.assertNotEqual(self.inventory(root).returncode, 0)

    def test_unrelated_cache_trees_are_outside_explicit_import_roots(self):
        for name in ("target", "tests/integration"):
            path = self.worktree / name / "__pycache__"; path.mkdir(parents=True, exist_ok=True)
            (path / "unrelated.pyc").write_bytes(b"not repository import code")
        self.assertEqual(self.inventory().returncode, 0)

    def test_finite_inventory_refuses_over_4096_entries(self):
        root = self.fixture.root / "inventory"; (root / "scripts/live-gates").mkdir(parents=True)
        for index in range(4096): (root / "scripts" / f"entry-{index}").touch()
        result = self.inventory(root)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("exceeds bound", result.stderr)

    def test_actual_core_and_guard_do_not_generate_repository_cache(self):
        self.fixture.emit(self.directory, self.worktree, "success")
        self.guard(expected=0)
        for root in ("scripts", "scripts/live-gates"):
            self.assertFalse((self.worktree / root / "__pycache__").exists())
            self.assertFalse(any((self.worktree / root).glob("*.py[co]")))


if __name__ == "__main__": unittest.main()
