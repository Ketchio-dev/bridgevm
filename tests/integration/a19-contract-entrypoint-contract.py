#!/usr/bin/env python3
"""The actual A19 driver stops at either extracted receipt suite's failure."""
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
RECEIPT = "tests/integration/a19-lifecycle-campaign-receipt-contract.py"
DISPATCH = "tests/integration/a19-lifecycle-campaign-dispatch-contract.py"


class EntrypointFailure(unittest.TestCase):
    def driver(self, failed=""):
        script = (ROOT / "scripts/check-a19-contracts.sh").read_text()
        suites = re.findall(r"python3 (tests/integration/[a-z0-9-]+\.py)", script)
        self.assertIn(RECEIPT, suites)
        self.assertIn(DISPATCH, suites)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "scripts").mkdir()
            (root / "bin").mkdir()
            driver = root / "scripts/check-a19-contracts.sh"
            driver.write_text(script)
            trace = root / "trace"
            python = root / "bin/python3"
            python.write_text('#!/bin/bash\nprintf "%s\\n" "$1" >> "$A19_CONTRACT_TRACE"\n'
                              'if [[ "$1" == "$A19_CONTRACT_FAIL" ]]; then exit 23; fi\n')
            python.chmod(0o700)
            tail = root / "scripts/check-t22-pair-preparation.sh"
            tail.write_text('#!/bin/bash\nprintf "%s\\n" t22-tail >> "$A19_CONTRACT_TRACE"\n')
            tail.chmod(0o700)
            env = dict(os.environ, PATH=str(root / "bin") + os.pathsep + os.environ["PATH"],
                       A19_CONTRACT_TRACE=str(trace), A19_CONTRACT_FAIL=failed)
            result = subprocess.run(["/bin/bash", str(driver)], env=env, capture_output=True,
                                    text=True, timeout=10)
            observed = trace.read_text().splitlines()
        return result, observed, suites

    def fails_at(self, suite):
        result, observed, suites = self.driver(suite)
        self.assertEqual(result.returncode, 23, result.stderr)
        self.assertEqual(observed, suites[:suites.index(suite) + 1])

    def test_each_suite_failure_stops_the_driver(self):
        _, _, suites = self.driver()
        for suite in suites:
            with self.subTest(suite=suite):
                self.fails_at(suite)

    def test_success_reaches_every_suite_and_t22_tail(self):
        result, observed, suites = self.driver()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(observed, [*suites, "t22-tail"])


if __name__ == "__main__":
    unittest.main()
