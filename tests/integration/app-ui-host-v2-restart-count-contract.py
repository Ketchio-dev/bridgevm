#!/usr/bin/env python3
"""The v2 driver completion records its AX walk restarts as an exact bounded integer.

The driver restarts a read-only AX pass after a transient AX status, at most
one restart per 25 ms of the 90 s session, and counts the restarts in
driver-completion.json `axRestarts`. The tier verifier requires that key with
the completion's other exact keys. Owned files only; no app is launched.
"""
import json
import unittest

from app_ui_host_v2_fixtures import Fixture, diagnostic


class RestartCountContracts(unittest.TestCase):
    def setUp(self):
        self.f = Fixture(self)
        _, self.completion = self.f.write_driver()
        self.path = self.f.private / "driver-completion.json"

    def verify(self, completion):
        self.path.write_text(json.dumps(completion))
        return diagnostic.verify_driver(self.f.private, self.f.launch)

    def test_fixture_completion_records_zero_restarts(self):
        self.assertIs(type(self.completion["axRestarts"]), int)
        self.assertEqual(self.completion["axRestarts"], 0)

    def test_every_count_within_the_session_bound_is_accepted(self):
        for count in (0, 1, 199, 3600):
            with self.subTest(count=count):
                self.verify(dict(self.completion, axRestarts=count))

    def test_counts_outside_the_bound_or_not_integers_are_refused(self):
        for count in (-1, 3601, 2**63, True, False, 1.0, "1", None, [1]):
            with self.subTest(count=count), self.assertRaises(ValueError):
                self.verify(dict(self.completion, axRestarts=count))

    def test_the_key_is_required_and_exact(self):
        missing = {key: value for key, value in self.completion.items() if key != "axRestarts"}
        for completion in (missing, dict(self.completion, restarts=0), dict(self.completion, axRestart=0)):
            with self.subTest(keys=sorted(completion)), self.assertRaises(ValueError):
                self.verify(completion)


if __name__ == "__main__":
    unittest.main()
