#!/usr/bin/env python3
"""Post-READY T17 private packets: kind admission, host-share listing and tier retention."""
from __future__ import annotations

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import unittest

FIXTURE = Path(__file__).with_name("t17-private-diagnostic-packet-test.py")
SPEC = importlib.util.spec_from_file_location("t17_packet_fixture_post_ready", FIXTURE)
assert SPEC and SPEC.loader
fixture = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(fixture)
packet = fixture.packet
PREFIX = fixture.NONCE[:12]
DETAIL = f"guest workload did not produce t17-keyboard-pointer-{PREFIX}.txt"
STOP = (f"host_stop=status=complete,generation=7,nonce={fixture.STOP_NONCE},"
        "report=complete,helper=terminal,log_offset=0")
LISTING = "t17-diagnostic-lane-1-share-listing.json"
SHARE_SECRET = b"synthetic-private-share-bytes"


class PostReadyCase(unittest.TestCase):
    def setUp(self) -> None:
        self.case = fixture.PacketTest(methodName="test_complete_packet_binds_identity_and_keeps_private_bytes_out_of_index")
        self.case.setUp()
        self.addCleanup(self.case.tearDown)
        self.case.result.update(first_ready=True, failure_detail=DETAIL)
        self.case.refresh_seal()
        self.args = self.case.args
        self.args.kind = "post-ready"
        (self.case.evidence / "run.log").write_bytes(
            b"BVAGENT READY FIRST\nT17-LAUNCHED-KeyboardPointer-" + PREFIX.encode() + b" pid=7\nsynthetic-private-guest-bytes\n")
        self.share = self.case.lane / "share"
        self.share.mkdir()
        (self.share / f"t17-{PREFIX}.txt").write_bytes(f"bridgevm-t17-share-v1\n{fixture.NONCE}\n".encode())
        (self.share / "bv-product-e2e.ps1").write_bytes(SHARE_SECRET)

    def listing(self) -> dict:
        return json.loads((self.case.private / LISTING).read_text())

    def refused(self) -> None:
        with self.assertRaises(packet.CaptureError):
            packet.capture(self.args)
        self.assertFalse((self.case.private / "t17-diagnostic-lane-1-index.json").exists())
        self.assertFalse((self.case.private / LISTING).exists())


class PostReadyPacketTest(PostReadyCase):
    def test_post_ready_failure_retains_a_verified_packet_with_share_listing(self) -> None:
        packet.capture(self.args)
        packet.verify(self.args)
        index = self.case.index()
        self.assertEqual((index["packet_kind"], index["host_stop_status"]), ("post-ready", "not-requested"))
        self.assertIsNone(index["observed_generation"])
        self.assertEqual([item["status"] for item in index["artifacts"]], ["retained", "missing", "missing", "missing"])
        self.assertEqual(index["guest_setup"]["outcome"], "disk-absent")
        body = (self.case.private / LISTING).read_bytes()
        self.assertEqual(index["share_listing"], {"file": LISTING, "sha256": hashlib.sha256(body).hexdigest()})
        listing = self.listing()
        self.assertEqual((listing["status"], listing["entry_count"], listing["truncated"]), ("listed", 2, False))
        self.assertEqual([(entry["name"], entry["type"], entry["bytes"], entry["sha256"], entry["reason"]) for entry in listing["entries"]],
                         [(path.name, "file", path.stat().st_size, hashlib.sha256(path.read_bytes()).hexdigest(), "none")
                          for path in sorted(self.share.iterdir())])
        self.assertEqual(listing["hashed_bytes"], sum(path.stat().st_size for path in self.share.iterdir()))
        self.assertNotIn(SHARE_SECRET, body)
        self.assertNotIn(SHARE_SECRET.decode(), json.dumps(index))
        self.assertEqual(stat.S_IMODE(self.case.private.stat().st_mode), 0o700)
        for name in [LISTING] + [item["file"] for item in index["artifacts"] if item["file"]]:
            self.assertEqual(stat.S_IMODE((self.case.private / name).stat().st_mode), 0o600)

    def test_first_ready_packet_records_its_kind_and_no_share_listing(self) -> None:
        self.case.result.update(first_ready=False, failure_detail=f"first boot has no BVAGENT READY/PONG evidence; {STOP}")
        self.case.refresh_seal()
        self.case.complete_sources()
        self.args.kind = "first-ready"
        packet.capture(self.args)
        packet.verify(self.args)
        index = self.case.index()
        self.assertEqual((index["packet_kind"], index["share_listing"], index["observed_generation"]), ("first-ready", None, 7))
        self.assertFalse((self.case.private / LISTING).exists())

    def test_kind_must_match_the_authenticated_result(self) -> None:
        for kind in ("first-ready", "later", None):
            with self.subTest(kind=kind):
                self.args.kind = kind
                self.refused()
        self.args.kind = "post-ready"
        for field, value in (("failure_code", "snapshot-unavailable"), ("failure_code", "internal-error"),
                             ("first_ready", False), ("failure_detail", None),
                             ("failure_detail", f"{DETAIL}; host_stop=status=missing,reason=a; host_stop=status=missing,reason=b"),
                             ("failure_detail", "first boot has no BVAGENT READY/PONG evidence; host_stop=status=missing,reason=a")):
            with self.subTest(field=field, value=value):
                original = dict(self.case.result)
                self.case.result[field] = value
                self.case.refresh_seal()
                self.refused()
                self.case.result = original
                self.case.refresh_seal()
        self.case.result.update(first_ready=False, failure_detail="runtime_state=timed-out; bounded")
        self.case.refresh_seal()
        for kind in ("first-ready", "post-ready"):
            with self.subTest(timed_out=kind):
                self.args.kind = kind
                self.refused()

    def test_verifier_rejects_kind_listing_and_result_tampering(self) -> None:
        packet.capture(self.args)
        index_path = self.case.private / "t17-diagnostic-lane-1-index.json"
        listing_path = self.case.private / LISTING
        index, body = index_path.read_text(), listing_path.read_bytes()
        self.args.kind = "first-ready"
        with self.assertRaises(packet.CaptureError):
            packet.verify(self.args)
        self.args.kind = "post-ready"
        for key, value in (("packet_kind", "first-ready"), ("share_listing", None),
                           ("share_listing", {"file": LISTING, "sha256": "0" * 64}),
                           ("share_listing", {"file": "t17-diagnostic-lane-1-index.json", "sha256": hashlib.sha256(body).hexdigest()}),
                           ("host_stop_status", "missing")):
            with self.subTest(key=key, value=value):
                changed = json.loads(index)
                changed[key] = value
                index_path.write_text(json.dumps(changed))
                with self.assertRaises(packet.CaptureError):
                    packet.verify(self.args)
        index_path.write_text(index)
        for number, mutate in enumerate((lambda value: value.update(truncated=True),
                                         lambda value: value["entries"].reverse(),
                                         lambda value: value["entries"][0].update(sha256=None),
                                         lambda value: value["entries"][0].update(name="../escape"),
                                         lambda value: value.update(hashed_bytes=value["hashed_bytes"] + 1))):
            with self.subTest(listing_mutation=number):
                changed = json.loads(body)
                mutate(changed)
                data = json.dumps(changed).encode()
                listing_path.write_bytes(data)
                index_path.write_text(json.dumps({**json.loads(index), "share_listing": {"file": LISTING, "sha256": hashlib.sha256(data).hexdigest()}}))
                with self.assertRaises(ValueError):
                    packet.verify(self.args)
        listing_path.write_bytes(body)
        index_path.write_text(index)
        packet.verify(self.args)
        listing_path.write_bytes(body.replace(b'"listed"', b'"absent"'))
        with self.assertRaises(packet.CaptureError):
            packet.verify(self.args)
        listing_path.write_bytes(body)
        listing_path.chmod(0o644)
        with self.assertRaises(packet.CaptureError):
            packet.verify(self.args)
        listing_path.chmod(0o600)
        self.case.result["first_ready"] = False
        result_sha = fixture.json_file(self.case.private / "lane-1-result.json", self.case.result)
        stamp = json.loads((self.case.private / "lane-1-authenticated.json").read_text())
        fixture.json_file(self.case.private / "lane-1-authenticated.json", {**stamp, "result_sha256": result_sha})
        with self.assertRaises(packet.CaptureError):
            packet.verify(self.args)

    def test_post_ready_host_stop_claim_uses_the_nonce_bound_report(self) -> None:
        self.case.result["failure_detail"] = f"{DETAIL}; {STOP}"
        self.case.refresh_seal()
        self.case.complete_sources()
        (self.case.evidence / "display.fb").unlink()
        packet.capture(self.args)
        packet.verify(self.args)
        index = self.case.index()
        self.assertEqual((index["host_stop_status"], index["observed_generation"]), ("complete", 7))
        self.assertEqual([item["status"] for item in index["artifacts"]], ["retained", "retained", "retained", "missing"])

    def test_listing_destination_collision_discards_the_whole_packet(self) -> None:
        (self.case.private / LISTING).write_bytes(b"{}")
        with self.assertRaises(FileExistsError):
            packet.capture(self.args)
        self.assertEqual(sorted(path.name for path in self.case.private.iterdir()),
                         ["lane-1-authenticated.json", "lane-1-result.json", LISTING])


def augment_synthetic_helper(path: Path) -> None:
    path.write_text(path.read_text() + '''
if r["job_id"].startswith("post-ready-"):
    for stage in stages[6:]: result[stage] = False
    result["failure_code"] = "snapshot-unavailable" if "other-failure" in r["job_id"] else "guest-evidence-missing"
    result["failure_detail"] = f"guest workload did not produce t17-keyboard-pointer-{prefix}.txt"
    pathlib.Path(a.result).write_text(json.dumps(result, sort_keys=True) + "\\n")
    (share/f"t17-keyboard-pointer-{prefix}.txt").unlink()
    if "capture-fail" in r["job_id"]: final_log.unlink(); final_log.mkdir()
''')


def run_tier_fixtures(tier: Path, manifest: Path, temporary: Path, commit: str) -> None:
    publisher = fixture.ROOT / "scripts/live-gates/publish-receipt.sh"
    for name, public, retained in (("post-ready-diagnostic-fixture", "integration-failed", True),
                                   ("post-ready-capture-fail-fixture", "integration-failed", False),
                                   ("post-ready-other-failure-fixture", "snapshot-failed", None)):
        out = temporary / f"{name}-out"
        completed = subprocess.run([str(tier), "--out", str(out), "--input-manifest", str(manifest),
                                    "--job-id", name], capture_output=True, text=True, check=False)
        assert completed.returncode != 0, "synthetic post-READY failure unexpectedly passed"
        receipt = json.loads((out / "receipt.json").read_text())
        private = out / "private"
        result = json.loads((private / "lane-1-result.json").read_text())
        assert (receipt["outcome"], receipt["failure_code"], receipt["pass"]) == ("failed", public, False), receipt
        assert receipt["worker_cleanup_verified"] is True and receipt["first_ready_passes"] == 1, receipt
        assert result["first_ready"] is True and result["keyboard_pointer"] is False
        helper_log = (private / "lane-1-helper.log").read_text()
        lane_root = Path(next(line.removeprefix("lane_root=") for line in helper_log.splitlines()
                              if line.startswith("lane_root=")))
        assert not os.path.lexists(lane_root.parent), "tier did not remove the owned lane"
        index = private / "t17-diagnostic-lane-1-index.json"
        marker = private / "lane-1-diagnostic-capture-failed"
        if retained:
            assert index.exists() and not marker.exists(), f"post-READY capture refused: {completed.stderr[-1200:]}"
            subprocess.run([sys.executable, str(fixture.SCRIPT), "verify", "--kind", "post-ready",
                            "--private", str(private.resolve()), "--job-id", name, "--commit", commit,
                            "--campaign-mode", "pilot", "--lane", "1"], check=True)
            value = json.loads(index.read_text())
            assert (value["packet_kind"], value["host_stop_status"]) == ("post-ready", "not-requested"), value
            assert [item["status"] for item in value["artifacts"]] == ["retained", "missing", "missing", "missing"], value
            names = [entry["name"] for entry in json.loads((private / LISTING).read_text())["entries"]]
            prefix = result["nonce"][:12]
            assert {f"t17-{prefix}.txt", f"t17-network-{prefix}.txt"} <= set(names), names
            assert f"t17-keyboard-pointer-{prefix}.txt" not in names, names
        else:
            assert not index.exists() and not (private / LISTING).exists()
            assert marker.exists() == (retained is False), completed.stderr[-1200:]
        subprocess.run([str(publisher), "t17-windows-hvf-product-e2e", str(out),
                        str(fixture.ROOT), commit], check=True, capture_output=True, text=True)
        public_text = (out / "receipt.public.json").read_text()
        assert public_text == (out / "receipt.json").read_text()
        assert str(lane_root) not in public_text and "t17-diagnostic-lane-1" not in public_text


if __name__ == "__main__":
    if sys.argv[1:2] == ["--augment-helper"]:
        augment_synthetic_helper(Path(sys.argv[2]))
    elif sys.argv[1:2] == ["--tier-fixtures"]:
        run_tier_fixtures(Path(sys.argv[2]), Path(sys.argv[3]), Path(sys.argv[4]), sys.argv[5])
    else:
        unittest.main()
