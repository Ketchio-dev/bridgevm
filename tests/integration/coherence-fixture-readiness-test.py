#!/usr/bin/env python3
"""Dependency readiness is mandatory even when the primary script arrived."""
import hashlib
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts/live-gates"))
from coherence_fixture_readiness import FILES, await_staged_fixture
from guest_input_fixture_staging import stage
from guest_input_profiles import COHERENCE_PROFILE


class Readiness(unittest.TestCase):
    def test_every_guest_hash_before_return(self):
        with tempfile.TemporaryDirectory() as tmp:
            share = Path(tmp)
            _, hashes = stage(Path(__file__).resolve().parents[2], share, COHERENCE_PROFILE)
            calls = []
            controller = SimpleNamespace(share=share, await_staged=lambda *args: calls.append(args))
            self.assertEqual(await_staged_fixture(controller), "C:\\BridgeVM\\input-proof\\" + FILES[0])
            self.assertEqual(calls, [("C:\\BridgeVM\\input-proof\\" + name, hashes[name].upper()) for name in FILES])

    def test_missing_or_unconfirmed_dependency_stops(self):
        for failed_index in range(len(FILES)):
            with tempfile.TemporaryDirectory() as tmp:
                share, calls = Path(tmp), []
                for name in FILES: (share / name).write_bytes(name.encode())
                def wait(path, digest):
                    calls.append(path)
                    self.assertEqual(digest, hashlib.sha256(FILES[len(calls)-1].encode()).hexdigest().upper())
                    if len(calls) == failed_index + 1: raise TimeoutError("guest file not confirmed")
                with self.assertRaises(TimeoutError):
                    await_staged_fixture(SimpleNamespace(share=share, await_staged=wait))
                self.assertEqual(len(calls), failed_index + 1)
                (share / FILES[failed_index]).unlink()
                with self.assertRaises(FileNotFoundError):
                    await_staged_fixture(SimpleNamespace(share=share, await_staged=lambda *args: None))


if __name__ == "__main__":
    unittest.main()
