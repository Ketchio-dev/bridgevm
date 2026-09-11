#!/usr/bin/env python3
"""Typed hash labels must not excuse unrelated or invented Git references."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts"))
from doc_code_identity import is_cdhash_reference

HASH = "3a7a18b8bb0bc68ef34ba83460f8ae9e80ce1391"


class CodeIdentityTests(unittest.TestCase):
    def typed(self, text: str, occurrence: int = 0) -> bool:
        lines = text.splitlines()
        matches = [(n, line.index(HASH)) for n, line in enumerate(lines, 1) if HASH in line]
        number, offset = matches[occurrence]
        return is_cdhash_reference(lines, number, offset)

    def test_inline_label(self):
        self.assertTrue(self.typed(f"on-disk CDHash: `{HASH}`"))

    def test_wrapped_label(self):
        self.assertTrue(self.typed(f"on-disk code CDHash\n`{HASH}`, recorded"))

    def test_lowercase_assignment(self):
        self.assertTrue(self.typed(f"cdhash = {HASH}"))

    def test_git_commit(self):
        self.assertFalse(self.typed(f"commit `{HASH}`"))

    def test_nearby_label_is_not_an_exemption(self):
        self.assertFalse(self.typed(f"CDHash metadata recorded\ncommit `{HASH}`"))

    def test_intervening_line(self):
        self.assertFalse(self.typed(f"CDHash\nother identity\n`{HASH}`"))

    def test_distinct_commit_after_typed_hash(self):
        self.assertFalse(self.typed(f"CDHash `{HASH}`\ncommit `{HASH}`", 1))

    def test_not_a_label_substring(self):
        self.assertFalse(self.typed(f"notCDHash `{HASH}`"))

    def test_wrapped_label_does_not_type_commit_prose(self):
        self.assertFalse(self.typed(f"CDHash\ncommit `{HASH}`"))


if __name__ == "__main__":
    unittest.main()
