#!/usr/bin/env python3
"""Authenticate retained bytes, not stale logical media or vTPM inputs."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from t22_pair_fixtures import PairFixture, write_json
from t22_pair_provenance import admit, unchanged


class Provenance(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="bridgevm-t22-provenance-")
        self.root = Path(self.temp.name).resolve()

    def tearDown(self):
        for path in self.root.rglob("*"):
            if not path.is_symlink(): path.chmod(0o700 if path.is_dir() else 0o600)
        self.temp.cleanup()

    def test_both_authenticated_origins_without_key_reads(self):
        real_open = os.open
        def guarded(path, *args, **kwargs):
            self.assertNotIn("never-open-keys", str(path))
            return real_open(path, *args, **kwargs)
        for tier in ("T17", "T19"):
            with self.subTest(tier=tier):
                directory = self.root / tier; directory.mkdir()
                fixture = PairFixture(directory, tier)
                with patch("os.open", side_effect=guarded):
                    rows, data, docs = admit(*fixture.args())
                    unchanged(fixture.manifest, data, docs, rows)

    def test_request_result_and_stamp_pins_refuse_mutation(self):
        fixture = PairFixture(self.root)
        for path in (fixture.request_path, fixture.result_path, fixture.stamp_path, fixture.pair_path):
            original = path.read_bytes(); mode = path.stat().st_mode & 0o777
            path.chmod(0o600); path.write_bytes(original + b" "); path.chmod(mode)
            with self.subTest(name=path.name), self.assertRaises(ValueError): admit(*fixture.args())
            path.chmod(0o600); path.write_bytes(original); path.chmod(mode)

    def test_result_identity_and_incomplete_stage_refuse(self):
        fixture = PairFixture(self.root)
        for key, value in (("job_id", "other"), ("nonce", "f" * 64), ("cleanup_verified", False),
                           ("first_ready", False), ("three_d_injection", True), ("lane", True)):
            if key not in fixture.result: continue
            original = fixture.result[key]; fixture.result[key] = value
            fixture.stamp["result_sha256"] = write_json(fixture.result_path, fixture.result)
            write_json(fixture.stamp_path, fixture.stamp); fixture.refresh()
            with self.subTest(key=key), self.assertRaises(ValueError): admit(*fixture.args())
            fixture.result[key] = original

    def test_stamp_must_bind_same_request_even_when_result_matches(self):
        fixture = PairFixture(self.root)
        fixture.stamp["request_sha256"] = "0" * 64
        write_json(fixture.stamp_path, fixture.stamp); fixture.refresh()
        with self.assertRaisesRegex(ValueError, "request"): admit(*fixture.args())

    def test_stale_logical_path_refuses_and_selected_path_admits(self):
        fixture = PairFixture(self.root)
        rows, data, docs = admit(*fixture.args())
        stale = self.root / "logical.raw"; stale.write_bytes(b"stale generation"); stale.chmod(0o400)
        fixture.rows["image"][0] = str(stale)
        altered = "".join("\t".join((k, *v)) + "\n" for k, v in fixture.rows.items()).encode()
        import hashlib
        fixture.manifest.write_bytes(altered)
        with self.assertRaises(ValueError):
            admit(fixture.manifest, hashlib.sha256(altered).hexdigest(), fixture.origin_path, fixture.origin_hash, fixture.rows["source_commit"][0])
        fixture.manifest.write_bytes(data)
        unchanged(fixture.manifest, data, docs, rows)

    def test_selected_metadata_types_and_unknown_fields_refuse(self):
        fixture = PairFixture(self.root)
        original = dict(fixture.pair)
        for key, value in (("format_version", True), ("vars_bytes", True), ("vm_id", "other"), ("extra", 0)):
            fixture.pair = {**original, key: value}
            write_json(fixture.pair_path, fixture.pair); fixture.refresh()
            with self.subTest(key=key), self.assertRaises(ValueError): admit(*fixture.args())

    def test_duplicate_json_and_writable_or_symlink_pair_refuse(self):
        fixture = PairFixture(self.root)
        raw = fixture.origin_path.read_bytes()
        fixture.origin_path.write_bytes(raw[:-1] + b',"tier":"T17"}')
        import hashlib
        with self.assertRaises(ValueError):
            admit(fixture.manifest, fixture.manifest_hash, fixture.origin_path,
                  hashlib.sha256(fixture.origin_path.read_bytes()).hexdigest(), fixture.rows["source_commit"][0])
        fixture.origin_path.write_bytes(raw)
        fixture.disk.chmod(0o600)
        with self.assertRaises(ValueError): admit(*fixture.args())
        fixture.disk.chmod(0o400)
        fixture.selected.chmod(0o700)
        saved = fixture.disk.with_name("saved.raw"); fixture.disk.rename(saved); fixture.disk.symlink_to(saved)
        with self.assertRaises(ValueError): admit(*fixture.args())

    def test_captured_documents_and_manifest_are_rechecked(self):
        fixture = PairFixture(self.root)
        rows, data, docs = admit(*fixture.args())
        for path in (fixture.request_path, fixture.origin_path, fixture.manifest):
            raw = path.read_bytes(); path.write_bytes(raw + b" ")
            with self.subTest(name=path.name), self.assertRaises(ValueError): unchanged(fixture.manifest, data, docs, rows)
            path.write_bytes(raw)

    def test_retained_layout_and_actual_vars_geometry_refuse(self):
        fixture = PairFixture(self.root)
        fixture.selected.chmod(0o700)
        extra = fixture.selected / "foreign-extra.txt"; extra.write_text("keep")
        fixture.selected.chmod(0o500)
        with self.assertRaisesRegex(ValueError, "layout"): admit(*fixture.args())
        self.assertEqual(extra.read_text(), "keep")
        fixture.selected.chmod(0o700); extra.unlink(); fixture.selected.chmod(0o500)
        fixture.vars.chmod(0o600)
        with fixture.vars.open("r+b") as stream: stream.truncate(32 << 20)
        fixture.vars.chmod(0o400)
        from t22_pair_provenance import file_hash
        fixture.result["final_vars_sha256"] = fixture.pair["vars_sha256"] = file_hash(fixture.vars)
        fixture.stamp["result_sha256"] = write_json(fixture.result_path, fixture.result)
        write_json(fixture.stamp_path, fixture.stamp); write_json(fixture.pair_path, fixture.pair); fixture.refresh()
        with self.assertRaisesRegex(ValueError, "geometry"): admit(*fixture.args())


if __name__ == "__main__": unittest.main()
