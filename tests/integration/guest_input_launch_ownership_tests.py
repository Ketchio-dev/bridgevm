"""Exercise the actual D5 finalizer after injected constructor uncertainty."""
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
                self.assertFalse(receipt["complete"])

    def test_d5_constructor_uncertainty_preserves_pair(self):
        self.uncertain("d5")

    def preflight(self, kind):
        with tempfile.TemporaryDirectory() as tmp:
            value = run_case(Path(tmp), kind, OSError("injected preflight refusal"), preflight=True)
            self.assertEqual(value["spawned"], 0)
            self.assertTrue(value["receipt"]["cleanup_complete"])

    def test_d5_preflight_without_launch_is_clean(self):
        self.preflight("d5")
