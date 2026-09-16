"""Wire/connection tests use a synthetic in-memory ticket, never a guest claim."""
import json
import socket
import time
import uuid


class NativeRuntimeControlCases:
    def control_request(self, operation="stop", operation_id=None):
        return dict(schema="bridgevm.app-runtime-control-request.v2", operation=operation,
            requestID=str(uuid.uuid4()).upper(), library=self.identity, vmID="fixture",
            operationID=operation_id or str(uuid.uuid4()).upper(),
            target=dict(appInstanceID="A0000000-0000-0000-0000-000000000001",
                runToken="B0000000-0000-0000-0000-000000000001", processID=123,
                acceptedConfigurationDigest="a" * 64))

    def control_exchange(self, request, raw=None):
        payload = raw if raw is not None else json.dumps(request, sort_keys=True, separators=(",", ":")).encode()
        with socket.socket(socket.AF_UNIX) as peer:
            peer.settimeout(3)
            peer.connect(str(self.namespace / "status.sock"))
            peer.sendall(len(payload).to_bytes(4, "big") + payload)
            header = peer.recv(4)
            if not header:
                return None
            while len(header) < 4:
                part = peer.recv(4 - len(header))
                self.assertTrue(part, "truncated response header")
                header += part
            size = int.from_bytes(header, "big")
            self.assertLessEqual(size, 65536)
            body = b""
            while len(body) < size:
                part = peer.recv(size - len(body))
                self.assertTrue(part, "truncated response body")
                body += part
            return json.loads(body)

    def test_control_and_v1_status_share_one_owned_socket(self):
        with self.owner(mode="control") as (_, control):
            request = self.control_request()
            result = self.control_exchange(request)
            self.assertEqual(result["disposition"], "accepted")
            status = self.call("query", self.library)
            self.assertEqual(status.returncode, 0, status.stderr)
            self.assertEqual(json.loads(status.stdout)["schema"], "bridgevm.app-runtime.v1")
            request["requestID"] = str(uuid.uuid4()).upper()
            self.assertEqual(self.control_exchange(request)["disposition"], "existing")
            request["operation"] = "stopStatus"
            self.assertEqual(self.control_exchange(request)["disposition"], "status")
            self.assertEqual(len(list(control.glob("admitted-*"))), 1)

    def test_status_only_owner_refuses_control_without_poisoning_v1(self):
        with self.owner() as (_, control):
            self.assertIsNone(self.control_exchange(self.control_request()))
            self.assertEqual(list(control.glob("admitted-*")), [])
            self.assertEqual(self.call("query", self.library).returncode, 0)

    def test_malformed_control_and_library_mismatch_never_admit(self):
        with self.owner(mode="control") as (_, control):
            request = self.control_request()
            payload = json.dumps(request, sort_keys=True, separators=(",", ":")).encode()
            for raw in [b" " + payload, payload[:-1] + b',"unknown":true}',
                        payload.replace(b'"operation":"stop"', b'"operation":"stop","operation":"stop"')]:
                self.assertIsNone(self.control_exchange(request, raw=raw))
            request["library"] = dict(self.identity, inode=self.identity["inode"] + 1)
            self.assertIsNone(self.control_exchange(request))
            self.assertEqual(list(control.glob("admitted-*")), [])

    def test_reply_timeout_preserves_admitted_ticket_for_status(self):
        with self.owner(mode="control-slow") as (_, control):
            request = self.control_request()
            began = time.monotonic()
            self.assertIsNone(self.control_exchange(request))
            self.assertLess(time.monotonic() - began, 2.8)
            self.assertTrue((control / ("admitted-" + request["operationID"])).exists())
            request["operation"] = "stopStatus"
            request["requestID"] = str(uuid.uuid4()).upper()
            self.assertEqual(self.control_exchange(request)["disposition"], "status")
            self.assertEqual(len(list(control.glob("admitted-*"))), 1)

    def test_disconnected_submitter_does_not_erase_admission(self):
        with self.owner(mode="control-slow") as (_, control):
            request = self.control_request()
            payload = json.dumps(request, sort_keys=True, separators=(",", ":")).encode()
            with socket.socket(socket.AF_UNIX) as peer:
                peer.connect(str(self.namespace / "status.sock"))
                peer.sendall(len(payload).to_bytes(4, "big") + payload)
                deadline = time.monotonic() + 1
                while not (control / ("admitted-" + request["operationID"])).exists():
                    self.assertLess(time.monotonic(), deadline, "control was never admitted")
                    time.sleep(0.01)
            request["operation"] = "stopStatus"
            request["requestID"] = str(uuid.uuid4()).upper()
            self.assertEqual(self.control_exchange(request)["disposition"], "status")

    def test_timed_out_control_handlers_keep_slots_until_they_finish(self):
        with self.owner(mode="control-slow") as (_, control):
            peers = []
            try:
                for _ in range(4):
                    request = self.control_request()
                    payload = json.dumps(request, sort_keys=True, separators=(",", ":")).encode()
                    peer = socket.socket(socket.AF_UNIX)
                    peers.append(peer)
                    peer.settimeout(3)
                    peer.connect(str(self.namespace / "status.sock"))
                    peer.sendall(len(payload).to_bytes(4, "big") + payload)
                deadline = time.monotonic() + 1
                while len(list(control.glob("admitted-*"))) != 4:
                    self.assertLess(time.monotonic(), deadline, "four controls were not admitted")
                    time.sleep(0.01)
                for peer in peers:
                    self.assertEqual(peer.recv(1), b"")
                began = time.monotonic()
                self.assertNotEqual(self.call("query", self.library).returncode, 0)
                self.assertLess(time.monotonic() - began, 0.8)
                self.assertEqual(len(list(control.glob("admitted-*"))), 4)
            finally:
                for peer in peers:
                    peer.close()
