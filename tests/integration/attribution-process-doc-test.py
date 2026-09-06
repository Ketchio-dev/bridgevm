#!/usr/bin/env python3
"""The documentation exception must not exempt runtime or unrelated documents."""
import os
from pathlib import Path
import subprocess
import sys
import unittest

POLICY = Path(__file__).resolve().parents[2] / 'scripts/check-attribution-product-names.sh'


class ProcessDocumentPolicy(unittest.TestCase):
    def check(self, path, success):
        # Assemble the synthetic forbidden token; the fixture is not a product claim.
        token = 'U' + 'TM'
        environment = dict(os.environ, POLICY_FIXTURE=path + ':1:' + token)
        shell = 'git() { printf "%s\\n" "$POLICY_FIXTURE"; }; fail() { exit 9; }; source "$1"'
        result = subprocess.run(['bash', '-c', shell, 'test', str(POLICY)],
                                env=environment, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0 if success else 9, result.stderr)

    def test_explicit_process_comparison_allowed(self):
        self.check('docs/reference/upstream-development-practices.md', True)

    def test_runtime_stays_protected(self):
        self.check('crates/bridgevm-hvf/src/lib.rs', False)

    def test_other_docs_and_similar_names_stay_protected(self):
        for path in ['README.md', 'docs/README.md', 'docs/reference/unrelated.md',
                     'docs/reference/upstream-development-practices.md.extra']:
            self.check(path, False)

    def test_operator_records_remain_allowed(self):
        self.check('PLAN.md', True)


if __name__ == '__main__':
    unittest.main(argv=[sys.argv[0]])
