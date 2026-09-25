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

    def test_code_change_is_stale(self):
        for path in ("apps/control.swift", "apps/control sample.swift", "Cargo.toml",
                     "Cargo.lock", ".github/workflows/ci.yml", ".github/actions/b6-tip-contracts/action.yml",
                     "install.sh", "deny.toml",
                     "packaging/macos/build-release-candidate.sh", "tools/venus-host-probe/Cargo.toml",
                     "schemas/bridgevm-capability-v1.json", "fuzz/src/lib.rs", ".gitattributes",
                     "LICENSE", "THIRD-PARTY-NOTICES.md", "THIRD-PARTY-PATCHES.tsv",
                     "docs/licenses/virglrenderer-MIT.txt", "docs/machine-contract/qemu-virt-deviations.json"):
            with self.subTest(path=path):
                self.git("reset", "--hard", self.base)
                target = self.root / path
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text("// changed\n")
                self.commit()
                self.assertEqual(freshness.code_changed_since(self.base, self.root), path)
        for action in (("rm", "apps/control.swift"), ("mv", "apps/control.swift", "README.md")):
            with self.subTest(action=action[0]):
                self.git("reset", "--hard", self.base)
                self.git(*action)
                self.commit()
                self.assertEqual(freshness.code_changed_since(self.base, self.root), "apps/control.swift")

    def test_documentation_checkpoint_preserves_code_identity(self):
        self.assertIsNone(freshness.code_changed_since(self.base, self.root))
        for path in ("README.md", ".github/ISSUE_TEMPLATE/bug_report.yml", ".github/dependabot.yml"):
            target = self.root / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text("Documented checkpoint.\n")
            self.commit()
            self.assertIsNone(freshness.code_changed_since(self.base, self.root))

    def test_non_commit_objects_are_not_current(self):
        self.git("tag", "-am", "evidence fixture", "evidence")
        for object_id, kind in ((self.git("rev-parse", "HEAD:apps/control.swift"), "blob"),
                                (self.git("rev-parse", "HEAD^{tree}"), "tree"),
                                (self.git("rev-parse", "evidence"), "tag")):
            self.assertEqual(self.git("cat-file", "-t", object_id), kind)
            self.assertIsNotNone(freshness.code_changed_since(object_id, self.root))
        self.assertIsNotNone(freshness.code_changed_since("0" * 40, self.root))

    def test_shallow_clone_requires_available_tested_commit(self):
        clone = Path(self.temp.name) / "current"
        self.git("clone", "-q", "--depth", "1", self.root.as_uri(), str(clone))
        self.assertIsNone(freshness.code_changed_since(self.base, clone))
        (self.root / "README.md").write_text("Second checkpoint.\n")
        self.commit()
        missing = Path(self.temp.name) / "missing"
        self.git("clone", "-q", "--depth", "1", self.root.as_uri(), str(missing))
        self.assertEqual(self.git("rev-parse", "--is-shallow-repository", root=missing), "true")
        self.assertIsNotNone(freshness.code_changed_since(self.base, missing))

    def test_git_failures_are_not_current(self):
        results = [subprocess.CompletedProcess([], 0, "commit\n", ""),
                   subprocess.CompletedProcess([], 128, "", "comparison failed")]
        with patch.object(freshness, "_git", side_effect=results):
            self.assertIn("could not compare", freshness.code_changed_since(self.base, self.root))
        failed = subprocess.CompletedProcess([], 128, "", "object lookup failed")
        with patch.object(freshness, "_git", return_value=failed) as query:
            self.assertIsNotNone(freshness.code_changed_since(self.base, self.root))
            self.assertEqual(query.call_count, 1)


if __name__ == "__main__":
    unittest.main(verbosity=2)
