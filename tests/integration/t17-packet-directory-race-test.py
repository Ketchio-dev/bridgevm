#!/usr/bin/env python3
"""A T17 packet path survives entries appearing inside its components, but not a swapped component.

Traversal compares each component's stat before and after opening it. A new entry
inside a component (another job writing to a shared temporary or queue directory)
changes that directory's links, size and times; only a different directory at that
name is a change of identity.
"""
from __future__ import annotations

from contextlib import ExitStack
import importlib.util
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

SCRIPT = Path(__file__).resolve().parents[2] / "scripts/live-gates/t17_private_diagnostic_packet.py"
SPEC = importlib.util.spec_from_file_location("t17_packet_race", SCRIPT)
assert SPEC and SPEC.loader
packet = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(packet)


class DirectoryRace(unittest.TestCase):
    def setUp(self) -> None:
        self.root = Path(tempfile.mkdtemp(prefix="t17-race-")).resolve()
        self.parent = self.root / "running"
        self.target = self.parent / "private"
        self.target.mkdir(parents=True, mode=0o700)

    def tearDown(self) -> None:
        for path in sorted(self.root.rglob("*"), key=lambda item: len(item.parts), reverse=True):
            path.rmdir() if path.is_dir() else path.unlink()
        self.root.rmdir()

    def open_with(self, before_open, component: str = "private") -> None:
        real_open = os.open

        def racing_open(name, flags, *args, **kwargs):
            if name == component and kwargs.get("dir_fd") is not None:
                before_open()
            return real_open(name, flags, *args, **kwargs)

        with ExitStack() as stack, mock.patch.object(packet.os, "open", racing_open):
            packet.open_absolute_directory(self.target, stack)

    def test_new_entries_inside_a_component_are_not_a_change(self) -> None:
        self.open_with(lambda: (self.parent / "receipt.json").write_text("{}\n"), "running")
        self.open_with(lambda: (self.parent / "lane-1").mkdir(), "running")
        self.open_with(lambda: (self.target / "index.json").write_text("{}\n"))

    def test_a_swapped_component_is_refused(self) -> None:
        def swap() -> None:
            self.target.rename(self.parent / "moved")
            self.target.mkdir(mode=0o700)
        with self.assertRaises(packet.CaptureError):
            self.open_with(swap)

    def test_a_changed_regular_file_is_still_refused(self) -> None:
        source = self.target / "run.log"
        source.write_text("before\n")
        with ExitStack() as stack:
            parent = packet.open_absolute_directory(self.target, stack)
            real_open = os.open

            def growing_open(name, flags, *args, **kwargs):
                if name == "run.log":
                    source.write_text("before and after\n")
                return real_open(name, flags, *args, **kwargs)

            with mock.patch.object(packet.os, "open", growing_open), self.assertRaises(packet.CaptureError):
                packet.source_file(parent, "run.log", os.fstat(parent).st_dev, stack)


if __name__ == "__main__":
    result = unittest.main(argv=[sys.argv[0]], exit=False).result
    raise SystemExit(0 if result.wasSuccessful() else 1)
