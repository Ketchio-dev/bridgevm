#!/usr/bin/env python3
"""Synthetic inventory contracts, not guest Coherence evidence."""
import base64
import copy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts/live-gates"))
from coherence_inventory_observation import fixture, inventory, compare
from coherence_inventory_controller import CoherenceController
from guest_input_profile_dispatch import make_controller, sink_filename, observation_succeeded
from guest_input_profiles import COHERENCE_PROFILE, PROFILE, PRODUCTION_PROFILE, asset_names

NONCE = "0e63b275-1332-46ba-bdba-23db73ad2198"
COMMAND = "WINLIST 277bdd87-9955-4af4-bc5c-f4fe4305c559"
PREFIX = "BVAGENT " + COMMAND + " "
FIXTURE = {"schema": "bridgevm.coherence-fixture.v1", "nonce": NONCE, "pid": 123,
           "windows": [{"id": str(100 + i), "title": "BridgeVM Coherence " + NONCE + " " + str(i)} for i in range(2)]}


def row(index):
    window = FIXTURE["windows"][index]
    return PREFIX + "WIN " + window["id"] + " 123 20 60 300 180 " + base64.b64encode(window["title"].encode()).decode()


class Observation(unittest.TestCase):
    def test_counts_are_not_promoted(self):
        expected = fixture(copy.deepcopy(FIXTURE), NONCE)
        for count in (0, 1, 2):
            response = inventory([row(i) for i in range(count)] + [PREFIX + "WINEND\r"], COMMAND)
            result = compare(expected, response)
            self.assertEqual(result["observed_windows"], count)
            self.assertEqual(result["all_fixture_windows_observed"], count == 2)
            self.assertTrue(result["collection_complete"])
            self.assertFalse(result["claim_eligible"])
            self.assertTrue(observation_succeeded(COHERENCE_PROFILE, {"coherence": result}))
        self.assertIsNone(inventory([row(0)], COMMAND))
        self.assertIsNone(inventory([row(0), PREFIX + "WINEND"], "WINLIST wrong"))

    def test_exact_pid_title_and_nonce(self):
        for key, value in (("pid", 124), ("title", "wrong"), ("id", "999")):
            response = inventory([row(0), PREFIX + "WINEND"], COMMAND)
            response["rows"][0][key] = value
            self.assertEqual(compare(FIXTURE, response)["observed_windows"], 0)
        for key, value in (("pid", True), ("nonce", "wrong"), ("windows", [])):
            bad = copy.deepcopy(FIXTURE); bad[key] = value
            with self.assertRaises(ValueError): fixture(bad, NONCE)
        for handle in ("0100", "-1", "0", str(2**64), "abc", 100):
            bad = copy.deepcopy(FIXTURE); bad["windows"][0]["id"] = handle
            with self.assertRaises(ValueError): fixture(bad, NONCE)
        bad = copy.deepcopy(FIXTURE); bad["windows"][1]["id"] = "100"
        with self.assertRaises(ValueError): fixture(bad, NONCE)

    def test_malformed_and_restart(self):
        for lines in ([row(0), row(0)], [PREFIX + "ERR unavailable"],
                      [row(0).rsplit(" ", 1)[0] + " !!!!"], [PREFIX + "WIN 1 2 0 0 0 1 QQ=="]):
            with self.assertRaises(ValueError): inventory(lines + [PREFIX + "WINEND"], COMMAND)
        for marker in ("BVAGENT READY", "BVAGENT re-READY", "BVAGENT SERVICE start", "PSCI_SYSTEM_RESET", "PSCI SYSTEM_RESET:"):
            for position in range(3):
                lines = [row(0), PREFIX + "WINEND"]
                lines.insert(position, marker)
                with self.assertRaises(ValueError): inventory(lines, COMMAND)

    def test_profile_dispatch(self):
        paths = {name: Path(name) for name in asset_names(COHERENCE_PROFILE)}
        self.assertIsInstance(make_controller("ctl", "log", "share", paths, COHERENCE_PROFILE), CoherenceController)
        self.assertEqual(sink_filename(COHERENCE_PROFILE), "bv-coherence-multiwindow.ps1")
        self.assertEqual(sink_filename(PROFILE), "bv-input-order-sink.ps1")
        self.assertFalse(observation_succeeded(COHERENCE_PROFILE, {}))
        self.assertFalse(observation_succeeded(PRODUCTION_PROFILE, {"guest_application_observed": True, "production_driver_observed": False}))
        with self.assertRaises(ValueError): make_controller("ctl", "log", "share", dict(paths, driver=Path("driver")), COHERENCE_PROFILE)
        with self.assertRaises(ValueError): sink_filename("unknown")


if __name__ == "__main__":
    unittest.main()
