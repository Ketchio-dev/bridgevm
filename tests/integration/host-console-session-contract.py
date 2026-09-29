#!/usr/bin/env python3
"""Console-session admission for app-UI tiers fails closed on every unproven state."""
from __future__ import annotations

import importlib.util
import plistlib
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("host_console_session", ROOT / "scripts/live-gates/host_console_session.py")
session = importlib.util.module_from_spec(spec)
spec.loader.exec_module(session)
UID = 501


def ioreg(*users: dict) -> bytes:
    return plistlib.dumps([{"IOConsoleUsers": list(users), "IOProviderClass": "IOResources"}], fmt=plistlib.FMT_XML)


def user(**overrides) -> dict:
    value = {"kCGSSessionUserIDKey": UID, "kCGSSessionOnConsoleKey": True, "kCGSessionLoginDoneKey": True}
    value.update(overrides)
    return value


class ConsoleSessionTests(unittest.TestCase):
    def refuses(self, data: bytes, fragment: str) -> None:
        with self.assertRaises(session.SessionError) as caught:
            session.console_state(data, UID)
        self.assertIn(fragment, str(caught.exception))

    def test_unlocked_on_console_session_is_admitted(self) -> None:
        self.assertEqual(session.console_state(ioreg(user(), user(kCGSSessionUserIDKey=502)), UID), "unlocked")

    def test_explicitly_unlocked_session_is_admitted(self) -> None:
        self.assertEqual(session.console_state(ioreg(user(CGSSessionScreenIsLocked=False)), UID), "unlocked")

    def test_locked_session_is_refused(self) -> None:
        self.refuses(ioreg(user(CGSSessionScreenIsLocked=True)), "screen is locked")

    def test_non_boolean_lock_state_is_refused(self) -> None:
        self.refuses(ioreg(user(CGSSessionScreenIsLocked=1)), "screen is locked")

    def test_switched_out_session_is_refused(self) -> None:
        self.refuses(ioreg(user(kCGSSessionOnConsoleKey=False)), "not on the console")

    def test_unfinished_login_is_refused(self) -> None:
        self.refuses(ioreg(user(kCGSessionLoginDoneKey=False)), "login has not finished")

    def test_missing_or_duplicate_session_is_refused(self) -> None:
        self.refuses(ioreg(user(kCGSSessionUserIDKey=502)), "found 0")
        self.refuses(ioreg(user(), user()), "found 2")

    def test_malformed_state_is_refused(self) -> None:
        self.refuses(b"not a plist", "unreadable")
        self.refuses(plistlib.dumps([{"IOProviderClass": "IOResources"}]), "no IOConsoleUsers")

    def test_t17_submission_requires_the_session_after_its_manifest_preflight(self) -> None:
        dispatch = (ROOT / "scripts/live-gates/app-ui-manifest-dispatch.sh").read_text()
        arm = dispatch.split("validate:t17-windows-hvf-product-e2e)", 1)[1].split(";;", 1)[0].strip()
        preflight = 'python3 "$HERE/windows-product-e2e-launchservices-preflight.py" --manifest "$1"'
        self.assertEqual(arm, preflight + ' && exec python3 "$HERE/host_console_session.py" --require-unlocked')


if __name__ == "__main__":
    result = unittest.main(argv=[sys.argv[0]], exit=False).result
    raise SystemExit(0 if result.wasSuccessful() else 1)
