#!/usr/bin/env python3
"""An added T22 case counts only with content hashes checked like the first case."""
from __future__ import annotations

import unittest

from a19_interrupt_case_fixtures import (AGREEING, COMMIT, SHA_A, SHA_C, SHA_D, SHA_E, SHA_F,
                                         flagged, passing, proven, receipt)
import a19_interrupt_case_hashes as hashes

BOTH = {**proven("swap_", SHA_C), **proven("create_", SHA_D), "interruption_case_count": 3}
FIELDS = {prefix + field for prefix, values in AGREEING.items() for field in ("stop_fd_log_sha256", *values)}
# Only a destination that did not exist before the create case may stay absent.
MAY_BE_ABSENT = {"create_preinterrupt_manifest_sha256", "create_postkill_manifest_sha256"}


def refused(test: unittest.TestCase, change: dict, base: dict = BOTH) -> None:
    with test.subTest(change=change), test.assertRaises(ValueError):
        receipt.validate(passing(**{**base, **change}), COMMIT)


class CaseHashContract(unittest.TestCase):
    def test_flags_and_a_stop_log_alone_do_not_count_a_case(self):
        refused(self, {**flagged("swap_", SHA_C), **flagged("create_", SHA_D), "interruption_case_count": 3}, {})
        refused(self, {**flagged("swap_", SHA_C), "interruption_case_count": 2}, {})
        refused(self, {**flagged("create_", SHA_D), "interruption_case_count": 2}, {})
        self.assertTrue(receipt.validate(passing(**BOTH), COMMIT)["pass"])
        for prefix, log in (("swap_", SHA_C), ("create_", SHA_D)):
            one = passing(**proven(prefix, log), interruption_case_count=2)
            self.assertTrue(receipt.validate(one, COMMIT)["pass"])

    def test_every_case_hash_must_be_present_and_well_formed(self):
        self.assertEqual(hashes.PROOF_FIELDS, FIELDS)
        for field in sorted(FIELDS):
            if field not in MAY_BE_ABSENT:
                refused(self, {field: "absent"})
            for bad in (5, None, "A" * 64, SHA_A[:-1], "../disk.raw"):
                refused(self, {field: bad})

    def test_the_kill_leaves_the_old_state_and_the_retry_replaces_it(self):
        # Swap: the old generation stays selected after the kill, then the snapshot is.
        for change in ({"swap_postkill_disk_sha256": SHA_A}, {"swap_postkill_vars_sha256": SHA_F},
                       {"swap_postretry_disk_sha256": SHA_E}, {"swap_postretry_vars_sha256": SHA_E},
                       {"swap_preinterrupt_disk_sha256": SHA_A, "swap_postkill_disk_sha256": SHA_A}):
            refused(self, change)
        # Create: the destination stays as it was, then holds a copy of the source pair.
        for change in ({"create_postkill_manifest_sha256": SHA_E},
                       {"create_preinterrupt_manifest_sha256": SHA_D},
                       {"create_preinterrupt_manifest_sha256": SHA_E, "create_postkill_manifest_sha256": SHA_E},
                       {"create_postretry_disk_sha256": SHA_A}, {"create_postretry_vars_sha256": SHA_A}):
            refused(self, change)
        previous = passing(**{**BOTH, "create_preinterrupt_manifest_sha256": SHA_D,
                              "create_postkill_manifest_sha256": SHA_D})
        self.assertTrue(receipt.validate(previous, COMMIT)["pass"])

    def test_an_absent_case_carries_no_hash_and_a_failed_one_counts_nothing(self):
        for field in sorted(FIELDS):
            refused(self, {field: SHA_E}, {})
        # A kill that swapped, or a hash never taken, leaves a failed receipt that counts nothing.
        failed = passing(**{**proven("swap_", SHA_C), "swap_postkill_disk_sha256": SHA_A, "pass": False,
                            "outcome": "failed", "interruption_case_count": 0, "sample_count": 0, "run_count": 0})
        self.assertFalse(receipt.validate(failed, COMMIT)["pass"])
        self.assertFalse(receipt.validate({**failed, "swap_retry_result_sha256": "absent"}, COMMIT)["pass"])
        with self.assertRaises(ValueError):
            receipt.validate({**failed, "interruption_case_count": 2}, COMMIT)


if __name__ == "__main__":
    unittest.main()
