#!/usr/bin/env python3
"""Retained allocation bindings remain authenticated after moving completed jobs."""
import importlib.util
import json
from pathlib import Path
import shutil
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("packet_fixture", HERE / "t17-private-diagnostic-packet-test.py")
fixture = importlib.util.module_from_spec(spec); spec.loader.exec_module(fixture)


class AllocationPacketTests(unittest.TestCase):
    def setUp(self):
        self.case = fixture.PacketTest()
        self.case.setUp()
        self.addCleanup(lambda: shutil.rmtree(self.case.output, ignore_errors=True))
        self.case.complete_sources()
        fixture.packet.capture(self.case.args)

    def test_offline_packet_survives_running_to_done_move(self):
        case = self.case
        shutil.rmtree(case.work)
        moved = case.output.with_name(case.output.name + "-done")
        case.output.rename(moved); case.output = moved
        case.args.private = moved / "private"; case.args.lane_root = None
        fixture.packet.verify(case.args)

    def test_allocation_mutation_refuses_with_and_without_live_lane(self):
        case = self.case
        path = case.private / "t17-diagnostic-lane-1-index.json"
        original = json.loads(path.read_text())
        for key in fixture.packet.WORK.FIELDS:
            for live in (True, False):
                with self.subTest(key=key, live=live):
                    path.write_text(json.dumps({**original, key: "0:0"}))
                    case.args.lane_root = case.lane if live else None
                    with self.assertRaises((ValueError, OSError)):
                        fixture.packet.verify(case.args)
        path.write_text(json.dumps(original))
        request = case.private / "t17-diagnostic-lane-1-request.json"
        request.write_bytes(request.read_bytes() + b" ")
        with self.assertRaises(ValueError):
            fixture.packet.verify(case.args)


if __name__ == "__main__":
    unittest.main()
