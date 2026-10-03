"""Exercise actual D5/B9 finalizers after injected constructor uncertainty."""
from pathlib import Path
import tempfile
import unittest

from guest_input_launch_fixture import run_case


class LaunchOwnershipCleanup(unittest.TestCase):
    def uncertain(self, kind):
        for failure in (OSError("injected constructor failure"), KeyboardInterrupt()):
            with self.subTest(kind=kind, failure=type(failure).__name__), tempfile.TemporaryDirectory() as tmp:
                value = run_case(Path(tmp), kind, failure)
                receipt = value["receipt"]
                self.assertEqual(value["spawned"], 1)
                self.assertTrue(value["owned_child_live"])
                self.assertTrue(value["work_retained"])
                self.assertFalse(receipt["cleanup_complete"])
                self.assertFalse(receipt["source_integrity"])
                self.assertNotIn("output_hashes", receipt)
                self.assertNotIn("clone_final_sha256", receipt)
                self.assertEqual(value["clone_modes"], value["before_modes"])
                self.assertEqual(value["hashes_after_spawn"], [])
                if kind == "d5": self.assertFalse(receipt["complete"])
                else: self.assertFalse(receipt["owned_process_group_stopped"])

    def test_d5_constructor_uncertainty_preserves_pair(self):
        self.uncertain("d5")

    def test_b9_constructor_uncertainty_preserves_pair(self):
        self.uncertain("b9")

    def preflight(self, kind):
        with tempfile.TemporaryDirectory() as tmp:
            value = run_case(Path(tmp), kind, OSError("injected preflight refusal"), preflight=True)
            self.assertEqual(value["spawned"], 0)
            self.assertTrue(value["receipt"]["cleanup_complete"])

    def test_d5_preflight_without_launch_is_clean(self):
        self.preflight("d5")

    def test_b9_preflight_without_launch_is_clean(self):
        self.preflight("b9")

    def test_b9_existing_work_is_not_acquired_or_cleaned(self):
        with tempfile.TemporaryDirectory() as tmp:
            value = run_case(Path(tmp), "b9", OSError("unused"), existing_work=True)
            self.assertEqual(value["spawned"], 0)
            self.assertTrue(value["prior_lane_unchanged"])
            self.assertFalse(value["receipt"]["cleanup_complete"])
            self.assertEqual(value["copy_raw_calls"], 0)
            self.assertEqual(value["verify_calls"], 0)
