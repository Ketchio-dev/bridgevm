#!/usr/bin/env python3
"""Exercise typed runtime cancellation with private synthetic children only."""
import argparse
import json
import os
from pathlib import Path
import select
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
import uuid

CHILD = r"""
import json, os, select, signal, socket, sys, time
from pathlib import Path
ROLE, SCENARIO, ENDPOINT, NONCE, STARTS = __CONFIG__
Path(STARTS, ROLE + ".started").write_text(json.dumps({"pid": os.getpid(),
    "parent": os.getppid(), "nonce": NONCE, "role": ROLE}))
channels = []
if ROLE == "swtpm":
    paths = [a.split("path=", 1)[1].split(",", 1)[0] for a in sys.argv if "path=" in a]
    assert len(paths) == 2
    for path in paths:
        listener = socket.socket(socket.AF_UNIX)
        listener.bind(path); listener.listen(2); listener.setblocking(False)
        channels.append(listener)
control = socket.socket(socket.AF_UNIX)
control.connect(ENDPOINT)
def send(kind):
    control.sendall((json.dumps({"kind": kind, "role": ROLE, "pid": os.getpid(),
        "parent": os.getppid(), "nonce": NONCE}) + "\n").encode())
terminated = False
def on_term(signum, frame):
    global terminated
    terminated = True
signal.signal(signal.SIGTERM, on_term)
send("ready")
control.setblocking(False)
reported = False
limit = time.monotonic() + 20
held = []
try:
    while time.monotonic() < limit:
        if terminated and not reported:
            reported = True
            send("term")
            if SCENARIO != "held" and not (SCENARIO == "ignore" and ROLE == "helper"):
                break
        ready, _, _ = select.select([control] + channels, [], [], 0.01)
        if control in ready:
            data = control.recv(256)
            if not data or data == b"finish\n": break
            assert data == b"ping\n"
            send("alive")
        for listener in channels:
            if listener not in ready: continue
            stream, _ = listener.accept(); stream.settimeout(0.1)
            if SCENARIO == "readiness":
                held.append(stream)
                continue
            try:
                if stream.recv(4) == b"\0\0\0\1":
                    stream.sendall(b"\0\0\0\0\0\0\0\1")
            finally:
                stream.close()
finally:
    for stream in held + channels: stream.close()
    control.close()
"""

class ExitWatch:
    def __init__(self, pid):
        self.pid = pid
        self.exited = False
        if hasattr(select, "kqueue"):
            self.handle = select.kqueue()
            self.handle.control([select.kevent(pid, filter=select.KQ_FILTER_PROC,
                flags=select.KQ_EV_ADD | select.KQ_EV_ENABLE | select.KQ_EV_ONESHOT,
                fflags=select.KQ_NOTE_EXIT)], 0, 0)
            self.mac = True
        else:
            self.handle = os.pidfd_open(pid)
            self.poller = select.poll()
            self.poller.register(self.handle, select.POLLIN)
            self.mac = False

    def wait(self, seconds):
        if self.exited:
            return True
        if self.mac:
            events = self.handle.control(None, 1, seconds)
            self.exited = any(event.fflags & select.KQ_NOTE_EXIT for event in events)
        else:
            self.exited = bool(self.poller.poll(int(seconds * 1000)))
        return self.exited

    def close(self):
        if self.mac:
            self.handle.close()
        else:
            os.close(self.handle)

class Peer:
    def __init__(self, connection, runner, nonce):
        self.connection = connection
        self.buffer = b""
        self.events = []
        hello = self.receive("ready", 2)
        assert hello["parent"] == runner.pid and hello["nonce"] == nonce
        self.role, self.pid = hello["role"], hello["pid"]
        self.watch = ExitWatch(self.pid)

    def receive(self, kind, timeout):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if b"\n" in self.buffer:
                row, self.buffer = self.buffer.split(b"\n", 1)
                event = json.loads(row)
                self.events.append(event)
                if event["kind"] == kind:
                    return event
                continue
            self.connection.settimeout(max(0.01, deadline - time.monotonic()))
            data = self.connection.recv(2048)
            assert data, "fixture child closed without expected event"
            self.buffer += data
            assert len(self.buffer) <= 4096
        raise AssertionError("fixture event deadline")

    def finish(self):
        try:
            self.connection.sendall(b"finish\n")
        except OSError:
            pass

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--runner", required=True)
    parser.add_argument("--scenario", choices=["held", "ignore", "readiness"], required=True)
    args = parser.parse_args()
    runner_path = str(Path(args.runner).resolve())
    # macOS Unix socket paths are short; do not nest this fixture in Darwin's
    # long per-user temporary path and accidentally test address overflow.
    work = Path(tempfile.mkdtemp(prefix="bv-owned-cancel-", dir="/tmp")).resolve()
    work.chmod(0o700)
    (work / "tmp").mkdir(mode=0o700)
    nonce = uuid.uuid4().hex
    listener = socket.socket(socket.AF_UNIX)
    listener.bind(str(work / "fixture.sock"))
    listener.listen(4)
    listener.settimeout(6)
    peers, runner = {}, None
    verified = False
    try:
        for name in ["disk.raw", "vars.fd", "firmware.fd"]:
            (work / name).write_bytes(b"synthetic fixture bytes\n")
        for role in ["helper", "swtpm"]:
            text = "#!" + sys.executable + "\n" + CHILD.replace("__CONFIG__", repr(
                (role, args.scenario, str(work / "fixture.sock"), nonce, str(work))))
            path = work / (role + ".py")
            path.write_text(text)
            path.chmod(0o700)
        spec = {"version": 1, "disk": str(work / "disk.raw"),
                "uefi_vars": str(work / "vars.fd"), "ram_mib": 1024, "vcpus": 1}
        (work / "spec.json").write_text(json.dumps(spec))
        environment = {key: value for key, value in os.environ.items()
                       if not key.startswith("BRIDGEVM_")}
        environment["TMPDIR"] = str(work / "tmp")
        command = [runner_path, "--launch-spec", str(work / "spec.json")]
        full = command + ["--helper", str(work / "helper.py"), "--helper-firmware",
            str(work / "firmware.fd"), "--helper-vtpm-state", str(work / "state"),
            "--helper-swtpm-bin", str(work / "swtpm.py"), "--supervise-receipt",
            str(work / "receipt"), "--helper-evidence-dir", str(work / "surfaces")]
        runner = subprocess.Popen(full, env=environment, stdin=subprocess.DEVNULL,
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        count = 1 if args.scenario == "readiness" else 2
        for _ in range(count):
            connection, _ = listener.accept()
            peer = Peer(connection, runner, nonce)
            assert peer.role not in peers
            peers[peer.role] = peer
        assert "swtpm" in peers

        def leases_held():
            for name in ["disk.raw", "vars.fd"]:
                other = work / ("competitor-" + name)
                other.write_bytes(b"synthetic competitor\n")
                one = dict(spec)
                if name == "disk.raw":
                    one["uefi_vars"] = str(other)
                else:
                    one["disk"] = str(other)
                (work / "competitor.json").write_text(json.dumps(one))
                result = subprocess.run([runner_path, "--launch-spec", str(work / "competitor.json")],
                    env=environment, capture_output=True, timeout=2)
                assert result.returncode != 0 and b"already open for writing" in result.stderr

        leases_held()
        starts_before = {role: (work / (role + ".started")).read_bytes()
                         if (work / (role + ".started")).exists() else None
                         for role in ["helper", "swtpm"]}
        duplicate = subprocess.run(full, env=environment, capture_output=True, timeout=2)
        assert duplicate.returncode != 0 and b"launch refused" in duplicate.stderr
        assert starts_before == {role: (work / (role + ".started")).read_bytes()
                                 if (work / (role + ".started")).exists() else None
                                 for role in ["helper", "swtpm"]}, "lease refusal launched a child"
        assert not select.select([listener], [], [], 0)[0], "lease refusal created a pending child"
        if args.scenario == "held":
            runner.terminate()
            peers["helper"].receive("term", 2)
            leases_held()
            assert not peers["swtpm"].watch.wait(0)
            peers["helper"].finish()
            assert peers["helper"].watch.wait(2)
            peers["swtpm"].receive("term", 2)
            leases_held()
            peers["swtpm"].finish()
        elif args.scenario == "ignore":
            runner.send_signal(signal.SIGINT)
            peers["helper"].receive("term", 2)
            assert not peers["helper"].watch.wait(0.2)
        else:
            runner.terminate()
        stdout, stderr = runner.communicate(timeout=7)
        assert runner.returncode == 1, (runner.returncode, stdout, stderr)
        assert b"cancelled" in stderr
        assert b"vm run complete" not in stdout
        for peer in peers.values():
            assert peer.watch.wait(2), "runner returned without exact child exit"
        if args.scenario == "readiness":
            assert not (work / "helper.started").exists(), "unexpected helper launch during TPM readiness"
            assert not select.select([listener], [], [], 0)[0], "unexpected pending helper identity"
        result = subprocess.run(command, env=environment, capture_output=True, timeout=2)
        assert result.returncode == 0, result.stderr
        assert not (work / "receipt").exists(), "cancellation cannot create reset success"
        verified = True
        print(json.dumps({"scenario": args.scenario, "result": "PASS",
            "children_reaped": sorted(peers), "leases_released_after_children": True,
            "guest_launched": False, "real_swtpm_launched": False}))
    finally:
        for peer in peers.values():
            peer.finish()
        if runner is not None and runner.poll() is None:
            runner.terminate()
            try:
                out, err = runner.communicate(timeout=5)
            except subprocess.TimeoutExpired:
                runner.kill()
                out, err = runner.communicate(timeout=2)
            if not verified:
                print(out.decode(errors="replace"), err.decode(errors="replace"), file=sys.stderr)
        exits = [peer.watch.wait(21) for peer in peers.values()]
        expected = {"swtpm"} if args.scenario == "readiness" else {"swtpm", "helper"}
        started = {role for role in ["swtpm", "helper"] if (work / (role + ".started")).exists()}
        starts_match = all(json.loads((work / (role + ".started")).read_text())["pid"] == peer.pid
                           for role, peer in peers.items() if role in started)
        cleanup_verified = set(peers) == expected and started == expected and starts_match and all(exits)
        for peer in peers.values():
            peer.watch.close()
            peer.connection.close()
        listener.close()
        if cleanup_verified:
            shutil.rmtree(work)
        else:
            raise AssertionError("fixture identities/exits unconfirmed; retained private evidence: " + str(work))
        if not verified:
            print("Failed fixture retained in test transcript; private tree cleaned.", file=sys.stderr)

if __name__ == "__main__":
    main()
