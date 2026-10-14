"""Cross-language negative controls found during independent protocol review."""
import json
import os
import unittest

from app_ui_install_cancel_fixtures import Fixture, save, digest, protocol, observations


class EvidenceRejections(unittest.TestCase):
    def rebind(self, fixture):
        save(fixture.private / "install-cancel-session.json", fixture.session)
        new_hash = digest(fixture.private / "install-cancel-session.json")
        for name in ("install-cancel-ready.json", "install-cancel-completion.json"):
            path = fixture.private / name
            value = json.loads(path.read_text())
            value["sessionSHA256"] = new_hash
            if "driver" in value:
                value["driver"] = fixture.session["driver"]
            if "hostPID" in value:
                value["hostPID"] = fixture.session["host"]["pid"]
            save(path, value)
        for path in sorted((fixture.private / "driver-requests").iterdir()):
            request = json.loads(path.read_text())
            request["sessionSHA256"] = new_hash
            save(path, request)
            reply_path = fixture.private / "driver-replies" / path.name
            reply = json.loads(reply_path.read_text())
            reply.update(sessionSHA256=new_hash, requestSHA256=digest(path), hostPID=fixture.session["host"]["pid"])
            save(reply_path, reply)

    def test_no_reply_after_terminal_retry_phase(self):
        fixture = Fixture(self)
        request = json.loads((fixture.private / "driver-requests/000008.json").read_text())
        reply = json.loads((fixture.private / "driver-replies/000008.json").read_text())
        request["sequence"] = 9
        path = fixture.private / "driver-requests/000009.json"
        save(path, request)
        reply.update(sequence=9, requestSHA256=digest(path))
        save(fixture.private / "driver-replies/000009.json", reply)
        save(fixture.private / "install-cancel-completion.json", dict(fixture.completion, requestsProcessed=9))
        with self.assertRaises(ValueError):
            protocol.verify_driver(fixture.private, fixture.launch)

    def test_equal_byte_hardlink_cannot_replace_owned_fixture_file(self):
        fixture = Fixture(self)
        path = fixture.observed / observations.FIXTURE_FILES[0]
        outside = fixture.root / "equal-byte-owned-copy"
        outside.write_bytes(path.read_bytes())
        path.unlink()
        os.link(outside, path)
        self.assertEqual(digest(path), fixture.report["fixture_files"][0]["sha256"])
        self.assertEqual(path.stat().st_nlink, 2)
        with self.assertRaises(ValueError):
            observations.verify_observations(fixture.observed, fixture.reads)

    def test_null_optional_cannot_masquerade_as_omitted_swift_success_field(self):
        fixture = Fixture(self)
        save(fixture.private / "install-cancel-completion.json", dict(fixture.completion, failureCode=None))
        with self.assertRaises(ValueError):
            protocol.verify_driver(fixture.private, fixture.launch)

    def test_session_deadline_remains_exactly_ninety_seconds(self):
        fixture = Fixture(self)
        fixture.session["deadlineUptime"] = 190.0000005
        fixture.launch["deadline_uptime"] = 190.0000005
        self.rebind(fixture)
        with self.assertRaises(ValueError):
            protocol.verify_driver(fixture.private, fixture.launch)

    def test_process_identity_fits_the_swift_int32_contract(self):
        fixture = Fixture(self)
        fixture.session["host"]["pid"] = 2 ** 31
        fixture.launch["host"]["pid"] = 2 ** 31
        self.rebind(fixture)
        with self.assertRaises(ValueError):
            protocol.verify_driver(fixture.private, fixture.launch)
