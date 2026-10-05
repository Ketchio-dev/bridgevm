#!/usr/bin/env python3
"""ATTACH type and current INFO ownership must identify the same APFS mount."""
import copy
import unittest

from d11_fixture_mount_test_support import MountFixture


class MountIdentity(unittest.TestCase):
    def fixture(self):
        item = MountFixture()
        self.addCleanup(item.close)
        return item

    def test_observed_info_without_type_uses_explicit_attach_apfs(self):
        item = self.fixture()
        item.invoke()
        self.assertIsNotNone(item.box.mounted_identity)
        self.assertEqual(item.processes.run.call_count, 2)

    def test_matching_explicit_types_are_accepted(self):
        item = self.fixture()
        item.allow_old_info_type()
        item.invoke()
        self.assertIsNotNone(item.box.mounted_identity)

    def test_attach_type_mount_and_device_are_not_optional(self):
        for field, value in (("volume-kind", "hfs"), ("volume-kind", False),
                             ("mount-point", "/synthetic/foreign"),
                             ("dev-entry", "/dev/disk82s1"), ("dev-entry", "/dev/disk81"),
                             ("dev-entry", "/dev/../dev/disk81s1")):
            with self.subTest(field=field, value=value):
                item = self.fixture()
                item.allow_old_info_type()
                item.attach_leaf()[field] = value
                with self.assertRaises(ValueError): item.invoke()
                self.assertIsNone(item.box.mounted_identity)

    def test_missing_attach_type_is_rejected(self):
        item = self.fixture()
        item.allow_old_info_type()
        del item.attach_leaf()["volume-kind"]
        with self.assertRaises(ValueError): item.invoke()

    def test_whole_device_and_info_leaf_must_match_attachment(self):
        for mode in ("whole-device", "leaf-device", "backing", "mount", "type"):
            with self.subTest(mode=mode):
                item = self.fixture()
                item.allow_old_info_type()
                if mode == "whole-device": item.info["system-entities"][0]["dev-entry"] = "/dev/disk82"
                elif mode == "leaf-device": item.info_leaf()["dev-entry"] = "/dev/disk82s1"
                elif mode == "backing": item.info["image-path"] = str(item.output / "foreign.sparseimage")
                elif mode == "mount": item.info_leaf()["mount-point"] += "/../mount"
                else: item.info_leaf()["volume-kind"] = "hfs"
                with self.assertRaises(ValueError): item.invoke()

    def test_duplicate_mount_or_leaf_is_rejected_on_both_surfaces(self):
        for surface in ("attach", "info"):
            for duplicate in ("mount", "leaf"):
                with self.subTest(surface=surface, duplicate=duplicate):
                    item = self.fixture()
                    item.allow_old_info_type()
                    rows = getattr(item, surface)["system-entities"]
                    extra = copy.deepcopy(rows[-1])
                    if duplicate == "leaf": extra["mount-point"] = "/synthetic/other"
                    rows.append(extra)
                    with self.assertRaises(ValueError): item.invoke()

    def test_malformed_alias_or_oversized_attach_plist_is_rejected(self):
        for mode in ("malformed", "not-dictionary", "not-entities", "non-dict-entity", "alias", "oversized"):
            with self.subTest(mode=mode):
                item = self.fixture()
                item.allow_old_info_type()
                if mode == "malformed": item.attach_data = b"not a plist"
                elif mode == "not-dictionary": item.attach = []
                elif mode == "not-entities": item.attach["system-entities"] = "wrong"
                elif mode == "non-dict-entity": item.attach["system-entities"].append("wrong")
                elif mode == "alias": item.alias_plist = True
                else: item.attach_data = b"x" * 65537
                with self.assertRaises((ValueError, OSError)): item.invoke()

    def test_same_filesystem_or_excess_capacity_is_still_refused(self):
        for mode in ("same-device", "capacity", "zero-capacity"):
            with self.subTest(mode=mode):
                item = self.fixture()
                item.allow_old_info_type()
                if mode == "same-device": item.distinct_device = False
                else: item.capacity = (22 << 30) + 4096 if mode == "capacity" else 0
                with self.assertRaises(ValueError): item.invoke()
                self.assertIsNone(item.box.mounted_identity)


if __name__ == "__main__": unittest.main()
