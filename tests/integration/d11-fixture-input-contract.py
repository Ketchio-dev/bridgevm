#!/usr/bin/env python3
"""Deterministic D11 manifest/descriptor/refusal contracts; no native providers."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from d11_fixture_test_support import COMMIT, HASH, rows, manifest, guest
from d11_fixture_files import FileSeal, TreeSeal, document, record, tree_paths
from d11_fixture_inputs import parse
from d11_fixture_process import capacity, RESERVE, OVERHEAD
from d11_fixture_guest import facts
from d11_fixture_executables import dependencies, system


class Inputs(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()

    def test_manifest_accepts_distinct_binary_source(self):
        self.assertEqual(parse(manifest(rows(self.root)), COMMIT)["binary_source_commit"], ["c" * 40])

    def test_manifest_refuses_unknown_credential_field(self):
        data = manifest(rows(self.root)) + b"credential\tprivate-value\n"
        with self.assertRaises(ValueError): parse(data, COMMIT)

    def test_manifest_refuses_missing_duplicate_and_wrong_source(self):
        value = rows(self.root)
        for data in (manifest(value) + b"container_gib\t24\n", manifest({k: v for k, v in value.items() if k != "iso"})):
            with self.assertRaises(ValueError): parse(data, COMMIT)
        with self.assertRaises(ValueError): parse(manifest(value), "c" * 40)

    def test_manifest_refuses_non_development_and_unbounded_capacity(self):
        for key, value in (("classification", "release"), ("container_gib", "64"), ("container_gib", "15")):
            item = rows(self.root); item[key] = [value]
            with self.assertRaises(ValueError): parse(manifest(item), COMMIT)

    def test_capacity_accounts_for_whole_cap_and_overhead(self):
        exact = RESERVE + OVERHEAD + (24 << 30)
        self.assertEqual(capacity(exact, 24), 24 << 30)
        for free, gib in ((exact - 1, 24), (exact, True), (exact, 33)):
            with self.assertRaises(ValueError): capacity(free, gib)

    def test_held_descriptor_detects_replacement(self):
        path = self.root / "file"; path.write_bytes(b"abc")
        with FileSeal(path, 10) as seal:
            path.rename(self.root / "old"); path.write_bytes(b"abc")
            with self.assertRaises(ValueError): seal.check()

    def test_held_descriptor_detects_in_place_mutation(self):
        path = self.root / "file"; path.write_bytes(b"abc")
        with FileSeal(path, 10) as seal:
            path.write_bytes(b"abd")
            with self.assertRaises(ValueError): seal.check()

    def test_tree_refuses_links_and_detects_addition(self):
        root = self.root / "tree"; root.mkdir(); (root / "a").write_bytes(b"a")
        with TreeSeal(root) as seal:
            (root / "b").write_bytes(b"b")
            with self.assertRaises(ValueError): seal.check()
        (root / "link").symlink_to("a")
        with self.assertRaises(ValueError): TreeSeal(root)

    def test_tree_inventory_stops_at_the_entry_bound(self):
        root = self.root / "tree"; root.mkdir()
        for index in range(1024): (root / str(index)).touch()
        with self.assertRaises(ValueError): tree_paths(root)

    def test_tree_addition_between_inventory_and_hashing_is_refused(self):
        root = self.root / "tree"; root.mkdir(); (root / "a").write_bytes(b"sealed")
        def changed(*args):
            paths = tree_paths(*args)
            (root / "late").write_bytes(b"unsealed")
            return paths
        with patch("d11_fixture_files.tree_paths", side_effect=changed):
            with self.assertRaises(ValueError): TreeSeal(root)

    def test_file_refuses_hardlink_and_tree_refuses_regular_root(self):
        path = self.root / "file"; path.write_bytes(b"abc")
        with self.assertRaises(ValueError): TreeSeal(path)
        os.link(path, self.root / "alias")
        with self.assertRaises(ValueError): FileSeal(path, 10)

    def test_records_never_overwrite_and_json_rejects_duplicate(self):
        path = self.root / "record"
        record(path, {"state": "synthetic"})
        with self.assertRaises(FileExistsError): record(path, {})
        other = self.root / "duplicate"; other.write_text('{"k":1,"k":2}')
        with self.assertRaises(ValueError): document(other)

    def test_guest_facts_are_exact_and_nonce_bound(self):
        value = guest(); self.assertEqual(facts(json.dumps(value).encode(), "d" * 64, HASH), value)
        for patch in ({"nonce": "e" * 64}, {"account_bound": False}, {"autologon_remaining": 0},
                      {"autologon_remaining": True}, {"password": "must-not-publish"}):
            with self.assertRaises(ValueError): facts(json.dumps({**value, **patch}).encode(), "d" * 64, HASH)

    def test_relative_helper_dependency_is_refused(self):
        raw = b"helper:\n\t/usr/lib/libSystem.B.dylib (compatibility version 1.0.0, current version 1.0.0)\n"
        self.assertEqual(dependencies(raw), ["/usr/lib/libSystem.B.dylib"])
        self.assertTrue(system(dependencies(raw)[0]))
        with self.assertRaises(ValueError): dependencies(raw.replace(b"/usr/lib/", b"@rpath/"))
        self.assertFalse(system("/tmp/libSystem.B.dylib"))


if __name__ == "__main__": unittest.main()
