#!/usr/bin/env python3
"""Actual typed runner frames, using only private synthetic owned processes."""
import argparse
import fcntl
import json
import select
from owned_protocol_fixture_support import Fixture, encode


def validate(fixture, events):
    assert events and [e["sequence"] for e in events] == list(range(1, len(events) + 1))
    assert all(e["runToken"] == fixture.token and e["runnerPID"] == fixture.runner.pid for e in events)
    assert events[0]["kind"] == "ready"
    assert events[0]["ready"]["manifestSHA256"] == fixture.hello["manifestSHA256"]
    live, counts = {}, {"helper": [0, 0], "swtpm": [0, 0]}
    for event in events:
        if event["kind"] == "childStarted":
            child = event["child"]
            role = child["role"]
            assert role not in live
            if role == "helper":
                assert child["generation"] == counts[role][0]
            live[role] = child
            counts[role][0] += 1
        if event["kind"] == "childReaped":
            child = event["child"]
            previous = live.pop(child["role"])
            assert (previous["pid"], previous["generation"]) == (child["pid"], child["generation"])
            assert child["reason"] in ["exit", "signal"] and child["status"] >= 0
            counts[child["role"]][1] += 1
    assert not live and events[-1]["kind"] == "complete"
    complete = events[-1]["complete"]
    assert complete["mediaLeaseDisposition"] == "releasedAfterReap"
    assert complete["runtimeDirectoryDisposition"] == "removed"
    for role in counts:
        assert counts[role][0] == counts[role][1] == complete[role]["spawnedCount"] == complete[role]["reapedCount"]
    expected = "ownerEOF" if fixture.scenario == "eof" else "stopRequested"
    assert complete["cause"] == expected
    assert complete["outcome"] == "cancelled" and complete["failureCode"] is None
    acks = [e for e in events if e["kind"] == "stopAck"]
    assert len(acks) == (0 if expected == "ownerEOF" else 1)
    if acks:
        assert complete["operationID"] == fixture.operation
    return complete


def run(executable, scenario):
    fixture = Fixture(executable, scenario)
    try:
        if scenario == "admission":
            fixture.expected = 0
            lock = open(str(fixture.work / "vars.fd") + ".bridgevm-writer.lock", "w")
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                fixture.launch()
                fixture.frames.drain(3)
                assert fixture.runner.wait(timeout=2) == 1 and fixture.frames.eof
                events = fixture.frames.events
                assert len(events) == 1 and events[0]["kind"] == "complete"
                complete = events[0]["complete"]
                assert complete["mediaLeaseDisposition"] == "notAdmitted"
                assert complete["failureCode"] == "mediaAdmissionFailed"
                assert complete["helper"]["spawnedCount"] == complete["swtpm"]["spawnedCount"] == 0
                assert not list(fixture.work.glob("*.started"))
                assert not (fixture.work / "state").exists() and not (fixture.work / "surfaces").exists()
                assert not select.select([fixture.listener], [], [], 0)[0]
            finally:
                lock.close()
            fixture.leases(False)
            fixture.verified = True
            print(json.dumps({"scenario": scenario, "result": "PASS", "partialAdmissionReleased": True}))
            return
        if scenario in ["invalid", "handshake"]:
            fixture.expected = 0
            bad = dict(fixture.hello, manifestSHA256="b" * 64)
            fixture.launch(b"\0\0" if scenario == "handshake" else encode(bad))
            if scenario != "handshake":
                fixture.close_input()
            fixture.frames.drain(3)
            assert fixture.runner.wait(timeout=2) == 1
            assert fixture.frames.eof and not fixture.frames.events
            assert not list(fixture.work.glob("*.started"))
            assert not (fixture.work / "state").exists()
            assert not (fixture.work / "surfaces").exists()
            assert not list(fixture.work.glob("*.bridgevm-writer.lock"))
            assert not select.select([fixture.listener], [], [], 0)[0]
            fixture.verified = True
            print(json.dumps({"scenario": scenario, "result": "PASS", "effects": 0}))
            return
        fixture.launch()
        tpm = fixture.accept()
        assert tpm.role == "swtpm"
        helper = None
        if scenario != "readiness":
            helper = fixture.accept()
            assert helper.role == "helper"
        if scenario == "reset":
            helper.finish()
            assert helper.watch.wait(2)
            helper = fixture.accept()
            assert helper.role == "helper" and not tpm.watch.wait(0)
        fixture.leases(True)
        if scenario == "eof":
            fixture.close_input()
        elif scenario == "output":
            fixture.runner.stdout.close()
        elif scenario == "partial":
            fixture.runner.stdin.write((50).to_bytes(4, "big") + b"{")
            fixture.runner.stdin.flush()
        else:
            fixture.runner.stdin.write(encode(fixture.stop) * 2)
            fixture.runner.stdin.flush()
        if helper:
            helper.receive("term", 4 if scenario == "partial" else 2)
            fixture.leases(True)
            assert not tpm.watch.wait(0)
            helper.finish()
            assert helper.watch.wait(2)
        if scenario != "readiness":
            tpm.receive("term", 2)
            fixture.leases(True)
            tpm.finish()
        if scenario in ["output", "partial"]:
            assert fixture.runner.wait(timeout=7) == 1
            for peer in fixture.peers:
                assert peer.watch.wait(2)
            fixture.leases(False)
            if scenario == "partial":
                fixture.frames.drain(2)
                assert fixture.frames.eof and all(e["kind"] != "complete" for e in fixture.frames.events)
            fixture.verified = True
            print(json.dumps({"scenario": scenario, "result": "PASS", "cleanupObservedWithoutDeliveredComplete": True}))
            return
        events = fixture.finish()
        complete = validate(fixture, events)
        if scenario == "readiness":
            assert complete["helper"]["spawnedCount"] == 0
            assert not list(fixture.work.glob("helper-*.started"))
            assert not select.select([fixture.listener], [], [], 0)[0]
        fixture.verified = True
        print(json.dumps({"scenario": scenario, "result": "PASS", "frames": len(events),
                          "helperGenerations": complete["helper"]["reapedCount"],
                          "ownedExitAndLeaseRelease": True}))
    finally:
        fixture.cleanup()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--runner", required=True)
    parser.add_argument("--scenario", choices=["stop", "eof", "reset", "readiness", "invalid", "output", "partial", "admission", "handshake"], required=True)
    args = parser.parse_args()
    run(args.runner, args.scenario)
