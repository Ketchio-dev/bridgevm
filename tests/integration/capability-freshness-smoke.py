#!/usr/bin/env python3
"""Evidence identities must be commits and comparison errors must fail closed."""
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))
import capability_freshness as freshness


class FreshnessTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="bridgevm-freshness-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "source"
        self.root.mkdir()
        self.git("init", "-q")
        self.git("config", "user.name", "Freshness fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        (self.root / "apps").mkdir()
        (self.root / "apps/control.swift").write_text("// fixture\n")
        self.base = self.commit()

    def git(self, *args, root=None):
        return subprocess.run(["git", *args], cwd=root or self.root,
                              capture_output=True, text=True, check=True).stdout.strip()

    def commit(self):
        self.git("add", ".")
        self.git("commit", "-qm", "fixture checkpoint")
        return self.git("rev-parse", "HEAD")

    def test_unchanged_commit_is_current(self):
        self.assertIsNone(freshness.code_changed_since(self.base, self.root))

    def test_code_change_is_stale(self):
        (self.root / "apps/control.swift").write_text("// changed\n")
        self.commit()
        self.assertEqual(freshness.code_changed_since(self.base, self.root), "apps/control.swift")

    def test_documentation_checkpoint_preserves_code_identity(self):
        (self.root / "README.md").write_text("Documented checkpoint.\n")
        self.commit()
        self.assertIsNone(freshness.code_changed_since(self.base, self.root))

    def test_blob_is_not_a_commit(self):
        blob = self.git("rev-parse", "HEAD:apps/control.swift")
        self.assertEqual(self.git("cat-file", "-t", blob), "blob")
        self.assertIsNotNone(freshness.code_changed_since(blob, self.root))

    def test_tree_is_not_a_commit(self):
        tree = self.git("rev-parse", "HEAD^{tree}")
        self.assertIsNotNone(freshness.code_changed_since(tree, self.root))

    def test_tag_object_is_not_a_commit(self):
        self.git("tag", "-am", "evidence fixture", "evidence")
        tag = self.git("rev-parse", "evidence")
        self.assertEqual(self.git("cat-file", "-t", tag), "tag")
        self.assertIsNotNone(freshness.code_changed_since(tag, self.root))

    def test_unknown_commit_is_not_current(self):
        self.assertIsNotNone(freshness.code_changed_since("0" * 40, self.root))

    def test_missing_history_in_shallow_clone_is_not_current(self):
        (self.root / "README.md").write_text("Second checkpoint.\n")
        self.commit()
        clone = Path(self.temp.name) / "shallow"
        self.git("clone", "-q", "--depth", "1", self.root.as_uri(), str(clone))
        self.assertEqual(self.git("rev-parse", "--is-shallow-repository", root=clone), "true")
        self.assertIsNotNone(freshness.code_changed_since(self.base, clone))

    def test_available_commit_in_shallow_clone_can_be_compared(self):
        clone = Path(self.temp.name) / "shallow"
        self.git("clone", "-q", "--depth", "1", self.root.as_uri(), str(clone))
        self.assertIsNone(freshness.code_changed_since(self.base, clone))

    def test_diff_failure_is_not_no_changes(self):
        results = [subprocess.CompletedProcess([], 0, "commit\n", ""),
                   subprocess.CompletedProcess([], 128, "", "comparison failed")]
        with patch.object(freshness, "_git", side_effect=results):
            self.assertIn("could not compare", freshness.code_changed_since(self.base, self.root))

    def test_object_lookup_failure_is_not_no_changes(self):
        failed = subprocess.CompletedProcess([], 128, "", "object lookup failed")
        with patch.object(freshness, "_git", return_value=failed) as query:
            self.assertIsNotNone(freshness.code_changed_since(self.base, self.root))
            self.assertEqual(query.call_count, 1)

    def test_changed_path_with_spaces_is_not_truncated(self):
        (self.root / "apps/control sample.swift").write_text("// changed\n")
        self.commit()
        self.assertEqual(freshness.code_changed_since(self.base, self.root), "apps/control sample.swift")


if __name__ == "__main__":
    unittest.main(verbosity=2)
