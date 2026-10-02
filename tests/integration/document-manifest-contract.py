#!/usr/bin/env python3
"""Document catalog shards preserve records and fail closed on damaged topology."""
from __future__ import annotations

from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
from document_manifest import HEADER, load_manifest

CURRENT = "docs/current.md\tcurrent\tproduct\t-"
HISTORY = "docs/history/old.md\thistorical-evidence\thistory\t-"
INCLUDE = "@include\tdocs/document-manifests/history.tsv\t-\t-"


class ManifestContract(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.manifest = self.root / "docs/document-manifest.tsv"
        self.shard = self.root / "docs/document-manifests/history.tsv"
        self.shard.parent.mkdir(parents=True)
        self.manifest.write_text(f"{HEADER}\n{CURRENT}\n{INCLUDE}\n")
        self.shard.write_text(f"{HEADER}\n{HISTORY}\n")

    def test_explicit_shard_preserves_all_fields_and_final_unterminated_row(self):
        self.shard.write_text(f"{HEADER}\n{HISTORY}")
        self.assertEqual(load_manifest(self.manifest), [tuple(CURRENT.split("\t")), tuple(HISTORY.split("\t"))])

    def test_missing_shard_refuses_without_partial_output_or_shell_success(self):
        self.shard.unlink()
        process = subprocess.run(["bash", "-eu", "-c",
            'records=$(python3 "$1" "$2"); printf "unexpected-success\\n"',
            "manifest-test", str(ROOT / "scripts/document_manifest.py"), str(self.manifest)],
            capture_output=True, text=True)
        self.assertNotEqual(process.returncode, 0)
        self.assertEqual(process.stdout, "")
        self.assertIn("shard is missing or unsafe", process.stderr)

    def test_bad_root_or_shard_header_refuses(self):
        for path in (self.manifest, self.shard):
            original = path.read_text()
            for invalid in ("", "path\tclass\ttopic\n", "path\ttopic\tclass\tsuperseded_by\n"):
                with self.subTest(path=path, invalid=invalid):
                    path.write_text(invalid)
                    with self.assertRaisesRegex(ValueError, "header"):
                        load_manifest(self.manifest)
            path.write_text(original)

    def test_duplicate_documents_are_rejected_across_root_and_shards(self):
        other = self.shard.with_name("other.tsv")
        other.write_text(f"{HEADER}\n{HISTORY}\n")
        for root_tail, shard_tail in (("", CURRENT), ("", HISTORY),
                ("@include\tdocs/document-manifests/other.tsv\t-\t-", "")):
            with self.subTest(root_tail=root_tail, shard_tail=shard_tail):
                self.manifest.write_text(f"{HEADER}\n{CURRENT}\n{INCLUDE}\n{root_tail}\n")
                self.shard.write_text(f"{HEADER}\n{HISTORY}\n{shard_tail}\n")
                with self.assertRaisesRegex(ValueError, "duplicate manifest path"):
                    load_manifest(self.manifest)

    def test_duplicate_nested_or_malformed_includes_refuse(self):
        for include in (INCLUDE, "@include\t../history.tsv\t-\t-",
                "@include\t/absolute/history.tsv\t-\t-", INCLUDE.replace("\t-\t-", "\tcurrent\t-")):
            with self.subTest(include=include):
                self.manifest.write_text(f"{HEADER}\n{INCLUDE}\n{include}\n")
                with self.assertRaisesRegex(ValueError, "include"):
                    load_manifest(self.manifest)
        self.manifest.write_text(f"{HEADER}\n{INCLUDE}\n")
        self.shard.write_text(f"{HEADER}\n{INCLUDE}\n")
        with self.assertRaisesRegex(ValueError, "nested"):
            load_manifest(self.manifest)

    def test_empty_or_extra_columns_refuse_in_shards(self):
        for record in (HISTORY + "\textra", HISTORY + "\t", HISTORY.replace("\thistory\t", "\t\t")):
            with self.subTest(record=record):
                self.shard.write_text(f"{HEADER}\n{record}\n")
                with self.assertRaisesRegex(ValueError, "columns"):
                    load_manifest(self.manifest)

    def test_symlink_shard_or_shard_directory_refuses(self):
        outside = self.root / "outside.tsv"
        outside.write_text(f"{HEADER}\n{HISTORY}\n")
        self.shard.unlink()
        self.shard.symlink_to(outside)
        with self.assertRaisesRegex(ValueError, "unsafe"):
            load_manifest(self.manifest)
        self.shard.unlink()
        self.shard.parent.rmdir()
        self.shard.parent.symlink_to(self.root, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "unsafe"):
            load_manifest(self.manifest)

    def test_shard_under_an_escaping_docs_ancestor_refuses(self):
        outside = tempfile.TemporaryDirectory()
        self.addCleanup(outside.cleanup)
        moved = Path(outside.name) / "docs"
        (self.root / "docs").rename(moved)
        (self.root / "docs").symlink_to(moved, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "escapes"):
            load_manifest(self.manifest)


if __name__ == "__main__":
    unittest.main()
