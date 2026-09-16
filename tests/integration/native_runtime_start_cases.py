"""Start wire cases retain synthetic tickets; no VM or application is launched."""
import json
import socket
import time
import uuid


class NativeRuntimeStartCases:
    def start_request(self, operation="start", operation_id=None):
        return dict(schema="bridgevm.app-runtime-start-request.v1", operation=operation,
            requestID=str(uuid.uuid4()).upper(), library=self.identity, vmID="fixture",
            operationID=operation_id or str(uuid.uuid4()).upper(),
            appInstanceID="A0000000-0000-0000-0000-000000000001",
            expectedSavedConfigurationDigest="a" * 64)

    def test_start_stop_and_status_share_one_owned_socket(self):
        with self.owner(mode="start") as (_, control):
            request = self.start_request()
            first = self.control_exchange(request)
            self.assertEqual(first["disposition"], "accepted")
            self.assertEqual(first["observation"]["deadlineUptime"], 40)
            self.assertNotIn("target", first["observation"])
            request["requestID"] = str(uuid.uuid4()).upper()
            self.assertEqual(self.control_exchange(request)["disposition"], "existing")
            request["operation"] = "startStatus"
            self.assertEqual(self.control_exchange(request)["observation"], first["observation"])
            self.assertEqual(self.call("query", self.library).returncode, 0)
            self.assertEqual(self.control_exchange(self.control_request())["disposition"], "accepted")
            self.assertEqual(len(list(control.glob("start-admitted-*"))), 1)

    def test_start_canonical_binding_and_schema_fail_closed(self):
        with self.owner(mode="start") as (_, control):
            request = self.start_request()
            payload = json.dumps(request, sort_keys=True, separators=(",", ":")).encode()
            for raw in [b" " + payload, payload[:-1] + b',"processID":123}',
                        payload.replace(b'"operation":"start"', b'"operation":"start","operation":"start"'),
                        payload.replace(b"app-runtime-start-request.v1", b"app-runtime-control-request.v2")]:
                self.assertIsNone(self.control_exchange(request, raw=raw))
            changed_library = dict(request, library=dict(self.identity, inode=self.identity["inode"] + 1))
            self.assertIsNone(self.control_exchange(changed_library))
            changed_owner = dict(request, appInstanceID=str(uuid.uuid4()).upper())
            self.assertEqual(self.control_exchange(changed_owner)["refusal"], "ownerChanged")
            self.assertEqual(list(control.glob("start-admitted-*")), [])
            self.assertEqual(self.control_exchange(request)["disposition"], "accepted")
            request["expectedSavedConfigurationDigest"] = "b" * 64
            self.assertEqual(self.control_exchange(request)["refusal"], "operationConflict")
            self.assertEqual(len(list(control.glob("start-admitted-*"))), 1)

    def test_unknown_start_status_and_status_only_owner_do_not_admit(self):
        with self.owner(mode="start") as (_, control):
            self.assertEqual(self.control_exchange(self.start_request("startStatus"))["refusal"], "operationUnknown")
            self.assertEqual(list(control.glob("start-admitted-*")), [])
        with self.owner() as (_, control):
            self.assertIsNone(self.control_exchange(self.start_request()))
            self.assertEqual(self.call("query", self.library).returncode, 0)
            self.assertEqual(list(control.glob("start-admitted-*")), [])

    def test_start_reply_timeout_preserves_original_deadline_and_ticket(self):
        with self.owner(mode="start-slow") as (_, control):
            request = self.start_request()
            began = time.monotonic()
            self.assertIsNone(self.control_exchange(request))
            self.assertLess(time.monotonic() - began, 2.8)
            request["operation"] = "startStatus"
            request["requestID"] = str(uuid.uuid4()).upper()
            result = self.control_exchange(request)
            self.assertEqual(result["disposition"], "status")
            self.assertEqual(result["observation"]["deadlineUptime"], 40)
            self.assertEqual(len(list(control.glob("start-admitted-*"))), 1)

    def test_start_disconnect_keeps_operation_queryable(self):
        with self.owner(mode="start-slow") as (_, control):
            request = self.start_request()
            payload = json.dumps(request, sort_keys=True, separators=(",", ":")).encode()
            with socket.socket(socket.AF_UNIX) as peer:
                peer.connect(str(self.namespace / "status.sock"))
                peer.sendall(len(payload).to_bytes(4, "big") + payload)
                deadline = time.monotonic() + 1
                while not (control / ("start-admitted-" + request["operationID"])).exists():
                    self.assertLess(time.monotonic(), deadline)
                    time.sleep(0.01)
            request["operation"] = "startStatus"
            request["requestID"] = str(uuid.uuid4()).upper()
            self.assertEqual(self.control_exchange(request)["disposition"], "status")
            self.assertEqual(len(list(control.glob("start-admitted-*"))), 1)
