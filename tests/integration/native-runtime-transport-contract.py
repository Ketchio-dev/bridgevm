#!/usr/bin/env python3
"""Compile and exercise owned Unix sockets/processes; never initialize AppKit."""
import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import time
import unittest
import uuid
from native_runtime_alias_cases import NativeRuntimeAliasCases
from native_runtime_control_cases import NativeRuntimeControlCases
from native_runtime_transport_build import build_transport_fixture
from native_runtime_start_cases import NativeRuntimeStartCases
ROOT = Path(__file__).resolve().parents[2]

class Contracts(NativeRuntimeAliasCases, NativeRuntimeControlCases, NativeRuntimeStartCases, unittest.TestCase):
    executable = None
    output = None

    def setUp(self):
        self.work = self.output / self._testMethodName
        self.work.mkdir(mode=0o700)
        self.library = self.work / "library"
        self.library.mkdir(mode=0o700)
        info = self.library.stat()
        self.identity = dict(canonicalPath=str(self.library), device=info.st_dev,
                             inode=info.st_ino, uid=os.geteuid())
        identity = f"bridgevm-native-runtime-v1\n{os.geteuid()}\n{info.st_dev}\n{info.st_ino}\n"
        key = hashlib.sha256(identity.encode()).hexdigest()[:32]
        self.namespace = Path(f"/private/tmp/bridgevm-app-{os.geteuid()}") / key
        self.addCleanup(lambda: shutil.rmtree(self.namespace) if self.namespace.exists() else None)
        self.control_count = 0

    def call(self, *args, timeout=4):
        return subprocess.run([str(self.executable), *map(str, args)], capture_output=True,
                              text=True, timeout=timeout)

    @contextmanager
    def owner(self, mode="owner"):
        self.control_count += 1
        control = self.work / f"control-{self.control_count}"
        control.mkdir(mode=0o700)
        with (control / "stdout.log").open("xb") as out, (control / "stderr.log").open("xb") as err:
            process = subprocess.Popen([str(self.executable), mode, str(self.library), str(control)],
                                       stdout=out, stderr=err)
            try:
                deadline = time.monotonic() + 3
                while not (control / "ready").exists():
                    self.assertIsNone(process.poll(), (control / "stderr.log").read_text())
                    self.assertLess(time.monotonic(), deadline, "owner startup deadline")
                    time.sleep(0.01)
                yield process, control
            finally:
                if process.poll() is None:
                    (control / "stop").touch()
                    try:
                        process.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=3)

    def test_pure_framing_identity_peer_and_codec(self):
        result = self.call("pure", self.work)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_repeated_owner_close_and_descriptor_reuse(self):
        result = self.call("churn", self.library)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_real_query_and_duplicate_alias_owner_has_zero_model_effects(self):
        alias = self.work / "library alias"
        alias.symlink_to(self.library, target_is_directory=True)
        duplicate = self.work / "duplicate"
        duplicate.mkdir()
        with self.owner():
            result = self.call("query", alias)
            self.assertEqual(result.returncode, 0, result.stderr)
            response = json.loads(result.stdout)
            self.assertEqual(response["vmID"], "개발-vm")
            self.assertEqual(response["library"], self.identity)
            self.assertEqual(response["sessions"], [])
            denied = self.call("owner", alias, duplicate)
            self.assertNotEqual(denied.returncode, 0)
            self.assertIn("ownerBusy", denied.stderr)
            self.assertFalse((duplicate / "ready").exists())
        self.assertTrue((self.namespace / "owner.lock").is_file())
        self.assertFalse((self.namespace / "status.sock").exists())

    def test_crash_reclaims_stale_socket_with_same_permanent_lock(self):
        with self.owner() as (process, _):
            lock_inode = (self.namespace / "owner.lock").stat().st_ino
            process.kill()
            process.wait(timeout=3)
        self.assertTrue((self.namespace / "status.sock").exists())
        with self.owner():
            self.assertEqual((self.namespace / "owner.lock").stat().st_ino, lock_inode)
            result = self.call("query", self.library)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_shutdown_never_removes_replacement_socket(self):
        replacement = socket.socket(socket.AF_UNIX)
        self.addCleanup(replacement.close)
        with self.owner():
            path = self.namespace / "status.sock"
            path.unlink()
            replacement.bind(str(path))
            path.chmod(0o600)
            inode = path.stat().st_ino
        self.assertEqual(path.stat().st_ino, inode)

    def test_symlink_socket_and_wrong_type_are_refused_without_unlink(self):
        with self.owner():
            pass
        target = self.work / "target"
        target.write_text("unchanged")
        path = self.namespace / "status.sock"
        control = self.work / "refused"
        control.mkdir()
        path.symlink_to(target)
        denied = self.call("owner", self.library, control)
        self.assertNotEqual(denied.returncode, 0)
        self.assertTrue(path.is_symlink())
        path.unlink()
        path.write_text("wrong type")
        denied = self.call("owner", self.library, control)
        self.assertNotEqual(denied.returncode, 0)
        self.assertEqual(path.read_text(), "wrong type")
        self.assertEqual(target.read_text(), "unchanged")

    def test_slow_handler_is_bounded_and_owner_can_close(self):
        with self.owner(mode="slow"):
            began = time.monotonic()
            result = self.call("query", self.library)
            self.assertNotEqual(result.returncode, 0)
            self.assertLess(time.monotonic() - began, 2.8)

    def test_invalid_request_does_not_poison_subsequent_queries(self):
        with self.owner():
            for payload in [b"not-json", b"{}", b'{"schema":1,"schema":2}']:
                with socket.socket(socket.AF_UNIX) as client:
                    client.settimeout(2)
                    client.connect(str(self.namespace / "status.sock"))
                    client.sendall(len(payload).to_bytes(4, "big") + payload)
                    self.assertEqual(client.recv(1), b"")
            result = self.call("query", self.library)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_four_admitted_handlers_refuse_fifth_without_waiting(self):
        with self.owner(mode="slow") as (_, control):
            peers = []
            try:
                for _ in range(4):
                    request = dict(schema="bridgevm.app-runtime-request.v1", operation="status",
                        requestID=str(uuid.uuid4()).upper(), library=self.identity,
                        vmID="fixture", savedConfiguration=dict(state="missing"))
                    payload = json.dumps(request, sort_keys=True, separators=(",", ":")).encode()
                    peer = socket.socket(socket.AF_UNIX)
                    peers.append(peer)
                    peer.settimeout(3)
                    peer.connect(str(self.namespace / "status.sock"))
                    peer.sendall(len(payload).to_bytes(4, "big") + payload)
                deadline = time.monotonic() + 1
                while len(list(control.glob("started-*"))) != 4:
                    self.assertLess(time.monotonic(), deadline, "four handlers were not admitted")
                    time.sleep(0.01)
                began = time.monotonic()
                result = self.call("query", self.library)
                self.assertNotEqual(result.returncode, 0)
                self.assertLess(time.monotonic() - began, 0.8)
                self.assertEqual(len(list(control.glob("started-*"))), 4)
                for peer in peers:
                    self.assertEqual(peer.recv(1), b"")
            finally:
                for peer in peers:
                    peer.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    output = args.output
    if not output.is_absolute() or output.exists() or output.resolve() != output:
        parser.error("output must be a new absolute canonical directory")
    output.mkdir(mode=0o700)
    executable, status = build_transport_fixture(ROOT, output)
    if status:
        return status
    Contracts.executable, Contracts.output = executable, output
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(Contracts)
    return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
