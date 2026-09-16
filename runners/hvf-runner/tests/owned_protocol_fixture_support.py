"""Private synthetic children and exact identity/exit witnesses for framed tests."""
import hashlib
import json
import os
from pathlib import Path
import select
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import uuid
from owned_cancellation_fixture import CHILD, Peer


def encode(value):
    data = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    return len(data).to_bytes(4, "big") + data


class Frames:
    def __init__(self, stream):
        self.stream, self.buffer, self.events = stream, b"", []
        self.eof = False

    def drain(self, timeout):
        deadline = time.monotonic() + timeout
        while not self.eof and time.monotonic() < deadline:
            if not select.select([self.stream], [], [], max(0, deadline - time.monotonic()))[0]:
                break
            data = os.read(self.stream.fileno(), 8192)
            if not data:
                self.eof = True
                break
            self.buffer += data
            while len(self.buffer) >= 4:
                size = int.from_bytes(self.buffer[:4], "big")
                assert 0 < size <= 8192
                if len(self.buffer) < size + 4:
                    break
                body, self.buffer = self.buffer[4:size + 4], self.buffer[size + 4:]
                value = json.loads(body)
                assert encode(value)[4:] == body
                self.events.append(value)
        if self.eof:
            assert not self.buffer, "partial final frame"
        return self.events


class Fixture:
    def __init__(self, executable, scenario):
        self.work = Path(tempfile.mkdtemp(prefix="bv-proto-", dir="/tmp")).resolve()
        self.work.chmod(0o700)
        (self.work / "tmp").mkdir(mode=0o700)
        self.token, self.operation, self.nonce = str(uuid.uuid4()), str(uuid.uuid4()), uuid.uuid4().hex
        self.listener = socket.socket(socket.AF_UNIX)
        self.listener.bind(str(self.work / "fixture.sock"))
        self.listener.listen(8)
        self.listener.settimeout(6)
        self.peers, self.runner = [], None
        self.expected = 1 if scenario == "readiness" else (3 if scenario == "reset" else 2)
        self.scenario, self.verified = scenario, False
        for name in ["disk.raw", "vars.fd", "firmware.fd"]:
            (self.work / name).write_bytes(b"synthetic fixture bytes\n")
        for role in ["helper", "swtpm"]:
            text = CHILD.replace("__CONFIG__", repr((role,
                "readiness" if scenario == "readiness" else "held",
                str(self.work / "fixture.sock"), self.nonce, str(self.work))))
            text = text.replace('ROLE + ".started"', 'ROLE + "-" + os.environ.get("BRIDGEVM_RESET_GENERATION", "tpm") + ".started"')
            if scenario == "reset":
                text += '\nif ROLE == "helper" and os.environ["BRIDGEVM_RESET_GENERATION"] == "0" and not terminated: sys.exit(42)\n'
            path = self.work / (role + ".py")
            path.write_text("#!" + sys.executable + "\n" + text)
            path.chmod(0o700)
        self.spec = {"version": 1, "disk": str(self.work / "disk.raw"),
                     "uefi_vars": str(self.work / "vars.fd"), "ram_mib": 1024, "vcpus": 1}
        spec_bytes = json.dumps(self.spec).encode()
        (self.work / "spec.json").write_bytes(spec_bytes)
        self.hello = {"schemaVersion": 1, "kind": "hello", "sequence": 0, "runToken": self.token,
                      "manifestSHA256": hashlib.sha256(spec_bytes).hexdigest(), "keyHex": None}
        self.stop = {"schemaVersion": 1, "kind": "stop", "sequence": 1,
                     "runToken": self.token, "operationID": self.operation}
        self.environment = {k: v for k, v in os.environ.items() if not k.startswith("BRIDGEVM_")}
        self.environment["TMPDIR"] = str(self.work / "tmp")
        self.base = [str(Path(executable).resolve()), "--launch-spec", str(self.work / "spec.json")]
        self.command = self.base + ["--helper", str(self.work / "helper.py"), "--helper-firmware",
            str(self.work / "firmware.fd"), "--helper-vtpm-state", str(self.work / "state"),
            "--helper-swtpm-bin", str(self.work / "swtpm.py"), "--supervise-receipt",
            str(self.work / "receipt"), "--helper-evidence-dir", str(self.work / "surfaces"),
            "--owned-runtime-stdio"]
        # Framed encrypted vTPM mode demands a key; fake swtpm never persists it.
        self.hello["keyHex"] = bytes(range(32)).hex()

    def launch(self, payload=None):
        self.runner = subprocess.Popen(self.command, env=self.environment, stdin=subprocess.PIPE,
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.frames = Frames(self.runner.stdout)
        self.runner.stdin.write(encode(self.hello) if payload is None else payload)
        self.runner.stdin.flush()

    def accept(self):
        stream, _ = self.listener.accept()
        peer = Peer(stream, self.runner, self.nonce)
        self.peers.append(peer)
        return peer

    def close_input(self):
        self.runner.stdin.close()
        self.runner.stdin = None

    def leases(self, held):
        for name in ["disk.raw", "vars.fd"]:
            other = self.work / ("other-" + name)
            other.write_bytes(b"synthetic competitor")
            spec = dict(self.spec)
            spec["uefi_vars" if name == "disk.raw" else "disk"] = str(other)
            path = self.work / "competitor.json"
            path.write_text(json.dumps(spec))
            result = subprocess.run([self.base[0], "--launch-spec", str(path)], env=self.environment,
                                    capture_output=True, timeout=2)
            assert (result.returncode != 0) == held, result.stderr
            if held:
                assert b"already open for writing" in result.stderr

    def finish(self):
        self.frames.drain(7)
        assert self.frames.eof
        code = self.runner.wait(timeout=2)
        stderr = self.runner.stderr.read(65536)
        assert code == 1, (code, stderr)
        for peer in self.peers:
            assert peer.watch.wait(2), "missing exact child exit"
        self.leases(False)
        return self.frames.events

    def cleanup(self):
        for peer in self.peers:
            peer.finish()
        if self.runner is not None and self.runner.poll() is None:
            if self.runner.stdin is not None:
                self.close_input()
            try:
                self.runner.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.runner.kill()
                self.runner.wait(timeout=2)
        exits = [peer.watch.wait(21) for peer in self.peers]
        markers = [json.loads(p.read_text()) for p in self.work.glob("*.started")]
        recorded = {(p.pid, p.role) for p in self.peers}
        started = {(m["pid"], m["role"]) for m in markers}
        clean = len(self.peers) == self.expected and recorded == started and all(exits)
        for peer in self.peers:
            peer.watch.close()
            peer.connection.close()
        self.listener.close()
        if self.runner:
            for stream in [self.runner.stdin, self.runner.stdout, self.runner.stderr]:
                if stream:
                    stream.close()
        if clean:
            shutil.rmtree(self.work)
        else:
            raise AssertionError("fixture cleanup unconfirmed; preserved " + str(self.work))
