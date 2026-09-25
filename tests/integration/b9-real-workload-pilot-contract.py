#!/usr/bin/env python3
"""Deterministic B9 pilot seals, raw recomputation and fail-closed fixtures."""

from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/live-gates"))
sys.path.insert(0, str(ROOT / "scripts"))
import b9_real_workload_inputs as inputs
from b9_real_workload_observation import observe, scanout_hashes
from b9_real_workload_queue import finalize
from b9_real_workload_receipt import (FLAGS, cleanup_guard, job_fields,
                                      public_view, validate_private)
from b9_workload_diagnostic import summarize_csv

RUNNER_SPEC = importlib.util.spec_from_file_location(
    "b9_real_playback_runner", ROOT / "scripts/live-gates/run-b9-real-workload-pilot.py")
RUNNER = importlib.util.module_from_spec(RUNNER_SPEC)
RUNNER_SPEC.loader.exec_module(RUNNER)


def digest(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def put(path: Path, raw: bytes) -> str:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(raw)
    return digest(raw)


class B9PilotContract(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir=str(Path(tempfile.gettempdir()).resolve()))
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.commit = "a" * 40
        self.job_id = "b9-contract-r1"
        self.job_dir = self.root / "queued" / self.job_id
        self.job_dir.mkdir(parents=True)
        self.asset_hashes = {key: digest(key.encode()) for key in inputs.KEYS}
        fields = {"job_id": self.job_id, "tier": "d9-b9-real-workload",
                  "commit": self.commit, "input_manifest_sha256": "b" * 64,
                  "sealed_binary_sha256": "c" * 64}
        fields.update({"asset_" + key + "_sha256": value
                       for key, value in self.asset_hashes.items()})
        (self.job_dir / "job.env").write_text(
            "".join(f"{key}={value}\n" for key, value in fields.items()), encoding="ascii")
        ledger = self.root / "job-ledger" / self.job_id / "entry.env"
        ledger.parent.mkdir(parents=True)
        ledger.write_text("".join(f"{key}={value}\n" for key, value in fields.items()),
                          encoding="ascii")
        ledger.chmod(0o400)
        self.job = job_fields(self.job_dir)

    def fixture(self):
        diagnostic = self.job_dir / "diagnostic"
        guest = diagnostic / "raw/guest"
        guest.mkdir(parents=True)
        nonce, pid, hwnd = "f" * 32, 4242, 777
        umd = "d" * 64
        csv = "Application,ProcessID,SwapChainAddress,CPUStartTime,FrameTime,CPUBusy,CPUWait\n"
        csv += "".join(f"vlc.exe,{pid},0x1234,{i * 1000},1000,600,400\n" for i in range(9))
        csv_raw = csv.encode()
        csv_sha = put(guest / "presentmon.csv", csv_raw)
        ready = {"schema": "bridgevm.b9-vlc-ready.v1", "nonce": nonce,
                 "pid": pid, "hwnd": hwnd, "title": "VLC media player",
                 "media_sha256": self.asset_hashes["media"],
                 "vlc_zip_sha256": self.asset_hashes["vlc_zip"],
                 "presentmon_sha256": self.asset_hashes["presentmon"],
                 "expected_driver_umd_sha256": umd}
        collector = {"schema": "bridgevm.b9-collector-ready.v1", "nonce": nonce,
                     "pid": pid, "session": "BridgeVM-B9-" + nonce, "collector_pid": 5252}
        finished = {"schema": "bridgevm.b9-vlc-finished.v1", "nonce": nonce,
                    "pid": pid, "hwnd": hwnd, "window_visible": True,
                    "vlc_exit_observed": True, "vlc_exit_code": 0,
                    "playback_elapsed_ms": 9000, "collector_started": True,
                    "collector_exit_code": 0, "csv_sha256": csv_sha,
                    "csv_bytes": len(csv_raw), "direct3d11_module_loaded": True,
                    "av1_decoder_module_loaded": True, "driver_umd_sha256": umd,
                    "failure_code": "none", "failure_detail": ""}
        ready_sha = put(guest / "guest-ready.json", json.dumps(ready).encode())
        collector_sha = put(guest / "guest-collector.json", json.dumps(collector).encode())
        finished_sha = put(guest / "guest-finished.json", json.dumps(finished).encode())
        run_log = (f"BVAGENT WINFOCUS {hwnd} -> OK WINFOCUS\r\n"
                   f"B9-FOREGROUND-{hwnd}\r\n"
                   "live input accepted: command=Key(<redacted>)\r\n"
                   "stop: PSCI SYSTEM_OFF (system off)\r\n").encode()
        put(guest / "run.log", run_log)
        frames = []
        for index in range(3):
            folder = diagnostic / "raw" / f"scanout-{index}"
            width, height = 640, 480
            bgra = bytes((index + 1, index + 2, index + 3, 255)) * (width * height)
            bgra_sha = put(folder / "presented.bgra", bgra)
            rgb = bytes((index + 3, index + 2, index + 1)) * (width * height)
            ppm_sha = put(folder / "presented.ppm",
                          f"P6\n{width} {height}\n255\n".encode() + rgb)
            metadata = ("source=active-cgl-iosurface\niosurface_id=42\n"
                        f"width={width}\nheight={height}\ninitial_seed={index}\n"
                        f"captured_seed={index + 1}\nnonblack_pixels={width * height}\n"
                        f"bgra_sha256={bgra_sha}\nppm_sha256={ppm_sha}\n").encode()
            put(folder / "capture.env", metadata)
            frames.append(folder / "presented.ppm")
        hashes, captures = scanout_hashes(frames)
        metrics = summarize_csv(guest / "presentmon.csv", csv_sha, pid)
        artifacts = {name: {"bytes": (guest / name).stat().st_size,
                            "sha256": inputs.stable_file(guest / name)[1]}
                     for name in ("guest-ready.json", "guest-collector.json",
                                  "guest-finished.json", "presentmon.csv", "run.log")}
        value = {"schema": "bridgevm.b9-real-workload-pilot.v1",
                 "tier": "d9-b9-real-workload", "criterion": "B9",
                 "candidate_id": "vlc-3.0.23-arm64-kodi-bbb-1080p-av1-10s-v1",
                 "job_id": self.job_id, "commit": self.commit,
                 "input_manifest_sha256": self.job["input_manifest_sha256"],
                 "sealed_binary_sha256": self.job["sealed_binary_sha256"],
                 "asset_hashes": self.asset_hashes, "driver_umd_sha256": umd,
                 "outcome": "diagnostic-complete", "result_class": "VLC_PID_PRESENTS_CAPTURED",
                 "pilot_count": 1, "required_workload_count": 20,
                 "cleanup_complete": True, "source_integrity": True,
                 "owned_process_group_stopped": True, "owned_vm_pgid": 999999,
                 "owned_work_path": str(self.root / ("b9-pilot-" + self.job_id)),
                 "guest_shutdown_observed": True, "frame_count": metrics["frame_count"],
                 "scanout_sample_count": 3, "distinct_scanout_count": 3,
                 "scanout_files": [str(path.relative_to(diagnostic / "raw")) for path in frames],
                 "scanout_hashes": hashes, "scanout_capture_hashes": captures,
                 "nonce_sha256": digest(nonce.encode()), "ready_sha256": ready_sha,
                 "collector_sha256": collector_sha, "finished_sha256": finished_sha,
                 "private_artifact_dir": "raw/guest", "private_artifacts": artifacts,
                 "csv_sha256": csv_sha, "target_pid": pid, "window_handle": hwnd,
                 "collector_session": collector["session"], "vlc_exit_code": 0,
                 "collector_exit_code": 0, "playback_elapsed_ms": 9000,
                 "swapchain_address": metrics["swapchain_address"],
                 "cpu_start_span_ms": metrics["cpu_start_span_ms"],
                 "p50_nearest_rank_ms": metrics["p50_nearest_rank_ms"],
                 "p95_nearest_rank_ms": metrics["p95_nearest_rank_ms"],
                 "p99_nearest_rank_ms": metrics["p99_nearest_rank_ms"],
                 **{flag: False for flag in FLAGS}}
        return diagnostic, value
    def test_raw_pid_capture_receipt_and_public_no_claim(self):
        diagnostic, value = self.fixture()
        validate_private(value, self.job, diagnostic)
        public = public_view(value, self.job)
        self.assertEqual(public["result_class"], "VLC_PID_PRESENTS_CAPTURED")
        self.assertTrue(all(public[flag] is False for flag in FLAGS))
        self.assertNotIn("private_artifacts", public)
        self.assertEqual(public["required_workload_count"], 20)
    def test_self_consistent_metric_shape_cannot_override_raw_csv(self):
        diagnostic, value = self.fixture()
        tampered = copy.deepcopy(value)
        for name in ("p50_nearest_rank_ms", "p95_nearest_rank_ms", "p99_nearest_rank_ms"):
            tampered[name] = 1001.0
        with self.assertRaisesRegex(ValueError, "raw data"):
            validate_private(tampered, self.job, diagnostic)

    def test_changed_scanout_and_claim_flag_are_rejected(self):
        diagnostic, value = self.fixture()
        frame = diagnostic / "raw/scanout-0/presented.bgra"
        frame.write_bytes(frame.read_bytes()[:-1] + b"\0")
        with self.assertRaises(ValueError):
            validate_private(value, self.job, diagnostic)
        value["criterion_pass"] = True
        with self.assertRaisesRegex(ValueError, "claims"):
            validate_private(value, self.job, diagnostic)

    def test_missing_runner_receipt_remains_incomplete_and_fenced(self):
        finalize(self.job_dir, self.commit)
        missing = json.loads((self.job_dir / "receipt.json").read_text())
        self.assertFalse(missing["cleanup_complete"])
        self.assertEqual(missing["result_class"], "INVALID_EVIDENCE")
        with self.assertRaises(ValueError):
            cleanup_guard(self.job_dir, self.commit, self.job_id)

    def test_queue_refuses_unsealed_pilot_before_creating_a_job(self):
        queue = self.root / "queue"
        env = dict(os.environ, BRIDGEVM_LIVE_ROOT=str(queue))
        result = subprocess.run([str(ROOT / "scripts/live-gates/bridgevm-live"),
                                 "submit", "d9-b9-real-workload"], env=env,
                                capture_output=True, text=True, check=False)
        self.assertEqual(result.returncode, 2)
        self.assertIn("needs --input-manifest", result.stderr)
        self.assertFalse(queue.exists())

    def test_queue_job_asset_tamper_cannot_override_immutable_ledger(self):
        path = self.job_dir / "job.env"
        changed = path.read_text().replace("asset_media_sha256=" + self.asset_hashes["media"],
                                           "asset_media_sha256=" + "f" * 64)
        path.write_text(changed)
        with self.assertRaisesRegex(ValueError, "immutable queue ledger"):
            job_fields(self.job_dir)
    def test_guest_share_waits_for_completed_transfer_and_exact_byte_count(self):
        share = self.root / "share"
        share.mkdir()
        name = "ready-" + "f" * 32 + ".json"
        target = share / name
        log = self.root / "run.log"
        target.write_bytes(b'{"schema":')  # visible destination while std::fs::write is incomplete
        process = type("UnfinishedProcess", (), {"poll": lambda self: None})()
        log.write_text(f"BVAGENT SHARE guest->host {name} bytes={target.stat().st_size} t=0\n")
        with self.assertRaisesRegex(ValueError, "malformed guest observation"):
            RUNNER.await_guest_file(share, name, log, process, 1)
        log.write_text("")
        raw = b'{"schema":"complete"}'
        def finish():
            target.write_bytes(raw)
            log.write_text(f"BVAGENT SHARE guest->host {name} bytes={len(raw)} t=1\n")
        timer = threading.Timer(0.3, finish)
        timer.start()
        try:
            value, digest_value = RUNNER.await_guest_file(share, name, log, process, 2)
        finally:
            timer.join()
        self.assertEqual(value, {"schema": "complete"})
        self.assertEqual(digest_value, digest(raw))
        log.write_text(f"BVAGENT SHARE guest->host {name} bytes={len(raw) + 1} t=2\n")
        with self.assertRaisesRegex(ValueError, "byte count"):
            RUNNER.await_guest_file(share, name, log, process, 1)
    def test_no_guest_ready_cannot_be_called_pid_capture(self):
        result = observe(self.root, "f" * 32,
                         {"media_sha256": "a" * 64, "vlc_zip_sha256": "b" * 64,
                          "presentmon_sha256": "c" * 64,
                          "expected_driver_umd_sha256": "d" * 64}, [])
        self.assertEqual(result["result_class"], "GUEST_NOT_READY")
        self.assertEqual(result["frame_count"], 0)

    def test_retained_writable_work_fences_even_with_valid_raw_receipt(self):
        diagnostic, value = self.fixture()
        home = self.root / "home"
        work = home / "BridgeVM/work" / ("b9-pilot-" + self.job_id)
        work.mkdir(parents=True)
        value["owned_work_path"] = str(work)
        (self.job_dir / "receipt.json").write_text(json.dumps(value))
        with patch.object(Path, "home", return_value=home):
            with self.assertRaisesRegex(ValueError, "remain after runner"):
                cleanup_guard(self.job_dir, self.commit, self.job_id)
            value["result_class"] = "PLAYBACK_INCOMPLETE"
            del value["owned_work_path"]
            (self.job_dir / "receipt.json").write_text(json.dumps(value))
            with self.assertRaisesRegex(ValueError, "remain after runner"):
                cleanup_guard(self.job_dir, self.commit, self.job_id)
    def test_chunked_share_reconstructs_exact_archive(self):
        source = self.root / "assets"
        source.mkdir()
        records = {}
        for key, name, data in (("media", "media.webm", b"abc"),
                                ("presentmon", "presentmon.exe", b"xyz"),
                                ("guest_script", "guest.ps1", b"script"),
                                ("vlc_zip", "vlc.zip", bytes(range(95)))):
            path = source / name
            records[key] = (path, put(path, data))
        with patch.object(inputs, "CHUNK_BYTES", 10), patch.object(inputs, "verify_inputs"):
            staged = inputs.stage_share(records, self.root / "share")
        parts = [self.root / "share" / f"b9-vlc-part-{i:02d}.bin" for i in range(10)]
        self.assertEqual(b"".join(path.read_bytes() for path in parts), bytes(range(95)))
        self.assertTrue(all(0 < size <= 10 for name, (size, _) in staged.items() if name != "b9-vlc-parts.tsv"))
        self.assertLess(staged["b9-vlc-parts.tsv"][0], 8000000)
        self.assertEqual(len(staged), 14)
    def test_guest_asset_is_crlf_and_launch_is_real_window_path(self):
        raw = (ROOT / "scripts/win-assets/bv-b9-vlc-playback.ps1").read_bytes()
        self.assertEqual(raw.count(b"\n"), raw.count(b"\r\n"))
        text = raw.decode("ascii")
        for marker in ("Invoke-CimMethod -ClassName Win32_Process", "--start-paused",
                       "--vout=direct3d11", "--process_id ", "viogpu_d3d10.dll",
                       "libdav1d_plugin.dll", "$ExpectedD3D11UmdSha",
                       "[IO.File]::WriteAllText($temp", "[IO.File]::Move($temp, $Path)",
                       "--output_file \"' + $captureCsvPath", "[IO.File]::Copy($captureCsvPath, $csvTempPath)",
                       "[IO.File]::Move($csvTempPath, $csvPath)",
                       "GetForegroundWindow() -eq $hwnd", "TotalMilliseconds -ge 7000", "if ($raw -ceq $Nonce) { return }"):
            self.assertIn(marker, text)
        self.assertNotIn("B9 host marker nonce differs", text)

if __name__ == "__main__":
    unittest.main(verbosity=2)
