#!/usr/bin/env python3
"""Declared swap and create stop points and separately counted T22 cases."""
from __future__ import annotations

from importlib.util import module_from_spec, spec_from_file_location
import json
from pathlib import Path
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
import a19_interrupt_cases as cases
import a19_interrupt_restore_child as observer
import a19_interrupt_stop_points as points
import a19_interrupted_restore_receipt as receipt
from a19_interrupt_case_fixtures import COMMIT, SHA_A, SHA_B, SHA_C, SHA_D, passing, proven
from a19_interrupt_stop_fixtures import HOLD_READ, helper, fixture
from a19_interrupt_stop_transitions import StopTransitionContract

spec = spec_from_file_location("redact_receipt", ROOT / "scripts/live-gates/redact-receipt.py")
redaction = module_from_spec(spec)
spec.loader.exec_module(redaction)
FROZEN_V1 = ROOT / "docs/windows-arm/evidence/a19-t22-r1-20260925-receipt.json"


class StopPointContract(unittest.TestCase):
    def setUp(self):
        if not Path(observer.LSOF).is_file():
            if sys.platform == "darwin":
                self.fail("macOS T22 test needs owner-readable lsof")
            self.skipTest("lsof is only required on the physical Mac and hosted macOS")

    def observe(self, helper_path, snapshot, disk, vars, output, deadline, point) -> dict:
        return observer.run(helper_path, snapshot, disk, vars, output, deadline, point.name)

    def test_undeclared_or_unmet_points_refuse_before_launch(self):
        with tempfile.TemporaryDirectory() as temporary:
            disk, vars, snapshot, output, managed = fixture(Path(temporary))
            path = helper(Path(temporary) / "helper.py", "raise SystemExit(9)\n", ())
            for point, error in (("rename-point", "not declared"), (points.SWAP_RESTORE.name, "declared old")):
                with self.subTest(point=point), self.assertRaisesRegex(ValueError, error):
                    observer.run(path, snapshot, disk, vars, output, 1, point)
            (managed / "current").mkdir(parents=True, mode=0o700)
            with self.assertRaisesRegex(ValueError, "declared old"):
                observer.run(path, snapshot, disk, vars, output, 1)
            self.assertFalse((output / "interrupt-helper.stdout").exists())

    def test_swap_point_stops_while_the_selected_generation_is_unchanged(self):
        with tempfile.TemporaryDirectory() as temporary:
            disk, vars, snapshot, output, managed = fixture(Path(temporary), generation=True)
            before = points.selection(points.SWAP_RESTORE, managed, snapshot)
            prelude = f"stage=pathlib.Path({str(managed)!r})/'staging'\n"
            names = ("disk.raw", "vars.fd", "manifest.json")
            path = helper(Path(temporary) / "helper.py", prelude, names)
            result = self.observe(path, snapshot, disk, vars, output, 5, points.SWAP_RESTORE)
            self.assertEqual(result["interruption_stage"], points.SWAP_RESTORE.name)
            self.assertTrue(result["helper_killed_and_reaped"])
            self.assertEqual(points.selection(points.SWAP_RESTORE, managed, snapshot), before)
            self.assertEqual((output / "interrupt-helper-fd.private.log").stat().st_mode & 0o777, 0o600)
        with tempfile.TemporaryDirectory() as temporary:
            disk, vars, snapshot, output, managed = fixture(Path(temporary), generation=True)
            prelude = f"stage=pathlib.Path({str(managed)!r})/'staging'\n"
            swapped = (f"current=pathlib.Path({str(managed)!r})/'current'\n"
                       "(current/'next').write_bytes(b'new'); os.replace(current/'next', current/'disk.raw')\n")
            path = helper(Path(temporary) / "helper.py", prelude, names, swapped)
            with self.assertRaises(TimeoutError):
                self.observe(path, snapshot, disk, vars, output, 3, points.SWAP_RESTORE)
            self.assertTrue(Path(f"{path}.held").exists(), "helper never reached its hold")
            self.assertFalse((output / "interrupt-helper-fd.private.log").exists())

    def test_create_point_stops_before_the_manifest_with_the_destination_unchanged(self):
        def argv_check(quota: int) -> str:
            return ("dest=pathlib.Path(sys.argv[4]); stage=dest.parent/('.'+dest.name+'.staging')\n"
                    f"if sys.argv[1]!='create' or sys.argv[5]!='{points.CREATE_VM_ID}' or sys.argv[6]!='{quota}':\n"
                    "    raise SystemExit(3)\n")
        # The quota is the selected pair: 8+8 original bytes, or 10+10 generation bytes.
        for previous, generation, quota in ((False, False, 16), (True, False, 16), (False, True, 20)):
            with self.subTest(previous=previous, generation=generation), \
                    tempfile.TemporaryDirectory() as temporary:
                disk, vars, snapshot, output, managed = fixture(Path(temporary), generation)
                destination = Path(temporary) / "export"
                if previous:
                    destination.mkdir()
                    for name in ("disk.raw", "vars.fd", "manifest.json"):
                        (destination / name).write_bytes(b"previous")
                before = points.selection(points.CREATE_EXPORT, managed, destination)
                path = helper(Path(temporary) / "helper.py", argv_check(quota), ("disk.raw", "vars.fd"))
                result = self.observe(path, destination, disk, vars, output, 5, points.CREATE_EXPORT)
                self.assertEqual(result["interruption_stage"], points.CREATE_EXPORT.name)
                self.assertEqual(points.selection(points.CREATE_EXPORT, managed, destination), before)
        replaced = ("dest.mkdir(exist_ok=True); (dest/'next').write_bytes(b'new')\n"
                    "os.replace(dest/'next', dest/'manifest.json')\n")
        # A manifest-first control never transiently satisfies create readiness.
        missed = ((("manifest.json", "disk.raw", "vars.fd"), "", HOLD_READ),
                  (("disk.raw", "vars.fd"), "", HOLD_READ.replace("'rb'", "'ab'")),
                  (("disk.raw", "vars.fd"), replaced, HOLD_READ))
        for names, before, hold in missed:
            with self.subTest(names=names, hold=hold[:40]), tempfile.TemporaryDirectory() as temporary:
                disk, vars, snapshot, output, managed = fixture(Path(temporary))
                path = helper(Path(temporary) / "helper.py", argv_check(16), names, before, hold)
                with self.assertRaises(TimeoutError):
                    self.observe(path, Path(temporary) / "export", disk, vars, output, 3, points.CREATE_EXPORT)
                self.assertTrue(Path(f"{path}.held").exists(), "helper never reached its hold")
                self.assertFalse((output / "interrupt-helper-fd.private.log").exists())


class CaseReceiptContract(unittest.TestCase):
    def test_frozen_schema_1_receipt_still_validates_as_one_case(self):
        value = receipt.load_receipt(FROZEN_V1)
        self.assertEqual(receipt.validate(value)["schema_version"], 1)
        self.assertEqual(cases.case_count(value), 1)
        with self.assertRaises(ValueError):
            receipt.validate({**value, **cases.absent_cases()})

    def test_each_added_case_counts_once_and_needs_its_own_proof(self):
        self.assertTrue(receipt.validate(passing(), COMMIT)["pass"])
        both = passing(**proven("swap_", SHA_C), **proven("create_", SHA_D), interruption_case_count=3)
        self.assertEqual(cases.case_count(receipt.validate(both, COMMIT)), 3)
        swap = passing(**proven("swap_", SHA_C), interruption_case_count=2)
        self.assertEqual(cases.case_count(receipt.validate(swap, COMMIT)), 2)
        for count in (0, 1, 2, 4):
            with self.subTest(count=count), self.assertRaises(ValueError):
                receipt.validate({**both, "interruption_case_count": count}, COMMIT)
        wrong = [{"swap_" + flag: False} for flag in cases.CASE_FLAGS]
        wrong += [{"swap_stop_fd_log_sha256": "absent"}, {"swap_stop_fd_log_sha256": SHA_A},
                  {"swap_stop_fd_log_sha256": SHA_D}, {"swap_interruption_stage": points.CREATE_EXPORT.name},
                  {"create_interruption_stage": "create-after-publication"}, {"swap_retry_succeeded": 1}]
        for change in wrong:
            with self.subTest(change=change), self.assertRaises(ValueError):
                receipt.validate({**both, **change}, COMMIT)

    def test_absent_cases_cannot_carry_proof_and_failures_count_nothing(self):
        for change in ({"create_helper_stop_verified": True}, {"swap_stop_fd_log_sha256": SHA_C}):
            with self.subTest(change=change), self.assertRaises(ValueError):
                receipt.validate(passing(**change), COMMIT)
        failed = receipt.initial("cases-fixture", COMMIT)
        failed.update(schema_version=2, finished_at=failed["started_at"], **cases.absent_cases())
        self.assertFalse(receipt.validate(failed, COMMIT)["pass"])
        stopped = {**failed, "swap_interruption_stage": points.SWAP_RESTORE.name,
                   "swap_stop_fd_log_sha256": SHA_C, "swap_helper_stop_verified": True}
        self.assertFalse(receipt.validate(stopped, COMMIT)["pass"])
        for change in ({"interruption_case_count": 1}, {"sample_count": 1}):
            with self.subTest(change=change), self.assertRaises(ValueError):
                receipt.validate({**stopped, **change}, COMMIT)

    def test_schema_2_keeps_every_schema_1_pass_condition_and_promotion_ban(self):
        both = passing(**proven("swap_", SHA_C), **proven("create_", SHA_D), interruption_case_count=3)
        changes = [{field: True} for field in ("claim_eligible", "criterion_pass",
                                               "capability_promotion", "three_d_injection")]
        changes += [{"interruption_stage": "absent"}, {"interruption_stage": points.SWAP_RESTORE.name},
                    {"boots_passed": 3, "natural_shutdown_count": 3}, {"sample_count": 3}, {"run_count": 3},
                    {"postkill_vars_sha256": SHA_B}, {"helper_stop_verified": False},
                    {"worker_cleanup_verified": False}, {"schema_version": 3}]
        for change in changes:
            with self.subTest(change=change), self.assertRaises(ValueError):
                receipt.validate({**both, **change}, COMMIT)
        missing = dict(both)
        del missing["create_retry_succeeded"]
        with self.assertRaises(ValueError):
            receipt.validate(missing, COMMIT)

    def test_schema_2_receipt_survives_the_flat_publication_allowlist(self):
        both = receipt.validate(passing(**proven("swap_", SHA_C), **proven("create_", SHA_D),
                                        interruption_case_count=3), COMMIT)
        public = redaction.redact(json.loads(json.dumps(both)))
        self.assertEqual(public, both)
        self.assertTrue(cases.ADDED_FIELDS <= redaction.ALLOWED_FIELDS)


if __name__ == "__main__":
    unittest.main()
