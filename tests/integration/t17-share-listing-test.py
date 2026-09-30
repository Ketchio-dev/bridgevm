#!/usr/bin/env python3
"""Post-READY T17 share listing: links, caps, absence and per-entry errors."""
from __future__ import annotations

import hashlib
import importlib.util
import os
from pathlib import Path
import unittest
from unittest import mock

FIXTURE = Path(__file__).with_name("t17-post-ready-packet-test.py")
SPEC = importlib.util.spec_from_file_location("t17_post_ready_fixture_share", FIXTURE)
assert SPEC and SPEC.loader
post_ready = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(post_ready)
packet, PREFIX, LISTING, SHARE_SECRET = post_ready.packet, post_ready.PREFIX, post_ready.LISTING, post_ready.SHARE_SECRET


class ShareListingTest(post_ready.PostReadyCase):
    def test_share_listing_never_follows_links_or_hashes_beyond_caps(self) -> None:
        outside = self.case.output / "outside-secret.txt"
        outside.write_bytes(SHARE_SECRET)
        (self.share / "link.txt").symlink_to(outside)
        (self.share / "nested").mkdir()
        (self.share / "nested" / "inner.txt").write_bytes(SHARE_SECRET)
        os.mkfifo(self.share / "pipe")
        big = self.share / "big.bin"
        with big.open("wb") as output:
            output.truncate(packet.KIND.FILE_HASH_CAP + 1)
        os.link(self.share / "bv-product-e2e.ps1", self.case.lane / "second-name")
        packet.capture(self.args)
        packet.verify(self.args)
        entries = {entry["name"]: entry for entry in self.listing()["entries"]}
        self.assertEqual(set(entries), {"big.bin", "bv-product-e2e.ps1", "link.txt", "nested", "pipe", f"t17-{PREFIX}.txt"})
        self.assertEqual((entries["big.bin"]["type"], entries["big.bin"]["reason"], entries["big.bin"]["sha256"]),
                         ("file", "over-cap", None))
        self.assertEqual((entries["bv-product-e2e.ps1"]["reason"], entries["bv-product-e2e.ps1"]["sha256"]), ("unsafe", None))
        for name, kind in (("link.txt", "symlink"), ("nested", "directory"), ("pipe", "other")):
            self.assertEqual((entries[name]["type"], entries[name]["bytes"], entries[name]["sha256"], entries[name]["reason"]),
                             (kind, None, None, "not-regular"))
        self.assertNotIn(SHARE_SECRET, (self.case.private / LISTING).read_bytes())

    def test_entry_and_total_hash_caps_truncate_without_refusing(self) -> None:
        for number in range(4):
            (self.share / f"extra-{number}.txt").write_bytes(b"x" * 10)
        with mock.patch.object(packet.KIND, "ENTRY_CAP", 3), mock.patch.object(packet.KIND, "TOTAL_HASH_CAP", 25):
            packet.capture(self.args)
            packet.verify(self.args)
        listing = self.listing()
        self.assertEqual((listing["entry_count"], listing["truncated"], len(listing["entries"])), (6, True, 3))
        self.assertEqual([entry["reason"] for entry in listing["entries"]], ["over-cap", "none", "none"])
        self.assertEqual(listing["hashed_bytes"], 20)

    def test_absent_share_is_recorded_and_verified(self) -> None:
        for path in self.share.iterdir():
            path.unlink()
        self.share.rmdir()
        packet.capture(self.args)
        packet.verify(self.args)
        listing = self.listing()
        self.assertEqual((listing["status"], listing["reason"], listing["entries"], listing["entry_count"]), ("absent", "none", [], 0))

    def test_linked_share_is_recorded_unsafe_and_never_followed(self) -> None:
        outside = self.case.output / "outside-share"
        self.share.rename(outside)
        self.share.symlink_to(outside)
        packet.capture(self.args)
        packet.verify(self.args)
        listing = self.listing()
        self.assertEqual((listing["status"], listing["reason"], listing["entries"]), ("unsafe", "share-directory", []))
        self.assertNotIn(b"bv-product-e2e.ps1", (self.case.private / LISTING).read_bytes())

    def test_an_unreadable_entry_keeps_every_other_name_size_and_digest(self) -> None:
        unreadable = self.share / "zz-unreadable.txt"
        unreadable.write_bytes(SHARE_SECRET)
        unreadable.chmod(0)
        packet.capture(self.args)
        packet.verify(self.args)
        listing = self.listing()
        self.assertEqual((listing["status"], listing["reason"], listing["entry_count"]), ("listed", "none", 3))
        self.assertEqual(listing["entries"][-1], {"name": unreadable.name, "type": "file", "bytes": len(SHARE_SECRET),
                                                  "sha256": None, "reason": "error"})
        readable = sorted(path for path in self.share.iterdir() if path != unreadable)
        self.assertEqual([(entry["name"], entry["bytes"], entry["sha256"], entry["reason"]) for entry in listing["entries"][:-1]],
                         [(path.name, path.stat().st_size, hashlib.sha256(path.read_bytes()).hexdigest(), "none") for path in readable])
        self.assertEqual(listing["hashed_bytes"], sum(path.stat().st_size for path in readable))

    def test_entries_that_cannot_be_examined_are_still_listed_by_name(self) -> None:
        self.share.chmod(0o600)
        self.addCleanup(self.share.chmod, 0o700)
        packet.capture(self.args)
        packet.verify(self.args)
        listing = self.listing()
        self.assertEqual((listing["status"], listing["entry_count"], listing["hashed_bytes"]), ("listed", 2, 0))
        self.assertEqual(listing["entries"], [{"name": name, "type": "unknown", "bytes": None, "sha256": None, "reason": "error"}
                                              for name in ("bv-product-e2e.ps1", f"t17-{PREFIX}.txt")])

    def test_verifier_holds_error_entries_to_their_shape(self) -> None:
        base = {"schema_version": packet.KIND.SCHEMA, "status": "listed", "reason": "none", "entry_count": 1,
                "truncated": False, "hashed_bytes": 0}
        entry = {"name": "a", "type": "unknown", "bytes": None, "sha256": None, "reason": "error"}
        for good in (entry, {**entry, "type": "file", "bytes": 4}):
            packet.KIND.verify({**base, "entries": [good]})
        for bad in ({**entry, "bytes": 4}, {**entry, "reason": "not-regular"}, {**entry, "type": "directory"},
                    {**entry, "type": "file", "bytes": 4, "sha256": "0" * 64}, {**entry, "type": "file", "bytes": None}):
            with self.subTest(entry=bad), self.assertRaises(ValueError):
                packet.KIND.verify({**base, "entries": [bad]})


if __name__ == "__main__":
    unittest.main()
